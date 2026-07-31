use crate::clipboard_history::{self, HistoryItem};
use crate::config_window;
use crate::logs_viewer;
use crate::notify_util;
use crate::state::AgentState;
use anyhow::{Context, Result};
use glib::ControlFlow;
use libappindicator::{AppIndicator, AppIndicatorStatus};
use muda::{
    CheckMenuItem, ContextMenu, IconMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem,
    Submenu,
};
use std::cell::{Cell, RefCell};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const ID_OPTIONS: &str = "options";
const ID_STATUS: &str = "status";
const ID_NODE: &str = "node";
const ID_HUB: &str = "hub";
const ID_MASTER: &str = "master";
const ID_CLIP_SYNC: &str = "clip_sync";
const ID_NOTIFY: &str = "notify";
const ID_KVM: &str = "kvm";
const ID_CLIP_HISTORY: &str = "clip_history";
const ID_CLIP_CLEAR: &str = "clip_clear";
const ID_CONFIG: &str = "config";
const ID_VIEW_LOGS: &str = "view_logs";
const ID_RESTART: &str = "restart";
const ID_QUIT: &str = "quit";
const CLIP_ID_PREFIX: &str = "clip:";

struct TrayUi {
    root_menu: Menu,
    last_tray_revision: RefCell<u64>,
    last_tray_fingerprint: RefCell<String>,
    status: MenuItem,
    node: MenuItem,
    hub: MenuItem,
    master: MenuItem,
    clip_sync: CheckMenuItem,
    notify: CheckMenuItem,
    kvm: Option<CheckMenuItem>,
}

pub fn run_tray(state: Arc<AgentState>) -> Result<()> {
    let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<Result<()>>(1);
    let state_thread = state.clone();
    let handle = std::thread::Builder::new()
        .name("poolsync-tray".into())
        .spawn(move || run_tray_gtk(state_thread, ready_tx))
        .context("spawn tray thread")?;

    ready_rx
        .recv()
        .context("systray ready signal")?
        .context("systray init")?;
    match handle.join() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(err)) => Err(err),
        Err(_) => Err(anyhow::anyhow!("tray thread panicked")),
    }
}

