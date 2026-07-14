use crate::kvm_input::{hide_cursor_best_effort, GrabEvent, InputGrab};
use crate::kvm_x11;
use crate::state::AgentState;
use anyhow::Result;
use poolsync_core::{decode_message, encode_message, Direction, Message, ScreenInfo};
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::info;

const RECENTER_PX: i32 = 48;
const SWITCH_COOLDOWN_MS: u64 = 500;
const EDGE_ARM_PX: i32 = 24;
const EDGE_BLOCK_MS: u64 = 700;
const PHYSICAL_CLAIM_PX: i32 = 3;
const CLAIM_COOLDOWN_MS: u64 = 1500;

#[derive(Clone, Copy, PartialEq, Eq)]
enum BlockedEdge {
    Left,
    Right,
    Up,
    Down,
}

pub fn kvm_poll_loop(state: &AgentState, out_tx: mpsc::UnboundedSender<String>) {
    let poll = Duration::from_millis(state.config.input_poll_ms);
    let local = state.config.node.clone();
    let mut focus = local.clone();
    let mut remote_x = 0i32;
    let mut remote_y = 0i32;
    let mut last_phys = (0i32, 0i32);
    let mut phys_seeded = false;
    let mut input_grab: Option<InputGrab> = None;
    let mut relay_motion = (0i32, 0i32);
    let mut last_sent = (-1i32, -1i32);
    let mut last_switch = Instant::now()
        .checked_sub(Duration::from_secs(1))
        .unwrap_or_else(Instant::now);
    let mut blocked_edges: Vec<(BlockedEdge, Instant)> = Vec::new();

    let mut last_screen_probe = Instant::now()
        .checked_sub(Duration::from_secs(60))
        .unwrap_or_else(Instant::now);
    let mut local_screen_cache = local_screen_from_config(state);

    loop {
        if !state.is_connected() {
            thread::sleep(poll);
            continue;
        }

        if !state.kvm_enabled() {
            focus = local.clone();
            input_grab = None;
            blocked_edges.clear();
            thread::sleep(poll);
            continue;
        }

        if last_screen_probe.elapsed() > Duration::from_secs(15) {
            if let Ok((w, h)) = kvm_x11::display_size() {
                local_screen_cache = ScreenInfo { width: w, height: h };
            }
            last_screen_probe = Instant::now();
        }

        prune_blocks(&mut blocked_edges);

        focus = state.kvm_focus();
        let edge = state.config.edge_px as i32;
        let screen = local_screen_cache.clone();

        // Écran distant : grab souris+clavier, pas de lecture locale.
        if state.is_input_owner() && focus != local {
            if input_grab.is_none() {
                hide_cursor_best_effort(screen.width, screen.height);
                match InputGrab::begin(screen.width, screen.height) {
                    Ok(grab) => input_grab = Some(grab),
                    Err(err) => {
                        tracing::warn!("grab KVM distant: {err:#}");
                        thread::sleep(poll);
                        continue;
                    }
                }
            }

            let grab = input_grab.as_mut().expect("grab");
            let events = match grab.poll() {
                Ok(ev) => ev,
                Err(err) => {
                    tracing::warn!("poll KVM grab: {err:#}");
                    input_grab = None;
                    thread::sleep(poll);
                    continue;
                }
            };

            for ev in events {
                match ev {
                    GrabEvent::Motion { dx, dy } => {
                        relay_motion.0 += dx;
                        relay_motion.1 += dy;
                    }
                    GrabEvent::Button { button, pressed } => {
                        send_mouse_button(&focus, remote_x, remote_y, button, pressed, &out_tx);
                    }
                    GrabEvent::Key { keycode, pressed } => {
                        send_key(&focus, keycode, pressed, &out_tx);
                    }
                }
            }

            if relay_motion.0.abs() > RECENTER_PX || relay_motion.1.abs() > RECENTER_PX {
                grab.recenter(screen.width, screen.height);
                relay_motion = (0, 0);
            } else if relay_motion.0 != 0 || relay_motion.1 != 0 {
                let ts = target_screen(state, &focus);
                remote_x = (remote_x + relay_motion.0).clamp(0, ts.width as i32 - 1);
                remote_y = (remote_y + relay_motion.1).clamp(0, ts.height as i32 - 1);
                if (remote_x, remote_y) != last_sent {
                    send_mouse_absolute(&focus, remote_x, remote_y, &out_tx);
                    last_sent = (remote_x, remote_y);
                }
                relay_motion = (0, 0);

                if remote_x > edge + EDGE_ARM_PX {
                    unblock(&mut blocked_edges, BlockedEdge::Left);
                }
                if remote_x < ts.width as i32 - edge - EDGE_ARM_PX {
                    unblock(&mut blocked_edges, BlockedEdge::Right);
                }

                if last_switch.elapsed() >= Duration::from_millis(SWITCH_COOLDOWN_MS) {
                    if remote_x <= edge && !is_blocked(&blocked_edges, BlockedEdge::Left) {
                        if let Some(back) = neighbor_of(&focus, Direction::Left, state) {
                            let bs = target_screen(state, &back);
                            let ty = map_coord(remote_y, ts.height, bs.height);
                            do_switch(
                                &local,
                                &back,
                                bs.width as i32 - 1,
                                ty,
                                state,
                                &out_tx,
                                &mut focus,
                                &mut remote_x,
                                &mut remote_y,
                            );
                            block_edge(&mut blocked_edges, BlockedEdge::Right);
                            if back == local {
                                input_grab = None;
                            } else if let Some(g) = input_grab.as_mut() {
                                g.recenter(screen.width, screen.height);
                            }
                            last_switch = Instant::now();
                        }
                    } else if remote_x >= ts.width as i32 - edge
                        && !is_blocked(&blocked_edges, BlockedEdge::Right)
                    {
                        if let Some(back) = neighbor_of(&focus, Direction::Right, state) {
                            let bs = target_screen(state, &back);
                            let ty = map_coord(remote_y, ts.height, bs.height);
                            do_switch(&local, &back, 0, ty, state, &out_tx, &mut focus, &mut remote_x, &mut remote_y);
                            block_edge(&mut blocked_edges, BlockedEdge::Left);
                            if back == local {
                                input_grab = None;
                            } else if let Some(g) = input_grab.as_mut() {
                                g.recenter(screen.width, screen.height);
                            }
                            last_switch = Instant::now();
                        }
                    }
                }
            }

            state.set_kvm_focus(&focus);
            thread::sleep(poll);
            continue;
        }

        let Ok((px, py)) = kvm_x11::mouse_location() else {
            thread::sleep(poll);
            continue;
        };

        if !phys_seeded {
            last_phys = (px, py);
            phys_seeded = true;
            thread::sleep(poll);
            continue;
        }

        // Primary dynamique : souris/clavier physique sur cette machine → prend le relais.
        if !state.is_input_owner() {
            if !state.kvm_inject_grace_active()
                && last_switch.elapsed() >= Duration::from_millis(CLAIM_COOLDOWN_MS)
            {
                let dx = (px - last_phys.0).abs();
                let dy = (py - last_phys.1).abs();
                if dx + dy >= PHYSICAL_CLAIM_PX {
                    let screen = local_screen_cache.clone();
                    let cx = screen.width as i32 / 2;
                    let cy = screen.height as i32 / 2;
                    info!("KVM primary → {local} (activité physique)");
                    state.set_kvm_input_node(&local);
                    state.set_master(&local);
                    send_master_claim(&local, &out_tx);
                    do_switch(&local, &local, cx, cy, state, &out_tx, &mut focus, &mut remote_x, &mut remote_y);
                    state.set_kvm_focus(&local);
                    input_grab = None;
                    blocked_edges.clear();
                    last_switch = Instant::now();
                    last_phys = (px, py);
                }
            }
            last_phys = (px, py);
            thread::sleep(poll);
            continue;
        }

        if focus == local {
            input_grab = None;
            last_sent = (-1, -1);

            if last_switch.elapsed() < Duration::from_millis(SWITCH_COOLDOWN_MS) {
                last_phys = (px, py);
                thread::sleep(poll);
                continue;
            }

            if px >= screen.width as i32 - edge && !is_blocked(&blocked_edges, BlockedEdge::Right) {
                if let Some(target) = neighbor(state, Direction::Right) {
                    let ty = map_coord(py, screen.height, target_screen(state, &target).height);
                    do_switch(&local, &target, 0, ty, state, &out_tx, &mut focus, &mut remote_x, &mut remote_y);
                    block_edge(&mut blocked_edges, BlockedEdge::Left);
                    let _ = warp_mouse(screen.width as i32 - edge - 1, py);
                    last_switch = Instant::now();
                }
            } else if px < edge && !is_blocked(&blocked_edges, BlockedEdge::Left) {
                if let Some(target) = neighbor(state, Direction::Left) {
                    let tw = target_screen(state, &target).width as i32;
                    let ty = map_coord(py, screen.height, target_screen(state, &target).height);
                    do_switch(&local, &target, tw - 1, ty, state, &out_tx, &mut focus, &mut remote_x, &mut remote_y);
                    block_edge(&mut blocked_edges, BlockedEdge::Right);
                    let _ = warp_mouse(edge + 1, py);
                    last_switch = Instant::now();
                }
            } else if py < edge && !is_blocked(&blocked_edges, BlockedEdge::Up) {
                if let Some(target) = neighbor(state, Direction::Up) {
                    let th = target_screen(state, &target).height as i32;
                    let tx = map_coord(px, screen.width, target_screen(state, &target).width);
                    do_switch(&local, &target, tx, th - 1, state, &out_tx, &mut focus, &mut remote_x, &mut remote_y);
                    block_edge(&mut blocked_edges, BlockedEdge::Down);
                    let _ = warp_mouse(px, edge + 1);
                    last_switch = Instant::now();
                }
            } else if py >= screen.height as i32 - edge
                && !is_blocked(&blocked_edges, BlockedEdge::Down)
            {
                if let Some(target) = neighbor(state, Direction::Down) {
                    let tx = map_coord(px, screen.width, target_screen(state, &target).width);
                    do_switch(&local, &target, tx, 0, state, &out_tx, &mut focus, &mut remote_x, &mut remote_y);
                    block_edge(&mut blocked_edges, BlockedEdge::Up);
                    let _ = warp_mouse(px, screen.height as i32 - edge - 1);
                    last_switch = Instant::now();
                }
            }
        }

        last_phys = (px, py);
        state.set_kvm_focus(&focus);
        thread::sleep(poll);
    }
}

