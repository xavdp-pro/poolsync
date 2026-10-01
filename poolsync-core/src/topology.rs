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
        ((x as f64 / g as f64).round() as i32) * g,
        ((y as f64 / g as f64).round() as i32) * g,
    )
}

/// Recalcule les voisins left/right/up/down à partir des rectangles (bidirectionnel).
/// Les nœuds `kvm_enabled = false` (presse-papiers seul) sont exclus du graphe KVM.
pub fn infer_neighbors(topology: &PoolTopology, tolerance_px: i32) -> PoolTopology {
    let tol = tolerance_px.max(1);
    let mut ids: Vec<String> = topology
        .nodes
        .iter()
        .filter(|(_, n)| n.kvm_enabled)
        .map(|(k, _)| k.clone())
        .collect();
    ids.sort();
    let mut candidates = Vec::new();
    let mut nodes = topology.nodes.clone();

    for n in nodes.values_mut() {
        n.neighbors.clear();
    }

    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            let a_id = ids[i].clone();
            let b_id = ids[j].clone();
            let a = nodes.get(&a_id).expect("node").clone();
            let b = nodes.get(&b_id).expect("node").clone();
            link_pair(&mut candidates, &a_id, &b_id, &a, &b, tol);
        }
    }

    // One route per edge: choose the closest edge, then greatest overlap.
    // Sorted IDs break ties identically in every process and frontend.
    candidates.sort_by_key(|c| {
        (
            c.gap,
            std::cmp::Reverse(c.overlap),
            c.a.clone(),
            c.b.clone(),
            c.dir,
        )
    });
    for c in candidates {
        if !nodes[&c.a].neighbors.contains_key(c.dir)
            && !nodes[&c.b].neighbors.contains_key(c.reverse)
        {
            set_neighbor(&mut nodes, &c.a, c.dir, &c.b);
            set_neighbor(&mut nodes, &c.b, c.reverse, &c.a);
        }
    }
    PoolTopology { nodes }
}

struct Candidate {
    a: String,
    b: String,
    dir: &'static str,
    reverse: &'static str,
    gap: i32,
    overlap: i32,
}

fn link_pair(
    candidates: &mut Vec<Candidate>,
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
        candidates.push(Candidate {
            a: a_id.into(),
            b: b_id.into(),
            dir: "right",
            reverse: "left",
            gap: gap_right,
            overlap: v_overlap,
        });
    }

    let gap_left = (a.x - b_right).abs();
    if gap_left <= tol && v_overlap >= MIN_EDGE_OVERLAP_PX {
        candidates.push(Candidate {
            a: a_id.into(),
            b: b_id.into(),
            dir: "left",
            reverse: "right",
            gap: gap_left,
            overlap: v_overlap,
        });
    }

    let gap_down = (b.y - a_bottom).abs();
    let h_overlap = overlap_len(a.x, a_right, b.x, b_right);
    if gap_down <= tol && h_overlap >= MIN_EDGE_OVERLAP_PX {
        candidates.push(Candidate {
            a: a_id.into(),
            b: b_id.into(),
            dir: "down",
            reverse: "up",
            gap: gap_down,
            overlap: h_overlap,
        });
    }

    let gap_up = (a.y - b_bottom).abs();
    if gap_up <= tol && h_overlap >= MIN_EDGE_OVERLAP_PX {
        candidates.push(Candidate {
            a: a_id.into(),
            b: b_id.into(),
            dir: "up",
            reverse: "down",
            gap: gap_up,
            overlap: h_overlap,
        });
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
    let min_x = nodes.values().map(|n| n.x).min().unwrap_or(0);
    let min_y = nodes.values().map(|n| n.y).min().unwrap_or(0);
    let max_x = nodes
        .values()
        .map(|n| n.x + n.width as i32)
        .max()
        .unwrap_or(1);
    let max_y = nodes
        .values()
        .map(|n| n.y + n.height as i32)
        .max()
        .unwrap_or(1);
    (max_w / (max_x - min_x).max(1) as f64)
        .min(max_h / (max_y - min_y).max(1) as f64)
        .min(0.4)
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
            monitor_x: 0,
            monitor_y: 0,
            desktop_x: 0,
            desktop_y: 0,
            desktop_width: w,
            desktop_height: h,
        }
    }

    #[test]
    fn negative_positions_and_mixed_resolutions_are_scaled_as_a_bounding_box() {
        let nodes = HashMap::from([
            ("laptop".into(), node(-1366, -200, 1366, 768)),
            ("desk".into(), node(0, 0, 2560, 1440)),
        ]);
        let topology = infer_neighbors(
            &PoolTopology {
                nodes: nodes.clone(),
            },
            48,
        );
        assert_eq!(topology.nodes["laptop"].neighbors["right"], "desk");
        assert!(layout_scale(&nodes, 720.0, 420.0) * 3926.0 <= 720.0);
        assert_eq!(snap_position(-31, -29, 20), (-40, -20));
    }

    #[test]
    fn an_ambiguous_edge_is_deterministic_and_prefers_largest_overlap() {
        let entries = [
            ("a".into(), node(0, 0, 1920, 1080)),
            ("b".into(), node(1920, 900, 800, 600)),
            ("c".into(), node(1920, 0, 2560, 1440)),
        ];
        let forward = infer_neighbors(
            &PoolTopology {
                nodes: entries.clone().into_iter().collect(),
            },
            48,
        );
        let reverse = infer_neighbors(
            &PoolTopology {
                nodes: entries.into_iter().rev().collect(),
            },
            48,
        );
        assert_eq!(forward.nodes["a"].neighbors["right"], "c");
        assert_eq!(forward.nodes["c"].neighbors["left"], "a");
        for name in ["a", "b", "c"] {
            assert_eq!(forward.nodes[name].neighbors, reverse.nodes[name].neighbors);
        }
    }

    #[test]
    fn infer_horizontal_neighbors() {
        let mut nodes = HashMap::new();
        nodes.insert("asus".into(), node(0, 0, 1920, 1080));
        nodes.insert("acer".into(), node(1920, 0, 1920, 1080));
        let topo = infer_neighbors(&PoolTopology { nodes }, DEFAULT_EDGE_TOLERANCE_PX);
        assert_eq!(
            topo.nodes["asus"].neighbors.get("right"),
            Some(&"acer".into())
        );
        assert_eq!(
            topo.nodes["acer"].neighbors.get("left"),
            Some(&"asus".into())
        );
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

    #[test]
    fn infer_skips_clipboard_only_nodes() {
        let mut nodes = HashMap::new();
        nodes.insert("asus".into(), node(0, 0, 1344, 756));
        nodes.insert("acer".into(), node(1344, 0, 1366, 768));
        let mut p2 = node(2710, 0, 1344, 756);
        p2.kvm_enabled = false;
        nodes.insert("gbs-p2".into(), p2);
        let topo = infer_neighbors(&PoolTopology { nodes }, DEFAULT_EDGE_TOLERANCE_PX);
        assert_eq!(
            topo.nodes["asus"].neighbors.get("right"),
            Some(&"acer".into())
        );
        assert_eq!(
            topo.nodes["acer"].neighbors.get("left"),
            Some(&"asus".into())
        );
        assert!(!topo.nodes["acer"].neighbors.contains_key("right"));
        assert!(topo.nodes["gbs-p2"].neighbors.is_empty());
    }
}
