use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use poolsync_core::{encode_message, hash_bytes, hash_text, Message};
use std::process::Stdio;
use std::sync::Mutex;
use tokio::process::Command;
use tokio::sync::mpsc::UnboundedSender;

const IMAGE_MIMES: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/jpg",
    "image/bmp",
    "image/x-xfce-ng",
    "image/x-qt-image",
];

pub struct ClipboardPayload {
    pub mime: String,
    pub wire_data: String,
    pub hash: String,
}

pub fn targets_have_image(targets: &[String]) -> bool {
    targets.iter().any(|t| t.starts_with("image/"))
}

pub async fn clipboard_targets(selection: &str) -> Result<Vec<String>> {
    let output = Command::new("xclip")
        .args(["-selection", selection, "-t", "TARGETS", "-o"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await?;
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
    for mime in IMAGE_MIMES {
        if targets.iter().any(|t| t == mime) {
            if let Ok(bytes) = read_selection_bytes("clipboard", mime).await {
                if !bytes.is_empty() {
                    return Ok(Some(ClipboardPayload {
                        mime: mime.to_string(),
                        wire_data: B64.encode(&bytes),
                        hash: hash_bytes(&bytes),
                    }));
                }
            }
        }
    }
    if let Ok(text) = read_selection_text("clipboard").await {
        if !text.is_empty() {
            return Ok(Some(ClipboardPayload {
                mime: "text/plain".into(),
                wire_data: text.clone(),
                hash: hash_text(&text),
            }));
        }
    }
    Ok(None)
}

pub fn try_send_payload(
    payload: &ClipboardPayload,
    out_tx: &UnboundedSender<String>,
    last_clip_hash: &Mutex<String>,
) -> bool {
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
        write_selection_bytes("clipboard", mime, &bytes).await
    } else {
        anyhow::bail!("unsupported clipboard mime: {mime}");
    }
}

pub async fn read_selection_text(selection: &str) -> Result<String> {
    let output = Command::new("xclip")
        .args(["-selection", selection, "-o"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await?;
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
    let output = Command::new("xclip")
        .args(["-selection", selection, "-t", mime, "-o"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await?;
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
