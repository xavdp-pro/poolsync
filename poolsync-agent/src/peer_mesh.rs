//! Mesh clipboard direct entre voisins (LAN/VPN) — sans relay hub bs1.

use crate::clipboard_incoming::apply_incoming_clipboard;
use crate::state::AgentState;
use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use poolsync_core::{decode_message, decrypt_clipboard, AgentConfig, Message, Neighbor};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::{sleep, timeout, Duration};
use tokio_rustls::{rustls::ServerConfig, TlsAcceptor};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{
        client::IntoClientRequest,
        http::{header::AUTHORIZATION, HeaderValue},
        Message as WsMessage,
    },
    WebSocketStream,
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
        Message::EncryptedClipboard { msg_id, .. } if !msg_id.is_empty() => Some(msg_id),
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
    let tls = peer_tls_acceptor(&state.config)?;
    info!(
        "peer mesh écoute sur {addr} ({})",
        if tls.is_some() { "wss" } else { "ws" }
    );

    loop {
        let (stream, peer_addr) = listener.accept().await.context("peer accept")?;
        let state_in = state.clone();
        let reg_in = reg.clone();
        let incoming_in = incoming.clone();
        let tls_in = tls.clone();
        tokio::spawn(async move {
            let result = if let Some(acceptor) = tls_in {
                match acceptor.accept(stream).await {
                    Ok(stream) => {
                        handle_inbound_stream(
                            state_in,
                            stream,
                            peer_addr.to_string(),
                            reg_in,
                            incoming_in,
                        )
                        .await
                    }
                    Err(err) => Err(anyhow::anyhow!("peer TLS accept: {err}")),
                }
            } else {
                handle_inbound_stream(state_in, stream, peer_addr.to_string(), reg_in, incoming_in)
                    .await
            };
            if let Err(err) = result {
                debug!("peer inbound {peer_addr}: {err:#}");
            }
        });
    }
}

fn peer_tls_acceptor(config: &AgentConfig) -> Result<Option<TlsAcceptor>> {
    let (cert_path, key_path) = match (&config.peer_tls_cert, &config.peer_tls_key) {
        (None, None) => return Ok(None),
        (Some(cert), Some(key)) => (cert, key),
        _ => anyhow::bail!("peer_tls_cert and peer_tls_key must be configured together"),
    };
    let mut cert_reader = BufReader::new(
        File::open(cert_path).with_context(|| format!("open peer TLS cert {cert_path}"))?,
    );
    let certs = rustls_pemfile::certs(&mut cert_reader)
        .collect::<std::result::Result<Vec<_>, _>>()
        .context("read peer TLS certificates")?;
    let mut key_reader = BufReader::new(
        File::open(key_path).with_context(|| format!("open peer TLS key {key_path}"))?,
    );
    let key = rustls_pemfile::private_key(&mut key_reader)
        .context("read peer TLS private key")?
        .context("peer TLS private key missing")?;
    let server = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .context("build peer TLS configuration")?;
    Ok(Some(TlsAcceptor::from(Arc::new(server))))
}

#[allow(clippy::result_large_err)]
async fn handle_inbound_stream<S>(
    state: Arc<AgentState>,
    stream: S,
    peer_addr: String,
    reg: mpsc::UnboundedSender<PeerLink>,
    incoming: mpsc::UnboundedSender<PeerInbound>,
) -> Result<()>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let mut remote_node: Option<String> = None;
    let config = state.config.clone();
    let ws = tokio_tungstenite::accept_hdr_async(
        stream,
        |req: &tokio_tungstenite::tungstenite::handshake::server::Request,
         res: tokio_tungstenite::tungstenite::handshake::server::Response| {
            let Some(node) = peer_request_identity(req, &config) else {
                let err_res = tokio_tungstenite::tungstenite::handshake::server::ErrorResponse::new(
                    Some("Invalid peer credentials".to_string()),
                );
                return Err(err_res);
            };
            remote_node = Some(node);
            Ok(res)
        },
    )
    .await
    .context("peer ws accept")?;
    let label = remote_node.clone().unwrap_or(peer_addr);
    serve_peer_session(state, ws, remote_node, label, reg, incoming).await
}

fn neighbor_accepts_token(neighbor: &Neighbor, token: &str, shared_token: &str) -> bool {
    let current = neighbor.auth_token.as_deref().unwrap_or(shared_token);
    token == current || neighbor.previous_auth_token.as_deref() == Some(token)
}

