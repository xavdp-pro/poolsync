use crate::clipboard::{
    align_hash_after_write, clipboard_targets, read_clipboard_payload, read_selection_text,
    targets_have_image, try_send_payload, write_clipboard, write_selection_text,
};
use crate::notify_thumb::notify_thumbnail_path;
use crate::rdp_detect::rdp_client_active;
use crate::state::{clip_preview_mime, AgentState};
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use poolsync_core::{decode_message, encode_message, AgentMode, Message};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::{
    process::Command,
    sync::mpsc,
    time::{sleep, Duration},
};
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};
use tracing::{info, warn};

pub async fn run_agent(state: Arc<AgentState>) -> Result<()> {
    let cfg = &state.config;
    let hub_url = format!(
        "{}?token={}",
        cfg.hub_url.trim_end_matches('/'),
        cfg.token
    );

    state.set_connected(false);
    let (ws, _) = connect_async(&hub_url)
        .await
        .context("connect hub websocket")?;
    let (mut write, mut read) = ws.split();

    write
        .send(WsMessage::Text(
            encode_message(&Message::Hello {
                node: cfg.node.clone(),
                mode: cfg.mode,
                screen: cfg.screen.clone(),
                neighbors: cfg.neighbors.clone(),
            })?
            .into(),
        ))
        .await?;

    state.set_connected(true);
    info!("connected to hub");

    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();
    let last_clip_hash = Arc::new(Mutex::new(String::new()));

    let state_bg = state.clone();
    let out_tx_bg = out_tx.clone();
    let last_clip_hash_bg = last_clip_hash.clone();
    let clip_task = tokio::spawn(async move {
        clipboard_poll_loop(&state_bg, out_tx_bg, last_clip_hash_bg).await;
    });

    let state_in = state.clone();
    let out_tx_in = out_tx.clone();
    let input_task = tokio::spawn(async move {
        if state_in.config.mode == AgentMode::Full {
            input_poll_loop(&state_in, out_tx_in).await;
        }
    });

    loop {
        tokio::select! {
            maybe_out = out_rx.recv() => {
                if let Some(payload) = maybe_out {
                    write.send(WsMessage::Text(payload.into())).await?;
                } else {
                    break;
                }
            }
            maybe_in = read.next() => {
                match maybe_in {
                    Some(Ok(WsMessage::Text(text))) => {
                        handle_incoming(&state, &text, &last_clip_hash).await?;
                    }
                    Some(Ok(WsMessage::Ping(payload))) => {
                        write.send(WsMessage::Pong(payload)).await?;
                    }
                    Some(Ok(WsMessage::Close(_))) | None => break,
                    Some(Err(err)) => return Err(err.into()),
                    _ => {}
                }
            }
        }
    }

    state.set_connected(false);
    clip_task.abort();
    input_task.abort();
    Ok(())
}

async fn handle_incoming(
    state: &AgentState,
    text: &str,
    last_clip_hash: &Mutex<String>,
) -> Result<()> {
    let msg = decode_message(text)?;
    match msg {
        Message::Clipboard {
            hash, data, mime, ..
        } => {
            if !state.clipboard_sync_enabled() {
                return Ok(());
            }
            {
                let mut last = last_clip_hash
                    .lock()
                    .map_err(|_| anyhow::anyhow!("clip hash lock"))?;
                if *last == hash {
                    return Ok(());
                }
                *last = hash.clone();
            }

            write_clipboard(&data, &mime).await?;
            align_hash_after_write(last_clip_hash).await;
            state.mark_hub_clipboard_applied();
            let preview = clip_preview_mime(&mime, &data);
            state.record_clip_received(preview.clone());
            info!("clipboard synced ({mime}, {} bytes wire)", data.len());

            if state.should_notify(&hash, &preview) {
                let preview = preview.clone();
                let mime = mime.clone();
                let data = data.clone();
                tokio::spawn(async move {
                    show_clip_notification(&preview, &mime, &data).await;
                });
            }
        }
        Message::MasterChanged { node } => {
            state.set_master(&node);
            info!("master is now {node}");
        }
        Message::Input { kind, .. } if state.config.mode == AgentMode::Full => {
            inject_input(&kind).await?;
        }
        Message::SwitchTo { x, y, .. } if state.config.mode == AgentMode::Full => {
            xdotool(&["mousemove", &x.to_string(), &y.to_string()]).await?;
            state.set_master(&state.config.node);
        }
        _ => {}
    }
    Ok(())
}

async fn show_clip_notification(preview: &str, mime: &str, wire_data: &str) {
    let body = if preview.is_empty() {
        "Nouveau contenu dans le presse-papiers".to_string()
    } else {
        preview.to_string()
    };
    let icon = if mime.starts_with("image/") {
        match notify_thumbnail_path(mime, wire_data) {
            Ok(path) => path,
            Err(err) => {
                warn!("miniature notification: {err:#}");
                notify_icon_path()
            }
        }
    } else {
        notify_icon_path()
    };
    let base_args = [
        "-a",
        "com.xavdp.poolsync",
        "-i",
        &icon,
        "-t",
        "5000",
        "-u",
        "normal",
        "PoolSync",
        &body,
    ];
    let with_replace = {
        let mut args = vec!["-r", "87001"];
        args.extend_from_slice(&base_args);
        args
    };
    if run_notify_send(&with_replace).await {
        info!("notification envoyée");
        return;
    }
    if run_notify_send(&base_args).await {
        info!("notification envoyée");
    } else {
        warn!("notify-send échoué");
    }
}

