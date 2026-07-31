//! Application d'un collage entrant (hub ou peer mesh).

use crate::clip_cache;
use crate::clipboard::{
    align_hash_after_write, should_reject_remote_text, write_clipboard,
};
use crate::state::{clip_preview_mime, AgentState};
use tracing::info;

/// Applique un collage reçu (hub ou voisin direct).
pub async fn apply_incoming_clipboard(
    state: &AgentState,
    hash: &str,
    data: &str,
    mime: &str,
    source_node: &str,
    from_hub: bool,
) -> anyhow::Result<()> {
    if !state.clipboard_sync_enabled() || !state.local_poolsync_active() {
        return Ok(());
    }
    if mime == "text/plain" && should_reject_remote_text().await {
        tracing::info!("ignore remote text — image locale récente (grace)");
        return Ok(());
    }
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

    write_clipboard(data, mime)
        .await
        .map_err(|e| {
            tracing::warn!("clipboard write failed ({mime} from {source_node}): {e:#}");
            e
        })?;
    // Ne pas relire l'image : xclip/GTK peut changer les octets → hash différent → reboucle.
    if mime == "text/plain" {
        align_hash_after_write(&last_clip_hash).await;
    } else if mime.starts_with("image/") {
        // Absorber le hash réellement présent après écriture (évite re-relay).
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        if let Ok(Some(local)) = crate::clipboard::read_clipboard_payload().await {
            if local.mime.starts_with("image/") {
                if let Ok(mut last) = last_clip_hash.lock() {
                    *last = local.hash;
                }
            }
        }
    }
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
