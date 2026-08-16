use crate::clipboard_history;
use crate::config_window;
use crate::notify_util;
use crate::state::AgentState;
use anyhow::{Context, Result};
use gtk::prelude::*;
use libappindicator::{AppIndicator, AppIndicatorStatus};
use std::fs;
use std::path::PathBuf;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

/// Nombre de copier-coller récents affichés directement dans le systray.
const RECENT_IN_TRAY: usize = 12;

thread_local! {
    /// Entrées de copier-coller du menu racine, créées une seule fois puis
    /// simplement renommées. Le menu vit dans le processus du panel (dbusmenu) :
    /// y détruire des widgets pendant qu'il les dessine fait segfauter Pango.
    static ITEM_SLOTS: std::cell::RefCell<Vec<gtk::MenuItem>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Lance le systray. **Doit être appelé depuis le thread principal** : GTK/GDK
/// n'est pas thread-safe, et les fenêtres ouvertes depuis le menu (historique,
/// configuration, logs) construisent des widgets et des `Pixbuf` qui segfaultent
/// si la boucle GTK ne tourne pas sur le thread qui a fait `gtk::init()`.
pub fn run_tray(state: Arc<AgentState>) -> Result<()> {
    run_tray_gtk(state)
}

fn run_tray_gtk(state: Arc<AgentState>) -> Result<()> {
    gtk::init().map_err(|e| anyhow::anyhow!("gtk init: {e}"))?;

    let (icon_dir, icon_path) = install_icon_png()?;

    // Menu principal (clic gauche) : uniquement les copier-coller, à la racine.
    // Il est reconstruit à chaque ouverture pour refléter l'historique courant.
    let menu = gtk::Menu::new();

    // ── Menu contextuel (clic droit) : options, statut, actions ──────────
    let opts_menu = gtk::Menu::new();

    let clip_item = gtk::CheckMenuItem::with_label(&clip_sync_label(state.clipboard_sync_enabled()));
    clip_item.set_active(state.clipboard_sync_enabled());
    let state_clip = state.clone();
    clip_item.connect_toggled(move |item| {
        let on = apply_clipboard_sync_toggle(&state_clip);
        item.set_label(&clip_sync_label(on));
    });
    opts_menu.append(&clip_item);

    let history_item = gtk::MenuItem::with_label("Ouvrir PoolSync…");
    let state_hist = state.clone();
    history_item.connect_activate(move |_| {
        config_window::show(state_hist.clone());
    });
    opts_menu.append(&history_item);

    let clear_item = gtk::MenuItem::with_label("Vider l'historique…");
    let state_clear = state.clone();
    clear_item.connect_activate(move |_| {
        clipboard_history::confirm_clear_from_tray(state_clear.clone());
    });
    opts_menu.append(&clear_item);

    opts_menu.append(&gtk::SeparatorMenuItem::new());

    if state.config.kvm_active() || state.config.kvm_enabled.is_some() {
        let kvm_item = gtk::CheckMenuItem::with_label("Clavier / souris KVM");
        kvm_item.set_active(state.kvm_enabled());
        let state_kvm = state.clone();
        kvm_item.connect_toggled(move |_| {
            state_kvm.toggle_kvm();
        });
        opts_menu.append(&kvm_item);
    }

    let notify_item = gtk::CheckMenuItem::with_label("Notifier copie / réception");
    notify_item.set_active(state.notify_enabled());
    let state_notif = state.clone();
    notify_item.connect_toggled(move |_| {
        state_notif.toggle_notify();
    });
    opts_menu.append(&notify_item);

    let config_item = gtk::MenuItem::with_label("Écrans & configuration…");
    let state_cfg = state.clone();
    config_item.connect_activate(move |_| {
        config_window::show(state_cfg.clone());
    });
    opts_menu.append(&config_item);

    // Statut en bas des options, non cliquable.
    opts_menu.append(&gtk::SeparatorMenuItem::new());
    let hotkey_hint = gtk::MenuItem::with_label(&format!(
        "Raccourci : {} (suspendre / reprendre)",
        crate::hotkey::HOTKEY_LABEL
    ));
    hotkey_hint.set_sensitive(false);
    opts_menu.append(&hotkey_hint);
    for label in [
        format!("Statut : {}", state.status_line()),
        format!("Nœud : {}", state.config.node),
        format!("Hub : {}", state.hub_display()),
        format!("Maître KVM : {}", state.master_node()),
    ] {
        let item = gtk::MenuItem::with_label(&label);
        item.set_sensitive(false);
        opts_menu.append(&item);
    }

    opts_menu.append(&gtk::SeparatorMenuItem::new());

    let restart_item = gtk::MenuItem::with_label("Redémarrer PoolSync");
    restart_item.connect_activate(|_| {
        std::thread::spawn(|| {
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "restart", "poolsync-agent.service"])
                .status();
        });
    });
    opts_menu.append(&restart_item);

    let quit_item = gtk::MenuItem::with_label("Quitter PoolSync");
    quit_item.connect_activate(|_| {
        std::thread::spawn(|| {
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "stop", "poolsync-agent.service"])
                .status();
        });
    });
    opts_menu.append(&quit_item);

    opts_menu.show_all();

    // Le panel garde en mémoire le thème d'icônes chargé à son démarrage : une icône
    // installée après coup reste introuvable et l'item s'affiche vide. On force donc
    // le thème courant à relire le cache avant d'enregistrer l'icône.
    if let Some(theme) = gtk::IconTheme::default() {
        theme.append_search_path(&icon_dir);
        theme.rescan_if_needed();
    }

    // Le protocole StatusNotifier n'expose qu'un seul menu et aucun signal de clic :
    // le panel ouvre toujours le même menu, quel que soit le bouton. On place donc
    // l'historique à la racine (ce qui s'ouvre au clic) et on regroupe le reste sous
    // une entrée « Options » en bas.
    let opts_sub = gtk::MenuItem::with_label("Options…");
    opts_sub.set_submenu(Some(&opts_menu));

    let app_id = format!("com.xavdp.poolsync.{}", state.config.node);
    let mut indicator = AppIndicator::new(&app_id, "poolsync-tray");
    indicator.set_status(AppIndicatorStatus::Active);
    indicator.set_icon_theme_path(&icon_dir.to_string_lossy());
    indicator.set_icon_full("poolsync-tray", "PoolSync");
    apply_tray_title(&mut indicator, &state);
    let indicator = Rc::new(RefCell::new(indicator));
    indicator.borrow_mut().set_menu(&mut menu.clone());

    // Entrées fixes : créées une fois, jamais détruites, seulement renommées.
    let mut slots = Vec::new();
    for _ in 0..RECENT_IN_TRAY {
        let item = gtk::MenuItem::with_label("");
        let state_pick = state.clone();
        let slot_hash: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let hash_for_click = slot_hash.clone();
        item.connect_activate(move |_| {
            let hash = hash_for_click.borrow().clone();
            if let Some(hash) = hash {
                if let Err(err) = clipboard_history::pick_and_paste(&state_pick, &hash) {
                    tracing::warn!("tray pick: {err:#}");
                }
            }
        });
        menu.append(&item);
        slots.push((item, slot_hash));
    }
    menu.append(&gtk::SeparatorMenuItem::new());
    menu.append(&opts_sub);
    menu.show_all();

    ITEM_SLOTS.with(|s| *s.borrow_mut() = slots.iter().map(|(i, _)| i.clone()).collect());
    let slots = Rc::new(slots);
    refresh_item_labels(&slots, &state);

    // Pas de reconstruction périodique du menu : il est sérialisé vers le panel via
    // dbusmenu et vit donc dans le processus du panel. `is_visible()` y répond
    // toujours faux, si bien que détruire ses widgets pendant que le panel les
    // dessine faisait segfauter Pango. À la place, on met à jour les libellés
    // existants — aucun widget n'est créé ni détruit, donc rien à casser.
    let items_slots = slots.clone();
    let state_tick = state.clone();
    let indicator_tick = indicator.clone();
    let mut last_revision = state.tray_history_revision();
    let mut last_status_revision = state.tray_status_revision();
    glib::timeout_add_local(std::time::Duration::from_millis(1200), move || {
        let revision = state_tick.tray_history_revision();
        if revision != last_revision {
            last_revision = revision;
            refresh_item_labels(&items_slots, &state_tick);
        }
        let status_rev = state_tick.tray_status_revision();
        if status_rev != last_status_revision {
            last_status_revision = status_rev;
            apply_tray_title(&mut indicator_tick.borrow_mut(), &state_tick);
        }
        glib::ControlFlow::Continue
    });

    tracing::info!("systray ready — historique à la racine, options en bas ({app_id})");
    gtk::main();
    Ok(())
}

