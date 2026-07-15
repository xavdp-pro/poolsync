use crate::clipboard::{
    align_hash_after_write, read_clipboard_payload, try_send_payload, write_clipboard,
};
use crate::kvm::{detect_screen, inject_input, kvm_poll_loop};
use crate::kvm_x11;
use crate::network::{hub_tcp_endpoint, hub_tcp_reachable, wait_for_hub};
use crate::notify_thumb::notify_thumbnail_path;
use crate::rdp_detect::rdp_client_active;
use crate::state::{clip_preview_mime, AgentState};
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use poolsync_core::{decode_message, encode_message, Message};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::{
    process::Command,
    sync::mpsc,
    time::{interval, sleep, timeout, Duration},
};
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};
use tracing::{info, warn};

const WS_CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
const HUB_LINK_CHECK: Duration = Duration::from_secs(10);

pub async fn run_agent(state: Arc<AgentState>) -> Result<()> {
    let cfg = &state.config;
    let hub_url = format!(
        "{}?token={}",
        cfg.hub_url.trim_end_matches('/'),
        cfg.token
    );
    let (hub_host, hub_port) = hub_tcp_endpoint(&cfg.hub_url)?;

    state.set_connected(false);
    wait_for_hub(&hub_host, hub_port).await;

    let (ws, _) = timeout(WS_CONNECT_TIMEOUT, connect_async(&hub_url))
        .await
        .context("délai connexion hub dépassé (VPN/réseau?)")?
        .context("connect hub websocket")?;
    let (mut write, mut read) = ws.split();

    let screen = detect_screen().await.unwrap_or_else(|| cfg.screen.clone());
    if screen.width != cfg.screen.width || screen.height != cfg.screen.height {
        info!(
            "écran détecté {}x{} (config {}x{})",
            screen.width, screen.height, cfg.screen.width, cfg.screen.height
        );
    }

    write
        .send(WsMessage::Text(
            encode_message(&Message::Hello {
                node: cfg.node.clone(),
                mode: cfg.mode,
                screen: screen.clone(),
                neighbors: cfg.neighbors.clone(),
                kvm_enabled: state.kvm_enabled(),
            })?
            .into(),
        ))
        .await?;

    state.set_connected(true);
    state.set_kvm_focus(&cfg.node);
    state.set_kvm_input_node(&cfg.node);
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
    let kvm_task = if cfg.kvm_active() {
        tokio::task::spawn_blocking(move || kvm_poll_loop(&state_in, out_tx_in))
    } else {
        tokio::task::spawn_blocking(|| std::thread::park())
    };

    let mut link_check = interval(HUB_LINK_CHECK);

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
            _ = link_check.tick() => {
                if !hub_tcp_reachable(&hub_host, hub_port).await {
                    warn!("liaison hub perdue ({hub_host}:{hub_port}) — reconnexion");
                    break;
                }
            }
        }
    }

    state.set_connected(false);
    clip_task.abort();
    if cfg.kvm_active() {
        kvm_task.abort();
    }
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
            // Ne pas modifier kvm_input_node / focus ici : sinon la machine
            // distante « vole » le clavier et les bords ne répondent plus localement.
            info!("primary KVM → {node}");
        }
        Message::TopologyUpdate { topology } => {
            state.set_topology(topology);
            info!("topology updated from hub");
        }
        Message::Input { kind, .. } if state.kvm_enabled() => {
            state.mark_kvm_inject();
            inject_input(&kind).await?;
        }
        Message::SwitchTo {
            node,
            x,
            y,
            input_node,
        } if state.kvm_enabled() => {
            state.set_kvm_focus(&node);
            let owner = if input_node.is_empty() {
                state.config.node.clone()
            } else {
                input_node.clone()
            };
            state.set_kvm_input_node(&owner);
            if node == state.config.node {
                state.mark_kvm_inject();
                let x = x;
                let y = y;
                tokio::task::spawn_blocking(move || kvm_x11::move_mouse_absolute(x, y))
                    .await??;
                info!("KVM cursor enter → {} ({x},{y})", state.config.node);
            }
            if !input_node.is_empty() {
                state.set_master(&input_node);
            }
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

    loop {
        if state.clipboard_sync_enabled() {
            let rdp_active =
                state.config.pause_clipboard_when_rdp && rdp_client_active().await;

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
                    }
                }
            }
        }
        sleep(poll).await;
    }
}