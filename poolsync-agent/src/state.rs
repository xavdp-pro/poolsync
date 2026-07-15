use poolsync_core::{AgentConfig, PoolTopology, TopologyNode};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

#[derive(Clone)]
pub struct AgentState {
    pub config: AgentConfig,
    connected: Arc<AtomicBool>,
    clipboard_sync: Arc<AtomicBool>,
    notify_on_receive: Arc<AtomicBool>,
    kvm_enabled: Arc<AtomicBool>,
    master_node: Arc<RwLock<String>>,
    kvm_focus: Arc<RwLock<String>>,
    kvm_input_node: Arc<RwLock<String>>,
    topology: Arc<RwLock<Option<PoolTopology>>>,
    last_clip_preview: Arc<RwLock<String>>,
    last_clip_at: Arc<RwLock<Option<Instant>>>,
    last_error: Arc<RwLock<Option<String>>>,
    last_notified_hash: Arc<RwLock<String>>,
    last_notified_preview: Arc<RwLock<String>>,
    last_notify_at: Arc<RwLock<Option<Instant>>>,
    /// Dernier collage PoolSync depuis le hub (cohabitation cliprdr RDP).
    last_hub_apply_at: Arc<RwLock<Option<Instant>>>,
    /// Ignore les mouvements souris causés par injection KVM distante.
    kvm_inject_until: Arc<RwLock<Option<Instant>>>,
}

impl AgentState {
    pub fn new(config: AgentConfig) -> Self {
        let kvm_default = config.kvm_active();
        let local_node = config.node.clone();
        Self {
            config,
            connected: Arc::new(AtomicBool::new(false)),
            clipboard_sync: Arc::new(AtomicBool::new(true)),
            notify_on_receive: Arc::new(AtomicBool::new(true)),
            kvm_enabled: Arc::new(AtomicBool::new(kvm_default)),
            master_node: Arc::new(RwLock::new(String::from("—"))),
            kvm_focus: Arc::new(RwLock::new(local_node.clone())),
            kvm_input_node: Arc::new(RwLock::new(local_node)),
            topology: Arc::new(RwLock::new(None)),
            last_clip_preview: Arc::new(RwLock::new(String::new())),
            last_clip_at: Arc::new(RwLock::new(None)),
            last_error: Arc::new(RwLock::new(None)),
            last_notified_hash: Arc::new(RwLock::new(String::new())),
            last_notified_preview: Arc::new(RwLock::new(String::new())),
            last_notify_at: Arc::new(RwLock::new(None)),
            last_hub_apply_at: Arc::new(RwLock::new(None)),
            kvm_inject_until: Arc::new(RwLock::new(None)),
        }
    }

    pub fn mark_kvm_inject(&self) {
        const GRACE: std::time::Duration = std::time::Duration::from_millis(120);
        if let Ok(mut until) = self.kvm_inject_until.write() {
            *until = Some(Instant::now() + GRACE);
        }
    }

    pub fn kvm_inject_grace_active(&self) -> bool {
        self.kvm_inject_until
            .read()
            .ok()
            .and_then(|u| *u)
            .is_some_and(|t| t > Instant::now())
    }

    pub fn mark_hub_clipboard_applied(&self) {
        if let Ok(mut t) = self.last_hub_apply_at.write() {
            *t = Some(Instant::now());
        }
    }

    /// Court délai après un collage hub : laisse cliprdr RDP digérer le clipboard X11.
    pub fn hub_apply_grace_active(&self) -> bool {
        const GRACE: std::time::Duration = std::time::Duration::from_millis(900);
        self.last_hub_apply_at
            .read()
            .ok()
            .and_then(|t| *t)
            .is_some_and(|t| t.elapsed() < GRACE)
    }

