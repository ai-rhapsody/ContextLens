//! Encoding, shrinking and thumbnails for images sent to a model.

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use contextlens_core::{media, Media, MediaKind, Provider};
use image::{codecs::jpeg::JpegEncoder, DynamicImage, ImageFormat, RgbaImage};
use std::io::Cursor;

/// Longest side sent to a model. Larger screenshots cost more and read no better.
const MAX_SIDE: u32 = 2000;

pub fn png_media(img: &RgbaImage, name: &str) -> Result<Media, String> {
    let img = fit(DynamicImage::ImageRgba8(img.clone()), MAX_SIDE);
    let mut buf = Vec::new();
    img.write_to(&mut Cursor::new(&mut buf), ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(Media { mime: "image/png".into(), data_b64: B64.encode(buf), name: Some(name.into()) })
}

pub fn jpeg_b64(img: &DynamicImage, quality: u8) -> Result<String, String> {
    let mut buf = Vec::new();
    JpegEncoder::new_with_quality(&mut buf, quality)
        .encode_image(&img.to_rgb8())
        .map_err(|e| e.to_string())?;
    Ok(B64.encode(buf))
}

fn fit(img: DynamicImage, side: u32) -> DynamicImage {
    if img.width().max(img.height()) > side {
        img.resize(side, side, image::imageops::FilterType::Lanczos3)
    } else {
        img
    }
}

fn decode(m: &Media) -> Option<DynamicImage> {
    let bytes = B64.decode(&m.data_b64).ok()?;
    image::load_from_memory(&bytes).ok()
}

/// Re-encodes an image that is over the provider's size limit, or larger than needed.
/// Anything that cannot be decoded is passed through; the provider check reports it.
pub fn prepare(provider: Provider, m: Media) -> Media {
    if m.kind() != Some(MediaKind::Image) {
        return m;
    }
    let too_big = m.byte_len() > media::image_byte_limit(provider);
    let Some(img) = decode(&m) else { return m };
    if !too_big && img.width().max(img.height()) <= MAX_SIDE {
        return m;
    }
    match jpeg_b64(&fit(img, MAX_SIDE), 88) {
        Ok(data_b64) => Media { mime: "image/jpeg".into(), data_b64, name: m.name },
        Err(_) => m,
    }
}

/// A small JPEG data URL for the history list.
pub fn thumbnail(m: &Media) -> Option<String> {
    if m.kind() != Some(MediaKind::Image) {
        return None;
    }
    let img = decode(m)?.thumbnail(240, 240);
    jpeg_b64(&img, 70).ok().map(|b| format!("data:image/jpeg;base64,{b}"))
}