fn run_tray_gtk(
    state: Arc<AgentState>,
    ready_tx: std::sync::mpsc::SyncSender<Result<()>>,
) -> Result<()> {
    gtk::init().map_err(|e| anyhow::anyhow!("gtk init: {e}"))?;
    let ui = Arc::new(build_menu(&state)?);
    refresh_clipboard_items(&ui, &state);

    let app_id = format!("com.xavdp.poolsync.{}", state.config.node);
    let mut indicator = AppIndicator::new(&app_id, "poolsync");
    indicator.set_status(AppIndicatorStatus::Active);

    let (icon_dir, icon_path) = install_icon_png()?;
    indicator.set_icon_theme_path(&icon_dir.to_string_lossy());
    indicator.set_icon_full(&icon_path.to_string_lossy(), "PoolSync");
    // Une seule liaison menu ↔ indicateur (ne pas rappeler set_menu : casse MenuEvent muda).
    indicator.set_menu(&mut ui.root_menu.gtk_context_menu());

    let gtk_ctx = glib::MainContext::ref_thread_default();
    let state_events = state.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let id = event.id().0.as_str();
        if let Some(hash) = id.strip_prefix(CLIP_ID_PREFIX) {
            if hash == "empty" {
                return;
            }
            let st = state_events.clone();
            let hash = hash.to_string();
            std::thread::spawn(move || {
                if let Err(err) = clipboard_history::pick_and_paste(&st, &hash) {
                    tracing::warn!("clipboard pick from tray: {err:#}");
                }
            });
            return;
        }
        match id {
            ID_CLIP_SYNC => {
                let on = apply_clipboard_sync_toggle(&state_events);
                tracing::info!("clipboard sync: {on}");
            }
            ID_NOTIFY => {
                let on = state_events.toggle_notify();
                tracing::info!("notify on receive: {on}");
            }
            ID_KVM => {
                let on = state_events.toggle_kvm();
                tracing::info!("kvm enabled: {on}");
            }
            ID_CLIP_HISTORY => {
                let ctx = gtk_ctx.clone();
                let st = state_events.clone();
                let _ = ctx.invoke(move || clipboard_history::show(st));
            }
            ID_CLIP_CLEAR => {
                let ctx = gtk_ctx.clone();
                let st = state_events.clone();
                let _ = ctx.invoke(move || clipboard_history::confirm_clear_from_tray(st));
            }
            ID_CONFIG => {
                let ctx = gtk_ctx.clone();
                let st = state_events.clone();
                let _ = ctx.invoke(move || config_window::show(st));
            }
            ID_VIEW_LOGS => {
                let ctx = gtk_ctx.clone();
                let node = state_events.config.node.clone();
                let _ = ctx.invoke(move || logs_viewer::show(&node));
            }
            ID_RESTART => {
                tracing::info!("systray: redémarrage demandé");
                std::thread::spawn(|| {
                    let _ = std::process::Command::new("systemctl")
                        .args(["--user", "restart", "poolsync-agent.service"])
                        .status();
                });
            }
            ID_QUIT => {
                tracing::info!("systray: arrêt demandé");
                std::thread::spawn(|| {
                    let _ = std::process::Command::new("systemctl")
                        .args(["--user", "stop", "poolsync-agent.service"])
                        .status();
                });
            }
            _ => {}
        }
    }));

    let ui_refresh = ui.clone();
    let state_refresh = state.clone();
    let tick = std::rc::Rc::new(Cell::new(0u32));
    let tick_refresh = tick.clone();
    glib::timeout_add_local(Duration::from_millis(120), move || {
        let rev = state_refresh.tray_history_revision();
        if rev != *ui_refresh.last_tray_revision.borrow() {
            *ui_refresh.last_tray_revision.borrow_mut() = rev;
            refresh_clipboard_items(&ui_refresh, &state_refresh);
        }
        let n = tick_refresh.get().wrapping_add(1);
        tick_refresh.set(n);
        if n % 12 == 0 {
            refresh_options_menu(&ui_refresh, &state_refresh);
        }
        ControlFlow::Continue
    });

    tracing::info!("systray ready — icône seule dans Indicator XFCE ({app_id})");
    let _ = ready_tx.send(Ok(()));
    gtk::main();
    Ok(())
}

fn install_icon_png() -> Result<(PathBuf, PathBuf)> {
    let base = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/run/user/{}", unsafe { libc::getuid() }));
    let icon_dir = PathBuf::from(base).join("poolsync-tray");
    fs::create_dir_all(&icon_dir).context("create icon dir")?;
    let icon_path = icon_dir.join("poolsync.png");
    fs::write(&icon_path, include_bytes!("../icons/poolsync-tray.png")).context("write icon")?;
    Ok((icon_dir, icon_path))
}

fn clip_sync_label(enabled: bool) -> String {
    if enabled {
        "Presse-papiers PoolSync — ON".into()
    } else {
        "Presse-papiers PoolSync — OFF (local)".into()
    }
}

/// Bascule le sync clipboard sur ce nœud + notification ; OFF = clipboard système seul.
fn apply_clipboard_sync_toggle(state: &AgentState) -> bool {
    let on = state.toggle_clipboard_sync();
    let node = &state.config.node;
    if on {
        notify_util::notify_local(
            "Presse-papiers PoolSync activé",
            &format!("Copier-coller synchronisé avec le pool sur {node}."),
        );
    } else {
        // Absorbe le contenu actuel pour ne pas le renvoyer au retour ON.
        state.mark_hub_clipboard_applied();
        notify_util::notify_local(
            "Presse-papiers local",
            &format!(
                "PoolSync ne touche plus le presse-papiers sur {node}.\n\
                 Recochez « Presse-papiers PoolSync » pour resynchroniser."
            ),
        );
    }
    on
}

