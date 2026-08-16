//! Raccourci global local : Ctrl+Alt+Shift+P — active/désactive PoolSync sur cette machine.
use crate::notify_util;
use crate::state::AgentState;
use global_hotkey::{
    hotkey::{Code, HotKey, Modifiers},
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// Combinaison recommandée : rarement utilisée par le OS / les apps, mémorable (P = PoolSync).
pub const HOTKEY_LABEL: &str = "Ctrl+Alt+Shift+P";

/// Ignore les doubles événements (Pressed+Released ou auto-repeat) pour un vrai toggle.
const TOGGLE_DEBOUNCE: Duration = Duration::from_millis(600);

fn default_hotkey() -> HotKey {
    HotKey::new(
        Some(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT),
        Code::KeyP,
    )
}

pub fn spawn_hotkey_listener(state: Arc<AgentState>) {
    thread::Builder::new()
        .name("poolsync-hotkey".into())
        .spawn(move || hotkey_loop(state))
        .map(|_| info!("raccourci global {HOTKEY_LABEL} — écoute"))
        .unwrap_or_else(|err| warn!("impossible de lancer le thread hotkey: {err}"));
}

fn hotkey_loop(state: Arc<AgentState>) {
    let manager = match GlobalHotKeyManager::new() {
        Ok(m) => m,
        Err(err) => {
            warn!("raccourci global indisponible: {err:#}");
            return;
        }
    };
    let hotkey = default_hotkey();
    if let Err(err) = manager.register(hotkey) {
        warn!("enregistrement {HOTKEY_LABEL} échoué: {err:#}");
        return;
    }
    info!("raccourci {HOTKEY_LABEL} enregistré (toggle PoolSync local)");

    let receiver = GlobalHotKeyEvent::receiver();
    let mut last_toggle = Instant::now()
        .checked_sub(TOGGLE_DEBOUNCE)
        .unwrap_or_else(Instant::now);
    loop {
        if let Ok(event) = receiver.try_recv() {
            // X11 envoie Pressed puis Released — un seul toggle par appui.
            if event.id == hotkey.id() && event.state == HotKeyState::Pressed {
                let now = Instant::now();
                if now.duration_since(last_toggle) >= TOGGLE_DEBOUNCE {
                    last_toggle = now;
                    on_hotkey(&state);
                } else {
                    tracing::debug!("raccourci ignoré (debounce / double événement)");
                }
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn on_hotkey(state: &AgentState) {
    let active = state.toggle_local_poolsync();
    let node = state.config.node.clone();
    if active {
        info!("PoolSync activé localement ({node}) via {HOTKEY_LABEL}");
    } else {
        info!("PoolSync désactivé localement ({node}) via {HOTKEY_LABEL}");
        // Libère le curseur si KVM avait un grab actif.
        crate::kvm_x11::set_cursor_visible_best_effort(true);
    }
    // Notification sur le thread GTK (session D-Bus / DISPLAY fiables).
    let _ = glib::MainContext::default().invoke(move || {
        notify_util::notify_poolsync_toggle(active, &node);
    });
}
