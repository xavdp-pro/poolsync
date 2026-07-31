use std::process::{Command, Stdio};
use tracing::warn;

pub fn notify_icon_path() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    format!("{home}/.local/share/poolsync/poolsync-tray.png")
}

/// Notification desktop locale (notify-send sous Linux).
pub fn notify_local(title: &str, body: &str) {
    let icon = notify_icon_path();
    let args = [
        "-a",
        "com.xavdp.poolsync",
        "-i",
        &icon,
        "-t",
        "4000",
        "-u",
        "normal",
        title,
        body,
    ];
    let mut cmd = Command::new("notify-send");
    cmd.args(args).stdout(Stdio::null()).stderr(Stdio::piped());
    for key in [
        "DISPLAY",
        "DBUS_SESSION_BUS_ADDRESS",
        "XAUTHORITY",
        "XDG_RUNTIME_DIR",
        "XDG_CURRENT_DESKTOP",
    ] {
        if let Ok(val) = std::env::var(key) {
            cmd.env(key, val);
        }
    }
    if let Err(err) = cmd.status() {
        warn!("notify-send: {err}");
    }
}
