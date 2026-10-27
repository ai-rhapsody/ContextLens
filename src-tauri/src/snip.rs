//! Screen capture for the snipping overlay: one monitor's screenshot plus the windows on it,
//! so the overlay can offer region, window and full-screen modes like the Windows snipping tool.

use crate::imaging;
use contextlens_core::Media;
use image::{DynamicImage, RgbaImage};
use serde::{Deserialize, Serialize};
use xcap::{Monitor, Window};

/// A window's visible rectangle, in physical pixels relative to the monitor's top-left.
#[derive(Serialize, Clone)]
pub struct WinRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub title: String,
}

pub struct Snip {
    pub monitor_x: i32,
    pub monitor_y: i32,
    pub image: RgbaImage,
    /// Front-most first.
    pub windows: Vec<WinRect>,
    pub tutor: bool,
}

/// What the overlay needs to draw.
#[derive(Serialize)]
pub struct SnipView {
    /// JPEG data URL of the whole monitor.
    pub preview: String,
    pub width: u32,
    pub height: u32,
    pub windows: Vec<WinRect>,
}

/// A selection in physical pixels relative to the monitor.
#[derive(Deserialize, Clone, Copy, Debug)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

pub fn take(cursor_x: i32, cursor_y: i32, tutor: bool) -> Result<Snip, String> {
    let err = |e: xcap::XCapError| format!("無法擷取螢幕：{e}");
    let monitor = Monitor::from_point(cursor_x, cursor_y)
        .or_else(|_| Monitor::all().map_err(err)?.into_iter().next().ok_or_else(|| "找不到螢幕".to_string()))
        .map_err(|e| e.to_string())?;
    let (mx, my) = (monitor.x().map_err(err)?, monitor.y().map_err(err)?);
    let image = monitor.capture_image().map_err(err)?;
    let (mw, mh) = (image.width() as i32, image.height() as i32);
    Ok(Snip { monitor_x: mx, monitor_y: my, windows: windows_on(mx, my, mw, mh), image, tutor })
}

fn windows_on(mx: i32, my: i32, mw: i32, mh: i32) -> Vec<WinRect> {
    let own = std::process::id();
    let mut found: Vec<(i32, WinRect)> = Window::all()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|w| {
            if w.is_minimized().unwrap_or(true) || w.pid().ok()? == own {
                return None;
            }
            let (x, y) = (w.x().ok()? - mx, w.y().ok()? - my);
            let (ww, wh) = (w.width().ok()? as i32, w.height().ok()? as i32);
            // Clip to the monitor; skip slivers and windows on other monitors.
            let (x0, y0) = (x.max(0), y.max(0));
            let (x1, y1) = ((x + ww).min(mw), (y + wh).min(mh));
            if x1 - x0 < 40 || y1 - y0 < 40 {
                return None;
            }
            let title = w.title().unwrap_or_default();
            let z = w.z().unwrap_or(0);
            Some((z, WinRect { x: x0, y: y0, w: x1 - x0, h: y1 - y0, title }))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, r)| r).collect()
}

pub fn view(s: &Snip) -> Result<SnipView, String> {
    let preview = imaging::jpeg_b64(&DynamicImage::ImageRgba8(s.image.clone()), 82)?;
    Ok(SnipView {
        preview: format!("data:image/jpeg;base64,{preview}"),
        width: s.image.width(),
        height: s.image.height(),
        windows: s.windows.clone(),
    })
}

/// Clamps a selection to the screenshot; None if it is too small to read.
pub fn clamp(r: Rect, width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
    let x0 = r.x.min(r.x + r.w).max(0.0).floor() as u32;
    let y0 = r.y.min(r.y + r.h).max(0.0).floor() as u32;
    let x1 = (r.x.max(r.x + r.w).ceil() as u32).min(width);
    let y1 = (r.y.max(r.y + r.h).ceil() as u32).min(height);
    (x1 > x0 + 4 && y1 > y0 + 4).then(|| (x0, y0, x1 - x0, y1 - y0))
}

pub fn crop(s: &Snip, r: Rect) -> Result<Media, String> {
    let (x, y, w, h) = clamp(r, s.image.width(), s.image.height()).ok_or("選取範圍太小")?;
    let part = image::imageops::crop_imm(&s.image, x, y, w, h).to_image();
    imaging::png_media(&part, "螢幕擷取.png")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_normalises_drag_direction_and_bounds() {
        let r = Rect { x: 120.0, y: 90.0, w: -100.0, h: -80.0 };
        assert_eq!(clamp(r, 1000, 1000), Some((20, 10, 100, 80)));
        let off = Rect { x: -50.0, y: 950.0, w: 200.0, h: 200.0 };
        assert_eq!(clamp(off, 1000, 1000), Some((0, 950, 150, 50)));
    }

    #[test]
    fn clamp_rejects_tiny_selections() {
        assert_eq!(clamp(Rect { x: 5.0, y: 5.0, w: 3.0, h: 300.0 }, 100, 100), None);
    }
}
