//! Géométrie de la mosaïque d'écrans (style Barrier) : voisins dérivés des positions.

use crate::{PoolTopology, TopologyNode};
use std::collections::HashMap;

pub const DEFAULT_EDGE_TOLERANCE_PX: i32 = 48;
pub const DEFAULT_SNAP_GRID_PX: i32 = 20;
pub const MIN_EDGE_OVERLAP_PX: i32 = 80;

/// Aligne x/y sur une grille (ex. 20 px).
pub fn snap_position(x: i32, y: i32, grid: i32) -> (i32, i32) {
    let g = grid.max(1);
    (
        ((x + g / 2) / g) * g,
        ((y + g / 2) / g) * g,
    )
}

/// Recalcule les voisins left/right/up/down à partir des rectangles (bidirectionnel).
pub fn infer_neighbors(topology: &PoolTopology, tolerance_px: i32) -> PoolTopology {
    let tol = tolerance_px.max(1);
    let ids: Vec<String> = topology.nodes.keys().cloned().collect();
    let mut nodes = topology.nodes.clone();

    for id in &ids {
        if let Some(n) = nodes.get_mut(id) {
            n.neighbors.clear();
        }
    }

    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            let a_id = ids[i].clone();
            let b_id = ids[j].clone();
            let a = nodes.get(&a_id).expect("node").clone();
            let b = nodes.get(&b_id).expect("node").clone();
            link_pair(&mut nodes, &a_id, &b_id, &a, &b, tol);
        }
    }

    PoolTopology { nodes }
}

fn link_pair(
    nodes: &mut HashMap<String, TopologyNode>,
    a_id: &str,
    b_id: &str,
    a: &TopologyNode,
    b: &TopologyNode,
    tol: i32,
) {
    let a_right = a.x + a.width as i32;
    let b_right = b.x + b.width as i32;
    let a_bottom = a.y + a.height as i32;
    let b_bottom = b.y + b.height as i32;

    let gap_right = (b.x - a_right).abs();
    let v_overlap = overlap_len(a.y, a_bottom, b.y, b_bottom);
    if gap_right <= tol && v_overlap >= MIN_EDGE_OVERLAP_PX {
        set_neighbor(nodes, a_id, "right", b_id);
        set_neighbor(nodes, b_id, "left", a_id);
    }

    let gap_left = (a.x - b_right).abs();
    if gap_left <= tol && v_overlap >= MIN_EDGE_OVERLAP_PX {
        set_neighbor(nodes, a_id, "left", b_id);
        set_neighbor(nodes, b_id, "right", a_id);
    }

    let gap_down = (b.y - a_bottom).abs();
    let h_overlap = overlap_len(a.x, a_right, b.x, b_right);
    if gap_down <= tol && h_overlap >= MIN_EDGE_OVERLAP_PX {
        set_neighbor(nodes, a_id, "down", b_id);
        set_neighbor(nodes, b_id, "up", a_id);
    }

    let gap_up = (a.y - b_bottom).abs();
    if gap_up <= tol && h_overlap >= MIN_EDGE_OVERLAP_PX {
        set_neighbor(nodes, a_id, "up", b_id);
        set_neighbor(nodes, b_id, "down", a_id);
    }
}

fn overlap_len(a0: i32, a1: i32, b0: i32, b1: i32) -> i32 {
    (a1.min(b1) - a0.max(b0)).max(0)
}

fn set_neighbor(nodes: &mut HashMap<String, TopologyNode>, id: &str, dir: &str, other: &str) {
    if let Some(n) = nodes.get_mut(id) {
        n.neighbors.insert(dir.to_string(), other.to_string());
    }
}

/// Échelle d'affichage pour la mosaïque (pixels canvas).
pub fn layout_scale(nodes: &HashMap<String, TopologyNode>, max_w: f64, max_h: f64) -> f64 {
    if nodes.is_empty() {
        return 0.2;
    }
    let mut max_x = 0i32;
    let mut max_y = 0i32;
    for n in nodes.values() {
        max_x = max_x.max(n.x + n.width as i32);
        max_y = max_y.max(n.y + n.height as i32);
    }
    let mx = max_x.max(1) as f64;
    let my = max_y.max(1) as f64;
    (max_w / mx).min(max_h / my).min(0.4)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(x: i32, y: i32, w: u32, h: u32) -> TopologyNode {
        TopologyNode {
            x,
            y,
            width: w,
            height: h,
            kvm_enabled: true,
            neighbors: HashMap::new(),
        }
    }

    #[test]
    fn infer_horizontal_neighbors() {
        let mut nodes = HashMap::new();
        nodes.insert("asus".into(), node(0, 0, 1920, 1080));
        nodes.insert("acer".into(), node(1920, 0, 1920, 1080));
        let topo = infer_neighbors(&PoolTopology { nodes }, DEFAULT_EDGE_TOLERANCE_PX);
        assert_eq!(topo.nodes["asus"].neighbors.get("right"), Some(&"acer".into()));
        assert_eq!(topo.nodes["acer"].neighbors.get("left"), Some(&"asus".into()));
    }

    #[test]
    fn infer_vertical_neighbors() {
        let mut nodes = HashMap::new();
        nodes.insert("a".into(), node(0, 0, 800, 600));
        nodes.insert("b".into(), node(0, 600, 800, 600));
        let topo = infer_neighbors(&PoolTopology { nodes }, DEFAULT_EDGE_TOLERANCE_PX);
        assert_eq!(topo.nodes["a"].neighbors.get("down"), Some(&"b".into()));
        assert_eq!(topo.nodes["b"].neighbors.get("up"), Some(&"a".into()));
    }

    #[test]
    fn snap_rounds_to_grid() {
        assert_eq!(snap_position(23, 37, 20), (20, 40));
    }
}
