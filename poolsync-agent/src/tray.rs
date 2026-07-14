use crate::logs_viewer;
use crate::state::{format_time_ago, AgentState};
use anyhow::{Context, Result};
use glib::ControlFlow;
use libappindicator::{AppIndicator, AppIndicatorStatus};
use muda::{CheckMenuItem, ContextMenu, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const ID_STATUS: &str = "status";
const ID_NODE: &str = "node";
const ID_HUB: &str = "hub";
const ID_MASTER: &str = "master";
const ID_LAST_CLIP: &str = "last_clip";
const ID_CLIP_SYNC: &str = "clip_sync";
const ID_NOTIFY: &str = "notify";
const ID_KVM: &str = "kvm";
const ID_VIEW_LOGS: &str = "view_logs";

struct TrayUi {
    status: MenuItem,
    node: MenuItem,
    hub: MenuItem,
    master: MenuItem,
    last_clip: MenuItem,
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

fn run_tray_gtk(state: Arc<AgentState>, ready_tx: std::sync::mpsc::SyncSender<Result<()>>) -> Result<()> {
    gtk::init().map_err(|e| anyhow::anyhow!("gtk init: {e}"))?;
    let menu = build_menu(&state)?;
    let kvm_item = find_check_optional(&menu, ID_KVM);
    let ui = Arc::new(TrayUi {
        status: find_item(&menu, ID_STATUS)?,
        node: find_item(&menu, ID_NODE)?,
        hub: find_item(&menu, ID_HUB)?,
        master: find_item(&menu, ID_MASTER)?,
        last_clip: find_item(&menu, ID_LAST_CLIP)?,
        clip_sync: find_check(&menu, ID_CLIP_SYNC)?,
        notify: find_check(&menu, ID_NOTIFY)?,
        kvm: kvm_item,
    });

    let app_id = format!("com.xavdp.poolsync.{}", state.config.node);
    let mut indicator = AppIndicator::new(&app_id, "poolsync");
    indicator.set_status(AppIndicatorStatus::Active);

    let (icon_dir, icon_path) = install_icon_png()?;
    indicator.set_icon_theme_path(&icon_dir.to_string_lossy());
    indicator.set_icon_full(&icon_path.to_string_lossy(), "PoolSync");
    indicator.set_menu(&mut menu.gtk_context_menu());

    let gtk_ctx = glib::MainContext::ref_thread_default();
    let state_events = state.clone();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let id = event.id().0.as_str();
        match id {
            ID_CLIP_SYNC => {
                let on = state_events.toggle_clipboard_sync();
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
            ID_VIEW_LOGS => {
                let ctx = gtk_ctx.clone();
                let node = state_events.config.node.clone();
                let _ = ctx.invoke(move || logs_viewer::show(&node));
            }
            _ => {}
        }
    }));

    let ui_refresh = ui.clone();
    let state_refresh = state.clone();
    glib::timeout_add_local(Duration::from_secs(2), move || {
        refresh_menu(&ui_refresh, &state_refresh);
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

fn build_menu(state: &AgentState) -> Result<Menu> {
    let menu = Menu::new();
    menu.append(&MenuItem::with_id(ID_STATUS, state.status_line(), false, None))?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&MenuItem::with_id(
        ID_NODE,
        format!("Nœud : {}", state.config.node),
        false,
        None,
    ))?;
    menu.append(&MenuItem::with_id(
        ID_HUB,
        format!("Hub : {}", state.hub_display()),
        false,
        None,
    ))?;
    menu.append(&MenuItem::with_id(
        ID_MASTER,
        format!("Maître : {}", state.master_node()),
        false,
        None,
    ))?;
    menu.append(&MenuItem::with_id(
        ID_LAST_CLIP,
        "Dernier reçu : —",
        false,
        None,
    ))?;
    menu.append(&PredefinedMenuItem::separator())?;
    let clip_sync = CheckMenuItem::with_id(
        ID_CLIP_SYNC,
        "Presse-papiers synchronisé",
        true,
        state.clipboard_sync_enabled(),
        None,
    );
    menu.append(&clip_sync)?;
    let notify = CheckMenuItem::with_id(
        ID_NOTIFY,
        "Notifier à la réception",
        true,
        state.notify_enabled(),
        None,
    );
    menu.append(&notify)?;
    if state.config.kvm_active() || state.config.kvm_enabled.is_some() {
        let kvm = CheckMenuItem::with_id(
            ID_KVM,
            "Clavier / souris KVM",
            true,
            state.kvm_enabled(),
            None,
        );
        menu.append(&kvm)?;
    }
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&MenuItem::with_id(ID_VIEW_LOGS, "Voir les logs…", true, None))?;
    Ok(menu)
}

fn refresh_menu(ui: &TrayUi, state: &AgentState) {
    let _ = ui.status.set_text(state.status_line());
    let _ = ui
        .node
        .set_text(format!("Nœud : {}", state.config.node));
    let _ = ui.hub.set_text(format!("Hub : {}", state.hub_display()));
    let _ = ui
        .master
        .set_text(format!("Maître : {}", state.master_node()));

    let last = match state.last_clip_ago_secs() {
        Some(secs) => format!(
            "Dernier reçu : {} — {}",
            format_time_ago(secs),
            state.last_clip_preview()
        ),
        None => "Dernier reçu : —".into(),
    };
    let _ = ui.last_clip.set_text(last);

    let _ = ui.clip_sync.set_checked(state.clipboard_sync_enabled());
    let _ = ui.notify.set_checked(state.notify_enabled());
    if let Some(kvm) = &ui.kvm {
        let _ = kvm.set_checked(state.kvm_enabled());
    }
}

fn find_item(menu: &Menu, id: &str) -> Result<MenuItem> {
    menu.items()
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

fn find_check_optional(menu: &Menu, id: &str) -> Option<CheckMenuItem> {
    menu.items().into_iter().find_map(|item| {
        if item.id().0 == id {
            item.as_check_menuitem().cloned()
        } else {
            None
        }
    })
}

fn find_check(menu: &Menu, id: &str) -> Result<CheckMenuItem> {
    menu.items()
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
