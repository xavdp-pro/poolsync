use std::{
    collections::{HashMap, VecDeque},
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
    http::StatusCode,
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
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
use tokio_stream::wrappers::BroadcastStream;
use tower_http::services::{ServeDir, ServeFile};
use tracing::{error, info, warn};

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use image::imageops::FilterType;
use image::{ImageFormat, ImageReader};
use std::io::Cursor;

const TRAY_THUMB_MAX_PX: u32 = 64;

fn image_thumb_b64(data_b64: &str) -> Option<String> {
    let bytes = B64.decode(data_b64).ok()?;
    let reader = ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .ok()?;
    let img = reader.decode().ok()?;
    let thumb = img.resize(TRAY_THUMB_MAX_PX, TRAY_THUMB_MAX_PX, FilterType::Triangle);
    let mut out = Vec::new();
    thumb
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .ok()?;
    Some(B64.encode(out))
}

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
    clipboard_history: Arc<RwLock<VecDeque<ClipboardHistoryEntry>>>,
    clipboard_history_revision: Arc<RwLock<u64>>,
    clipboard_events: broadcast::Sender<u64>,
}

const CLIPBOARD_HISTORY_MAX: usize = 50;

#[derive(Clone)]
struct ClipboardHistoryEntry {
    hash: String,
    mime: String,
    preview: String,
    data: String,
    thumb_b64: Option<String>,
    source_node: String,
    at: u64,
}

#[derive(Serialize)]
struct ClipboardHistoryItem {
    hash: String,
    mime: String,
    preview: String,
    source_node: String,
    at: u64,
    is_image: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    thumb_b64: Option<String>,
}

#[derive(Serialize)]
struct ClipboardHistoryResponse {
    items: Vec<ClipboardHistoryItem>,
}

#[derive(Deserialize)]
struct ClipboardPickBody {
    hash: String,
    #[serde(default)]
    node: Option<String>,
}

#[derive(Deserialize)]
struct ClipboardDeleteBody {
    hashes: Vec<String>,
}

#[derive(Serialize)]
struct ClipboardItemResponse {
    hash: String,
    mime: String,
    preview: String,
    data: String,
    source_node: String,
    at: u64,
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
    let (clipboard_events, _) = broadcast::channel(64);
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
        clipboard_history: Arc::new(RwLock::new(VecDeque::new())),
        clipboard_history_revision: Arc::new(RwLock::new(0)),
        clipboard_events,
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/api/status", get(api_status))
        .route(
            "/api/topology",
            get(api_topology_get).post(api_topology_post),
        )
        .route("/api/clipboard/history", get(api_clipboard_history))
        .route("/api/clipboard/item", get(api_clipboard_item))
        .route("/api/clipboard/events", get(api_clipboard_events))
        .route("/api/clipboard/pick", axum::routing::post(api_clipboard_pick))
        .route("/api/clipboard/clear", axum::routing::post(api_clipboard_clear))
        .route("/api/clipboard/delete", axum::routing::post(api_clipboard_delete))
        .route("/ws", get(ws_handler))
        .with_state(state.clone());

