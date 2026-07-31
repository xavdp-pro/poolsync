//! Mesh clipboard direct entre voisins (LAN/VPN) — sans relay hub bs1.

use crate::clipboard_incoming::apply_incoming_clipboard;
use crate::state::AgentState;
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use poolsync_core::{decode_message, Message};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout, Duration};
use tokio_tungstenite::{
    connect_async, tungstenite::Message as WsMessage, WebSocketStream,
};
use tracing::{debug, info, warn};

const PEER_CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
const PEER_RECONNECT_INITIAL: Duration = Duration::from_secs(2);
const PEER_RECONNECT_MAX: Duration = Duration::from_secs(20);

/// Lance l'écoute + connexions sortantes ; retourne un canal pour diffuser le clipboard local.
pub fn spawn(state: Arc<AgentState>) -> Option<mpsc::UnboundedSender<String>> {
    if !state.config.peer_direct_clipboard {
        return None;
    }
    let has_peer = state
        .config
        .neighbors
        .iter()
        .any(|n| n.peer_url.is_some() || n.peer_url_vpn.is_some());
    if !has_peer {
        return None;
    }

    let (local_tx, mut local_rx) = mpsc::unbounded_channel::<String>();
    let (peer_reg_tx, mut peer_reg_rx) = mpsc::unbounded_channel::<PeerLink>();

    let state_listen = state.clone();
    let reg_listen = peer_reg_tx.clone();
    tokio::spawn(async move {
        if let Err(err) = run_listener(state_listen, reg_listen).await {
            warn!("peer listener: {err:#}");
        }
    });

    for neighbor in state.config.neighbors.clone() {
        let urls: Vec<String> = [neighbor.peer_url.clone(), neighbor.peer_url_vpn.clone()]
            .into_iter()
            .flatten()
            .collect();
        if urls.is_empty() {
            continue;
        }
        let state_out = state.clone();
        let node = neighbor.node.clone();
        let reg = peer_reg_tx.clone();
        tokio::spawn(async move {
            peer_outbound_loop(state_out, node, urls, reg).await;
        });
    }

    tokio::spawn(async move {
        let mut peers: HashMap<String, mpsc::UnboundedSender<String>> = HashMap::new();
        loop {
            tokio::select! {
                Some(payload) = local_rx.recv() => {
                    for tx in peers.values() {
                        let _ = tx.send(payload.clone());
                    }
                }
                Some(link) = peer_reg_rx.recv() => {
                    info!("peer mesh connecté: {}", link.node);
                    peers.insert(link.node, link.tx);
                }
            }
        }
    });

    Some(local_tx)
}

struct PeerLink {
    node: String,
    tx: mpsc::UnboundedSender<String>,
}

async fn run_listener(
    state: Arc<AgentState>,
    reg: mpsc::UnboundedSender<PeerLink>,
) -> Result<()> {
    let port = state.config.peer_listen_port;
    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&addr)
        .await
        .with_context(|| format!("bind peer listen {addr}"))?;
    info!("peer mesh écoute sur {addr}");

    loop {
        let (stream, peer_addr) = listener.accept().await.context("peer accept")?;
        let state_in = state.clone();
        let reg_in = reg.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_inbound(state_in, stream, peer_addr.to_string(), reg_in).await
            {
                debug!("peer inbound {peer_addr}: {err:#}");
            }
        });
    }
}

async fn handle_inbound(
    state: Arc<AgentState>,
    stream: TcpStream,
    peer_addr: String,
    reg: mpsc::UnboundedSender<PeerLink>,
) -> Result<()> {
    let ws = tokio_tungstenite::accept_async(stream)
        .await
        .context("peer ws accept")?;
    serve_peer_session(state, ws, None, peer_addr, reg).await
}

async fn peer_outbound_loop(
    state: Arc<AgentState>,
    neighbor: String,
    urls: Vec<String>,
    reg: mpsc::UnboundedSender<PeerLink>,
) {
    let mut backoff = PEER_RECONNECT_INITIAL;
    loop {
        let mut connected = false;
        for url in &urls {
            let full = peer_ws_url(url, &state.config.token, &state.config.node);
            match timeout_connect(&full).await {
                Ok(ws) => {
                    info!("peer mesh → {neighbor} via {url}");
                    if serve_peer_session(state.clone(), ws, Some(neighbor.clone()), url.clone(), reg.clone())
                        .await
                        .is_ok()
                    {
                        connected = true;
                        break;
                    }
                }
                Err(err) => {
                    debug!("peer connect {neighbor} {url}: {err:#}");
                }
            }
        }
        if connected {
            backoff = PEER_RECONNECT_INITIAL;
        } else {
            sleep(backoff).await;
            backoff = std::cmp::min(backoff * 2, PEER_RECONNECT_MAX);
        }
    }
}

async fn timeout_connect(
    url: &str,
) -> Result<WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>> {
    let (ws, _) = timeout(PEER_CONNECT_TIMEOUT, connect_async(url))
        .await
        .context("peer connect timeout")?
        .with_context(|| format!("peer connect {url}"))?;
    Ok(ws)
}

fn peer_ws_url(base: &str, token: &str, node: &str) -> String {
    let base = base.trim_end_matches('/');
    if base.contains('?') {
        format!("{base}&token={token}&node={node}")
    } else {
        format!("{base}?token={token}&node={node}")
    }
}

async fn serve_peer_session<S>(
    state: Arc<AgentState>,
    ws: WebSocketStream<S>,
    remote_node: Option<String>,
    label: String,
    reg: mpsc::UnboundedSender<PeerLink>,
) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (mut write, mut read) = ws.split();
    let (peer_tx, mut peer_rx) = mpsc::unbounded_channel::<String>();
    let node_name = remote_node.clone().unwrap_or_else(|| label.clone());
    let _ = reg.send(PeerLink {
        node: node_name.clone(),
        tx: peer_tx,
    });

    let state_read = state.clone();
    let remote = remote_node.clone();
    loop {
        tokio::select! {
            maybe = peer_rx.recv() => {
                match maybe {
                    Some(payload) => {
                        if write.send(WsMessage::Text(payload.into())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            msg = read.next() => {
                match msg {
                    Some(Ok(WsMessage::Text(text))) => {
                        if let Ok(Message::Clipboard { hash, data, mime, .. }) = decode_message(&text) {
                            let source = remote.as_deref().unwrap_or("peer");
                            if let Err(err) = apply_incoming_clipboard(
                                &state_read, &hash, &data, &mime, source, false,
                            ).await {
                                debug!("peer clipboard apply: {err:#}");
                            }
                        }
                    }
                    Some(Ok(WsMessage::Ping(payload))) => {
                        if write.send(WsMessage::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(WsMessage::Close(_))) | Some(Err(_)) | None => break,
                    _ => {}
                }
            }
        }
    }
    debug!("peer session ended: {node_name}");
    Ok(())
}
