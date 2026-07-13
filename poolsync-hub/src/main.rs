use std::{
    collections::HashMap,
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
};

use anyhow::{anyhow, Context, Result};
use axum::{
    extract::{
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use clap::Parser;
use futures_util::StreamExt;
use poolsync_core::{
    decode_message, encode_message, AgentMode, Message, Neighbor, ScreenInfo,
};
use serde::Serialize;
use tokio::sync::{broadcast, RwLock};
use tower_http::services::{ServeDir, ServeFile};
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
#[command(name = "poolsync-hub", about = "PoolSync hub — presse-papiers + KVM maître dynamique")]
struct Args {
    /// Adresse d'écoute (0.0.0.0 pour LAN/VPN/public)
    #[arg(long, default_value = "0.0.0.0:9470")]
    listen: String,

    /// Token partagé avec les agents
    #[arg(long, default_value = "poolsync-dev")]
    token: String,

    /// Répertoire des fichiers statiques (dashboard web)
    #[arg(long)]
    web_dir: Option<PathBuf>,
}

#[derive(Clone)]
struct NodeInfo {
    mode: AgentMode,
    screen: ScreenInfo,
    neighbors: Vec<Neighbor>,
    connected_at: u64,
    sender: broadcast::Sender<String>,
}

#[derive(Clone)]
struct HubState {
    token: String,
    started_at: u64,
    nodes: Arc<RwLock<HashMap<String, NodeInfo>>>,
    master: Arc<RwLock<Option<String>>>,
    last_clipboard_hash: Arc<RwLock<Option<String>>>,
    last_clipboard_at: Arc<RwLock<Option<u64>>>,
}

#[derive(Deserialize)]
struct WsQuery {
    token: String,
}

use serde::Deserialize;

#[derive(Serialize)]
struct StatusResponse {
    hub: HubInfo,
    master: Option<String>,
    clipboard: ClipboardInfo,
    nodes: Vec<NodeStatus>,
}

#[derive(Serialize)]
struct HubInfo {
    version: &'static str,
    started_at: u64,
    node_count: usize,
    listen: String,
}

#[derive(Serialize)]
struct ClipboardInfo {
    last_hash: Option<String>,
    last_at: Option<u64>,
}

#[derive(Serialize)]
struct NodeStatus {
    name: String,
    mode: AgentMode,
    screen: ScreenInfo,
    neighbors: Vec<Neighbor>,
    connected_at: u64,
    online: bool,
    is_master: bool,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "poolsync_hub=info".into()),
        )
        .init();

    let args = Args::parse();
    let listen: SocketAddr = args
        .listen
        .parse()
        .with_context(|| format!("invalid listen address {}", args.listen))?;

    let started_at = now_secs();
    let state = HubState {
        token: args.token.clone(),
        started_at,
        nodes: Arc::new(RwLock::new(HashMap::new())),
        master: Arc::new(RwLock::new(None)),
        last_clipboard_hash: Arc::new(RwLock::new(None)),
        last_clipboard_at: Arc::new(RwLock::new(None)),
    };

    let mut app = Router::new()
        .route("/health", get(health))
        .route("/api/status", get(api_status))
        .route("/ws", get(ws_handler))
        .with_state(state);

    if let Some(web_dir) = args.web_dir.as_ref() {
        let index = web_dir.join("index.html");
        if web_dir.is_dir() && index.is_file() {
            let serve = ServeDir::new(web_dir).not_found_service(ServeFile::new(index));
            app = app.fallback_service(serve);
            info!("serving web dashboard from {}", web_dir.display());
        } else {
            warn!(
                "web_dir {} missing or no index.html — dashboard disabled",
                web_dir.display()
            );
        }
    }

    info!("poolsync-hub listening on {listen}");
    let listener = tokio::net::TcpListener::bind(listen).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> impl IntoResponse {
    "ok"
}

async fn api_status(State(state): State<HubState>) -> Json<StatusResponse> {
    let nodes_map = state.nodes.read().await;
    let master = state.master.read().await.clone();
    let last_hash = state.last_clipboard_hash.read().await.clone();
    let last_at = state.last_clipboard_at.read().await;
    let node_count = nodes_map.len();

    let nodes: Vec<NodeStatus> = nodes_map
        .iter()
        .map(|(name, info)| NodeStatus {
            name: name.clone(),
            mode: info.mode,
            screen: info.screen.clone(),
            neighbors: info.neighbors.clone(),
            connected_at: info.connected_at,
            online: true,
            is_master: master.as_deref() == Some(name.as_str()),
        })
        .collect();

    Json(StatusResponse {
        hub: HubInfo {
            version: env!("CARGO_PKG_VERSION"),
            started_at: state.started_at,
            node_count,
            listen: "0.0.0.0:9470".into(),
        },
        master,
        clipboard: ClipboardInfo {
            last_hash,
            last_at: *last_at,
        },
        nodes,
    })
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
    State(state): State<HubState>,
) -> impl IntoResponse {
    if query.token != state.token {
        return (axum::http::StatusCode::UNAUTHORIZED, "invalid token").into_response();
    }
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: HubState) {
    if let Err(err) = run_session(socket, state).await {
        error!("session ended: {err:#}");
    }
}

async fn run_session(mut socket: WebSocket, state: HubState) -> Result<()> {
    let (tx, mut rx) = broadcast::channel::<String>(256);
    let mut node_name: Option<String> = None;

    loop {
        tokio::select! {
            maybe_in = socket.recv() => {
                let Some(frame) = maybe_in else { break; };
                match frame? {
                    WsMessage::Text(text) => {
                        if let Some(name) = node_name.as_deref() {
                            handle_message(&state, name, &text, &tx).await?;
                        } else {
                            node_name = register_node(&state, &text, tx.clone()).await?;
                            info!("node registered: {}", node_name.as_deref().unwrap_or("?"));
                        }
                    }
                    WsMessage::Ping(payload) => {
                        socket.send(WsMessage::Pong(payload)).await?;
                    }
                    WsMessage::Close(_) => break,
                    _ => {}
                }
            }
            Ok(outgoing) = rx.recv() => {
                socket.send(WsMessage::Text(outgoing.into())).await?;
            }
        }
    }

    if let Some(name) = node_name {
        unregister_node(&state, &name).await;
        info!("node disconnected: {name}");
    }
    Ok(())
}

async fn register_node(
    state: &HubState,
    text: &str,
    sender: broadcast::Sender<String>,
) -> Result<Option<String>> {
    let msg = decode_message(text)?;
    match msg {
        Message::Hello {
            node,
            mode,
            screen,
            neighbors,
        } => {
            let mut nodes = state.nodes.write().await;
            nodes.insert(
                node.clone(),
                NodeInfo {
                    mode,
                    screen,
                    neighbors,
                    connected_at: now_secs(),
                    sender,
                },
            );
            Ok(Some(node))
        }
        _ => Err(anyhow!("first message must be hello")),
    }
}

async fn unregister_node(state: &HubState, node: &str) {
    let mut nodes = state.nodes.write().await;
    nodes.remove(node);
    let mut master = state.master.write().await;
    if master.as_deref() == Some(node) {
        *master = None;
    }
}

async fn handle_message(
    state: &HubState,
    from: &str,
    text: &str,
    local_tx: &broadcast::Sender<String>,
) -> Result<()> {
    let msg = decode_message(text)?;
    match msg {
        Message::Clipboard {
            msg_id,
            hash,
            mime,
            data,
        } => {
            let mut last = state.last_clipboard_hash.write().await;
            if last.as_deref() == Some(&hash) {
                return Ok(());
            }
            *last = Some(hash.clone());
            *state.last_clipboard_at.write().await = Some(now_secs());

            let payload = encode_message(&Message::Clipboard {
                msg_id,
                hash,
                mime,
                data,
            })?;
            broadcast_except(state, from, &payload).await;
        }
        Message::MasterClaim { node, ts: _ } => {
            let mut master = state.master.write().await;
            if master.as_deref() != Some(&node) {
                *master = Some(node.clone());
                let payload = encode_message(&Message::MasterChanged { node })?;
                broadcast_all(state, &payload).await;
            }
        }
        Message::Input { target, kind } => {
            let payload = encode_message(&Message::Input {
                target: target.clone(),
                kind,
            })?;
            route_to_node(state, &target, &payload).await;
        }
        Message::SwitchTo { node, x, y } => {
            let node_name = node.clone();
            let payload = encode_message(&Message::SwitchTo { node, x, y })?;
            route_to_node(state, &node_name, &payload).await;
            let mut master = state.master.write().await;
            *master = Some(node_name.clone());
            broadcast_all(
                state,
                &encode_message(&Message::MasterChanged { node: node_name })?,
            )
            .await;
        }
        Message::Ping => {
            local_tx.send(encode_message(&Message::Pong)?)?;
        }
        _ => warn!("ignored message from {from}: {msg:?}"),
    }
    Ok(())
}

async fn broadcast_except(state: &HubState, except: &str, payload: &str) {
    let nodes = state.nodes.read().await;
    for (name, info) in nodes.iter() {
        if name != except {
            let _ = info.sender.send(payload.to_string());
        }
    }
}

async fn broadcast_all(state: &HubState, payload: &str) {
    let nodes = state.nodes.read().await;
    for info in nodes.values() {
        let _ = info.sender.send(payload.to_string());
    }
}

async fn route_to_node(state: &HubState, target: &str, payload: &str) {
    let nodes = state.nodes.read().await;
    if let Some(info) = nodes.get(target) {
        let _ = info.sender.send(payload.to_string());
    }
}