    let mut app = app;
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

fn clip_preview_hub(mime: &str, data: &str) -> String {
    if mime.starts_with("image/") {
        let label = mime.strip_prefix("image/").unwrap_or(mime);
        let bytes = data.len().saturating_mul(3) / 4;
        let size = if bytes >= 1024 * 1024 {
            format!("{:.1} Mo", bytes as f64 / (1024.0 * 1024.0))
        } else if bytes >= 1024 {
            format!("{} Ko", bytes / 1024)
        } else {
            format!("{bytes} o")
        };
        format!("[Image {label} — {size}]")
    } else {
        let one_line: String = data.chars().take(80).collect();
        if data.len() > 80 {
            format!("{one_line}…")
        } else {
            one_line
        }
    }
}

async fn push_clipboard_history(
    state: &HubState,
    source_node: &str,
    hash: &str,
    mime: &str,
    data: &str,
) {
    let entry = ClipboardHistoryEntry {
        hash: hash.to_string(),
        mime: mime.to_string(),
        preview: clip_preview_hub(mime, data),
        data: data.to_string(),
        thumb_b64: mime
            .starts_with("image/")
            .then(|| image_thumb_b64(data))
            .flatten(),
        source_node: source_node.to_string(),
        at: now_secs(),
    };
    let mut hist = state.clipboard_history.write().await;
    hist.retain(|e| e.hash != entry.hash);
    hist.push_front(entry);
    while hist.len() > CLIPBOARD_HISTORY_MAX {
        hist.pop_back();
    }
}

async fn notify_clipboard_history(state: &HubState) {
    let revision = {
        let mut rev = state.clipboard_history_revision.write().await;
        *rev += 1;
        *rev
    };
    let _ = state.clipboard_events.send(revision);
    if let Ok(payload) = encode_message(&Message::ClipboardHistoryUpdated { revision }) {
        broadcast_all(state, &payload).await;
    }
}

async fn api_clipboard_events(
    Query(query): Query<TokenQuery>,
    State(state): State<HubState>,
) -> Result<Sse<impl futures_util::Stream<Item = Result<Event, std::convert::Infallible>>>, StatusCode>
{
    if query.token != state.token {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let initial = *state.clipboard_history_revision.read().await;
    let initial_event = futures_util::stream::once(async move {
        Ok(Event::default().data(format!("{{\"revision\":{initial}}}")))
    });
    let updates = BroadcastStream::new(state.clipboard_events.subscribe())
        .filter_map(|r| async move { r.ok() })
        .map(|revision| Ok(Event::default().data(format!("{{\"revision\":{revision}}}"))));
    let stream = initial_event.chain(updates);
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

#[derive(Deserialize)]
struct HistoryQuery {
    token: String,
    limit: Option<usize>,
}

async fn api_clipboard_history(
    Query(query): Query<HistoryQuery>,
    State(state): State<HubState>,
) -> Result<Json<ClipboardHistoryResponse>, StatusCode> {
    if query.token != state.token {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let limit = query.limit.unwrap_or(50).min(CLIPBOARD_HISTORY_MAX);
    let hist = state.clipboard_history.read().await;
    let items = hist
        .iter()
        .take(limit)
        .map(|e| ClipboardHistoryItem {
            hash: e.hash.clone(),
            mime: e.mime.clone(),
            preview: e.preview.clone(),
            source_node: e.source_node.clone(),
            at: e.at,
            is_image: e.mime.starts_with("image/"),
            thumb_b64: e.thumb_b64.clone(),
        })
        .collect();
    Ok(Json(ClipboardHistoryResponse { items }))
}

#[derive(Deserialize)]
struct ItemQuery {
    token: String,
    hash: String,
}

async fn api_clipboard_item(
    Query(query): Query<ItemQuery>,
    State(state): State<HubState>,
) -> Result<Json<ClipboardItemResponse>, StatusCode> {
    if query.token != state.token {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let hist = state.clipboard_history.read().await;
    let entry = hist
        .iter()
        .find(|e| e.hash == query.hash)
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(ClipboardItemResponse {
        hash: entry.hash.clone(),
        mime: entry.mime.clone(),
        preview: entry.preview.clone(),
        data: entry.data.clone(),
        source_node: entry.source_node.clone(),
        at: entry.at,
    }))
}

async fn api_clipboard_pick(
    Query(query): Query<TokenQuery>,
    State(state): State<HubState>,
    Json(body): Json<ClipboardPickBody>,
) -> Result<StatusCode, StatusCode> {
    if query.token != state.token {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let entry = {
        let hist = state.clipboard_history.read().await;
        hist.iter()
            .find(|e| e.hash == body.hash)
            .cloned()
            .ok_or(StatusCode::NOT_FOUND)?
    };
    *state.last_clipboard_hash.write().await = Some(entry.hash.clone());
    *state.last_clipboard_at.write().await = Some(now_secs());
    push_clipboard_history(
        &state,
        "pick",
        &entry.hash,
        &entry.mime,
        &entry.data,
    )
    .await;
    let payload = encode_message(&Message::Clipboard {
        msg_id: uuid::Uuid::new_v4().to_string(),
        hash: entry.hash,
        mime: entry.mime,
        data: entry.data,
    })
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if let Some(ref node) = body.node {
        broadcast_except(&state, node, &payload).await;
    } else {
        broadcast_all(&state, &payload).await;
    }
    notify_clipboard_history(&state).await;
    Ok(StatusCode::OK)
}

async fn api_clipboard_clear(
    Query(query): Query<TokenQuery>,
    State(state): State<HubState>,
) -> Result<StatusCode, StatusCode> {
    if query.token != state.token {
        return Err(StatusCode::UNAUTHORIZED);
    }
    state.clipboard_history.write().await.clear();
    *state.last_clipboard_hash.write().await = None;
    *state.last_clipboard_at.write().await = None;
    notify_clipboard_history(&state).await;
    info!("clipboard history cleared");
    Ok(StatusCode::OK)
}

async fn api_clipboard_delete(
    Query(query): Query<TokenQuery>,
    State(state): State<HubState>,
    Json(body): Json<ClipboardDeleteBody>,
) -> Result<StatusCode, StatusCode> {
    if query.token != state.token {
        return Err(StatusCode::UNAUTHORIZED);
    }
    if body.hashes.is_empty() {
        return Ok(StatusCode::OK);
    }
    let to_remove: std::collections::HashSet<String> =
        body.hashes.iter().cloned().collect();
    let mut hist = state.clipboard_history.write().await;
    hist.retain(|e| !to_remove.contains(&e.hash));
    let last = state.last_clipboard_hash.read().await.clone();
    if last.as_ref().is_some_and(|h| to_remove.contains(h)) {
        *state.last_clipboard_hash.write().await = hist.front().map(|e| e.hash.clone());
        *state.last_clipboard_at.write().await = hist.front().map(|e| e.at);
    }
    drop(hist);
    notify_clipboard_history(&state).await;
    info!("clipboard history deleted {} item(s)", to_remove.len());
    Ok(StatusCode::OK)
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
            let revision = *state.clipboard_history_revision.read().await;
            if revision > 0 {
                let payload =
                    encode_message(&Message::ClipboardHistoryUpdated { revision })?;
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
            let duplicate = {
                let last = state.last_clipboard_hash.read().await;
                last.as_deref() == Some(&hash)
            };
            if !duplicate {
                *state.last_clipboard_hash.write().await = Some(hash.clone());
            }
            *state.last_clipboard_at.write().await = Some(now_secs());
            // Toujours remonter en tête (même hash recopié) ; broadcast seulement si nouveau.
            push_clipboard_history(&state, from, &hash, &mime, &data).await;
            notify_clipboard_history(state).await;
            if duplicate {
                return Ok(());
            }

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
