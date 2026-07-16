use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, GenericImageView, ImageEncoder, ImageReader};
use poolsync_core::{encode_message, hash_bytes, hash_text, Message};
use std::io::Cursor;
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Instant;
use tokio::process::Command;
use tokio::sync::mpsc::UnboundedSender;
use tokio::time::{timeout, Duration};

const IMAGE_MIMES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/jpg",
    "image/bmp",
    "image/x-xfce-ng",
    "image/x-qt-image",
];

const MIN_TEXT_SYNC_LEN: usize = 2;
const MAX_IMAGE_BYTES: usize = 4 * 1024 * 1024;
/// Timeout dur sur chaque lecture xclip : un propriétaire de sélection figé
/// (app morte, RDP…) ne doit jamais geler la boucle presse-papiers.
const XCLIP_READ_TIMEOUT: Duration = Duration::from_secs(2);
/// Après envoi d'une image, ignorer le texte résiduel sur le presse-papiers X11.
const IMAGE_TEXT_GRACE: Duration = Duration::from_secs(4);

static LAST_IMAGE_SENT_AT: Mutex<Option<Instant>> = Mutex::new(None);

pub fn mark_image_clipboard_epoch() {
    if let Ok(mut guard) = LAST_IMAGE_SENT_AT.lock() {
        *guard = Some(Instant::now());
    }
}

pub fn image_clipboard_grace_active() -> bool {
    LAST_IMAGE_SENT_AT
        .lock()
        .ok()
        .and_then(|g| *g)
        .is_some_and(|t| t.elapsed() < IMAGE_TEXT_GRACE)
}

fn clear_image_clipboard_epoch() {
    if let Ok(mut guard) = LAST_IMAGE_SENT_AT.lock() {
        *guard = None;
    }
}

fn clipboard_has_image_targets_sync() -> bool {
    std::process::Command::new("xclip")
        .args(["-selection", "clipboard", "-t", "TARGETS", "-o"])
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .any(|l| l.trim().starts_with("image/"))
        })
        .unwrap_or(false)
}

/// Ignore un texte distant qui écraserait une image locale récente.
pub async fn should_reject_remote_text() -> bool {
    if image_clipboard_grace_active() {
        return true;
    }
    clipboard_targets("clipboard")
        .await
        .map(|t| targets_have_image(&t))
        .unwrap_or(false)
}

/// Lance `xclip` en lecture avec timeout. `kill_on_drop` garantit qu'un xclip
/// bloqué est tué (pas d'accumulation de processus zombies figés).
async fn xclip_read(args: &[&str]) -> Result<std::process::Output> {
    let child = Command::new("xclip")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .context("spawn xclip")?;
    match timeout(XCLIP_READ_TIMEOUT, child.wait_with_output()).await {
        Ok(res) => res.context("xclip wait"),
        Err(_) => anyhow::bail!("xclip timeout: {}", args.join(" ")),
    }
}

pub struct ClipboardPayload {
    pub mime: String,
    pub wire_data: String,
    pub hash: String,
}

pub fn targets_have_image(targets: &[String]) -> bool {
    targets.iter().any(|t| t.starts_with("image/"))
}