fn block_edge(blocked: &mut Vec<(BlockedEdge, Instant)>, edge: BlockedEdge) {
    blocked.retain(|(e, _)| *e != edge);
    blocked.push((
        edge,
        Instant::now() + Duration::from_millis(EDGE_BLOCK_MS),
    ));
}

fn unblock(blocked: &mut Vec<(BlockedEdge, Instant)>, edge: BlockedEdge) {
    blocked.retain(|(e, _)| *e != edge);
}

fn prune_blocks(blocked: &mut Vec<(BlockedEdge, Instant)>) {
    let now = Instant::now();
    blocked.retain(|(_, until)| *until > now);
}

fn is_blocked(blocked: &[(BlockedEdge, Instant)], edge: BlockedEdge) -> bool {
    let now = Instant::now();
    blocked
        .iter()
        .any(|(e, until)| *e == edge && *until > now)
}

fn send_mouse_button(
    target: &str,
    x: i32,
    y: i32,
    button: u8,
    pressed: bool,
    out_tx: &mpsc::UnboundedSender<String>,
) {
    if let Ok(payload) = encode_message(&Message::Input {
        target: target.to_string(),
        kind: poolsync_core::InputKind::MouseButton {
            button,
            pressed,
            x,
            y,
        },
    }) {
        let _ = out_tx.send(payload);
    }
}

