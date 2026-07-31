use poolsync_core::{AgentConfig, PoolTopology, TopologyNode};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

#[derive(Clone)]
pub struct AgentState {
    pub config: AgentConfig,
    pub config_path: PathBuf,
    connected: Arc<AtomicBool>,
    clipboard_sync: Arc<AtomicBool>,
    notify_on_receive: Arc<AtomicBool>,
    kvm_enabled: Arc<AtomicBool>,
    /// Pause locale (raccourci clavier) — n'affecte que cette machine.
    local_active: Arc<AtomicBool>,
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
    /// Dernier collage entrant (hub ou peer) — évite reboucle poll / double hub+peer.
    last_incoming_apply_at: Arc<RwLock<Option<Instant>>>,
    last_incoming_mime: Arc<RwLock<String>>,
    /// Dernière position injectée (ignore le warp KVM, pas l'activité physique).
    kvm_inject_pos: Arc<RwLock<Option<(i32, i32, Instant)>>>,
    /// Court délai après injection clavier synthétique (xtest).
    kvm_inject_key_until: Arc<RwLock<Option<Instant>>>,
    /// Court délai après injection clic synthétique.
    kvm_inject_button_until: Arc<RwLock<Option<Instant>>>,
    /// Après un SwitchTo local (warp curseur) — ne pas confondre avec activité physique.
    kvm_switch_enter_until: Arc<RwLock<Option<Instant>>>,
    /// Révision hub de l'historique presse-papiers (rafraîchit le menu systray).
    tray_history_revision: Arc<AtomicU64>,
    /// Entrée locale envoyée au hub — affichée tout de suite dans le systray (sans attendre le WS retour).
    optimistic_tray: Arc<RwLock<Option<crate::clipboard_history::HistoryItem>>>,
    /// Dernier hash clipboard connu (évite reboucles poll / echo hub après pick).
    last_clip_hash: Arc<Mutex<String>>,
    /// Après vidage historique : ne pas ré-uploader le presse-papiers local tout de suite.
    history_clear_until: Arc<RwLock<Option<Instant>>>,
    /// Dernier positionnement KVM local (évite doublons SwitchTo).
    last_kvm_enter: Arc<RwLock<Option<(i32, i32, Instant)>>>,
}

fn incoming_grace_elapsed_less_than(
    state: &AgentState,
    grace_for: impl Fn(&str) -> std::time::Duration,
) -> bool {
    let Some(started) = state
        .last_incoming_apply_at
        .read()
        .ok()
        .and_then(|t| *t)
    else {
        return false;
    };
    let mime = state
        .last_incoming_mime
        .read()
        .map(|m| m.clone())
        .unwrap_or_default();
    started.elapsed() < grace_for(&mime)
}

impl AgentState {
    pub fn new(config: AgentConfig, config_path: PathBuf) -> Self {
        let kvm_default = config.kvm_active();
        let local_node = config.node.clone();
        Self {
            config,
            config_path,
            connected: Arc::new(AtomicBool::new(false)),
            clipboard_sync: Arc::new(AtomicBool::new(true)),
            notify_on_receive: Arc::new(AtomicBool::new(true)),
            kvm_enabled: Arc::new(AtomicBool::new(kvm_default)),
            local_active: Arc::new(AtomicBool::new(true)),
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
            last_incoming_apply_at: Arc::new(RwLock::new(None)),
            last_incoming_mime: Arc::new(RwLock::new(String::new())),
            kvm_inject_pos: Arc::new(RwLock::new(None)),
            kvm_inject_key_until: Arc::new(RwLock::new(None)),
            kvm_inject_button_until: Arc::new(RwLock::new(None)),
            kvm_switch_enter_until: Arc::new(RwLock::new(None)),
            tray_history_revision: Arc::new(AtomicU64::new(0)),
            optimistic_tray: Arc::new(RwLock::new(None)),
            last_clip_hash: Arc::new(Mutex::new(String::new())),
            history_clear_until: Arc::new(RwLock::new(None)),
            last_kvm_enter: Arc::new(RwLock::new(None)),
        }
    }

    pub fn last_clip_hash_handle(&self) -> Arc<Mutex<String>> {
        Arc::clone(&self.last_clip_hash)
    }

    pub fn set_last_clip_hash(&self, hash: &str) {
        if let Ok(mut guard) = self.last_clip_hash.lock() {
            *guard = hash.to_string();
        }
    }

