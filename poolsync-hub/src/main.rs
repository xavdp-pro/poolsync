use std::{
    collections::HashMap,
    net::SocketAddr,
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
    Router,
};
use clap::Parser;
use futures_util::StreamExt;
use poolsync_core::{
    decode_message, encode_message, AgentMode, Message, Neighbor, ScreenInfo,
};
use tokio::sync::{broadcast, RwLock};
use tracing::{error, info, warn};
use uuid::Uuid;

#[derive(Parser, Debug)]
#[command(name = "poolsync-hub", about = "PoolSync hub — presse-papiers + KVM maître dynamique")]
struct Args {
    /// Adresse d'écoute (0.0.0.0 pour LAN/VPN/public)
    #[arg(long, default_value = "0.0.0.0:9470")]
    listen: String,

    /// Token partagé avec les agents
    #[arg(long, default_value = "poolsync-dev")]
    token: String,
}

#[derive(Clone)]
struct NodeInfo {
    mode: AgentMode,
    screen: ScreenInfo,
    neighbors: Vec<Neighbor>,
    sender: broadcast::Sender<String>,
}

#[derive(Clone)]
struct HubState {
    token: String,
    nodes: Arc<RwLock<HashMap<String, NodeInfo>>>,
    master: Arc<RwLock<Option<String>>>,
    last_clipboard_hash: Arc<RwLock<Option<String>>>,
}

#[derive(Deserialize)]
struct WsQuery {
    token: String,
}

use serde::Deserialize;

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

    let state = HubState {
        token: args.token.clone(),
        nodes: Arc::new(RwLock::new(HashMap::new())),
        master: Arc::new(RwLock::new(None)),
        last_clipboard_hash: Arc::new(RwLock::new(None)),
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/ws", get(ws_handler))
        .with_state(state);

    info!("poolsync-hub listening on {listen}");
    let listener = tokio::net::TcpListener::bind(listen).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> impl IntoResponse {
    "ok"
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