fn send_key(target: &str, keycode: u8, pressed: bool, out_tx: &mpsc::UnboundedSender<String>) {
    if keycode == 0 {
        return;
    }
    if let Ok(payload) = encode_message(&Message::Input {
        target: target.to_string(),
        kind: poolsync_core::InputKind::Key {
            keycode: u32::from(keycode),
            pressed,
        },
    }) {
        let _ = out_tx.send(payload);
    }
}

fn do_switch(
    input: &str,
    target: &str,
    x: i32,
    y: i32,
    state: &AgentState,
    out_tx: &mpsc::UnboundedSender<String>,
    focus: &mut String,
    remote_x: &mut i32,
    remote_y: &mut i32,
) {
    switch_to(input, target, x, y, out_tx);
    state.set_kvm_input_node(input);
    *focus = target.to_string();
    *remote_x = x;
    *remote_y = y;
}

fn local_screen_from_config(state: &AgentState) -> ScreenInfo {
    state
        .topology_node(&state.config.node)
        .map(|n| ScreenInfo {
            width: n.width,
            height: n.height,
        })
        .unwrap_or_else(|| state.config.screen.clone())
}

pub async fn detect_screen() -> Option<ScreenInfo> {
    tokio::task::spawn_blocking(|| {
        kvm_x11::display_size()
            .ok()
            .map(|(w, h)| ScreenInfo { width: w, height: h })
    })
    .await
    .ok()
    .flatten()
}

fn warp_mouse(x: i32, y: i32) -> Result<()> {
    kvm_x11::warp_mouse(x, y)
}

fn target_screen(state: &AgentState, node: &str) -> ScreenInfo {
    state
        .topology_node(node)
        .map(|n| ScreenInfo {
            width: n.width,
            height: n.height,
        })
        .unwrap_or_else(|| state.config.screen.clone())
}