fn build_menu(state: &AgentState) -> Result<TrayUi> {
    let root_menu = Menu::new();
    let options = build_options_submenu(state)?;

    let status = find_item_in_submenu(&options, ID_STATUS)?;
    let node = find_item_in_submenu(&options, ID_NODE)?;
    let hub = find_item_in_submenu(&options, ID_HUB)?;
    let master = find_item_in_submenu(&options, ID_MASTER)?;
    let notify = find_check_in_submenu(&options, ID_NOTIFY)?;
    let kvm = find_check_optional_in_submenu(&options, ID_KVM);

    // Switch visible tout en haut du menu (pas seulement dans Options).
    let clip_sync = CheckMenuItem::with_id(
        ID_CLIP_SYNC,
        clip_sync_label(state.clipboard_sync_enabled()),
        true,
        state.clipboard_sync_enabled(),
        None,
    );
    root_menu.append(&clip_sync)?;
    root_menu.append(&PredefinedMenuItem::separator())?;
    root_menu.append(&options)?;

    Ok(TrayUi {
        root_menu,
        last_tray_revision: RefCell::new(0),
        last_tray_fingerprint: RefCell::new(String::new()),
        status,
        node,
        hub,
        master,
        clip_sync,
        notify,
        kvm,
    })
}

fn build_options_submenu(state: &AgentState) -> Result<Submenu> {
    let status = MenuItem::with_id(ID_STATUS, state.status_line(), false, None);
    let node = MenuItem::with_id(
        ID_NODE,
        format!("Nœud : {}", state.config.node),
        false,
        None,
    );
    let hub = MenuItem::with_id(
        ID_HUB,
        format!("Hub : {}", state.hub_display()),
        false,
        None,
    );
    let master = MenuItem::with_id(
        ID_MASTER,
        format!("Maître : {}", state.master_node()),
        false,
        None,
    );
    let sep1 = PredefinedMenuItem::separator();
    let notify = CheckMenuItem::with_id(
        ID_NOTIFY,
        "Notifier copie / réception",
        true,
        state.notify_enabled(),
        None,
    );
    let kvm_item = if state.config.kvm_active() || state.config.kvm_enabled.is_some() {
        Some(CheckMenuItem::with_id(
            ID_KVM,
            "Clavier / souris KVM",
            true,
            state.kvm_enabled(),
            None,
        ))
    } else {
        None
    };
    let sep2 = PredefinedMenuItem::separator();
    let clip_history = MenuItem::with_id(ID_CLIP_HISTORY, "Historique complet…", true, None);
    let clip_clear = MenuItem::with_id(ID_CLIP_CLEAR, "Vider l'historique…", true, None);
    let config = MenuItem::with_id(ID_CONFIG, "Configuration du pool…", true, None);
    let logs = MenuItem::with_id(ID_VIEW_LOGS, "Voir les logs…", true, None);
    let restart = MenuItem::with_id(ID_RESTART, "Redémarrer PoolSync", true, None);
    let quit = MenuItem::with_id(ID_QUIT, "Quitter PoolSync", true, None);
    let sep_mid = PredefinedMenuItem::separator();

    let mut items: Vec<&dyn muda::IsMenuItem> = vec![
        &status,
        &sep1,
        &node,
        &hub,
        &master,
        &sep_mid,
        &notify,
    ];
    if let Some(ref kvm) = kvm_item {
        items.push(kvm);
    }
    items.push(&sep2);
    items.push(&clip_history);
    items.push(&clip_clear);
    items.push(&config);
    items.push(&logs);
    items.push(&restart);
    items.push(&quit);

    Submenu::with_id_and_items(ID_OPTIONS, "Options PoolSync", true, &items)
        .context("options submenu")
}

fn find_item_in_submenu(sub: &Submenu, id: &str) -> Result<MenuItem> {
    sub.items()
        .into_iter()
        .find_map(|item| {
            if item.id().0 == id {
                item.as_menuitem().cloned()
            } else {
                None
            }
        })
        .context(format!("menu item {id}"))
}

