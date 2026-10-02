//! User-owned local IPC opens the configuration of the running agent.

use crate::state::AgentState;
use anyhow::{Context, Result};
use std::os::unix::{fs::PermissionsExt, net::UnixDatagram};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

fn socket_path() -> PathBuf {
    PathBuf::from(
        std::env::var("XDG_RUNTIME_DIR")
            .unwrap_or_else(|_| format!("/run/user/{}", unsafe { libc::getuid() })),
    )
    .join("poolsync-agent.control")
}

pub fn request_window(config: &Path) -> Result<()> {
    let socket = UnixDatagram::unbound()?;
    socket
        .connect(socket_path())
        .context("running agent control unavailable")?;
    let path = std::fs::canonicalize(config)?;
    socket.send(path.as_os_str().as_encoded_bytes())?;
    Ok(())
}

/// Called only after acquiring the user runtime's instance lock.
pub fn spawn(state: Arc<AgentState>) -> Result<()> {
    let path = socket_path();
    let _ = std::fs::remove_file(&path);
    let socket = UnixDatagram::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    socket.set_read_timeout(Some(Duration::from_secs(5)))?;
    let expected = std::fs::canonicalize(&state.config_path)?;
    std::thread::Builder::new()
        .name("local-window-control".into())
        .spawn(move || {
            let mut bytes = [0_u8; 4096];
            loop {
                match socket.recv(&mut bytes) {
                    Ok(n) if bytes[..n] == *expected.as_os_str().as_encoded_bytes() => {
                        state.request_config_window();
                    }
                    Ok(_) => {}
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) => {}
                    Err(error) => {
                        tracing::warn!("local window control stopped: {error}");
                        break;
                    }
                }
            }
        })?;
    Ok(())
}
