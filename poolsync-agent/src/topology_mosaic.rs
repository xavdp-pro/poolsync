//! Mosaïque drag-and-drop des écrans (style Barrier) pour la fenêtre GTK de config.

use gtk::gdk::prelude::*;
use gtk::prelude::*;
use gtk::{DrawingArea, EventBox, Fixed, Label, Orientation, ScrolledWindow};
use poolsync_core::{
    infer_neighbors, layout_scale, snap_position, PoolTopology, TopologyNode,
    DEFAULT_EDGE_TOLERANCE_PX, DEFAULT_SNAP_GRID_PX,
};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

const PAD: i32 = 24;
const MAX_CANVAS_W: f64 = 680.0;
const MAX_CANVAS_H: f64 = 320.0;

pub struct TopologyMosaic {
    root: gtk::Box,
    canvas: Fixed,
    scale: RefCell<f64>,
    canvas_pos: RefCell<HashMap<String, (i32, i32)>>,
    on_layout: Rc<dyn Fn(PoolTopology)>,
}

impl TopologyMosaic {
    pub fn new(on_layout: Rc<dyn Fn(PoolTopology)>) -> Self {
        let root = gtk::Box::new(Orientation::Vertical, 4);
        let hint = Label::new(Some(
            "Glissez les écrans pour les positionner — les voisins se recalculent au relâchement.",
        ));
        hint.set_halign(gtk::Align::Start);
        hint.set_margin_start(4);
        let scrolled = ScrolledWindow::builder()
            .vexpand(false)
            .hexpand(true)
            .height_request(340)
            .build();
        let canvas = Fixed::new();
        canvas.set_size_request(400, 200);
        scrolled.add(&canvas);
        root.pack_start(&hint, false, false, 0);
        root.pack_start(&scrolled, false, false, 0);

        TopologyMosaic {
            root,
            canvas,
            scale: RefCell::new(0.2),
            canvas_pos: RefCell::new(HashMap::new()),
            on_layout,
        }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    pub fn rebuild(&self, topo: &PoolTopology) {
        for child in self.canvas.children() {
            self.canvas.remove(&child);
        }
        self.canvas_pos.borrow_mut().clear();
        if topo.nodes.is_empty() {
            return;
        }

        let scale = layout_scale(&topo.nodes, MAX_CANVAS_W, MAX_CANVAS_H);
        *self.scale.borrow_mut() = scale;

        let mut max_x = 0i32;
        let mut max_y = 0i32;
        for n in topo.nodes.values() {
            max_x = max_x.max(n.x + n.width as i32);
            max_y = max_y.max(n.y + n.height as i32);
        }
        let cw = (max_x as f64 * scale) as i32 + PAD * 2;
        let ch = (max_y as f64 * scale) as i32 + PAD * 2;
        self.canvas.set_size_request(cw.max(320), ch.max(160));

        let lines = connection_lines(topo, scale);
        if !lines.is_empty() {
            let area = DrawingArea::new();
            area.set_size_request(cw, ch);
            let lines_rc = Rc::new(lines);
            area.connect_draw(move |_area, cr| {
                cr.set_source_rgb(0.45, 0.48, 0.95);
                cr.set_line_width(2.0);
                for ln in lines_rc.iter() {
                    cr.move_to(ln.0, ln.1);
                    cr.line_to(ln.2, ln.3);
                    cr.stroke().ok();
                }
                gtk::glib::Propagation::Stop
            });
            self.canvas.put(&area, 0, 0);
        }

        let mut ids: Vec<_> = topo.nodes.keys().cloned().collect();
        ids.sort_by(|a, b| {
            let na = &topo.nodes[a];
            let nb = &topo.nodes[b];
            na.x.cmp(&nb.x).then_with(|| a.cmp(b))
        });

        for id in ids {
            let node = topo.nodes.get(&id).expect("node");
            self.add_screen(&id, node, topo.clone());
        }
    }

    fn add_screen(&self, id: &str, node: &TopologyNode, topo: PoolTopology) {
        let scale = *self.scale.borrow();
        let w = (node.width as f64 * scale).max(64.0) as i32;
        let h = (node.height as f64 * scale).max(40.0) as i32;
        let px = PAD + (node.x as f64 * scale) as i32;
        let py = PAD + (node.y as f64 * scale) as i32;

        self.canvas_pos
            .borrow_mut()
            .insert(id.to_string(), (px, py));

        let event_box = EventBox::new();
        event_box.set_size_request(w, h);
        let frame = gtk::Frame::new(None);
        let vbox = gtk::Box::new(Orientation::Vertical, 2);
        vbox.set_halign(gtk::Align::Center);
        vbox.set_valign(gtk::Align::Center);
        let name = Label::new(None);
        name.set_markup(&format!("<b>{}</b>", escape_markup(id)));
        let size = Label::new(Some(&format!("{}×{}", node.width, node.height)));
        vbox.pack_start(&name, false, false, 0);
        vbox.pack_start(&size, false, false, 0);
        if !node.kvm_enabled {
            vbox.pack_start(&Label::new(Some("clip only")), false, false, 0);
        }
        frame.add(&vbox);
        event_box.add(&frame);

        self.canvas.put(&event_box, px, py);
        self.wire_drag(id, &event_box, topo);
    }

    fn wire_drag(&self, id: &str, widget: &EventBox, topo: PoolTopology) {
        widget.add_events(
            gtk::gdk::EventMask::BUTTON_PRESS_MASK
                | gtk::gdk::EventMask::BUTTON_RELEASE_MASK
                | gtk::gdk::EventMask::BUTTON_MOTION_MASK,
        );

        let canvas = self.canvas.clone();
        let scale = *self.scale.borrow();
        let on_layout = self.on_layout.clone();
        let positions = self.canvas_pos.clone();
        let drag = Rc::new(RefCell::new(None::<(f64, f64, i32, i32)>));
        let id = id.to_string();

        let d0 = drag.clone();
        let id0 = id.clone();
        let pos0 = positions.clone();
        widget.connect_button_press_event(move |w, event| {
            if event.button() != 1 {
                return gtk::glib::Propagation::Proceed;
            }
            let (ox, oy) = pos0.borrow().get(&id0).copied().unwrap_or((0, 0));
            *d0.borrow_mut() = Some((event.root().0, event.root().1, ox, oy));
            w.grab_add();
            gtk::glib::Propagation::Stop
        });

        let d1 = drag.clone();
        let c1 = canvas.clone();
        let w1 = widget.clone();
        let id1 = id.clone();
        let pos1 = positions.clone();
        widget.connect_motion_notify_event(move |_w, event| {
            let Some((rx0, ry0, ox, oy)) = *d1.borrow() else {
                return gtk::glib::Propagation::Proceed;
            };
            let dx = (event.root().0 - rx0) as i32;
            let dy = (event.root().1 - ry0) as i32;
            let nx = ox + dx;
            let ny = oy + dy;
            c1.move_(&w1, nx, ny);
            pos1.borrow_mut().insert(id1.clone(), (nx, ny));
            gtk::glib::Propagation::Stop
        });

        let d2 = drag;
        let id2 = id.clone();
        let pos2 = positions.clone();
        let on_layout2 = on_layout.clone();
        widget.connect_button_release_event(move |w, event| {
            if event.button() != 1 {
                return gtk::glib::Propagation::Proceed;
            }
            w.grab_remove();
            if d2.borrow().is_none() {
                return gtk::glib::Propagation::Stop;
            }
            *d2.borrow_mut() = None;

            let (alloc_x, alloc_y) = pos2.borrow().get(&id2).copied().unwrap_or((0, 0));
            let tx = ((alloc_x - PAD) as f64 / scale).round() as i32;
            let ty = ((alloc_y - PAD) as f64 / scale).round() as i32;
            let (sx, sy) = snap_position(tx.max(0), ty.max(0), DEFAULT_SNAP_GRID_PX);

            let mut nodes = topo.nodes.clone();
            if let Some(n) = nodes.get_mut(&id2) {
                n.x = sx;
                n.y = sy;
            }
            let inferred =
                infer_neighbors(&PoolTopology { nodes }, DEFAULT_EDGE_TOLERANCE_PX);
            on_layout2(inferred);
            gtk::glib::Propagation::Stop
        });
    }
}

fn connection_lines(topo: &PoolTopology, scale: f64) -> Vec<(f64, f64, f64, f64)> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let pad = PAD as f64;