fn neighbor(state: &AgentState, dir: Direction) -> Option<String> {
    neighbor_of(&state.config.node, dir, state)
}

fn neighbor_of(node: &str, dir: Direction, state: &AgentState) -> Option<String> {
    let key = direction_key(dir);
    if let Some(topo) = state.topology() {
        if let Some(n) = topo.nodes.get(node) {
            if let Some(target) = n.neighbors.get(key) {
                if state.target_kvm_enabled(target) {
                    return Some(target.clone());
                }
                return None;
            }
        }
    }
    state
        .config
        .neighbors
        .iter()
        .find(|n| n.direction == dir)
        .map(|n| n.node.clone())
        .filter(|t| state.target_kvm_enabled(t))
}

fn direction_key(dir: Direction) -> &'static str {
    match dir {
        Direction::Left => "left",
        Direction::Right => "right",
        Direction::Up => "up",
        Direction::Down => "down",
    }
}

fn map_coord(v: i32, from: u32, to: u32) -> i32 {
    let ratio = (v as f64 + 0.5) / from.max(1) as f64;
    ((ratio * to.max(1) as f64) - 0.5).round() as i32
}

fn send_master_claim(node: &str, out_tx: &mpsc::UnboundedSender<String>) {
    if let Ok(payload) = encode_message(&Message::MasterClaim {
        node: node.to_string(),
        ts: 0,
    }) {
        let _ = out_tx.send(payload);
    }
}

fn switch_to(input: &str, target: &str, x: i32, y: i32, out_tx: &mpsc::UnboundedSender<String>) {
    info!("KVM switch {input} → {target} ({x},{y})");
    if let Ok(payload) = encode_message(&Message::SwitchTo {
        node: target.to_string(),
        x,
        y,
        input_node: input.to_string(),
    }) {
        let _ = out_tx.send(payload);
    }
}

fn send_mouse_absolute(target: &str, x: i32, y: i32, out_tx: &mpsc::UnboundedSender<String>) {
    if let Ok(payload) = encode_message(&Message::Input {
        target: target.to_string(),
        kind: poolsync_core::InputKind::MouseMove { x, y },
    }) {
        let _ = out_tx.send(payload);
    }
}

#[allow(dead_code)]
fn send_mouse_relative(target: &str, dx: i32, dy: i32, out_tx: &mpsc::UnboundedSender<String>) {
    if dx == 0 && dy == 0 {
        return;
    }
    if let Ok(payload) = encode_message(&Message::Input {
        target: target.to_string(),
        kind: poolsync_core::InputKind::MouseMoveRelative { dx, dy },
    }) {
        let _ = out_tx.send(payload);
    }
}

pub async fn inject_input(kind: &poolsync_core::InputKind) -> Result<()> {
    let kind = kind.clone();
    tokio::task::spawn_blocking(move || inject_input_sync(&kind)).await?
}

fn inject_input_sync(kind: &poolsync_core::InputKind) -> Result<()> {
    let screen = kvm_x11::display_size()
        .ok()
        .map(|(w, h)| ScreenInfo { width: w, height: h });
    let clamp = |x: i32, y: i32| {
        if let Some(s) = screen {
            (
                x.clamp(0, s.width as i32 - 1),
                y.clamp(0, s.height as i32 - 1),
            )
        } else {
            (x, y)
        }
    };
    match kind {
        poolsync_core::InputKind::MouseMove { x, y } => {
            let (x, y) = clamp(*x, *y);
            kvm_x11::move_mouse_absolute(x, y)?;
        }
        poolsync_core::InputKind::MouseMoveRelative { dx, dy } => {
            kvm_x11::move_mouse_relative(*dx, *dy)?;
        }
        poolsync_core::InputKind::MouseButton {
            button,
            pressed,
            x,
            y,
        } => {
            let (x, y) = clamp(*x, *y);
            kvm_x11::move_mouse_absolute(x, y)?;
            kvm_x11::mouse_button(*button, *pressed)?;
        }
        poolsync_core::InputKind::Key { keycode, pressed } => {
            kvm_x11::key_event(*keycode, *pressed)?;
        }
        poolsync_core::InputKind::MouseWheel { delta, x, y } => {
            let (x, y) = clamp(*x, *y);
            kvm_x11::move_mouse_absolute(x, y)?;
            kvm_x11::click_wheel_button(if *delta > 0 { 4 } else { 5 })?;
        }
    }
    Ok(())
}

#[allow(dead_code)]
pub fn parse_switch_message(text: &str) -> Option<Message> {
    decode_message(text).ok()
}