/// Nettoie un libellé destiné au menu systray.
///
/// Le presse-papiers peut contenir n'importe quoi : caractères de contrôle, texte
/// vide, séquences trop longues. Passés tels quels au
/// panel via dbusmenu, ils font segfauter son rendu Pango.
fn safe_menu_label(raw: &str) -> String {
    let cleaned: String = raw.chars().filter(|c| !c.is_control()).collect();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        "(vide)".to_string()
    } else {
        cleaned.chars().take(80).collect()
    }
}

/// Met à jour les libellés des entrées de copier-coller, sans créer ni détruire
/// aucun widget : c'est ce qui rend le rafraîchissement sûr alors que le menu est
/// affiché par le panel.
type ItemSlot = (gtk::MenuItem, Rc<RefCell<Option<String>>>);

fn refresh_item_labels(slots: &[ItemSlot], state: &Arc<AgentState>) {
    let items = clipboard_history::fetch_history_tray(state).unwrap_or_default();

    for (idx, (item, hash_slot)) in slots.iter().enumerate() {
        match items.get(idx) {
            Some(entry) => {
                let text = safe_menu_label(&clipboard_history::tray_label(entry));
                if let Some(label) = item.child().and_then(|c| c.downcast::<gtk::Label>().ok()) {
                    label.set_text(&text);
                    label.set_xalign(0.0);
                }
                *hash_slot.borrow_mut() = Some(entry.hash.clone());
                item.set_sensitive(true);
                item.show();
            }
            None => {
                // Emplacement inutilisé : masqué, jamais supprimé.
                *hash_slot.borrow_mut() = None;
                item.hide();
            }
        }
    }

    if items.is_empty() {
        if let Some((item, _)) = slots.first() {
            if let Some(label) = item.child().and_then(|c| c.downcast::<gtk::Label>().ok()) {
                label.set_text("(aucun copier-coller récent)");
            }
            item.set_sensitive(false);
            item.show();
        }
    }
}