    /// Après vidage historique hub : aligner le hash local sans ré-envoyer au hub.
    pub fn mark_history_cleared(&self) {
        const SUPPRESS_MS: u64 = 8000;
        if let Ok(mut until) = self.history_clear_until.write() {
            *until = Some(Instant::now() + std::time::Duration::from_millis(SUPPRESS_MS));
        }
    }

    pub fn history_clear_suppress_active(&self) -> bool {
        self.history_clear_until
            .read()
            .ok()
            .and_then(|u| *u)
            .is_some_and(|deadline| Instant::now() < deadline)
    }

    /// Ignore un SwitchTo local identique reçu en double (hub / nœuds voisins).
    pub fn should_skip_kvm_enter(&self, x: i32, y: i32) -> bool {
        const DEDUP_MS: u128 = 250;
        let now = Instant::now();
        if let Ok(mut slot) = self.last_kvm_enter.write() {
            if let Some((lx, ly, t)) = *slot {
                if lx == x && ly == y && now.duration_since(t).as_millis() < DEDUP_MS {
                    return true;
                }
            }
            *slot = Some((x, y, now));
        }
        false
    }

    /// Affiche immédiatement la copie locale en tête du menu systray.
    pub fn set_optimistic_tray_item(&self, item: crate::clipboard_history::HistoryItem) {
        if let Ok(mut slot) = self.optimistic_tray.write() {
            *slot = Some(item);
        }
        self.notify_tray_history_changed();
    }

    /// Incrémente le compteur local — ne jamais écraser avec la révision hub (évite régressions).
    pub fn notify_tray_history_changed(&self) {
        self.tray_history_revision
            .fetch_add(1, Ordering::SeqCst);
    }

    pub fn clear_optimistic_tray(&self, hash: &str) {
        if let Ok(mut slot) = self.optimistic_tray.write() {
            if slot.as_ref().is_some_and(|i| i.hash == hash) {
                *slot = None;
            }
        }
    }

    pub fn clear_optimistic_tray_all(&self) {
        if let Ok(mut slot) = self.optimistic_tray.write() {
            *slot = None;
        }
    }

    pub fn optimistic_tray_item(&self) -> Option<crate::clipboard_history::HistoryItem> {
        self.optimistic_tray
            .read()
            .ok()
            .and_then(|g| g.clone())
    }

    pub fn tray_history_revision(&self) -> u64 {
        self.tray_history_revision.load(Ordering::SeqCst)
    }

    pub fn mark_kvm_inject_at(&self, x: i32, y: i32) {
        if let Ok(mut pos) = self.kvm_inject_pos.write() {
            *pos = Some((x, y, Instant::now()));
        }
    }

    pub fn mark_kvm_inject_key(&self) {
        const GRACE: std::time::Duration = std::time::Duration::from_millis(40);
        if let Ok(mut until) = self.kvm_inject_key_until.write() {
            *until = Some(Instant::now() + GRACE);
        }
    }

    pub fn mark_kvm_inject_button(&self) {
        const GRACE: std::time::Duration = std::time::Duration::from_millis(50);
        if let Ok(mut until) = self.kvm_inject_button_until.write() {
            *until = Some(Instant::now() + GRACE);
        }
    }

    pub fn mark_kvm_switch_enter(&self) {
        const GRACE: std::time::Duration = std::time::Duration::from_millis(450);
        if let Ok(mut until) = self.kvm_switch_enter_until.write() {
            *until = Some(Instant::now() + GRACE);
        }
    }

    pub fn switch_enter_grace_active(&self) -> bool {
        self.kvm_switch_enter_until
            .read()
            .ok()
            .and_then(|u| *u)
            .is_some_and(|t| t > Instant::now())
    }

    /// Pilotage KVM distant actif (injections récentes sur cette machine).
    pub fn remote_drive_active(&self) -> bool {
        const GRACE: std::time::Duration = std::time::Duration::from_millis(350);
        self.kvm_inject_pos
            .read()
            .ok()
            .and_then(|p| *p)
            .map(|(_, _, t)| t.elapsed() < GRACE)
            .unwrap_or(false)
    }

