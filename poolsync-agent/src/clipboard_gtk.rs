//! GTK clipboard owner (systray thread).
//!
//! Images advertise image/png plus a real image/bmp (not GTK set_image).

use gtk::gdk;
use gtk::{Clipboard, TargetEntry, TargetFlags};
use poolsync_core::hash_bytes;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const INFO_PNG: u32 = 1;
const INFO_TEXT: u32 = 2;
const INFO_BMP: u32 = 3;
/// xrdp-chansrv often strips PNG within seconds — keep re-offering for local paste.
pub const IMAGE_CLAIM_KEEPALIVE: Duration = Duration::from_secs(45);

#[derive(Clone)]
pub enum ClipboardOffer {
    Text(String),
    Rich { plain: String, html: String },
    Image { mime: String, bytes: Vec<u8> },
    /// Drop GTK ownership so the native X11 clipboard can work (PoolSync sync OFF).
    Release,
}

static GTK_TX: OnceLock<glib::Sender<ClipboardOffer>> = OnceLock::new();
static LAST_PNG: Mutex<Option<Vec<u8>>> = Mutex::new(None);
static LAST_REOFFER: Mutex<Option<Instant>> = Mutex::new(None);
static LAST_IMAGE_CLAIM_AT: Mutex<Option<Instant>> = Mutex::new(None);
static IMAGE_OWNER: AtomicU32 = AtomicU32::new(0);
/// Fenêtre X11 propriétaire de CLIPBOARD après *notre* offre de texte.
/// Symétrique de `IMAGE_OWNER` : sans elle, l'agent relit son propre texte.
static TEXT_OWNER: AtomicU32 = AtomicU32::new(0);
/// Dernière fois qu'une application nous a *demandé* le contenu de la
/// sélection. C'est le seul signal fiable qu'un collage est en cours : X11 ne
/// dit pas « je colle », mais il vient chercher la donnée chez le propriétaire.
static LAST_SERVE_AT: Mutex<Option<Instant>> = Mutex::new(None);
/// Nombre de lectures que l'agent fait lui-même en ce moment (xclip interne).
/// Nos propres lectures passent par le même rappel GTK que celles des autres
/// applications : sans ce compteur, l'agent se prend pour un collage en cours
/// et diffère ses écritures pour rien.
static INTERNAL_READS: AtomicU32 = AtomicU32::new(0);

pub fn current_clipboard_owner() -> u32 {
    use x11rb::protocol::xproto::ConnectionExt;
    let Ok((conn, _)) = x11rb::connect(None) else {
        return 0;
    };
    let Ok(atom_cookie) = conn.intern_atom(false, b"CLIPBOARD") else {
        return 0;
    };
    let Ok(atom) = atom_cookie.reply() else {
        return 0;
    };
    let Ok(owner_cookie) = conn.get_selection_owner(atom.atom) else {
        return 0;
    };
    owner_cookie.reply().map(|r| r.owner).unwrap_or(0)
}

/// True only while the X11 selection is still the image offered by our GTK
/// clipboard. Unlike the 45s keepalive timer, this remains exact indefinitely.
/// Appelé depuis les rappels GTK quand un client vient lire notre sélection.
fn note_selection_served() {
    if INTERNAL_READS.load(Ordering::SeqCst) > 0 {
        return; // c'est nous qui lisons : ce n'est pas un collage utilisateur
    }
    if let Ok(mut g) = LAST_SERVE_AT.lock() {
        *g = Some(Instant::now());
    }
}

/// Garde RAII : marque la durée d'une lecture faite par l'agent lui-même.
pub struct InternalRead;

impl InternalRead {
    pub fn begin() -> Self {
        INTERNAL_READS.fetch_add(1, Ordering::SeqCst);
        Self
    }
}

