use std::{collections::HashMap, net::SocketAddr, path::PathBuf, sync::Arc};

use anyhow::{anyhow, Context, Result};
use axum::{
    extract::{
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use clap::Parser;
use futures_util::StreamExt;
use poolsync_core::{
    decode_message, encode_message, AgentMode, Message, Neighbor, PoolTopology, ScreenInfo,
    TopologyNode,
};
use serde::Serialize;
use tokio::sync::{broadcast, RwLock};
use tower_http::services::{ServeDir, ServeFile};
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
#[command(
    name = "poolsync-hub",
    about = "PoolSync hub — presse-papiers + KVM maître dynamique"
)]
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

    /// Fichier JSON de topologie KVM (mosaïque écrans)
    #[arg(long, default_value = "/var/lib/poolsync/topology.json")]
    topology_file: PathBuf,
}

#[derive(Clone)]
struct NodeInfo {
    mode: AgentMode,
    screen: ScreenInfo,
    neighbors: Vec<Neighbor>,
    kvm_enabled: bool,
    connected_at: u64,
    sender: broadcast::Sender<String>,
}

#[derive(Clone)]
struct HubState {
    token: String,
    started_at: u64,
    topology_file: PathBuf,
    topology: Arc<RwLock<PoolTopology>>,
    nodes: Arc<RwLock<HashMap<String, NodeInfo>>>,
    master: Arc<RwLock<Option<String>>>,
    input_owner: Arc<RwLock<Option<String>>>,
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
    kvm_enabled: bool,
    connected_at: u64,
    online: bool,
    is_master: bool,
}

#[derive(Deserialize)]
struct TokenQuery {
    token: String,
}

fn load_topology(path: &PathBuf) -> PoolTopology {
    match std::fs::read_to_string(path) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|err| {
            warn!(
                "invalid topology {}: {err:#} — using empty default",
                path.display()
            );
            PoolTopology::default()
        }),
        Err(_) => {
            let topo = PoolTopology::default();
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            if let Ok(json) = serde_json::to_string_pretty(&topo) {
                let _ = std::fs::write(path, json);
            }
            topo
        }
    }
}

async fn save_topology(state: &HubState, topology: PoolTopology) -> Result<()> {
    let json = serde_json::to_string_pretty(&topology)?;
    if let Some(parent) = state.topology_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&state.topology_file, &json)?;
    *state.topology.write().await = topology;
    Ok(())
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
    let topology = load_topology(&args.topology_file);
    let state = HubState {
        token: args.token.clone(),
        started_at,
        topology_file: args.topology_file.clone(),
        topology: Arc::new(RwLock::new(topology)),
        nodes: Arc::new(RwLock::new(HashMap::new())),
        master: Arc::new(RwLock::new(None)),
        input_owner: Arc::new(RwLock::new(None)),
        last_clipboard_hash: Arc::new(RwLock::new(None)),
        last_clipboard_at: Arc::new(RwLock::new(None)),
    };

    let mut app = Router::new()
        .route("/health", get(health))
        .route("/api/status", get(api_status))
        .route(
            "/api/topology",
            get(api_topology_get).post(api_topology_post),
        )
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
            kvm_enabled: info.kvm_enabled,
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

async fn api_topology_get(State(state): State<HubState>) -> Json<PoolTopology> {
    Json(state.topology.read().await.clone())
}

async fn api_topology_post(
    Query(query): Query<TokenQuery>,
    State(state): State<HubState>,
    Json(body): Json<PoolTopology>,
) -> Result<StatusCode, StatusCode> {
    if query.token != state.token {
        return Err(StatusCode::UNAUTHORIZED);
    }
    save_topology(&state, body.clone())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let payload = encode_message(&Message::TopologyUpdate { topology: body })
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    broadcast_all(&state, &payload).await;
    info!("topology saved and broadcast");
    Ok(StatusCode::OK)
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
            kvm_enabled,
        } => {
            let topology_update = {
                let mut topo = state.topology.write().await;
                match topo.nodes.get_mut(&node) {
                    Some(n) => {
                        if n.width != screen.width || n.height != screen.height {
                            info!(
                                "topology {node}: {}x{} → {}x{}",
                                n.width, n.height, screen.width, screen.height
                            );
                            n.width = screen.width;
                            n.height = screen.height;
                        }
                    }
                    None => {
                        // Nouveau nœud : placé à la suite horizontalement (mosaïque par défaut).
                        let x = topo
                            .nodes
                            .values()
                            .map(|n| n.x + n.width as i32)
                            .max()
                            .unwrap_or(0);
                        info!(
                            "topology: nouveau nœud {node} ({}x{}) @ x={x}",
                            screen.width, screen.height
                        );
                        topo.nodes.insert(
                            node.clone(),
                            TopologyNode {
                                x,
                                y: 0,
                                width: screen.width,
                                height: screen.height,
                                kvm_enabled,
                                neighbors: HashMap::new(),
                            },
                        );
                    }
                }
                topo.clone()
            };
            if let Err(err) = save_topology(state, topology_update.clone()).await {
                warn!("topology save: {err:#}");
            }

            {
                let mut nodes = state.nodes.write().await;
                nodes.insert(
                    node.clone(),
                    NodeInfo {
                        mode,
                        screen: screen.clone(),
                        neighbors,
                        kvm_enabled,
                        connected_at: now_secs(),
                        sender: sender.clone(),
                    },
                );
            }

            let payload = encode_message(&Message::TopologyUpdate {
                topology: topology_update,
            })?;
            broadcast_all(state, &payload).await;
            let _ = sender.send(payload);

            let owner = {
                let input_owner = state.input_owner.read().await.clone();
                if input_owner.is_some() {
                    input_owner
                } else {
                    state.master.read().await.clone()
                }
            };
            if let Some(owner_node) = owner {
                let payload = encode_message(&Message::MasterChanged { node: owner_node })?;
                let _ = sender.send(payload);
            }
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
    let mut owner = state.input_owner.write().await;
    if owner.as_deref() == Some(node) {
        *owner = None;
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
            let changed = {
                let mut owner = state.input_owner.write().await;
                let changed = owner.as_deref() != Some(node.as_str());
                *owner = Some(node.clone());
                changed
            };
            if changed {
                *state.master.write().await = Some(node.clone());
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
        Message::SwitchTo {
            node,
            x,
            y,
            input_node,
        } => {
            let input = if input_node.is_empty() {
                from.to_string()
            } else {
                input_node
            };
            let payload = encode_message(&Message::SwitchTo {
                node: node.clone(),
                x,
                y,
                input_node: input.clone(),
            })?;
            route_to_node(state, &node, &payload).await;
            broadcast_all(state, &payload).await;
            *state.input_owner.write().await = Some(input.clone());
            let mut master = state.master.write().await;
            if master.as_deref() != Some(input.as_str()) {
                *master = Some(input.clone());
                broadcast_all(
                    state,
                    &encode_message(&Message::MasterChanged { node: input })?,
                )
                .await;
            }
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
