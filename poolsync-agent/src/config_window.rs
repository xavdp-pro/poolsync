//! Fenêtre GTK native : configuration du pool servie par le hub.
//!
//! Chaque agent peut ouvrir cette fenêtre depuis le systray pour consulter et
//! éditer la topologie du hub (voisins, KVM par nœud, taille d'écran) sans passer
//! par le dashboard web. Les échanges se font via l'API HTTP du hub
//! (`GET`/`POST /api/topology`), avec le token de l'agent.

use crate::logs_viewer::fetch_journal_logs;
use crate::network::hub_tcp_endpoint;
use crate::state::AgentState;
use anyhow::{anyhow, Result};
use gtk::prelude::*;
use gtk::{
    Align, Box as GtkBox, Button, CheckButton, ComboBoxText, Frame, Grid, Label, Notebook,
    Orientation, ScrolledWindow, SpinButton, TextView, Window,
};
use poolsync_core::{PoolTopology, TopologyNode};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

const HTTP_TIMEOUT: Duration = Duration::from_secs(5);
const DIRS: [&str; 4] = ["left", "right", "up", "down"];

thread_local! {
    static OPEN_WINDOW: RefCell<Option<Rc<ConfigWindow>>> = const { RefCell::new(None) };
}

/// Ouvre (ou ré-affiche) la fenêtre de configuration. À appeler sur le thread GTK.
pub fn show(state: Arc<AgentState>) {
    OPEN_WINDOW.with(|slot| {
        if let Some(existing) = slot.borrow().as_ref() {
            existing.window.present();
            existing.load();
            return;
        }
        let win = ConfigWindow::new(state);
        win.load();
        *slot.borrow_mut() = Some(win);
    });
}

/// Widgets d'édition d'un nœud (position conservée telle quelle en v1).
struct NodeRow {
    id: String,
    x: i32,
    y: i32,
    kvm: CheckButton,
    width: SpinButton,
    height: SpinButton,
    neighbors: Vec<(String, ComboBoxText)>,
}

struct ConfigWindow {
    window: Window,
    state: Arc<AgentState>,
    nodes_box: GtkBox,
    status: Label,
    rows: RefCell<Vec<NodeRow>>,
    logs_view: TextView,
}

impl ConfigWindow {
    fn new(state: Arc<AgentState>) -> Rc<Self> {
        let win = Rc::new_cyclic(|weak: &std::rc::Weak<ConfigWindow>| {
            let window = Window::builder()
                .title(format!(
                    "PoolSync — configuration du pool ({})",
                    state.config.node
                ))
                .default_width(660)
                .default_height(580)
                .build();

            let notebook = Notebook::new();

            // --- Onglet Configuration ---------------------------------------
            let config_page = GtkBox::new(Orientation::Vertical, 0);

            let toolbar = GtkBox::new(Orientation::Horizontal, 6);
            toolbar.set_margin_start(8);
            toolbar.set_margin_end(8);
            toolbar.set_margin_top(8);
            toolbar.set_margin_bottom(4);
            let reload_btn = Button::with_label("Recharger");
            let save_btn = Button::with_label("Enregistrer → hub");
            let status = Label::new(None);
            status.set_halign(Align::Start);
            toolbar.pack_start(&reload_btn, false, false, 0);
            toolbar.pack_start(&save_btn, false, false, 0);
            toolbar.pack_start(&status, false, false, 8);

            let scrolled = ScrolledWindow::builder()
                .vexpand(true)
                .hexpand(true)
                .margin_start(8)
                .margin_end(8)
                .margin_bottom(8)
                .build();
            let nodes_box = GtkBox::new(Orientation::Vertical, 10);
            nodes_box.set_margin_top(4);
            nodes_box.set_margin_bottom(4);
            scrolled.add(&nodes_box);

            config_page.pack_start(&toolbar, false, false, 0);
            config_page.pack_start(&scrolled, true, true, 0);
            notebook.append_page(&config_page, Some(&Label::new(Some("Configuration"))));

            // --- Onglet Logs ------------------------------------------------
            let logs_page = GtkBox::new(Orientation::Vertical, 0);
            let logs_toolbar = GtkBox::new(Orientation::Horizontal, 6);
            logs_toolbar.set_margin_start(8);
            logs_toolbar.set_margin_end(8);
            logs_toolbar.set_margin_top(8);
            logs_toolbar.set_margin_bottom(4);
            let logs_refresh = Button::with_label("Actualiser");
            logs_toolbar.pack_start(&logs_refresh, false, false, 0);
            let logs_scrolled = ScrolledWindow::builder()
                .vexpand(true)
                .hexpand(true)
                .margin_start(8)
                .margin_end(8)
                .margin_bottom(8)
                .build();
            let logs_view = TextView::builder()
                .editable(false)
                .monospace(true)
                .wrap_mode(gtk::WrapMode::Word)
                .left_margin(8)
                .right_margin(8)
                .top_margin(6)
                .bottom_margin(6)
                .build();
            logs_scrolled.add(&logs_view);
            logs_page.pack_start(&logs_toolbar, false, false, 0);
            logs_page.pack_start(&logs_scrolled, true, true, 0);
            notebook.append_page(&logs_page, Some(&Label::new(Some("Logs"))));

            window.add(&notebook);

            // --- Signaux ----------------------------------------------------
            let w = weak.clone();
            reload_btn.connect_clicked(move |_| {
                if let Some(v) = w.upgrade() {
                    v.load();
                }
            });

            let w = weak.clone();
            save_btn.connect_clicked(move |_| {
                if let Some(v) = w.upgrade() {
                    v.save();
                }
            });

            let w = weak.clone();
            logs_refresh.connect_clicked(move |_| {
                if let Some(v) = w.upgrade() {
                    v.refresh_logs();
                }
            });

            window.connect_destroy(|_| {
                OPEN_WINDOW.with(|slot| *slot.borrow_mut() = None);
            });

            ConfigWindow {
                window,
                state: state.clone(),
                nodes_box,
                status,
                rows: RefCell::new(Vec::new()),
                logs_view,
            }
        });

        win.refresh_logs();
        win.window.show_all();
        win
    }

