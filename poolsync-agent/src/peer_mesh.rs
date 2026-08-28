//! Mesh clipboard direct entre voisins (LAN/VPN) — sans relay hub bs1.

use crate::clipboard_incoming::apply_incoming_clipboard;
use crate::state::AgentState;
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use poolsync_core::{decode_message, Message};
use std::collections::{HashMap, HashSet};
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
    let (peer_in_tx, mut peer_in_rx) = mpsc::unbounded_channel::<PeerInbound>();

    let state_listen = state.clone();
    let reg_listen = peer_reg_tx.clone();
    let in_listen = peer_in_tx.clone();
    tokio::spawn(async move {
        if let Err(err) = run_listener(state_listen, reg_listen, in_listen).await {
            warn!("peer listener: {err:#}");
        }
    });

    for neighbor in state.config.neighbors.clone() {
        // A link is initiated by exactly one deterministic endpoint.  The
        // other endpoint accepts it and registers the same direct channel.
        // This avoids duplicate sessions and image echo/reconnect storms.
        if !should_initiate_link(&state.config.node, &neighbor.node) {
            continue;
        }
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
        let incoming = peer_in_tx.clone();
        tokio::spawn(async move {
            peer_outbound_loop(state_out, node, urls, reg, incoming).await;
        });
    }

    tokio::spawn(async move {
        let mut peers: HashMap<String, mpsc::UnboundedSender<String>> = HashMap::new();
        let mut seen_messages: HashSet<String> = HashSet::new();
        loop {
            tokio::select! {
                Some(payload) = local_rx.recv() => {
                    if let Some(id) = clipboard_message_id(&payload) {
                        remember_message(&mut seen_messages, id);
                    }
                    for tx in peers.values() {
                        let _ = tx.send(payload.clone());
                    }
                }
                Some(incoming) = peer_in_rx.recv() => {
                    let Some(id) = clipboard_message_id(&incoming.payload) else {
                        continue;
                    };
                    if !seen_messages.insert(id) {
                        continue;
                    }
                    trim_seen_messages(&mut seen_messages);
                    for (node, tx) in &peers {
                        if node != &incoming.source {
                            let _ = tx.send(incoming.payload.clone());
                        }
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

fn should_initiate_link(local: &str, remote: &str) -> bool {
    !local.is_empty() && !remote.is_empty() && local < remote
}

fn should_forward_incoming(mime: &str, local_copy_priority: bool) -> bool {
    mime.starts_with("image/") || !local_copy_priority
}

struct PeerLink {
    node: String,
    tx: mpsc::UnboundedSender<String>,
}

struct PeerInbound {
    source: String,
    payload: String,
}

const MAX_SEEN_MESSAGES: usize = 4096;

fn clipboard_message_id(payload: &str) -> Option<String> {
    match decode_message(payload).ok()? {
        Message::Clipboard { msg_id, .. } if !msg_id.is_empty() => Some(msg_id),
        _ => None,
    }
}

fn trim_seen_messages(seen: &mut HashSet<String>) {
    if seen.len() > MAX_SEEN_MESSAGES {
        seen.clear();
    }
}

fn remember_message(seen: &mut HashSet<String>, id: String) {
    seen.insert(id);
    trim_seen_messages(seen);
}

async fn run_listener(
    state: Arc<AgentState>,
    reg: mpsc::UnboundedSender<PeerLink>,
    incoming: mpsc::UnboundedSender<PeerInbound>,
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
        let incoming_in = incoming.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_inbound(
                state_in,
                stream,
                peer_addr.to_string(),
                reg_in,
                incoming_in,
            )
            .await {
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
    incoming: mpsc::UnboundedSender<PeerInbound>,
) -> Result<()> {
    let mut remote_node: Option<String> = None;
    let expected_token = state.config.token.clone();
    let mut token_valid = true;
    let ws = tokio_tungstenite::accept_hdr_async(
        stream,
        |req: &tokio_tungstenite::tungstenite::handshake::server::Request,
         res: tokio_tungstenite::tungstenite::handshake::server::Response| {
            if let Some(query) = req.uri().query() {
                for part in query.split('&') {
                    if let Some((k, v)) = part.split_once('=') {
                        if k == "node" && !v.is_empty() {
                            remote_node = Some(v.to_string());
                        } else if k == "token" && v != expected_token {
                            token_valid = false;
                        }
                    }
                }
            }
            if !token_valid {
                let err_res = tokio_tungstenite::tungstenite::handshake::server::ErrorResponse::new(
                    Some("Invalid Token".to_string()),
                );
                return Err(err_res);
            }
            Ok(res)
        },
    )
    .await
    .context("peer ws accept")?;
    let label = remote_node.clone().unwrap_or(peer_addr);
    serve_peer_session(state, ws, remote_node, label, reg, incoming).await
}

async fn peer_outbound_loop(
    state: Arc<AgentState>,
    neighbor: String,
    urls: Vec<String>,
    reg: mpsc::UnboundedSender<PeerLink>,
    incoming: mpsc::UnboundedSender<PeerInbound>,
) {
    let mut backoff = PEER_RECONNECT_INITIAL;
    loop {
        let mut session_ended = false;
        for url in &urls {
            let full = peer_ws_url(url, &state.config.token, &state.config.node);
            match timeout_connect(&full).await {
                Ok(ws) => {
                    info!("peer mesh → {neighbor} via {url}");
                    if serve_peer_session(
                        state.clone(),
                        ws,
                        Some(neighbor.clone()),
                        url.clone(),
                        reg.clone(),
                        incoming.clone(),
                    )
                        .await
                        .is_ok()
                    {
                        // A clean WebSocket close still means the session is gone.  Without
                        // this pause the outer loop reconnects immediately, creating thousands
                        // of sockets and starving clipboard work on every peer.
                        session_ended = true;
                        break;
                    }
                }
                Err(err) => {
                    debug!("peer connect {neighbor} {url}: {err:#}");
                }
            }
        }
        if session_ended {
            backoff = PEER_RECONNECT_INITIAL;
        }
        sleep(backoff).await;
        backoff = std::cmp::min(backoff * 2, PEER_RECONNECT_MAX);
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
    incoming: mpsc::UnboundedSender<PeerInbound>,
) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (mut write, mut read) = ws.split();
    let (peer_tx, mut peer_rx) = mpsc::unbounded_channel::<String>();
    let node_name = remote_node.clone().unwrap_or_else(|| label.clone());
    // Inbound handshakes include their node name.  With one deterministic
    // dialer per pair, both endpoints may register this single channel.
    if remote_node.is_some() {
        let _ = reg.send(PeerLink {
            node: node_name.clone(),
            tx: peer_tx,
        });
    }

    let state_read = state.clone();
    let remote = remote_node.clone();
    let mut ping_interval = tokio::time::interval(Duration::from_secs(10));

    loop {
        tokio::select! {
            _ = ping_interval.tick() => {
                if write.send(WsMessage::Ping(vec![].into())).await.is_err() {
                    warn!("peer mesh ping échoué vers {node_name}");
                    break;
                }
            }
            maybe = peer_rx.recv() => {
                match maybe {
                    Some(payload) => {
                        if let Ok(Message::Clipboard { hash, ref mime, ref data, .. }) =
                            decode_message(&payload)
                        {
                            if mime.starts_with("image/") {
                                info!(
                                    "image-trace PEER-SEND id={} to={} mime={} wire_bytes={}",
                                    crate::clipboard::trace_id(&hash),
                                    node_name,
                                    mime,
                                    data.len()
                                );
                            }
                        }
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
                            // A stale text message rejected in favour of a
                            // just-copied local selection must stop here.  If
                            // it is forwarded, another neighbour can still
                            // overwrite the local copy a hop later.
                            if !should_forward_incoming(
                                &mime,
                                state_read.local_clipboard_priority_active(),
                            ) {
                                info!("peer mesh: stale text dropped from {source}");
                                continue;
                            }
                            if let Err(err) = apply_incoming_clipboard(
                                &state_read, &hash, &data, &mime, source, false,
                            ).await {
                                debug!("peer clipboard apply: {err:#}");
                            }
                            let _ = incoming.send(PeerInbound {
                                source: node_name.clone(),
                                payload: text.to_string(),
                            });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exactly_one_endpoint_dials_each_pair() {
        for (a, b) in [("asus", "gbs-p2"), ("gbs-p2", "gbs-p3"), ("acer", "asus")] {
            assert_ne!(should_initiate_link(a, b), should_initiate_link(b, a));
        }
    }

    #[test]
    fn invalid_or_self_links_are_never_dialed() {
        assert!(!should_initiate_link("asus", "asus"));
        assert!(!should_initiate_link("", "p2"));
        assert!(!should_initiate_link("p2", ""));
    }

    #[test]
    fn stale_text_is_not_forwarded_while_local_copy_has_priority() {
        assert!(!should_forward_incoming("text/plain", true));
        assert!(should_forward_incoming("text/plain", false));
        assert!(should_forward_incoming("image/png", true));
    }

    #[test]
    fn websocket_url_preserves_existing_query() {
        assert_eq!(
            peer_ws_url("ws://p2:9472/ws?lan=1", "tok", "asus"),
            "ws://p2:9472/ws?lan=1&token=tok&node=asus"
        );
    }

    #[test]
    fn clipboard_message_id_drives_mesh_deduplication() {
        let payload = poolsync_core::encode_message(&Message::Clipboard {
            msg_id: "copy-42".into(),
            hash: "hash".into(),
            mime: "text/plain".into(),
            data: "hello".into(),
        })
        .unwrap();
        assert_eq!(clipboard_message_id(&payload).as_deref(), Some("copy-42"));

        let mut seen = HashSet::new();
        assert!(seen.insert(clipboard_message_id(&payload).unwrap()));
        assert!(!seen.insert(clipboard_message_id(&payload).unwrap()));
    }
}
