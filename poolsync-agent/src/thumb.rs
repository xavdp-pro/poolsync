use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use image::imageops::FilterType;
use image::{ImageFormat, ImageReader};
use muda::Icon;
use std::io::Cursor;

pub const NOTIFY_THUMB_MAX_PX: u32 = 96;
pub const TRAY_THUMB_MAX_PX: u32 = 64;
pub const TRAY_MENU_DISPLAY_PX: u32 = 48;
pub const TRAY_MENU_SOURCE_PX: u32 = 144;
pub const LIST_THUMB_MAX_PX: u32 = 128;
pub const PREVIEW_THUMB_MAX_PX: u32 = 320;

/// PNG redimensionné (carré max `max_px`).
pub fn thumb_png_bytes_from_wire(wire_b64: &str, max_px: u32) -> Result<Vec<u8>> {
    let bytes = B64
        .decode(wire_b64)
        .context("decode image base64 for thumbnail")?;
    let reader = ImageReader::new(Cursor::new(&bytes)).with_guessed_format()?;
    let img = reader.decode().context("decode image pixels")?;
    let thumb = img.resize(max_px, max_px, FilterType::Triangle);
    let mut out = Vec::new();
    thumb
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .context("encode thumbnail png")?;
    Ok(out)
}

pub fn thumb_b64_from_wire(wire_b64: &str, max_px: u32) -> Result<String> {
    Ok(B64.encode(thumb_png_bytes_from_wire(wire_b64, max_px)?))
}

pub fn muda_icon_from_thumb_b64(thumb_b64: &str) -> Option<Icon> {
    let bytes = B64.decode(thumb_b64).ok()?;
    let img = image::load_from_memory(&bytes).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    Icon::from_rgba(rgba.into_raw(), w, h).ok()
}
