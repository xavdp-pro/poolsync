mod agent;
mod clipboard;
mod config_window;
mod kvm;
mod kvm_input;
mod kvm_x11;
mod logs_viewer;
mod network;
mod notify_thumb;
mod rdp_detect;
mod single;
mod state;
mod tray;

use agent::run_agent;
use anyhow::{Context, Result};
use clap::Parser;
use poolsync_core::AgentConfig;
use state::AgentState;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use tracing::{info, warn};

const RECONNECT_INITIAL: Duration = Duration::from_secs(2);
const RECONNECT_MAX: Duration = Duration::from_secs(30);

#[derive(Parser, Debug)]
#[command(
    name = "poolsync-agent",
    about = "PoolSync agent — client presse-papiers + KVM"
)]
struct Args {
    #[arg(long, default_value = "/etc/poolsync/agent.toml")]
    config: PathBuf,

    #[arg(long)]
    no_tray: bool,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "poolsync_agent=info".into()),
        )
        .init();

    let args = Args::parse();
    let _instance = match single::InstanceLock::acquire() {
        Ok(lock) => lock,
        Err(_) => {
            tracing::info!("poolsync-agent déjà actif — sortie");
            return Ok(());
        }
    };
    let raw = std::fs::read_to_string(&args.config)
        .with_context(|| format!("read config {}", args.config.display()))?;
    let cfg: AgentConfig = toml::from_str(&raw).context("parse agent config")?;
    let state = Arc::new(AgentState::new(cfg.clone()));

    info!(
        "starting agent node={} hub={} mode={:?}",
        cfg.node, cfg.hub_url, cfg.mode
    );

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("tokio runtime")?;

    let state_agent = state.clone();
    rt.spawn(async move {
        let mut backoff = RECONNECT_INITIAL;
        loop {
            match run_agent(state_agent.clone()).await {
                Ok(()) => {
                    state_agent.set_error(None);
                    backoff = RECONNECT_INITIAL;
                    warn!("session hub terminée — reconnexion…");
                }
                Err(err) => {
                    state_agent.set_connected(false);
                    state_agent.set_error(Some(err.to_string()));
                    tracing::error!("agent session ended: {err:#}");
                }
            }
            info!("nouvelle tentative hub dans {}s", backoff.as_secs());
            sleep(backoff).await;
            backoff = std::cmp::min(backoff * 2, RECONNECT_MAX);
        }
    });

    let show_tray = !args.no_tray && std::env::var("DISPLAY").is_ok();
    if show_tray {
        info!("starting systray");
        if let Err(err) = tray::run_tray(state.clone()) {
            tracing::error!("systray failed ({err:#}), agent continues without tray");
            rt.block_on(async { std::future::pending::<()>().await });
        }
    } else {
        rt.block_on(async { std::future::pending::<()>().await });
    }

    Ok(())
}
