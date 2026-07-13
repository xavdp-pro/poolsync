use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use image::imageops::FilterType;
use image::ImageReader;
use std::io::Cursor;
use std::path::PathBuf;

const THUMB_MAX_PX: u32 = 96;

pub fn notify_thumbnail_path(mime: &str, wire_b64: &str) -> Result<String> {
    let bytes = B64
        .decode(wire_b64)
        .with_context(|| format!("decode image for thumbnail ({mime})"))?;
    let reader = ImageReader::new(Cursor::new(&bytes)).with_guessed_format()?;
    let img = reader.decode().context("decode image pixels")?;
    let thumb = img.resize(THUMB_MAX_PX, THUMB_MAX_PX, FilterType::Triangle);

    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let dir = PathBuf::from(home).join(".cache/poolsync");
    std::fs::create_dir_all(&dir).context("create notify thumb cache dir")?;
    let path = dir.join("notify-thumb.png");
    thumb
        .save(&path)
        .with_context(|| format!("write {}", path.display()))?;
    Ok(path.to_string_lossy().into_owned())
}