fn find_check_in_submenu(sub: &Submenu, id: &str) -> Result<CheckMenuItem> {
    sub.items()
        .into_iter()
        .find_map(|item| {
            if item.id().0 == id {
                item.as_check_menuitem().cloned()
            } else {
                None
            }
        })
        .context(format!("check item {id}"))
}

fn find_check_optional_in_submenu(sub: &Submenu, id: &str) -> Option<CheckMenuItem> {
    sub.items().into_iter().find_map(|item| {
        if item.id().0 == id {
            item.as_check_menuitem().cloned()
        } else {
            None
        }
    })
}

fn clear_clipboard_menu_items(menu: &Menu) {
    loop {
        let Some(first) = menu.items().into_iter().next() else {
            break;
        };
        if !first.id().0.starts_with(CLIP_ID_PREFIX) {
            break;
        }
        let _ = menu.remove_at(0);
    }
}

fn prepend_clip_item(menu: &Menu, entry: &HistoryItem, state: &AgentState) -> Result<()> {
    let id = format!("{CLIP_ID_PREFIX}{}", entry.hash);
    let label = clipboard_history::tray_label(entry);
    if entry.is_image {
        let icon = clipboard_history::tray_image_icon(state, entry);
        let item = IconMenuItem::with_id(id, label, true, icon, None);
        menu.prepend(&item)?;
    } else {
        let item = MenuItem::with_id(id, label, true, None);
        menu.prepend(&item)?;
    }
    Ok(())
}

fn tray_fingerprint(entries: &[HistoryItem]) -> String {
    if entries.is_empty() {
        return String::from("empty");
    }
    entries
        .iter()
        .map(|e| e.hash.as_str())
        .collect::<Vec<_>>()
        .join("|")
}

fn refresh_clipboard_items(ui: &TrayUi, state: &AgentState) {
    let entries: Vec<HistoryItem> = match clipboard_history::fetch_history_tray(state) {
        Ok(items) => items,
        Err(err) => {
            tracing::debug!("tray history fetch: {err:#}");
            Vec::new()
        }
    };

    let fingerprint = tray_fingerprint(&entries);
    if fingerprint == *ui.last_tray_fingerprint.borrow() {
        return;
    }
    *ui.last_tray_fingerprint.borrow_mut() = fingerprint;

    clear_clipboard_menu_items(&ui.root_menu);

    if entries.is_empty() {
        let empty = MenuItem::with_id(
            format!("{CLIP_ID_PREFIX}empty"),
            "(presse-papiers vide)",
            false,
            None,
        );
        if let Err(err) = ui.root_menu.prepend(&empty) {
            tracing::debug!("tray empty item: {err}");
        }
        return;
    }

    for entry in entries.iter().rev() {
        if let Err(err) = prepend_clip_item(&ui.root_menu, entry, state) {
            tracing::debug!("tray clip item: {err}");
        }
    }
}

fn refresh_options_menu(ui: &TrayUi, state: &AgentState) {
    let _ = ui.status.set_text(state.status_line());
    let _ = ui.node.set_text(format!("Nœud : {}", state.config.node));
    let _ = ui.hub.set_text(format!("Hub : {}", state.hub_display()));
    let _ = ui
        .master
        .set_text(format!("Maître : {}", state.master_node()));
    let clip_on = state.clipboard_sync_enabled();
    let _ = ui.clip_sync.set_text(clip_sync_label(clip_on));
    if ui.clip_sync.is_checked() != clip_on {
        ui.clip_sync.set_checked(clip_on);
    }
    let notify_on = state.notify_enabled();
    if ui.notify.is_checked() != notify_on {
        ui.notify.set_checked(notify_on);
    }
    if let Some(kvm) = &ui.kvm {
        let kvm_on = state.kvm_enabled();
        if kvm.is_checked() != kvm_on {
            kvm.set_checked(kvm_on);
        }
    }
}
