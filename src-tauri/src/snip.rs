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

/// What the selection is for.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    /// Crop and translate it.
    Translate,
    /// Watch it in a Live session.
    LiveTarget,
}

/// One monitor's frozen screenshot.
pub struct Shot {
    pub monitor_x: i32,
    pub monitor_y: i32,
    pub image: RgbaImage,
    /// Front-most first, clipped to this monitor.
    pub windows: Vec<WinRect>,
}

/// A snipping session covers every monitor; each gets its own overlay window ("snip-<index>"),
/// so per-monitor scaling stays exact.
pub struct Snip {
    pub purpose: Purpose,
    /// What to do with the selection (for `Purpose::Translate`).
    pub task: contextlens_core::Task,
    pub tutor: bool,
    pub shots: Vec<Shot>,
}

/// What the overlay needs to draw.
#[derive(Serialize)]
pub struct SnipView {
    /// JPEG data URL of the whole monitor.
    pub preview: String,
    pub width: u32,
    pub height: u32,
    pub windows: Vec<WinRect>,
    pub purpose: Purpose,
}

/// A selection in physical pixels relative to the monitor.
#[derive(Deserialize, Clone, Copy, Debug)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

pub fn take(tutor: bool, task: contextlens_core::Task, purpose: Purpose) -> Result<Snip, String> {
    let err = |e: xcap::XCapError| format!("無法擷取螢幕：{e}");
    let windows = all_windows();
    let mut shots = Vec::new();
    let mut last_error = None;
    for monitor in Monitor::all().map_err(err)? {
        let shot = (|| {
            let (mx, my) = (monitor.x().map_err(err)?, monitor.y().map_err(err)?);
            let image = monitor.capture_image().map_err(err)?;
            let (mw, mh) = (image.width() as i32, image.height() as i32);
            Ok::<_, String>(Shot { monitor_x: mx, monitor_y: my, windows: clip_to(&windows, mx, my, mw, mh), image })
        })();
        // One monitor failing (asleep, mirrored) should not cost the others.
        match shot {
            Ok(s) => shots.push(s),
            Err(e) => last_error = Some(e),
        }
    }
    if shots.is_empty() {
        return Err(last_error.unwrap_or_else(|| "找不到螢幕".into()));
    }
    Ok(Snip { purpose, task, tutor, shots })
}

/// Every other app's visible window in screen coordinates, front-most first.
fn all_windows() -> Vec<(i32, i32, i32, i32, String)> {
    let own = std::process::id();
    let mut found: Vec<(i32, (i32, i32, i32, i32, String))> = Window::all()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|w| {
            if w.is_minimized().unwrap_or(true) || w.pid().ok()? == own {
                return None;
            }
            let rect = (w.x().ok()?, w.y().ok()?, w.width().ok()? as i32, w.height().ok()? as i32);
            Some((w.z().unwrap_or(0), (rect.0, rect.1, rect.2, rect.3, w.title().unwrap_or_default())))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, r)| r).collect()
}

/// The windows on one monitor, clipped to it, relative to its top-left; slivers dropped.
fn clip_to(windows: &[(i32, i32, i32, i32, String)], mx: i32, my: i32, mw: i32, mh: i32) -> Vec<WinRect> {
    windows
        .iter()
        .filter_map(|(x, y, w, h, title)| {
            let (x, y) = (x - mx, y - my);
            let (x0, y0) = (x.max(0), y.max(0));
            let (x1, y1) = ((x + w).min(mw), (y + h).min(mh));
            (x1 - x0 >= 40 && y1 - y0 >= 40).then(|| WinRect { x: x0, y: y0, w: x1 - x0, h: y1 - y0, title: title.clone() })
        })
        .collect()
}

pub fn view(s: &Snip, index: usize) -> Result<SnipView, String> {
    let shot = s.shots.get(index).ok_or("沒有這個螢幕")?;
    let preview = imaging::jpeg_b64(&DynamicImage::ImageRgba8(shot.image.clone()), 82)?;
    Ok(SnipView {
        preview: format!("data:image/jpeg;base64,{preview}"),
        width: shot.image.width(),
        height: shot.image.height(),
        windows: shot.windows.clone(),
        purpose: s.purpose,
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

pub fn crop(shot: &Shot, r: Rect) -> Result<Media, String> {
    let (x, y, w, h) = clamp(r, shot.image.width(), shot.image.height()).ok_or("選取範圍太小")?;
    let part = image::imageops::crop_imm(&shot.image, x, y, w, h).to_image();
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
    fn windows_are_clipped_to_each_monitor() {
        // Two 1000-wide monitors side by side; one window straddles both.
        let wins = vec![(900, 100, 300, 200, "straddler".to_string()), (1500, 50, 30, 30, "tiny".to_string())];
        let left = clip_to(&wins, 0, 0, 1000, 800);
        assert_eq!((left[0].x, left[0].w), (900, 100));
        let right = clip_to(&wins, 1000, 0, 1000, 800);
        assert_eq!(right.len(), 1); // the 30x30 window is a sliver
        assert_eq!((right[0].x, right[0].w, right[0].title.as_str()), (0, 200, "straddler"));
    }

    #[test]
    fn clamp_rejects_tiny_selections() {
        assert_eq!(clamp(Rect { x: 5.0, y: 5.0, w: 3.0, h: 300.0 }, 100, 100), None);
    }
}
