use anyhow::{Context, Result};
use clap::Parser;
use futures_util::{SinkExt, StreamExt};
use poolsync_core::{
    decode_message, encode_message, hash_text, AgentConfig, AgentMode, Message,
};
use std::{
    path::PathBuf,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::{
    process::Command,
    sync::mpsc,
    time::{sleep, Duration},
};
use tokio_tungstenite::{connect_async, tungstenite::Message as WsMessage};
use tracing::{info, warn};

#[derive(Parser, Debug)]
#[command(name = "poolsync-agent", about = "PoolSync agent — client presse-papiers + KVM")]
struct Args {
    #[arg(long, default_value = "/etc/poolsync/agent.toml")]
    config: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "poolsync_agent=info".into()),
        )
        .init();

    let args = Args::parse();
    let raw = tokio::fs::read_to_string(&args.config)
        .await
        .with_context(|| format!("read config {}", args.config.display()))?;
    let cfg: AgentConfig = toml::from_str(&raw).context("parse agent config")?;

    info!(
        "starting agent node={} hub={} mode={:?}",
        cfg.node, cfg.hub_url, cfg.mode
    );

    loop {
        if let Err(err) = run_agent(&cfg).await {
            tracing::error!("agent session ended: {err:#}");
        }
        sleep(Duration::from_secs(3)).await;
    }
}

async fn run_agent(cfg: &AgentConfig) -> Result<()> {
    let hub_url = format!(
        "{}?token={}",
        cfg.hub_url.trim_end_matches('/'),
        cfg.token
    );
    let (ws, _) = connect_async(&hub_url).await.context("connect hub websocket")?;
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

    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<String>();
    let last_clip_hash = Arc::new(tokio::sync::Mutex::new(String::new()));
    let is_master = Arc::new(AtomicBool::new(false));

    let cfg_bg = cfg.clone();
    let out_tx_bg = out_tx.clone();
    let last_clip_hash_bg = last_clip_hash.clone();
    let clip_task = tokio::spawn(async move {
        clipboard_poll_loop(&cfg_bg, out_tx_bg, last_clip_hash_bg).await;
    });

    let cfg_in = cfg.clone();
    let out_tx_in = out_tx.clone();
    let is_master_in = is_master.clone();
    let input_task = tokio::spawn(async move {
        if cfg_in.mode == AgentMode::Full {
            input_poll_loop(&cfg_in, out_tx_in, is_master_in).await;
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
                        handle_incoming(cfg, &text, &is_master, &last_clip_hash).await?;
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

    clip_task.abort();
    input_task.abort();
    Ok(())
}

async fn handle_incoming(
    cfg: &AgentConfig,
    text: &str,
    is_master: &AtomicBool,
    last_clip_hash: &tokio::sync::Mutex<String>,
) -> Result<()> {
    let msg = decode_message(text)?;
    match msg {
        Message::Clipboard {
            hash, data, mime, ..
        } => {
            let mut last = last_clip_hash.lock().await;
            if *last == hash {
                return Ok(());
            }
            *last = hash;
            set_clipboard(&data, &mime).await?;
            info!("clipboard synced ({mime}, {} bytes)", data.len());
        }
        Message::MasterChanged { node } => {
            is_master.store(node == cfg.node, Ordering::SeqCst);
            info!("master is now {node}");
        }
        Message::Input { kind, .. } if cfg.mode == AgentMode::Full => {
            inject_input(&kind).await?;
        }
        Message::SwitchTo { x, y, .. } if cfg.mode == AgentMode::Full => {
            xdotool(&["mousemove", &x.to_string(), &y.to_string()]).await?;
            is_master.store(true, Ordering::SeqCst);
        }
        _ => {}
    }
    Ok(())
}

async fn clipboard_poll_loop(
    cfg: &AgentConfig,
    out_tx: mpsc::UnboundedSender<String>,
    last_clip_hash: Arc<tokio::sync::Mutex<String>>,
) {
    let poll = Duration::from_millis(cfg.clipboard_poll_ms);
    loop {
        if let Ok(text) = get_clipboard_text().await {
            if !text.is_empty() {
                let hash = hash_text(&text);
                let mut last = last_clip_hash.lock().await;
                if *last != hash {
                    *last = hash.clone();
                    if let Ok(payload) = encode_message(&Message::Clipboard {
                        msg_id: uuid::Uuid::new_v4().to_string(),
                        hash,
                        mime: "text/plain".into(),
                        data: text,
                    }) {
                        let _ = out_tx.send(payload);
                    }
                }
            }
        }
        sleep(poll).await;
    }
}

async fn input_poll_loop(
    cfg: &AgentConfig,
    out_tx: mpsc::UnboundedSender<String>,
    is_master: Arc<AtomicBool>,
) {
    let poll = Duration::from_millis(cfg.input_poll_ms);
    let mut last_pos = (0i32, 0i32);
    loop {
        if let Ok((x, y)) = get_mouse_location().await {
            if (x, y) != last_pos {
                last_pos = (x, y);
                if let Ok(payload) = encode_message(&Message::MasterClaim {
                    node: cfg.node.clone(),
                    ts: 0,
                }) {
                    let _ = out_tx.send(payload);
                }
                let _ = is_master.load(Ordering::SeqCst);
            }
        }
        sleep(poll).await;
    }
}

async fn get_clipboard_text() -> Result<String> {
    let output = Command::new("xclip")
        .args(["-selection", "clipboard", "-o"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await?;
    if !output.status.success() {
        anyhow::bail!("xclip read failed");
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

async fn set_clipboard(text: &str, mime: &str) -> Result<()> {
    if mime != "text/plain" {
        warn!("unsupported clipboard mime: {mime}");
        return Ok(());
    }
    for selection in ["clipboard", "primary"] {
        let mut child = Command::new("xclip")
            .args(["-selection", selection])
            .stdin(Stdio::piped())
            .spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(text.as_bytes()).await?;
        }
        child.wait().await?;
    }
    Ok(())
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