pub async fn clipboard_targets(selection: &str) -> Result<Vec<String>> {
    let output = xclip_read(&["-selection", selection, "-t", "TARGETS", "-o"]).await?;
    if !output.status.success() {
        return Ok(Vec::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

pub async fn read_clipboard_payload() -> Result<Option<ClipboardPayload>> {
    let targets = clipboard_targets("clipboard").await?;
    let mut image_mimes: Vec<String> = targets
        .iter()
        .filter(|t| t.starts_with("image/"))
        .cloned()
        .collect();
    image_mimes.sort_by_key(|m| match m.as_str() {
        "image/png" => 0,
        "image/jpeg" | "image/jpg" => 1,
        _ => 2,
    });
    for mime in &image_mimes {
        if let Ok(bytes) = read_selection_bytes("clipboard", mime).await {
            if !bytes.is_empty() {
                return image_payload_from_bytes(&bytes).map(Some);
            }
        }
    }
    for mime in IMAGE_MIMES {
        if targets.is_empty() || targets.iter().any(|t| t == mime) {
            if let Ok(bytes) = read_selection_bytes("clipboard", mime).await {
                if !bytes.is_empty() {
                    return image_payload_from_bytes(&bytes).map(Some);
                }
            }
        }
    }
    // Ne pas synchroniser un vieux texte tant que le presse-papiers expose encore une image.
    if targets_have_image(&targets) {
        return Ok(None);
    }
    if let Ok(text) = read_selection_text("clipboard").await {
        if is_syncable_text(&text) {
            return Ok(Some(ClipboardPayload {
                mime: "text/plain".into(),
                wire_data: text.clone(),
                hash: hash_text(&text),
            }));
        }
    }
    Ok(None)
}

/// Après écriture distante, aligne le hash sur le clipboard local (évite boucle xclip).
pub async fn align_hash_after_write(last_clip_hash: &Mutex<String>) {
    if let Ok(Some(payload)) = read_clipboard_payload().await {
        if let Ok(mut last) = last_clip_hash.lock() {
            *last = payload.hash;
        }
    }
}

pub fn try_send_payload(
    payload: &ClipboardPayload,
    out_tx: &UnboundedSender<String>,
    last_clip_hash: &Mutex<String>,
) -> bool {
    if payload.mime == "text/plain" {
        if !is_syncable_text(&payload.wire_data) {
            return false;
        }
        if let Ok(guard) = LAST_IMAGE_SENT_AT.lock() {
            if guard
                .as_ref()
                .is_some_and(|t| t.elapsed() < IMAGE_TEXT_GRACE)
            {
                if clipboard_has_image_targets_sync() {
                    return false;
                }
                clear_image_clipboard_epoch();
            }
        }
    }
    let mut last = match last_clip_hash.lock() {
        Ok(guard) => guard,
        Err(_) => return false,
    };
    if *last == payload.hash {
        return false;
    }
    *last = payload.hash.clone();
    drop(last);
    if let Ok(encoded) = encode_message(&Message::Clipboard {
        msg_id: uuid::Uuid::new_v4().to_string(),
        hash: payload.hash.clone(),
        mime: payload.mime.clone(),
        data: payload.wire_data.clone(),
    }) {
        let _ = out_tx.send(encoded);
        if payload.mime.starts_with("image/") {
            mark_image_clipboard_epoch();
        }
        return true;
    }
    false
}

pub async fn write_clipboard(data: &str, mime: &str) -> Result<()> {
    if mime == "text/plain" {
        write_selection_text("clipboard", data).await
    } else if mime.starts_with("image/") {
        let bytes = B64
            .decode(data)
            .with_context(|| format!("decode base64 image ({mime})"))?;
        write_image_clipboard(&bytes).await
    } else {
        anyhow::bail!("unsupported clipboard mime: {mime}");
    }
}

/// Écriture presse-papiers depuis le thread GTK (systray / historique).
pub fn write_clipboard_sync(data: &str, mime: &str) -> Result<()> {
    if mime == "text/plain" {
        write_selection_text_sync("clipboard", data)
    } else if mime.starts_with("image/") {
        let bytes = B64
            .decode(data)
            .with_context(|| format!("decode base64 image ({mime})"))?;
        if write_image_clipboard_gtk(&bytes) {
            Ok(())
        } else {
            write_selection_bytes_sync("clipboard", "image/png", &bytes)
        }
    } else {
        anyhow::bail!("unsupported clipboard mime: {mime}");
    }
}

fn write_selection_text_sync(selection: &str, text: &str) -> Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new("xclip")
        .args(["-selection", selection])
        .stdin(Stdio::piped())
        .spawn()
        .with_context(|| format!("xclip -selection {selection}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(text.as_bytes())
            .context("xclip stdin")?;
    }
    let status = child.wait().context("xclip wait")?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("xclip write {selection} failed");
    }
}

fn write_selection_bytes_sync(selection: &str, mime: &str, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let mut child = Command::new("xclip")
        .args(["-selection", selection, "-t", mime])
        .stdin(Stdio::piped())
        .spawn()
        .with_context(|| format!("xclip -selection {selection} -t {mime}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(bytes).context("xclip stdin")?;
    }
    let status = child.wait().context("xclip wait")?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("xclip write {selection} {mime} failed");
    }
}

async fn write_image_clipboard(png_bytes: &[u8]) -> Result<()> {
    let bytes = png_bytes.to_vec();
    let gtk_ok = tokio::task::spawn_blocking(move || write_image_clipboard_gtk(&bytes))
        .await
        .context("gtk clipboard task")?;
    if gtk_ok {
        return Ok(());
    }
    write_selection_bytes("clipboard", "image/png", png_bytes).await
}

fn write_image_clipboard_gtk(png_bytes: &[u8]) -> bool {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let script = format!("{home}/.local/bin/write-image-clipboard.py");
    if !std::path::Path::new(&script).exists() {
        return false;
    }
    let mut child = match Command::new("python3")
        .arg(&script)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    if let Some(mut stdin) = child.stdin.take() {
        if stdin.write_all(png_bytes).is_err() {
            let _ = child.kill();
            return false;
        }
    }
    child.wait().map(|s| s.success()).unwrap_or(false)
}

fn is_syncable_text(text: &str) -> bool {
    text.trim().len() >= MIN_TEXT_SYNC_LEN
}

fn image_payload_from_bytes(bytes: &[u8]) -> Result<ClipboardPayload> {
    if bytes.len() > MAX_IMAGE_BYTES {
        anyhow::bail!("image too large ({} bytes)", bytes.len());
    }
    let img = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .context("guess image format")?
        .decode()
        .context("decode image")?;
    let (w, h) = img.dimensions();
    let rgba = img.to_rgba8();
    let mut png = Vec::new();
    PngEncoder::new(&mut png)
        .write_image(rgba.as_raw(), w, h, ExtendedColorType::Rgba8)
        .context("encode png")?;
    Ok(ClipboardPayload {
        mime: "image/png".into(),
        wire_data: B64.encode(&png),
        hash: hash_bytes(&png),
    })
}

pub async fn read_selection_text(selection: &str) -> Result<String> {
    let output = xclip_read(&["-selection", selection, "-o"]).await?;
    if !output.status.success() {
        anyhow::bail!("xclip read {selection} text failed");
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub async fn write_selection_text(selection: &str, text: &str) -> Result<()> {
    let mut child = Command::new("xclip")
        .args(["-selection", selection])
        .stdin(Stdio::piped())
        .spawn()
        .with_context(|| format!("xclip -selection {selection}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin.write_all(text.as_bytes()).await?;
    }
    child.wait().await?;
    Ok(())
}

async fn read_selection_bytes(selection: &str, mime: &str) -> Result<Vec<u8>> {
    let output = xclip_read(&["-selection", selection, "-t", mime, "-o"]).await?;
    if !output.status.success() {
        anyhow::bail!("xclip read {selection} {mime} failed");
    }
    Ok(output.stdout)
}

async fn write_selection_bytes(selection: &str, mime: &str, bytes: &[u8]) -> Result<()> {
    let mut child = Command::new("xclip")
        .args(["-selection", selection, "-t", mime])
        .stdin(Stdio::piped())
        .spawn()
        .with_context(|| format!("xclip -selection {selection} -t {mime}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin.write_all(bytes).await?;
    }
    child.wait().await?;
    Ok(())
}
