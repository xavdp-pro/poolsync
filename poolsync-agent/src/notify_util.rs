use std::process::{Command, Stdio};
use std::time::Duration;
use tracing::{info, warn};

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

/// True if a non-zombie xfce4-notifyd is running.
fn notifyd_process_alive() -> bool {
    let Ok(output) = Command::new("ps")
        .args(["-C", "xfce4-notifyd", "-o", "stat="])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .any(|stat| {
            let s = stat.trim();
            !s.is_empty() && !s.starts_with('Z')
        })
}

/// True if org.freedesktop.Notifications answers on the session bus.
fn notifications_dbus_ok() -> bool {
    let mut cmd = Command::new("timeout");
    session_env(&mut cmd);
    cmd.args([
        "1",
        "busctl",
        "--user",
        "call",
        "org.freedesktop.Notifications",
        "/org/freedesktop/Notifications",
        "org.freedesktop.Notifications",
        "GetServerInformation",
    ])
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status()
    .map(|s| s.success())
    .unwrap_or(false)
}

/// Relance xfce4-notifyd si absent / zombie (sinon notify-send hang / exit 1).
///
/// Important: never spawn notifyd as a direct child of poolsync-agent — if it
/// dies unreaped it becomes a zombie, `pgrep` still matches, and we never restart.
pub fn ensure_notify_daemon() {
    if notifications_dbus_ok() {
        return;
    }
    if notifyd_process_alive() && notifications_dbus_ok() {
        return;
    }

    warn!("xfce4-notifyd absent ou mort — relance");

    // Prefer systemd user unit (reparents under user@.service, no zombie under agent).
    let mut cmd = Command::new("systemctl");
    session_env(&mut cmd);
    let _ = cmd
        .args(["--user", "reset-failed", "xfce4-notifyd.service"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let mut cmd = Command::new("systemctl");
    session_env(&mut cmd);
    let _ = cmd
        .args(["--user", "restart", "xfce4-notifyd.service"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    std::thread::sleep(Duration::from_millis(500));
    if notifications_dbus_ok() {
        info!("xfce4-notifyd relancé via systemd");
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
        // Detach via bash so notifyd is not our child (avoids zombie under agent).
        let mut cmd = Command::new("bash");
        session_env(&mut cmd);
        cmd.args([
            "-c",
            &format!("nohup {path} >/dev/null 2>&1 </dev/null &"),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
        match cmd.status() {
            Ok(s) if s.success() => {
                std::thread::sleep(Duration::from_millis(500));
                if notifications_dbus_ok() {
                    info!("xfce4-notifyd relancé ({path})");
                    return;
                }
            }
            Ok(_) => warn!("démarrage xfce4-notifyd ({path}): exit non-zéro"),
            Err(err) => warn!("démarrage xfce4-notifyd ({path}): {err}"),
        }
    }
}

/// Notification desktop locale (notify-send sous Linux).
pub fn notify_local(title: &str, body: &str) {
    let _ = notify_send(title, body, "normal", 5000);
}

/// Notification visible pour le raccourci Ctrl+Alt+Shift+P (suspend / resume).
pub fn notify_poolsync_toggle(active: bool, node: &str) {
    const HOTKEY: &str = "Ctrl+Alt+Shift+P";
    let (title, body, urgency, timeout_ms) = if active {
        (
            "PoolSync — ACTIVÉ",
            format!(
                "PoolSync réactivé sur {node}\n\
                 KVM + presse-papiers synchronisés.\n\
                 {HOTKEY} pour suspendre."
            ),
            "normal",
            6000u32,
        )
    } else {
        (
            "PoolSync — DÉSACTIVÉ",
            format!(
                "PoolSync suspendu sur {node}\n\
                 KVM et presse-papiers réseau coupés sur cette machine.\n\
                 {HOTKEY} pour réactiver."
            ),
            "critical",
            10000u32,
        )
    };
    if notify_send(title, &body, urgency, timeout_ms) {
        return;
    }
    warn!("notify-send toggle échoué — repli zenity");
    notify_zenity_fallback(title, &body);
}

fn notify_send(title: &str, body: &str, urgency: &str, timeout_ms: u32) -> bool {
    ensure_notify_daemon();
    let icon = notify_icon_path();
    let icon_arg = if std::path::Path::new(&icon).is_file() {
        icon
    } else {
        "dialog-information".into()
    };
    let timeout_secs = ((timeout_ms + 999) / 1000).max(1).to_string();
    let mut cmd = Command::new("timeout");
    session_env(&mut cmd);
    cmd.args([
        &timeout_secs,
        "notify-send",
        "-a",
        "com.xavdp.poolsync",
        "-i",
        &icon_arg,
        "-t",
        &timeout_ms.to_string(),
        "-u",
        urgency,
    ])
    .arg(title)
    .arg(body)
    .stdout(Stdio::null())
    .stderr(Stdio::piped());

    match cmd.output() {
        Ok(out) if out.status.success() => true,
        Ok(out) => {
            let err = String::from_utf8_lossy(&out.stderr);
            warn!(
                "notify-send exit {:?} — {}",
                out.status.code(),
                err.trim()
            );
            false
        }
        Err(err) => {
            warn!("notify-send: {err}");
            false
        }
    }
}

fn notify_zenity_fallback(title: &str, body: &str) {
    let mut cmd = Command::new("zenity");
    session_env(&mut cmd);
    cmd.args(["--info", "--title", title, "--text", body, "--width", "420", "--timeout", "8"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if cmd.spawn().is_err() {
        warn!("zenity indisponible — notification toggle non affichée");
    }
}