/// Installe l'icône dans le thème `hicolor` de l'utilisateur.
///
/// Un `IconThemePath` pointant vers un dossier plat ne suffit pas : GTK y cherche
/// une arborescence de thème (`<taille>/apps/…` + `index.theme`) et n'y trouve
/// rien, donc le panel enregistre l'item sans jamais dessiner d'icône. On écrit
/// donc dans `~/.local/share/icons/hicolor/<taille>/apps/` puis on rafraîchit le
/// cache, seul chemin que le panel résout de façon fiable après un redémarrage.
fn install_icon_png() -> Result<(PathBuf, PathBuf)> {
    const ICON: &[u8] = include_bytes!("../icons/poolsync-tray.png");
    const SIZES: [&str; 5] = ["16x16", "22x22", "24x24", "48x48", "scalable"];

    let home = std::env::var("HOME").context("HOME absent")?;
    let theme_dir = PathBuf::from(&home).join(".local/share/icons/hicolor");

    let mut installed = None;
    for size in SIZES {
        let dir = theme_dir.join(size).join("apps");
        if fs::create_dir_all(&dir).is_err() {
            continue;
        }
        let path = dir.join("poolsync-tray.png");
        if fs::write(&path, ICON).is_ok() && installed.is_none() {
            installed = Some(path);
        }
    }
    let icon_path = installed.context("écriture icône hicolor")?;

    // Sans cache à jour, le panel peut ignorer une icône fraîchement installée.
    let _ = std::process::Command::new("gtk-update-icon-cache")
        .args(["-f", "-t", "-q"])
        .arg(&theme_dir)
        .status();

    Ok((theme_dir, icon_path))
}

fn clip_sync_label(enabled: bool) -> String {
    if enabled {
        "Presse-papiers PoolSync : activé".into()
    } else {
        "Presse-papiers PoolSync : désactivé".into()
    }
}

fn apply_tray_title(indicator: &mut AppIndicator, state: &AgentState) {
    if state.local_poolsync_active() {
        indicator.set_title("PoolSync");
    } else {
        indicator.set_title("PoolSync — OFF");
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