    pub fn set_connected(&self, value: bool) {
        self.connected.store(value, Ordering::SeqCst);
        if value {
            if let Ok(mut err) = self.last_error.write() {
                *err = None;
            }
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    pub fn clipboard_sync_enabled(&self) -> bool {
        self.clipboard_sync.load(Ordering::SeqCst)
    }

    pub fn set_clipboard_sync(&self, value: bool) {
        self.clipboard_sync.store(value, Ordering::SeqCst);
    }

    pub fn toggle_clipboard_sync(&self) -> bool {
        let new = !self.clipboard_sync_enabled();
        self.set_clipboard_sync(new);
        new
    }

    pub fn notify_enabled(&self) -> bool {
        self.notify_on_receive.load(Ordering::SeqCst)
    }

    pub fn set_notify(&self, value: bool) {
        self.notify_on_receive.store(value, Ordering::SeqCst);
    }

    pub fn toggle_notify(&self) -> bool {
        let new = !self.notify_enabled();
        self.set_notify(new);
        new
    }

    pub fn kvm_enabled(&self) -> bool {
        self.kvm_enabled.load(Ordering::SeqCst)
    }

    pub fn set_kvm_enabled(&self, value: bool) {
        self.kvm_enabled.store(value, Ordering::SeqCst);
    }

    pub fn toggle_kvm(&self) -> bool {
        let new = !self.kvm_enabled();
        self.set_kvm_enabled(new);
        new
    }

    pub fn set_topology(&self, topology: PoolTopology) {
        if let Ok(mut t) = self.topology.write() {
            *t = Some(topology);
        }
    }

    pub fn topology(&self) -> Option<PoolTopology> {
        self.topology.read().ok().and_then(|t| t.clone())
    }

    pub fn topology_node(&self, name: &str) -> Option<TopologyNode> {
        self.topology().and_then(|t| t.nodes.get(name).cloned())
    }

    pub fn target_kvm_enabled(&self, node: &str) -> bool {
        if node == self.config.node {
            return self.kvm_enabled();
        }
        self.topology_node(node)
            .map(|n| n.kvm_enabled)
            .unwrap_or(true)
    }

    pub fn set_kvm_focus(&self, node: &str) {
        if let Ok(mut f) = self.kvm_focus.write() {
            *f = node.to_string();
        }
    }

    pub fn kvm_focus(&self) -> String {
        self.kvm_focus
            .read()
            .map(|f| f.clone())
            .unwrap_or_else(|_| self.config.node.clone())
    }

    pub fn set_kvm_input_node(&self, node: &str) {
        if let Ok(mut n) = self.kvm_input_node.write() {
            *n = node.to_string();
        }
    }

    pub fn kvm_input_node(&self) -> String {
        self.kvm_input_node
            .read()
            .map(|n| n.clone())
            .unwrap_or_else(|_| self.config.node.clone())
    }

    pub fn is_input_owner(&self) -> bool {
        self.kvm_input_node() == self.config.node
    }

    /// Notifie à la réception si le hash clipboard est nouveau.
    pub fn should_notify(&self, hash: &str, preview: &str) -> bool {
        if !self.notify_enabled() {
            return false;
        }
        let Ok(mut last_hash) = self.last_notified_hash.write() else {
            return false;
        };
        if *last_hash == hash {
            return false;
        }
        *last_hash = hash.to_string();
        if let Ok(mut last_preview) = self.last_notified_preview.write() {
            *last_preview = preview.to_string();
        }
        if let Ok(mut last_at) = self.last_notify_at.write() {
            *last_at = Some(Instant::now());
        }
        true
    }

    pub fn set_master(&self, node: &str) {
        if let Ok(mut m) = self.master_node.write() {
            *m = node.to_string();
        }
    }

    pub fn master_node(&self) -> String {
        self.master_node
            .read()
            .map(|m| m.clone())
            .unwrap_or_else(|_| "—".into())
    }

    pub fn set_error(&self, msg: Option<String>) {
        if let Ok(mut err) = self.last_error.write() {
            *err = msg;
        }
    }

    pub fn last_error(&self) -> Option<String> {
        self.last_error.read().ok().and_then(|e| e.clone())
    }

    pub fn record_clip_received(&self, preview: String) {
        if let Ok(mut p) = self.last_clip_preview.write() {
            *p = preview;
        }
        if let Ok(mut t) = self.last_clip_at.write() {
            *t = Some(Instant::now());
        }
    }

    pub fn last_clip_preview(&self) -> String {
        self.last_clip_preview
            .read()
            .map(|p| p.clone())
            .unwrap_or_default()
    }

    pub fn last_clip_ago_secs(&self) -> Option<u64> {
        self.last_clip_at
            .read()
            .ok()
            .and_then(|t| t.map(|i| i.elapsed().as_secs()))
    }

    pub fn status_line(&self) -> String {
        if self.is_connected() {
            "● Connecté — tout OK".into()
        } else if let Some(err) = self.last_error() {
            format!("● Reconnexion… — {err}")
        } else {
            "● Reconnexion VPN/hub…".into()
        }
    }

    pub fn hub_display(&self) -> String {
        self.config
            .hub_url
            .trim_start_matches("ws://")
            .trim_start_matches("wss://")
            .trim_end_matches("/ws")
            .to_string()
    }
}

pub fn clip_preview(text: &str) -> String {
    let one_line: String = text.chars().take(80).collect();
    if text.len() > 80 {
        format!("{one_line}…")
    } else {
        one_line
    }
}

pub fn clip_preview_mime(mime: &str, data: &str) -> String {
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
        clip_preview(data)
    }
}

pub fn format_time_ago(secs: u64) -> String {
    if secs < 5 {
        "à l'instant".into()
    } else if secs < 60 {
        format!("il y a {secs}s")
    } else {
        format!("il y a {}min", secs / 60)
    }
}