impl Drop for InternalRead {
    fn drop(&mut self) {
        INTERNAL_READS.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Une application a-t-elle lu notre sélection dans les `window` dernières ms ?
///
/// Chromium — donc VSCode, Cursor, Slack — fait cette lecture de façon
/// synchrone : lui retirer la sélection en plein transfert laisse sa fenêtre
/// bloquée (« CodeWindow unresponsive »). Tant que la réponse est fraîche, un
/// collage est probablement en cours et il ne faut pas toucher à la sélection.
pub fn selection_served_recently(window: Duration) -> bool {
    LAST_SERVE_AT
        .lock()
        .ok()
        .and_then(|g| *g)
        .is_some_and(|at| at.elapsed() < window)
}

pub fn owns_image_clipboard() -> bool {
    let expected = IMAGE_OWNER.load(Ordering::SeqCst);
    expected != 0 && current_clipboard_owner() == expected
}

/// Le texte actuellement dans CLIPBOARD est-il notre propre offre GTK ?
///
/// Notre propriétaire GTK annonce UTF8_STRING/STRING mais ne répond pas
/// toujours aux demandes de conversion venant de `xclip` : la lecture échoue
/// alors sur toutes les cibles texte, et seules les métadonnées (TARGETS,
/// TIMESTAMP) répondent encore. C'est ainsi que la sortie de nos propres
/// sondes s'est retrouvée diffusée dans tout le pool. On ne relit donc jamais
/// notre propre offre : on sait déjà ce qu'elle contient.
pub fn owns_text_clipboard() -> bool {
    let expected = TEXT_OWNER.load(Ordering::SeqCst);
    expected != 0 && current_clipboard_owner() == expected
}

pub fn mark_image_claim() {
    if let Ok(mut t) = LAST_IMAGE_CLAIM_AT.lock() {
        *t = Some(Instant::now());
    }
}

pub fn clear_image_claim() {
    if let Ok(mut t) = LAST_IMAGE_CLAIM_AT.lock() {
        *t = None;
    }
}

/// Un vrai texte vient de remplacer l'image : aucun keepalive XRDP ne doit
/// pouvoir réoffrir l'ancien PNG après l'écriture texte.
pub fn discard_last_image() {
    IMAGE_OWNER.store(0, Ordering::SeqCst);
    if let Ok(mut last) = LAST_PNG.lock() {
        *last = None;
    }
    if let Ok(mut last) = LAST_REOFFER.lock() {
        *last = None;
    }
    clear_image_claim();
}

pub fn recent_image_claim_active() -> bool {
    LAST_IMAGE_CLAIM_AT
        .lock()
        .ok()
        .and_then(|g| *g)
        .is_some_and(|t| t.elapsed() < IMAGE_CLAIM_KEEPALIVE)
}

pub fn image_claim_debug_state() -> (bool, usize) {
    let active = recent_image_claim_active();
    let bytes = LAST_PNG
        .lock()
        .ok()
        .and_then(|last| last.as_ref().map(Vec::len))
        .unwrap_or(0);
    (active, bytes)
}

#[cfg(test)]
mod internal_read_tests {
    use super::*;

    /// Ces tests manipulent des états globaux ; les sérialiser évite qu'ils se
    /// marchent dessus quand cargo les exécute en parallèle.
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Nos propres lectures xclip passent par le même rappel GTK que celles des
    /// applications. Sans le compteur, l'agent se prenait lui-même pour un
    /// collage en cours et différait ses écritures pour rien (observé le 29/08 :
    /// « écriture après 909 ms d'attente — lectures continues »).
    #[test]
    fn our_own_reads_are_ignored_but_a_real_application_read_is_seen() {
        let _serial = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        *LAST_SERVE_AT.lock().unwrap() = None;

        {
            let _internal = InternalRead::begin();
            note_selection_served();
        }
        assert!(
            !selection_served_recently(Duration::from_secs(5)),
            "une lecture interne ne doit pas ressembler à un collage"
        );

        note_selection_served();
        assert!(
            selection_served_recently(Duration::from_secs(5)),
            "une vraie lecture applicative doit rester détectée"
        );
        *LAST_SERVE_AT.lock().unwrap() = None;
    }

    #[test]
    fn nested_internal_reads_restore_the_counter() {
        let _serial = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(INTERNAL_READS.load(Ordering::SeqCst), 0);
        {
            let _outer = InternalRead::begin();
            {
                let _inner = InternalRead::begin();
                assert_eq!(INTERNAL_READS.load(Ordering::SeqCst), 2);
            }
            assert_eq!(INTERNAL_READS.load(Ordering::SeqCst), 1);
        }
        assert_eq!(INTERNAL_READS.load(Ordering::SeqCst), 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD as B64, Engine};

    #[test]
    fn text_discards_stale_image_keepalive() {
        *LAST_PNG.lock().unwrap() = Some(vec![1, 2, 3]);
        mark_image_claim();
        discard_last_image();
        assert!(LAST_PNG.lock().unwrap().is_none());
        assert!(!recent_image_claim_active());
    }

    #[test]
    fn valid_png_has_a_real_bmp_fallback() {
        let png = B64
            .decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=")
            .unwrap();
        let bmp = png_to_bmp(&png).expect("a valid PNG must encode as BMP");
        assert!(bmp.starts_with(b"BM"));
        assert!(bmp.len() > 32);
    }
}

/// À appeler sur le thread GTK après `gtk::init()`.
pub fn attach_gtk_handler() {
    #[allow(deprecated)]
    let (tx, rx) = glib::MainContext::channel(glib::Priority::DEFAULT);
    let _ = GTK_TX.set(tx);
    rx.attach(None, |offer| {
        apply_offer(offer);
        glib::ControlFlow::Continue
    });
}

pub fn try_offer(offer: ClipboardOffer) -> bool {
    GTK_TX.get().and_then(|tx| tx.send(offer).ok()).is_some()
}

/// Apply immediately when the caller is already running on the GTK main
/// thread (the tray/history menu).  Queuing from that same thread lets the
/// click handler report success before X11 ownership actually changes.
pub fn offer_now_from_gtk(offer: ClipboardOffer) {
    apply_offer(offer);
}

/// xrdp-chansrv often replaces a PNG offer with empty image/bmp. Put PNG back.
pub fn reoffer_last_image() -> bool {
    let min_ms = if recent_image_claim_active() { 200 } else { 400 };
    let too_soon = LAST_REOFFER
        .lock()
        .ok()
        .and_then(|g| *g)
        .is_some_and(|t| t.elapsed().as_millis() < min_ms);
    if too_soon {
        return false;
    }
    let png = LAST_PNG.lock().ok().and_then(|g| g.clone());
    let Some(png) = png else {
        return false;
    };
    if let Ok(mut t) = LAST_REOFFER.lock() {
        *t = Some(Instant::now());
    }
    try_offer(ClipboardOffer::Image {
        mime: "image/png".into(),
        bytes: png,
    })
}

fn apply_offer(offer: ClipboardOffer) {
    let Some(display) = gdk::Display::default() else {
        tracing::warn!("gtk clipboard: no display");
        return;
    };
    let clip = Clipboard::default(&display)
        .unwrap_or_else(|| Clipboard::get(&gdk::SELECTION_CLIPBOARD));
    let primary = Clipboard::get(&gdk::SELECTION_PRIMARY);
    match offer {
        ClipboardOffer::Text(text) => {
            IMAGE_OWNER.store(0, Ordering::SeqCst);
            if let Ok(mut last) = LAST_PNG.lock() {
                *last = None;
            }
            clear_image_claim();
            // xrdp / Chrome Ctrl+V: CLIPBOARD without UTF8 falls back to PRIMARY
            // (stale URL). Mirror text on both selections.
            let ok_clip = set_text_only(&clip, text.clone());
            let _ = set_text_only(&primary, text);
            TEXT_OWNER.store(
                if ok_clip { current_clipboard_owner() } else { 0 },
                Ordering::SeqCst,
            );
            if !ok_clip {
                tracing::warn!("gtk clipboard set_with_data failed — text not owned by agent");
            }
        }
        ClipboardOffer::Rich { plain, html } => {
            IMAGE_OWNER.store(0, Ordering::SeqCst);
            if let Ok(mut last) = LAST_PNG.lock() {
                *last = None;
            }
            clear_image_claim();
            let ok_html = set_text_and_html(&clip, plain.clone(), html);
            TEXT_OWNER.store(
                if ok_html { current_clipboard_owner() } else { 0 },
                Ordering::SeqCst,
            );
            if !ok_html {
                tracing::warn!("gtk clipboard set_with_data failed — html not owned by agent");
            }
            let _ = set_text_only(&primary, plain);
        }
        ClipboardOffer::Image { mime, bytes } => {
            TEXT_OWNER.store(0, Ordering::SeqCst);
            // Ctrl+V falls back to PRIMARY when CLIPBOARD has no UTF8 → old text.
            primary.clear();
            if !set_image_png_bmp(&clip, &mime, bytes) {
                tracing::warn!("gtk clipboard set_with_data failed — image not owned by agent");
            } else {
                IMAGE_OWNER.store(current_clipboard_owner(), Ordering::SeqCst);
            }
        }
        ClipboardOffer::Release => {
            TEXT_OWNER.store(0, Ordering::SeqCst);
            IMAGE_OWNER.store(0, Ordering::SeqCst);
            if let Ok(mut last) = LAST_PNG.lock() {
                *last = None;
            }
            clear_image_claim();
            clip.clear();
        }
    }
}

/// Stop owning CLIPBOARD so Ctrl+C/Ctrl+V of the desktop session work again.
pub fn release_ownership() -> bool {
    try_offer(ClipboardOffer::Release)
}

fn set_text_and_html(clip: &Clipboard, plain: String, html: String) -> bool {
    const INFO_HTML: u32 = 3;
    let targets = [
        TargetEntry::new("text/html", TargetFlags::empty(), INFO_HTML),
        TargetEntry::new("UTF8_STRING", TargetFlags::empty(), INFO_TEXT),
        TargetEntry::new("STRING", TargetFlags::empty(), INFO_TEXT),
        TargetEntry::new("TEXT", TargetFlags::empty(), INFO_TEXT),
        TargetEntry::new("text/plain", TargetFlags::empty(), INFO_TEXT),
        TargetEntry::new("text/plain;charset=utf-8", TargetFlags::empty(), INFO_TEXT),
    ];
    clip.set_with_data(&targets, move |_cb, selection, info| {
        note_selection_served();
        if info == INFO_HTML {
            selection.set(&gdk::Atom::intern("text/html"), 8, html.as_bytes());
        } else if info == INFO_TEXT {
            selection.set_text(&plain);
        }
    })
}

fn set_text_only(clip: &Clipboard, text: String) -> bool {
    let targets = [
        TargetEntry::new("UTF8_STRING", TargetFlags::empty(), INFO_TEXT),
        TargetEntry::new("STRING", TargetFlags::empty(), INFO_TEXT),
        TargetEntry::new("TEXT", TargetFlags::empty(), INFO_TEXT),
        TargetEntry::new("text/plain", TargetFlags::empty(), INFO_TEXT),
        TargetEntry::new("text/plain;charset=utf-8", TargetFlags::empty(), INFO_TEXT),
    ];
    clip.set_with_data(&targets, move |_cb, selection, info| {
        note_selection_served();
        if info == INFO_TEXT {
            selection.set_text(&text);
        }
    })
}

fn set_image_png_bmp(clip: &Clipboard, mime: &str, bytes: Vec<u8>) -> bool {
    let png = ensure_png(mime, &bytes);
    // GDK's image clipboard consumer (including several Electron widgets)
    // asks for a bitmap target first.  Give it a *real* BMP: advertising an
    // empty BMP is what caused xrdp-chansrv / Chrome paste failures before.
    let bmp = png_to_bmp(&png);
    let hash = hash_bytes(&png);
    let trace = hash.get(..12.min(hash.len())).unwrap_or(&hash).to_string();
    if let Ok(mut last) = LAST_PNG.lock() {
        *last = Some(png.clone());
    }
    mark_image_claim();
    tracing::info!(
        "image-trace OFFER id={} mime=image/png bytes={} bmp_bytes={}",
        trace,
        png.len(),
        bmp.as_ref().map_or(0, Vec::len)
    );
    // Empty text targets: xrdp-chansrv asks TEXT/STRING before it asks the
    // bitmap target.  They must be advertised (but never contain PNG bytes),
    // otherwise xrdp logs "unknown target TEXT" in a loop and Electron never
    // reaches image/png or image/bmp.
    let mut targets = vec![
        TargetEntry::new("image/png", TargetFlags::empty(), INFO_PNG),
        TargetEntry::new("UTF8_STRING", TargetFlags::empty(), INFO_TEXT),
        TargetEntry::new("STRING", TargetFlags::empty(), INFO_TEXT),
        TargetEntry::new("TEXT", TargetFlags::empty(), INFO_TEXT),
        TargetEntry::new("text/plain", TargetFlags::empty(), INFO_TEXT),
        TargetEntry::new("text/plain;charset=utf-8", TargetFlags::empty(), INFO_TEXT),
    ];
    if bmp.is_some() {
        targets.extend([
            TargetEntry::new("image/bmp", TargetFlags::empty(), INFO_BMP),
            TargetEntry::new("image/x-bmp", TargetFlags::empty(), INFO_BMP),
        ]);
    }
    clip.set_with_data(&targets, move |_cb, selection, info| {
        note_selection_served();
        if info == INFO_PNG {
            tracing::info!(
                "image-trace SERVE id={} target=image/png bytes={}",
                trace,
                png.len()
            );
            selection.set(&selection.target(), 8, &png);
        } else if info == INFO_BMP {
            if let Some(bmp) = bmp.as_ref() {
                tracing::info!(
                    "image-trace SERVE id={} target={} bytes={}",
                    trace,
                    selection.target().name(),
                    bmp.len()
                );
                selection.set(&selection.target(), 8, bmp);
            }
        } else if info == INFO_TEXT {
            selection.set_text("");
        }
    })
}

fn ensure_png(_mime: &str, bytes: &[u8]) -> Vec<u8> {
    if bytes.len() >= 4 && bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        return bytes.to_vec();
    }
    encode_png(bytes).unwrap_or_else(|| bytes.to_vec())
}

fn encode_png(bytes: &[u8]) -> Option<Vec<u8>> {
    use image::codecs::png::PngEncoder;
    use image::{ExtendedColorType, ImageEncoder, ImageReader};
    use std::io::Cursor;
    let img = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    let rgba = img.to_rgba8();
    let mut out = Vec::new();
    PngEncoder::new(&mut out)
        .write_image(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
            ExtendedColorType::Rgba8,
        )
        .ok()?;
    Some(out)
}

fn png_to_bmp(png: &[u8]) -> Option<Vec<u8>> {
    use image::codecs::bmp::BmpEncoder;
    use image::{ExtendedColorType, ImageEncoder, ImageReader};
    use std::io::Cursor;

    let image = ImageReader::new(Cursor::new(png))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    let rgba = image.to_rgba8();
    let mut bmp = Vec::new();
    BmpEncoder::new(&mut bmp)
        .write_image(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
            ExtendedColorType::Rgba8,
        )
        .ok()?;
    Some(bmp)
}
