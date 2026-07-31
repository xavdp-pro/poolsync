use std::process::{Command, Stdio};
use std::time::Duration;
use tracing::warn;

pub fn notify_icon_path() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    format!("{home}/.local/share/poolsync/poolsync-tray.png")
}

fn session_env(cmd: &mut Command) {
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
}

/// Relance xfce4-notifyd si absent (sinon notify-send timeout / exit 1).
pub fn ensure_notify_daemon() {
    if Command::new("pgrep")
        .args(["-x", "xfce4-notifyd"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return;
    }
    let candidates = [
        "/usr/lib/x86_64-linux-gnu/xfce4/notifyd/xfce4-notifyd",
        "/usr/lib/xfce4/notifyd/xfce4-notifyd",
    ];
    for path in candidates {
        if !std::path::Path::new(path).is_file() {
            continue;
        }
        let mut cmd = Command::new(path);
        session_env(&mut cmd);
        cmd.stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        match cmd.spawn() {
            Ok(_) => {
                std::thread::sleep(Duration::from_millis(400));
                return;
            }
            Err(err) => warn!("démarrage xfce4-notifyd ({path}): {err}"),
        }
    }
    // Fallback systemd user unit si présent.
    let mut cmd = Command::new("systemctl");
    session_env(&mut cmd);
    let _ = cmd
        .args(["--user", "start", "xfce4-notifyd.service"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    std::thread::sleep(Duration::from_millis(400));
}

/// Notification desktop locale (notify-send sous Linux).
pub fn notify_local(title: &str, body: &str) {
    ensure_notify_daemon();
    let icon = notify_icon_path();
    let icon_arg = if std::path::Path::new(&icon).is_file() {
        icon
    } else {
        "dialog-information".into()
    };
    let mut cmd = Command::new("timeout");
    session_env(&mut cmd);
    cmd.args([
        "3",
        "notify-send",
        "-a",
        "com.xavdp.poolsync",
        "-i",
        &icon_arg,
        "-t",
        "5000",
        "-u",
        "normal",
        title,
        body,
    ])
    .stdout(Stdio::null())
    .stderr(Stdio::piped());

    match cmd.output() {
        Ok(out) if out.status.success() => {}
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            warn!(
                "notify-send exit {:?} — {}",
                out.status.code(),
                err.trim()
            );
        }
        Err(err) => warn!("notify-send: {err}"),
    }
}
