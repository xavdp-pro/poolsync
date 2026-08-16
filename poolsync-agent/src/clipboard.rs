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
const XCLIP_IMAGE_READ_TIMEOUT: Duration = Duration::from_secs(10);
const CLIPBOARD_PY_TIMEOUT: Duration = Duration::from_secs(3);
/// Ne pas relancer le secours GTK à chaque poll (évite python3 à 100 % CPU).
const GTK_READ_COOLDOWN: Duration = Duration::from_secs(3);
/// Après envoi d'une image, ignorer le texte résiduel sur le presse-papiers X11.
const IMAGE_TEXT_GRACE: Duration = Duration::from_secs(4);

static LAST_IMAGE_SENT_AT: Mutex<Option<Instant>> = Mutex::new(None);
static LAST_GTK_READ_AT: Mutex<Option<Instant>> = Mutex::new(None);

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

fn gtk_read_allowed() -> bool {
    LAST_GTK_READ_AT
        .lock()
        .ok()
        .and_then(|g| *g)
        .is_none_or(|t| t.elapsed() >= GTK_READ_COOLDOWN)
}

fn note_gtk_read_attempt() {
    if let Ok(mut guard) = LAST_GTK_READ_AT.lock() {
        *guard = Some(Instant::now());
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

/// Ignore un texte distant qui écraserait une image locale *récemment* envoyée.
/// Ne pas bloquer tant qu'il reste une cible image/ (sinon texte asus↔acer mort après 1 image).
pub async fn should_reject_remote_text() -> bool {
    image_clipboard_grace_active()
}

/// Lance `xclip` en lecture avec timeout. `kill_on_drop` garantit qu'un xclip
/// bloqué est tué (pas d'accumulation de processus zombies figés).
async fn xclip_read(args: &[&str]) -> Result<std::process::Output> {
    xclip_read_timeout(args, XCLIP_READ_TIMEOUT).await
}

async fn xclip_read_timeout(args: &[&str], limit: Duration) -> Result<std::process::Output> {
    let child = Command::new("xclip")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .context("spawn xclip")?;
    match timeout(limit, child.wait_with_output()).await {
        Ok(res) => res.context("xclip wait"),
        Err(_) => anyhow::bail!("xclip timeout: {}", args.join(" ")),
    }
}

pub struct ClipboardPayload {
    pub mime: String,
    pub wire_data: String,
    pub hash: String,
}

impl Clone for ClipboardPayload {
    fn clone(&self) -> Self {
        Self {
            mime: self.mime.clone(),
            wire_data: self.wire_data.clone(),
            hash: self.hash.clone(),
        }
    }
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
    // Secours GTK (xfce4-screenshooter…) — seulement si X11 annonce une image, pas à chaque poll texte.
    if targets_have_image(&targets) && gtk_read_allowed() {
        note_gtk_read_attempt();
        if let Ok(bytes) = read_image_via_gtk().await {
            if !bytes.is_empty() {
                if let Ok(payload) = image_payload_from_bytes(&bytes) {
                    tracing::debug!("clipboard image lu via GTK ({} bytes)", bytes.len());
                    return Ok(Some(payload));
                }
            }
        }
    }
    // Ne pas synchroniser un vieux texte tant que le presse-papiers expose encore une image.
    if targets_have_image(&targets) {
        return Ok(None);
    }
    // Même sans cible image/ annoncée : magie PNG/JPEG dans le buffer « texte ».
    if let Ok(raw) = read_selection_bytes("clipboard", "UTF8_STRING").await {
        if raw.starts_with(&[0x89, 0x50, 0x4E, 0x47])
            || (raw.len() >= 3 && raw[0] == 0xFF && raw[1] == 0xD8 && raw[2] == 0xFF)
        {
            return image_payload_from_bytes(&raw).map(Some);
        }
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

/// Détection locale : met à jour le hash, enregistre le cache, retourne true si nouveau contenu.
pub fn prepare_local_clipboard(
    payload: &ClipboardPayload,
    last_clip_hash: &Mutex<String>,
) -> bool {
    if payload.mime == "text/plain" && !is_syncable_text(&payload.wire_data) {
        return false;
    }
    // Texte local = intention utilisateur : toujours autoriser (ne pas bloquer
    // après envoi d'image — sinon copier-coller texte mort pendant IMAGE_TEXT_GRACE).
    if payload.mime == "text/plain" {
        clear_image_clipboard_epoch();
    }
    let mut last = match last_clip_hash.lock() {
        Ok(guard) => guard,
        Err(_) => return false,
    };
    if *last == payload.hash {
        return false;
    }
    *last = payload.hash.clone();
    true
}

/// Envoi réseau (hub optionnel + peer) — après cache local, ne bloque pas l'usage local.
pub fn send_payload_network(
    payload: &ClipboardPayload,
    hub_tx: &UnboundedSender<String>,
    peer_tx: &Option<UnboundedSender<String>>,
    relay_hub: bool,
) -> bool {
    if let Ok(encoded) = encode_message(&Message::Clipboard {
        msg_id: uuid::Uuid::new_v4().to_string(),
        hash: payload.hash.clone(),
        mime: payload.mime.clone(),
        data: payload.wire_data.clone(),
    }) {
        let mut sent = false;
        if relay_hub {
            sent |= hub_tx.send(encoded.clone()).is_ok();
        }
        if let Some(peer) = peer_tx {
            sent |= peer.send(encoded).is_ok();
        }
        if !sent {
            return false;
        }
        if payload.mime.starts_with("image/") {
            mark_image_clipboard_epoch();
        }
        return true;
    }
    false
}

pub fn try_send_payload(
    payload: &ClipboardPayload,
    hub_tx: &UnboundedSender<String>,
    peer_tx: &Option<UnboundedSender<String>>,
    last_clip_hash: &Mutex<String>,
    relay_hub: bool,
) -> bool {
    if !prepare_local_clipboard(payload, last_clip_hash) {
        return false;
    }
    send_payload_network(payload, hub_tx, peer_tx, relay_hub)
}

pub async fn write_clipboard(data: &str, mime: &str) -> Result<()> {
    if mime == "text/plain" {
        write_selection_text("clipboard", data).await
    } else if mime.starts_with("image/") {
        let bytes = B64
            .decode(data)
            .with_context(|| format!("decode base64 image ({mime})"))?;
        write_image_to_clipboard_async(&bytes, mime).await
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
        write_image_to_clipboard(&bytes, mime)
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
    // stdin fermé : xclip possède la sélection. On le laisse vivre pour la servir
    // aux collages suivants — cf. detach_selection_owner.
    detach_selection_owner_sync(child);
    Ok(())
}

/// Variante synchrone de [`detach_selection_owner`] pour les écritures hors runtime
/// tokio (systray, fenêtre d'historique).
fn detach_selection_owner_sync(mut child: std::process::Child) {
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}

fn detect_image_mime(bytes: &[u8]) -> &'static str {
    if bytes.len() >= 4 && bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        "image/png"
    } else if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        "image/jpeg"
    } else {
        "image/png"
    }
}

fn write_image_to_clipboard(bytes: &[u8], mime: &str) -> Result<()> {
    let mime = if mime.starts_with("image/") {
        mime
    } else {
        detect_image_mime(bytes)
    };
    // Écriture multi-mimes : certaines applications X11 (Qt/Chromium) attendent image/png ou TARGETS précises.
    let res_xclip = write_selection_bytes_sync("clipboard", mime, bytes)
        .or_else(|_| write_selection_bytes_sync("clipboard", "image/png", bytes));

    // Le fallback GTK est un *repli*, pas un complément : il prend lui aussi la
    // sélection et évincerait le xclip détaché qui la conserve. Deux propriétaires
    // successifs laissaient une image tronquée que l'agent relisait comme une
    // nouvelle copie — l'image repartait alors sur le réseau en se dégradant à
    // chaque tour. On ne l'exécute donc que si xclip a réellement échoué.
    if res_xclip.is_ok() {
        return Ok(());
    }
    if write_image_clipboard_gtk(bytes) {
        Ok(())
    } else {
        anyhow::bail!("échec de l'écriture image dans le presse-papiers X11/GTK")
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
    // xclip doit rester propriétaire de la sélection (cf. detach_selection_owner).
    // On ne peut donc pas juger l'écriture sur son code de sortie : on lui laisse un
    // court instant pour échouer bruyamment (mime refusé, DISPLAY absent), et s'il
    // est toujours là c'est qu'il sert la sélection.
    std::thread::sleep(std::time::Duration::from_millis(120));
    match child.try_wait().context("xclip try_wait")? {
        Some(status) if !status.success() => {
            anyhow::bail!("xclip write {selection} {mime} failed");
        }
        Some(_) => Ok(()),
        None => {
            detach_selection_owner_sync(child);
            Ok(())
        }
    }
}

async fn write_image_to_clipboard_async(bytes: &[u8], mime: &str) -> Result<()> {
    let owned = bytes.to_vec();
    let mime = mime.to_string();
    tokio::task::spawn_blocking(move || write_image_to_clipboard(&owned, &mime))
        .await
        .context("image clipboard task")?
}

fn write_image_clipboard_gtk(image_bytes: &[u8]) -> bool {
    run_clipboard_py_script("write-image-clipboard.py", image_bytes)
}

fn read_image_via_gtk_sync() -> Result<Vec<u8>> {
    let output = run_clipboard_py_script_output("read-image-clipboard.py", &[])?;
    if output.status.success() && !output.stdout.is_empty() {
        Ok(output.stdout)
    } else {
        anyhow::bail!("gtk clipboard read failed");
    }
}

async fn read_image_via_gtk() -> Result<Vec<u8>> {
    tokio::task::spawn_blocking(read_image_via_gtk_sync)
        .await
        .context("gtk read task")?
}

fn clipboard_py_script(name: &str) -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    let path = std::path::PathBuf::from(home)
        .join(".local/bin")
        .join(name);
    if path.is_file() {
        Some(path)
    } else {
        None
    }
}

fn run_clipboard_py_script(name: &str, stdin_bytes: &[u8]) -> bool {
    run_clipboard_py_script_output(name, stdin_bytes)
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn run_clipboard_py_script_output(name: &str, stdin_bytes: &[u8]) -> Result<std::process::Output> {
    let script = clipboard_py_script(name).context("clipboard py script missing")?;
    use std::io::Write;
    use std::process::{Command, Stdio};

    let mut child = Command::new("timeout")
        .args([
            &CLIPBOARD_PY_TIMEOUT.as_secs().to_string(),
            "python3",
        ])
        .arg(&script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .context("spawn clipboard py")?;
    if !stdin_bytes.is_empty() {
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(stdin_bytes).context("clipboard py stdin")?;
        }
    }
    child.wait_with_output().context("clipboard py wait")
}

fn is_syncable_text(text: &str) -> bool {
    let t = text.trim();
    if t.len() < MIN_TEXT_SYNC_LEN {
        return false;
    }
    let b = text.as_bytes();
    // PNG/JPEG lu par erreur via xclip texte — ne jamais synchroniser comme text/plain.
    if b.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        return false;
    }
    if b.len() >= 3 && b[0] == 0xFF && b[1] == 0xD8 && b[2] == 0xFF {
        return false;
    }
    if b.iter().any(|&c| c == 0) {
        return false;
    }
    // Trop de non-texte → probablement binaire.
    let non_text = b
        .iter()
        .filter(|&&c| c < 0x09 || (c > 0x0d && c < 0x20) || c == 0x7f)
        .count();
    if !b.is_empty() && non_text * 10 > b.len() {
        return false;
    }
    true
}

fn image_payload_from_bytes(bytes: &[u8]) -> Result<ClipboardPayload> {
    if bytes.len() > MAX_IMAGE_BYTES {
        anyhow::bail!("image too large ({} bytes)", bytes.len());
    }
    // Fast path: keep native PNG/JPEG in clipboard without re-encoding (local-first).
    if bytes.len() >= 4 && bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        return Ok(ClipboardPayload {
            mime: "image/png".into(),
            wire_data: B64.encode(bytes),
            hash: hash_bytes(bytes),
        });
    }
    if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        return Ok(ClipboardPayload {
            mime: "image/jpeg".into(),
            wire_data: B64.encode(bytes),
            hash: hash_bytes(bytes),
        });
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
        stdin.shutdown().await.ok();
        drop(stdin);
    }
    detach_selection_owner(child);
    Ok(())
}

/// Laisse `xclip` vivre en arrière-plan après l'écriture.
///
/// Sous X11 la sélection n'est pas un stockage central : le processus qui l'écrit
/// en reste **propriétaire** et sert le contenu aux applications qui collent. Si on
/// l'attend avec `wait()`, il rend la sélection en mourant et le contenu disparaît —
/// le copier fonctionne, le coller ne trouve plus rien. On le détache donc, et on
/// se contente de le moissonner pour ne pas laisser de zombies : le prochain
/// propriétaire de la sélection (nouvelle copie locale ou distante) le terminera.
fn detach_selection_owner(mut child: tokio::process::Child) {
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
}

async fn read_selection_bytes(selection: &str, mime: &str) -> Result<Vec<u8>> {
    let limit = if mime.starts_with("image/") {
        XCLIP_IMAGE_READ_TIMEOUT
    } else {
        XCLIP_READ_TIMEOUT
    };
    let output = xclip_read_timeout(
        &["-selection", selection, "-t", mime, "-o"],
        limit,
    )
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
        stdin.shutdown().await.ok();
        drop(stdin);
    }
    detach_selection_owner(child);
    Ok(())
}