fn config_accepts_peer_token(config: &AgentConfig, neighbor: &Neighbor, token: &str) -> bool {
    if let Some(expected) = config.peer_tokens.get(&neighbor.node) {
        return token == expected
            || config
                .previous_peer_tokens
                .get(&neighbor.node)
                .is_some_and(|old| old == token);
    }
    neighbor_accepts_token(neighbor, token, &config.token)
}

fn peer_request_identity(
    req: &tokio_tungstenite::tungstenite::handshake::server::Request,
    config: &AgentConfig,
) -> Option<String> {
    let node = req
        .headers()
        .get("x-poolsync-node")?
        .to_str()
        .ok()?
        .to_string();
    let authorization = req.headers().get(AUTHORIZATION)?.to_str().ok()?;
    let (scheme, token) = authorization.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") || node == config.node {
        return None;
    }
    let neighbor = config.neighbors.iter().find(|peer| peer.node == node)?;
    config_accepts_peer_token(config, neighbor, token).then_some(node)
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
            match timeout_connect(url, state.config.authentication_token(), &state.config.node)
                .await
            {
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
    token: &str,
    node: &str,
) -> Result<WebSocketStream<tokio_tungstenite::MaybeTlsStream<TcpStream>>> {
    let mut request = url.into_client_request()?;
    request.headers_mut().insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}"))?,
    );
    request
        .headers_mut()
        .insert("x-poolsync-node", HeaderValue::from_str(node)?);
    let (ws, _) = timeout(PEER_CONNECT_TIMEOUT, connect_async(request))
        .await
        .context("peer connect timeout")?
        .with_context(|| format!("peer connect {url}"))?;
    Ok(ws)
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
                        let decoded = decode_message(&payload).ok().and_then(|message| {
                            if matches!(message, Message::EncryptedClipboard { .. }) {
                                state
                                    .config
                                    .e2e_key
                                    .as_deref()
                                    .and_then(|key| decrypt_clipboard(&message, key).ok())
                            } else if state.config.e2e_key.is_none() {
                                Some(message)
                            } else {
                                None
                            }
                        });
                        if let Some(Message::Clipboard { hash, ref mime, ref data, .. }) = decoded {
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
                        let wire = decode_message(&text);
                        let decoded = wire.as_ref().ok().and_then(|message| {
                            if matches!(message, Message::EncryptedClipboard { .. }) {
                                state_read
                                    .config
                                    .e2e_key
                                    .as_deref()
                                    .and_then(|key| decrypt_clipboard(message, key).ok())
                            } else if state_read.config.e2e_key.is_none() {
                                Some(message.clone())
                            } else {
                                None
                            }
                        });
                        if let Some(Message::Clipboard {
                            hash, data, mime, origin, seq, ..
                        }) = decoded {
                            let source = remote.as_deref().unwrap_or("peer");
                            // Toujours relayer : `(origin, seq)` voyage avec le
                            // message, donc chaque nœud tranche lui-même. Filtrer
                            // ici priverait un voisin plus lointain d'un message
                            // qui est peut-être le plus récent pour lui.
                            if let Err(err) = apply_incoming_clipboard(
                                &state_read, &hash, &data, &mime, source, false, &origin, seq,
                            ).await {
                                debug!("peer clipboard apply: {err:#}");
                            }
                            let _ = incoming.send(PeerInbound {
                                source: node_name.clone(),
                                payload: text.to_string(),
                            });
                        } else if matches!(wire, Ok(Message::EncryptedClipboard { .. })) {
                            warn!("peer clipboard chiffré rejeté depuis {node_name}: clef absente ou invalide");
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
    fn peer_credentials_support_individual_rotation() {
        let neighbor = Neighbor {
            direction: poolsync_core::Direction::Left,
            node: "desk-b".into(),
            peer_url: None,
            peer_url_vpn: None,
            auth_token: Some("new".into()),
            previous_auth_token: Some("old".into()),
        };
        assert!(neighbor_accepts_token(&neighbor, "new", "shared"));
        assert!(neighbor_accepts_token(&neighbor, "old", "shared"));
        assert!(!neighbor_accepts_token(&neighbor, "shared", "shared"));
    }

    #[test]
    fn clipboard_message_id_drives_mesh_deduplication() {
        let payload = poolsync_core::encode_message(&Message::Clipboard {
            msg_id: "copy-42".into(),
            hash: "hash".into(),
            mime: "text/plain".into(),
            data: "hello".into(),
            origin: "asus".into(),
            seq: 7,
        })
        .unwrap();
        assert_eq!(clipboard_message_id(&payload).as_deref(), Some("copy-42"));

        let mut seen = HashSet::new();
        assert!(seen.insert(clipboard_message_id(&payload).unwrap()));
        assert!(!seen.insert(clipboard_message_id(&payload).unwrap()));
    }
}
