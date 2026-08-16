//! Raccourcis globaux locaux :
//! - Ctrl+Alt+Shift+P — active/désactive PoolSync sur cette machine
//! - Ctrl+Alt+Shift+M — réclame le master KVM sur cette machine
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

/// Combinaison recommandée : rarement utilisée, mémorable (P = PoolSync).
pub const HOTKEY_LABEL: &str = "Ctrl+Alt+Shift+P";
/// Réclame le clavier/souris sur la machine où on appuie (M = Master).
pub const HOTKEY_MASTER_LABEL: &str = "Ctrl+Alt+Shift+M";

/// Ignore les doubles événements (Pressed+Released ou auto-repeat).
const TOGGLE_DEBOUNCE: Duration = Duration::from_millis(600);

fn mods_cas() -> Option<Modifiers> {
    Some(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT)
}

fn toggle_hotkey() -> HotKey {
    HotKey::new(mods_cas(), Code::KeyP)
}

fn master_hotkey() -> HotKey {
    HotKey::new(mods_cas(), Code::KeyM)
}

pub fn spawn_hotkey_listener(state: Arc<AgentState>) {
    thread::Builder::new()
        .name("poolsync-hotkey".into())
        .spawn(move || hotkey_loop(state))
        .map(|_| info!("raccourcis globaux {HOTKEY_LABEL} / {HOTKEY_MASTER_LABEL} — écoute"))
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
    let toggle = toggle_hotkey();
    let master = master_hotkey();
    if let Err(err) = manager.register(toggle) {
        warn!("enregistrement {HOTKEY_LABEL} échoué: {err:#}");
    } else {
        info!("raccourci {HOTKEY_LABEL} enregistré (toggle PoolSync local)");
    }
    if let Err(err) = manager.register(master) {
        warn!("enregistrement {HOTKEY_MASTER_LABEL} échoué: {err:#}");
    } else {
        info!("raccourci {HOTKEY_MASTER_LABEL} enregistré (réclamer master KVM)");
    }

    let receiver = GlobalHotKeyEvent::receiver();
    let mut last_toggle = Instant::now()
        .checked_sub(TOGGLE_DEBOUNCE)
        .unwrap_or_else(Instant::now);
    let mut last_master = last_toggle;
    loop {
        if let Ok(event) = receiver.try_recv() {
            if event.state != HotKeyState::Pressed {
                continue;
            }
            let now = Instant::now();
            if event.id == toggle.id() {
                if now.duration_since(last_toggle) >= TOGGLE_DEBOUNCE {
                    last_toggle = now;
                    on_toggle(&state);
                }
            } else if event.id == master.id() {
                if now.duration_since(last_master) >= TOGGLE_DEBOUNCE {
                    last_master = now;
                    on_master_claim(&state);
                }
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn on_toggle(state: &AgentState) {
    let active = state.toggle_local_poolsync();
    let node = state.config.node.clone();
    if active {
        info!("PoolSync activé localement ({node}) via {HOTKEY_LABEL}");
    } else {
        info!("PoolSync désactivé localement ({node}) via {HOTKEY_LABEL}");
        crate::kvm_x11::set_cursor_visible_best_effort(true);
    }
    let _ = glib::MainContext::default().invoke(move || {
        notify_util::notify_poolsync_toggle(active, &node);
    });
}

fn on_master_claim(state: &AgentState) {
    let node = state.config.node.clone();
    if !state.kvm_enabled() {
        info!("master claim ignoré ({node}) : KVM inactif");
        let _ = glib::MainContext::default().invoke(move || {
            notify_util::notify_master_claim(&node, false);
        });
        return;
    }
    if !state.local_poolsync_active() {
        state.set_local_poolsync_active(true);
    }
    state.request_master_claim();
    info!("master KVM réclamé localement ({node}) via {HOTKEY_MASTER_LABEL}");
    let _ = glib::MainContext::default().invoke(move || {
        notify_util::notify_master_claim(&node, true);
    });
}