    for (id, n) in &topo.nodes {
        for (dir, other) in &n.neighbors {
            if topo.nodes.get(other).is_none() {
                continue;
            }
            let mut pair = [id.as_str(), other.as_str()];
            pair.sort();
            let key = pair.join("|");
            if seen.contains(&key) {
                continue;
            }
            seen.insert(key);
            let o = &topo.nodes[other];
            let (x1, y1) = edge_point(n, dir, scale, pad);
            let opp = opposite(dir);
            let (x2, y2) = edge_point(o, opp, scale, pad);
            out.push((x1, y1, x2, y2));
        }
    }
    out
}

fn edge_point(n: &TopologyNode, dir: &str, scale: f64, pad: f64) -> (f64, f64) {
    let cx = pad + (n.x as f64 + n.width as f64 / 2.0) * scale;
    let cy = pad + (n.y as f64 + n.height as f64 / 2.0) * scale;
    match dir {
        "left" => (pad + n.x as f64 * scale, cy),
        "right" => (pad + (n.x as f64 + n.width as f64) * scale, cy),
        "up" => (cx, pad + n.y as f64 * scale),
        "down" => (cx, pad + (n.y as f64 + n.height as f64) * scale),
        _ => (cx, cy),
    }
}

fn opposite(dir: &str) -> &str {
    match dir {
        "left" => "right",
        "right" => "left",
        "up" => "down",
        "down" => "up",
        _ => "left",
    }
}

fn escape_markup(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