async fn run_notify_send(args: &[&str]) -> bool {
    let mut cmd = Command::new("notify-send");
    cmd.args(args).stdout(Stdio::null()).stderr(Stdio::piped());
    for key in [
        "DISPLAY",
        "DBUS_SESSION_BUS_ADDRESS",
        "XAUTHORITY",
        "XDG_RUNTIME_DIR",
        "XDG_CURRENT_DESKTOP",
    ] {
        if let Ok(val) = std::env::var(key) {
            cmd.env(key, val);
        }
    }
    match cmd.status().await {
        Ok(s) if s.success() => true,
        Ok(s) => {
            if let Some(code) = s.code() {
                warn!("notify-send exit {code}");
            }
            false
        }
        Err(err) => {
            warn!("notify-send: {err}");
            false
        }
    }
}

fn notify_icon_path() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    format!("{home}/.local/share/poolsync/poolsync-tray.png")
}

async fn clipboard_poll_loop(
    state: &AgentState,
    out_tx: mpsc::UnboundedSender<String>,
    last_clip_hash: Arc<Mutex<String>>,
) {
    let poll = Duration::from_millis(state.config.clipboard_poll_ms);
    let primary_stable = Duration::from_millis(2500);
    let mut primary_pending: Option<(String, Instant)> = None;

    loop {
        if state.clipboard_sync_enabled() {
            let rdp_active =
                state.config.pause_clipboard_when_rdp && rdp_client_active().await;

            let clip_targets = clipboard_targets("clipboard").await.unwrap_or_default();
            let image_on_clipboard = targets_have_image(&clip_targets);

            // Sélection souris → presse-papiers, sauf image déjà au clipboard ou RDP actif.
            if state.primary_sync_enabled() && !image_on_clipboard && !rdp_active {
                match read_selection_text("primary").await {
                    Ok(text) if text.is_empty() => primary_pending = None,
                    Ok(text) => {
                        let now = Instant::now();
                        let stable = match &primary_pending {
                            Some((pending, since)) if pending == &text => {
                                now.duration_since(*since) >= primary_stable
                            }
                            _ => {
                                primary_pending = Some((text.clone(), now));
                                false
                            }
                        };
                        if stable {
                            if write_selection_text("clipboard", &text).await.is_ok() {
                                primary_pending = None;
                            }
                        }
                    }
                    Err(_) => primary_pending = None,
                }
            } else if image_on_clipboard {
                primary_pending = None;
            } else if !state.primary_sync_enabled() {
                primary_pending = None;
            }

            let skip_send = rdp_active && state.hub_apply_grace_active();
            if !skip_send {
                if let Ok(Some(payload)) = read_clipboard_payload().await {
                    if try_send_payload(&payload, &out_tx, &last_clip_hash) {
                        if payload.mime.starts_with("image/") {
                            let approx_bytes = payload.wire_data.len().saturating_mul(3) / 4;
                            info!(
                                "clipboard image sent ({}, ~{approx_bytes} bytes)",
                                payload.mime
                            );
                        }
                        primary_pending = None;
                    }
                }
            }
        } else {
            primary_pending = None;
        }
        sleep(poll).await;
    }
}

async fn input_poll_loop(state: &AgentState, out_tx: mpsc::UnboundedSender<String>) {
    let poll = Duration::from_millis(state.config.input_poll_ms);
    let mut last_pos = (0i32, 0i32);
    loop {
        if let Ok((x, y)) = get_mouse_location().await {
            if (x, y) != last_pos {
                last_pos = (x, y);
                if let Ok(payload) = encode_message(&Message::MasterClaim {
                    node: state.config.node.clone(),
                    ts: 0,
                }) {
                    let _ = out_tx.send(payload);
                }
            }
        }
        sleep(poll).await;
    }
}

async fn get_mouse_location() -> Result<(i32, i32)> {
    let output = Command::new("xdotool")
        .args(["getmouselocation", "--shell"])
        .output()
        .await?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut x = 0;
    let mut y = 0;
    for line in text.lines() {
        if let Some(val) = line.strip_prefix("X=") {
            x = val.parse().unwrap_or(0);
        }
        if let Some(val) = line.strip_prefix("Y=") {
            y = val.parse().unwrap_or(0);
        }
    }
    Ok((x, y))
}

async fn inject_input(kind: &poolsync_core::InputKind) -> Result<()> {
    match kind {
        poolsync_core::InputKind::MouseMove { x, y } => {
            xdotool(&["mousemove", &x.to_string(), &y.to_string()]).await?;
        }
        poolsync_core::InputKind::MouseButton {
            button,
            pressed,
            x,
            y,
        } => {
            xdotool(&["mousemove", &x.to_string(), &y.to_string()]).await?;
            let action = if *pressed { "mousedown" } else { "mouseup" };
            xdotool(&[action, &button.to_string()]).await?;
        }
        poolsync_core::InputKind::Key { keycode, pressed } => {
            let action = if *pressed { "keydown" } else { "keyup" };
            xdotool(&[action, &keycode.to_string()]).await?;
        }
        poolsync_core::InputKind::MouseWheel { delta, x, y } => {
            xdotool(&["mousemove", &x.to_string(), &y.to_string()]).await?;
            let button = if *delta > 0 { "4" } else { "5" };
            xdotool(&["click", button]).await?;
        }
    }
    Ok(())
}

async fn xdotool(args: &[&str]) -> Result<()> {
    let status = Command::new("xdotool").args(args).status().await?;
    if !status.success() {
        anyhow::bail!("xdotool {:?} failed", args);
    }
    Ok(())
}