    /// Récupère la topologie du hub et (re)construit l'éditeur.
    fn load(&self) {
        for child in self.nodes_box.children() {
            self.nodes_box.remove(&child);
        }
        self.rows.borrow_mut().clear();

        let topo = match fetch_topology(&self.state) {
            Ok(topo) => topo,
            Err(err) => {
                self.set_status(&format!("Hub injoignable : {err}"), true);
                return;
            }
        };

        if topo.nodes.is_empty() {
            let empty = Label::new(Some(
                "Aucun nœud dans la topologie du hub.\n\
                 Les agents doivent d'abord se connecter au hub.",
            ));
            empty.set_halign(Align::Start);
            self.nodes_box.pack_start(&empty, false, false, 0);
            self.nodes_box.show_all();
            self.set_status("Topologie vide", false);
            return;
        }

        let mut ids: Vec<String> = topo.nodes.keys().cloned().collect();
        ids.sort_by(|a, b| {
            let na = &topo.nodes[a];
            let nb = &topo.nodes[b];
            na.x.cmp(&nb.x).then_with(|| a.cmp(b))
        });

        for id in &ids {
            let node = &topo.nodes[id];
            let row = self.build_node_frame(id, node, &ids);
            self.rows.borrow_mut().push(row);
        }
        self.nodes_box.show_all();
        self.set_status(
            &format!("{} nœud(s) chargé(s) depuis le hub", ids.len()),
            false,
        );
    }

