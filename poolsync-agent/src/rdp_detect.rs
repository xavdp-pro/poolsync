use tokio::process::Command;

/// Client RDP local actif (xfreerdp ou remmina) — la synchro clipboard X11 entre en conflit.
pub async fn rdp_client_active() -> bool {
    if process_matches("xfreerdp", "/v:").await {
        return true;
    }
    if process_matches("remmina", "").await {
        // remmina lance souvent un sous-processus freerdp
        if process_matches("xfreerdp", "/v:").await {
            return true;
        }
        // remmina peut aussi apparaître seul avec connexion active
        if process_matches("remmina", "-c").await || process_matches("remmina", "rdp://").await {
            return true;
        }
    }
    false
}

async fn process_matches(name: &str, needle: &str) -> bool {
    let output = Command::new("pgrep")
        .args(["-af", name])
        .output()
        .await;
    match output {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            if needle.is_empty() {
                !text.trim().is_empty()
            } else {
                text.lines().any(|line| line.contains(needle))
            }
        }
        _ => false,
    }
}