    /// Le mouvement souris peut reprendre le master (pas un warp / injection KVM).
    pub fn motion_claim_allowed(&self, local: &str, px: i32, py: i32) -> bool {
        if self.remote_drive_active() {
            return false;
        }
        if self.kvm_focus() != local {
            return false;
        }
        if self.switch_enter_grace_active() {
            return false;
        }
        const INJECT_GRACE: std::time::Duration = std::time::Duration::from_millis(300);
        const DIST_MIN: i32 = 20;
        if let Ok(pos) = self.kvm_inject_pos.read() {
            if let Some((ix, iy, t)) = *pos {
                if t.elapsed() < INJECT_GRACE {
                    let dist = (px - ix).abs() + (py - iy).abs();
                    if dist < DIST_MIN {
                        return false;
                    }
                }
            }
        }
        true
    }

    pub fn inject_blocks_key_claim(&self) -> bool {
        self.kvm_inject_key_until
            .read()
            .ok()
            .and_then(|u| *u)
            .is_some_and(|t| t > Instant::now())
    }

    pub fn inject_blocks_button_claim(&self) -> bool {
        self.kvm_inject_button_until
            .read()
            .ok()
            .and_then(|u| *u)
            .is_some_and(|t| t > Instant::now())
    }

    pub fn note_kvm_inject(&self, kind: &poolsync_core::InputKind) {
        use poolsync_core::InputKind;
        match kind {
            InputKind::MouseMove { x, y } | InputKind::MouseWheel { x, y, .. } => {
                self.mark_kvm_inject_at(*x, *y);
            }
            InputKind::MouseButton { x, y, .. } => {
                self.mark_kvm_inject_at(*x, *y);
                self.mark_kvm_inject_button();
            }
            InputKind::Key { .. } => self.mark_kvm_inject_key(),
            InputKind::MouseMoveRelative { .. } => {}
        }
    }

    pub fn mark_incoming_clipboard_applied(&self, mime: &str) {
        if let Ok(mut t) = self.last_incoming_apply_at.write() {
            *t = Some(Instant::now());
        }
        if let Ok(mut m) = self.last_incoming_mime.write() {
            *m = mime.to_string();
        }
    }

    /// Alias RDP / legacy.
    pub fn mark_hub_clipboard_applied(&self) {
        self.mark_incoming_clipboard_applied("text/plain");
    }

    /// Après collage entrant : ne pas relayer immédiatement ce qu'on vient d'écrire.
    pub fn incoming_poll_suppress_active(&self) -> bool {
        incoming_grace_elapsed_less_than(self, |mime| {
            if mime.starts_with("image/") {
                // Long enough for xclip/GTK settle + peer echo without re-send.
                std::time::Duration::from_millis(4000)
            } else {
                std::time::Duration::from_millis(900)
            }
        })
    }

    /// Hub + peer livrent parfois la même image deux fois (hash différent).
    pub fn incoming_duplicate_suppress_active(&self) -> bool {
        incoming_grace_elapsed_less_than(self, |mime| {
            if mime.starts_with("image/") {
                std::time::Duration::from_millis(5000)
            } else {
                std::time::Duration::from_millis(1200)
            }
        })
    }

    /// Court délai après un collage hub : laisse cliprdr RDP digérer le clipboard X11.
    pub fn hub_apply_grace_active(&self) -> bool {
        self.incoming_poll_suppress_active()
    }

    /// Alias legacy.
    pub fn incoming_apply_grace_active(&self) -> bool {
        self.incoming_poll_suppress_active()
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


    pub fn local_poolsync_active(&self) -> bool {
        self.local_active.load(Ordering::SeqCst)
    }

    pub fn set_local_poolsync_active(&self, value: bool) {
        self.local_active.store(value, Ordering::SeqCst);
    }

    /// Bascule KVM + presse-papiers sur cette machine (raccourci global).
    pub fn toggle_local_poolsync(&self) -> bool {
        let new = !self.local_poolsync_active();
        self.set_local_poolsync_active(new);
        new
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

    /// Notifie à la réception si le hash clipboard est nouveau (anti-rafale hub+peer).
    pub fn should_notify(&self, hash: &str, preview: &str) -> bool {
        if !self.notify_enabled() {
            return false;
        }
        const MIN_INTERVAL: std::time::Duration = std::time::Duration::from_millis(2000);
        if let Ok(last_at) = self.last_notify_at.read() {
            if last_at
                .is_some_and(|t| t.elapsed() < MIN_INTERVAL)
            {
                return false;
            }
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