    /// Construit le cadre d'un nœud et renvoie les widgets pour lecture au save.
    fn build_node_frame(&self, id: &str, node: &TopologyNode, all_ids: &[String]) -> NodeRow {
        let frame = Frame::new(Some(id));
        let grid = Grid::new();
        grid.set_row_spacing(6);
        grid.set_column_spacing(10);
        grid.set_margin_start(10);
        grid.set_margin_end(10);
        grid.set_margin_top(8);
        grid.set_margin_bottom(8);

        let kvm = CheckButton::with_label("KVM actif (clavier / souris)");
        kvm.set_active(node.kvm_enabled);
        grid.attach(&kvm, 0, 0, 4, 1);

        let screen_label = Label::new(Some("Écran"));
        screen_label.set_halign(Align::Start);
        grid.attach(&screen_label, 0, 1, 1, 1);
        let width = SpinButton::with_range(320.0, 16000.0, 1.0);
        width.set_value(node.width as f64);
        let height = SpinButton::with_range(240.0, 16000.0, 1.0);
        height.set_value(node.height as f64);
        grid.attach(&width, 1, 1, 1, 1);
        let x_label = Label::new(Some("×"));
        grid.attach(&x_label, 2, 1, 1, 1);
        grid.attach(&height, 3, 1, 1, 1);

        let mut neighbors = Vec::new();
        for (i, dir) in DIRS.iter().enumerate() {
            let dir_label = Label::new(Some(dir_fr(dir)));
            dir_label.set_halign(Align::Start);
            grid.attach(&dir_label, 0, 2 + i as i32, 1, 1);

            let combo = ComboBoxText::new();
            combo.append(Some(""), "—");
            for other in all_ids {
                if other != id {
                    combo.append(Some(other), other);
                }
            }
            let current = node.neighbors.get(*dir).map(String::as_str).unwrap_or("");
            combo.set_active_id(Some(current));
            grid.attach(&combo, 1, 2 + i as i32, 3, 1);
            neighbors.push((dir.to_string(), combo));
        }

        frame.add(&grid);
        self.nodes_box.pack_start(&frame, false, false, 0);

        NodeRow {
            id: id.to_string(),
            x: node.x,
            y: node.y,
            kvm,
            width,
            height,
            neighbors,
        }
    }

    /// Reconstruit la topologie depuis les widgets et l'envoie au hub.
    fn save(&self) {
        let mut nodes = HashMap::new();
        for row in self.rows.borrow().iter() {
            let mut neighbors = HashMap::new();
            for (dir, combo) in &row.neighbors {
                if let Some(id) = combo.active_id() {
                    let id = id.to_string();
                    if !id.is_empty() {
                        neighbors.insert(dir.clone(), id);
                    }
                }
            }
            nodes.insert(
                row.id.clone(),
                TopologyNode {
                    x: row.x,
                    y: row.y,
                    width: row.width.value_as_int() as u32,
                    height: row.height.value_as_int() as u32,
                    kvm_enabled: row.kvm.is_active(),
                    neighbors,
                },
            );
        }

        if nodes.is_empty() {
            self.set_status("Rien à enregistrer", true);
            return;
        }

        match post_topology(&self.state, &PoolTopology { nodes }) {
            Ok(()) => self.set_status("Topologie enregistrée et diffusée aux agents ✓", false),
            Err(err) => self.set_status(&format!("Échec enregistrement : {err}"), true),
        }
    }

    fn refresh_logs(&self) {
        if let Some(buffer) = self.logs_view.buffer() {
            buffer.set_text(&fetch_journal_logs());
        }
    }

    fn set_status(&self, msg: &str, is_error: bool) {
        self.status.set_text(msg);
        // Couleur : rouge pour une erreur, gris sinon (via markup léger).
        let markup = if is_error {
            format!("<span foreground=\"#b91c1c\">{}</span>", glib_escape(msg))
        } else {
            format!("<span foreground=\"#475569\">{}</span>", glib_escape(msg))
        };
        self.status.set_markup(&markup);
    }
}

fn dir_fr(dir: &str) -> &'static str {
    match dir {
        "left" => "← Gauche",
        "right" => "→ Droite",
        "up" => "↑ Haut",
        "down" => "↓ Bas",
        _ => "?",
    }
}

fn glib_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn http_base(state: &AgentState) -> Result<String> {
    let (host, port) = hub_tcp_endpoint(&state.config.hub_url)?;
    Ok(format!("http://{host}:{port}"))
}

fn fetch_topology(state: &AgentState) -> Result<PoolTopology> {
    let url = format!("{}/api/topology", http_base(state)?);
    let body = ureq::get(&url)
        .timeout(HTTP_TIMEOUT)
        .call()
        .map_err(|e| anyhow!("{e}"))?
        .into_string()?;
    Ok(serde_json::from_str(&body)?)
}

fn post_topology(state: &AgentState, topo: &PoolTopology) -> Result<()> {
    let url = format!(
        "{}/api/topology?token={}",
        http_base(state)?,
        state.config.token
    );
    let body = serde_json::to_string(topo)?;
    ureq::post(&url)
        .timeout(HTTP_TIMEOUT)
        .set("Content-Type", "application/json")
        .send_string(&body)
        .map_err(|e| anyhow!("{e}"))?;
    Ok(())
}
