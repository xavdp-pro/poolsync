//! Application d'un collage entrant (hub ou peer mesh).

use crate::clip_cache;
use crate::clipboard::{
    clipboard_targets, local_write_text, targets_have_pasteable_image, write_clipboard,
};
use crate::state::{clip_preview_mime, AgentState};
use tracing::info;

fn applied_text_hash(data: &str) -> String {
    poolsync_core::hash_text(data)
}

/// Applique un collage reçu (hub ou voisin direct).
pub async fn apply_incoming_clipboard(
    state: &AgentState,
    hash: &str,
    data: &str,
    mime: &str,
    source_node: &str,
    from_hub: bool,
) -> anyhow::Result<()> {
    if mime.starts_with("image/") {
        let via = if from_hub { "hub" } else { "peer" };
        info!(
            "image-trace RECEIVE id={} source={} via={} mime={} wire_bytes={}",
            crate::clipboard::trace_id(hash),
            source_node,
            via,
            mime,
            data.len()
        );
    }
    if !state.clipboard_sync_enabled() || !state.local_poolsync_active() {
        tracing::info!(
            "ignore incoming clipboard ({source_node}): sync={} local_active={}",
            state.clipboard_sync_enabled(),
            state.local_poolsync_active()
        );
        return Ok(());
    }
    // A message can take a few hops through the mesh.  Do not let an older
    // remote text overwrite the user's just-copied local text while it is
    // still being broadcast from this machine.
    if !mime.starts_with("image/") && state.local_clipboard_priority_active() {
        tracing::info!(
            "ignore stale incoming text during local-copy priority ({source_node})"
        );
        return Ok(());
    }
    // Local image grace only filters *poll echo* of leftover UTF8 on this
    // display. Remote text from another node must always land so Ctrl+V works.
    // Hub + peer peuvent livrer la même image avec des hash différents — ignorer le doublon.
    if mime.starts_with("image/") && state.incoming_duplicate_suppress_active() {
        tracing::debug!("ignore incoming image duplicate during grace ({source_node})");
        return Ok(());
    }

    let last_clip_hash = state.last_clip_hash_handle();
    {
        let mut last = last_clip_hash
            .lock()
            .map_err(|_| anyhow::anyhow!("clip hash lock"))?;
        if *last == hash {
            return Ok(());
        }
        *last = hash.to_string();
    }

    let (write_data, write_mime) = if mime.starts_with("image/") {
        crate::clipboard::seed_primary_baseline().await;
        (data.to_string(), mime.to_string())
    } else {
        local_write_text(data, mime, state.keep_formatting())
    };
    let context = format!("incoming-{source_node}");
    match write_clipboard(&write_data, &write_mime).await {
        Ok(()) => {
            if write_mime.starts_with("image/") {
                info!(
                    "image-trace APPLY id={} source={} mime={}",
                    crate::clipboard::trace_id(hash),
                    source_node,
                    write_mime
                );
                // GTK applies the queued offer on its main loop. Let it settle
                // before checking TARGETS, otherwise diagnostics report empty.
                tokio::time::sleep(std::time::Duration::from_millis(40)).await;
                // xrdp-chansrv can take CLIPBOARD immediately after a PNG
                // offer, then expose only text targets. Re-offer while the
                // same image claim is still active; a later text copy clears
                // that claim, so it is never overwritten by an old image.
                tokio::spawn(async {
                    for delay_ms in [200_u64, 700, 1_500] {
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
                        let (claim_active, cached_png_bytes) =
                            crate::clipboard_gtk::image_claim_debug_state();
                        if !claim_active {
                            tracing::info!(
                                "clipboard incoming image: no active claim after {delay_ms}ms (cached_png={cached_png_bytes})"
                            );
                            break;
                        }
                        let targets = clipboard_targets("clipboard").await.unwrap_or_default();
                        let reoffered = !targets_have_pasteable_image(&targets)
                            && crate::clipboard_gtk::reoffer_last_image();
                        tracing::info!(
                            "clipboard incoming image: xrdp check after {delay_ms}ms claim={} cached_png={} targets={} reoffered={}",
                            claim_active,
                            cached_png_bytes,
                            targets.join(","),
                            reoffered
                        );
                        if reoffered {
                            tracing::info!(
                                "clipboard incoming image: PNG reoffer after xrdp takeover"
                            );
                        }
                    }
                });
            }
            crate::clipboard_diag::log_post_write(&write_mime, &context, true).await;
        }
        Err(e) => {
            crate::clipboard_diag::log_post_write(&write_mime, &context, false).await;
            tracing::warn!("clipboard write failed ({write_mime} from {source_node}): {e:#}");
            return Err(e);
        }
    }
    if write_mime == "text/plain" || write_mime == "text/html" {
        crate::clipboard::record_incoming_applied(&write_data);
        // `write_clipboard` puts text on the GTK main loop. Reading it back
        // immediately races that loop and used to store the *previous* hash,
        // causing every peer to re-broadcast old text. Hash the exact text we
        // wrote instead; this is also correct when incoming HTML was flattened.
        if let Ok(mut last) = last_clip_hash.lock() {
            *last = applied_text_hash(&write_data);
        }
    }
    // Do not xclip-read an image we just offered: we own CLIPBOARD on the
    // GTK thread; a same-process xclip -o deadlocks and leaves image/bmp empty.
    // Ne pas mark_image_clipboard_epoch ici : ça bloquerait le texte distant
    // pendant 4s après chaque image reçue (copier-coller texte mort après image).
    state.mark_incoming_clipboard_applied(mime);

    let preview = clip_preview_mime(mime, data);
    state.record_clip_received(preview.clone());
    clip_cache::store_received(hash, mime, data, &preview, source_node);

    let via = if from_hub { "hub" } else { "peer" };
    info!("clipboard synced via {via} ({mime}, {} bytes wire)", data.len());

    if state.should_notify(hash, &preview) {
        let preview = preview.clone();
        let mime = mime.to_string();
        let data = data.to_string();
        tokio::spawn(async move {
            crate::agent::show_clip_notification("PoolSync — Reçu", &preview, &mime, &data).await;
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn applied_text_hash_tracks_the_transformed_text_not_the_wire_html() {
        let wire_html = "<p>Hello <b>PoolSync</b></p>";
        let (written, mime) = local_write_text(wire_html, "text/html", false);
        assert_eq!(mime, "text/plain");
        assert_eq!(written, "Hello PoolSync");
        assert_eq!(applied_text_hash(&written), poolsync_core::hash_text("Hello PoolSync"));
        assert_ne!(applied_text_hash(&written), poolsync_core::hash_text(wire_html));
    }
}
