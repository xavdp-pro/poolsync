use crate::kvm_x11::{self, set_cursor_visible_best_effort, KvmDisplay};
use crate::kvm_input::{GrabEvent, InputGrab};
use crate::state::AgentState;
use anyhow::Result;
use poolsync_core::{encode_message, Direction, KvmDesktopInfo, Message, ScreenInfo};
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::info;

const RECENTER_PX: i32 = 32;
const SWITCH_COOLDOWN_MS: u64 = 500;
const EDGE_ARM_PX: i32 = 24;
const EDGE_BLOCK_MS: u64 = 700;

/// Landing position after crossing a pool edge (must be past EDGE_ARM_PX to unblock return).
fn entry_inset_from_left(edge: i32) -> i32 {
    edge + EDGE_ARM_PX + 1
}

fn entry_inset_from_right(width: i32, edge: i32) -> i32 {
    (width - edge - EDGE_ARM_PX - 1).max(entry_inset_from_left(edge))
}
const PHYSICAL_CLAIM_PX: i32 = 3;
const PHYSICAL_CLAIM_COOLDOWN_MS: u64 = 250;
const PHYSICAL_INSTANT_COOLDOWN_MS: u64 = 80;

#[derive(Clone, Copy)]
enum PhysicalClaimReason {
    Motion,
    Key,
    Button,
}

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
    let mut local_kvm_info = local_kvm_info_from_config(state);
    // Taille primaire live (RandR) — utilisée pour les bords locaux sans attendre le hub.
    let mut local_primary = local_screen_from_config(state);
    let mut last_announced: Option<(ScreenInfo, KvmDesktopInfo)> = None;
    let mut last_hello_kvm: Option<bool> = None;

    loop {
        if !state.is_connected() {
            last_announced = None;
            last_hello_kvm = None;
            thread::sleep(poll);
            continue;
        }

        let effective = state.kvm_effective();
        if last_hello_kvm != Some(effective) {
            announce_screen_to_hub(state, &local_primary, &local_kvm_info, &out_tx);
            last_hello_kvm = Some(effective);
            if effective {
                info!("KVM pool: nœud actif");
            } else {
                info!("KVM pool: pause locale — hors bords d'écran");
            }
        }

        if state.take_master_claim_request() {
            if state.kvm_enabled() {
                apply_hotkey_master_claim(
                    &local,
                    state,
                    &local_kvm_info,
                    &mut blocked_edges,
                    &out_tx,
                    &mut focus,
                    &mut remote_x,
                    &mut remote_y,
                    &mut input_grab,
                );
                last_switch = Instant::now();
            }
            thread::sleep(poll);
            continue;
        }

        if !state.kvm_enabled() || !state.local_poolsync_active() {
            focus = local.clone();
            input_grab = None;
            blocked_edges.clear();
            thread::sleep(poll);
            continue;
        }

        // Hotplug HDMI / changement résolution : sonde rapide + annonce hub.
        if last_screen_probe.elapsed() > Duration::from_secs(3) {
            match (
                kvm_x11::kvm_layout_snapshot(),
                kvm_x11::kvm_display(),
            ) {
                (Ok(info), Ok(disp)) => {
                    let primary = ScreenInfo {
                        width: disp.width,
                        height: disp.height,
                    };
                    let changed = last_announced
                        .map(|(s, d)| s != primary || d != info)
                        .unwrap_or(true)
                        || local_primary != primary
                        || local_kvm_info != info;
                    if changed {
                        info!(
                            "KVM écran pool: {}x{} @ ({},{}) bureau {}x{}",
                            disp.width,
                            disp.height,
                            disp.x,
                            disp.y,
                            info.desktop_width,
                            info.desktop_height
                        );
                        local_primary = primary;
                        local_kvm_info = info;
                        if last_announced
                            .map(|(s, d)| s != primary || d != info)
                            .unwrap_or(true)
                        {
                            announce_screen_to_hub(state, &primary, &info, &out_tx);
                            last_announced = Some((primary, info));
                        }
                    }
                }
                (Err(err), _) | (_, Err(err)) => {
                    tracing::debug!("kvm screen probe: {err:#}");
                }
            }
            last_screen_probe = Instant::now();
        }

        prune_blocks(&mut blocked_edges);

        focus = state.kvm_focus();
        let edge = state.config.edge_px as i32;
        let pool = local_display_from_info(&local_kvm_info, local_primary);
        let pool_w = pool.width as i32;
        let pool_h = pool.height as i32;

        // Écran distant : grab souris+clavier, pas de lecture locale.
        if state.is_input_owner() && focus != local {
            if input_grab.is_none() {
                match InputGrab::begin(pool.width, pool.height) {
                    Ok(grab) => {
                        input_grab = Some(grab);
                    }
                    Err(err) => {
                        tracing::warn!("grab KVM distant: {err:#} — retour local immédiat");
                        set_cursor_visible_best_effort(true);
                        let cx = pool_w / 2;
                        let cy = pool_h / 2;
                        let (rx, ry) = pool_to_root(state, &local, cx, cy, &local_kvm_info);
                        do_switch(
                            &local,
                            &local,
                            rx,
                            ry,
                            state,
                            &local_kvm_info,
                            &mut blocked_edges,
                            &out_tx,
                            &mut focus,
                            &mut remote_x,
                            &mut remote_y,
                        );
                        state.set_kvm_focus(&local);
                        thread::sleep(poll);
                        continue;
                    }
                }
            }

            let events = match input_grab.as_mut().expect("grab").poll() {
                Ok(ev) => ev,
                Err(err) => {
                    tracing::warn!("poll KVM grab: {err:#}");
                    input_grab = None;
                    set_cursor_visible_best_effort(true);
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

            if relay_motion.0 != 0 || relay_motion.1 != 0 {
                let focus_primary = target_screen(state, &focus);
                let focus_layout = kvm_layout_for(state, &focus, &local_kvm_info);
                let focus_desktop = focus_layout.desktop_bounds(focus_primary.clone());
                let focus_pool = focus_layout.primary_bounds(focus_primary);

                remote_x += relay_motion.0;
                remote_y += relay_motion.1;
                (remote_x, remote_y) = focus_desktop.clamp(remote_x, remote_y);
                if (remote_x, remote_y) != last_sent {
                    send_mouse_absolute(&focus, remote_x, remote_y, &out_tx);
                    last_sent = (remote_x, remote_y);
                }
                relay_motion = (0, 0);

                let (plx, ply) = focus_pool.to_local(remote_x, remote_y);
                let on_primary = focus_pool.contains(remote_x, remote_y);

                if on_primary && plx > edge + EDGE_ARM_PX {
                    unblock(&mut blocked_edges, BlockedEdge::Left);
                }
                if on_primary && plx < focus_pool.width as i32 - edge - EDGE_ARM_PX {
                    unblock(&mut blocked_edges, BlockedEdge::Right);
                }

                if last_switch.elapsed() >= Duration::from_millis(SWITCH_COOLDOWN_MS) {
                    if on_primary
                        && plx <= edge
                        && !is_blocked(&blocked_edges, BlockedEdge::Left)
                    {
                        if let Some(back) = neighbor_of(&focus, Direction::Left, state) {
                            let bs = target_screen(state, &back);
                            let ty = map_coord(ply, focus_pool.height, bs.height);
                            let entry_x =
                                entry_inset_from_right(bs.width as i32, edge);
                            let (rx, ry) =
                                pool_to_root(state, &back, entry_x, ty, &local_kvm_info);
                            do_switch(
                                &local,
                                &back,
                                rx,
                                ry,
                                state,
                                &local_kvm_info,
                                &mut blocked_edges,
                                &out_tx,
                                &mut focus,
                                &mut remote_x,
                                &mut remote_y,
                            );
                            block_edge(&mut blocked_edges, BlockedEdge::Right);
                            if back == local {
                                input_grab = None;
                            } else if let Some(g) = input_grab.as_mut() {
                                g.recenter(pool.width, pool.height);
                            }
                            last_switch = Instant::now();
                        }
                    } else if on_primary
                        && plx >= focus_pool.width as i32 - edge
                        && !is_blocked(&blocked_edges, BlockedEdge::Right)
                    {
                        if let Some(back) = neighbor_of(&focus, Direction::Right, state) {
                            let bs = target_screen(state, &back);
                            let ty = map_coord(ply, focus_pool.height, bs.height);
                            let (rx, ry) = pool_to_root(
                                state,
                                &back,
                                entry_inset_from_left(edge),
                                ty,
                                &local_kvm_info,
                            );
                            do_switch(
                                &local,
                                &back,
                                rx,
                                ry,
                                state,
                                &local_kvm_info,
                                &mut blocked_edges,
                                &out_tx,
                                &mut focus,
                                &mut remote_x,
                                &mut remote_y,
                            );
                            block_edge(&mut blocked_edges, BlockedEdge::Left);
                            if back == local {
                                input_grab = None;
                            } else if let Some(g) = input_grab.as_mut() {
                                g.recenter(pool.width, pool.height);
                            }
                            last_switch = Instant::now();
                        }
                    }
                }
            }

            if let Some(grab) = input_grab.as_mut() {
                if grab.needs_recenter(RECENTER_PX) {
                    grab.recenter(pool.width, pool.height);
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

        // Primary dynamique : clic / touche sur esclave → master immédiat.
        // Mouvement souris seulement si pas de pilotage KVM distant (sinon = inject).
        if !state.is_input_owner() {
            if input_grab.is_some() {
                input_grab = None;
                set_cursor_visible_best_effort(true);
            }
            let instant = kvm_x11::poll_physical_input();
            let reason = match instant {
                Some(kvm_x11::PhysicalInput::Key) if !state.inject_blocks_key_claim() => {
                    Some(PhysicalClaimReason::Key)
                }
                Some(kvm_x11::PhysicalInput::Button) if !state.inject_blocks_button_claim() => {
                    Some(PhysicalClaimReason::Button)
                }
                _ if state.motion_claim_allowed(&local, px, py) => Some(PhysicalClaimReason::Motion),
                _ => None,
            };
            if let Some(reason) = reason {
                if try_physical_claim(
                px,
                py,
                last_phys,
                last_switch,
                reason,
                &local,
                state,
                &local_kvm_info,
                &mut blocked_edges,
                &out_tx,
                &mut focus,
                &mut remote_x,
                &mut remote_y,
                &mut input_grab,
                ) {
                last_switch = Instant::now();
                last_phys = (px, py);
                thread::sleep(poll);
                continue;
                }
            }
            // Bords pool sur machine esclave (asus pilote ailleurs) : souris physique locale.
            // Au bord d'écran, autoriser le switch même pendant pilotage distant (retour acer→asus).
            let at_pool_edge = pool.contains(px, py) && {
                let (lx, ly) = pool.to_local(px, py);
                lx < edge
                    || lx >= pool_w - edge
                    || ly < edge
                    || ly >= pool_h - edge
            };
            if focus == local
                && (!state.remote_drive_active() || at_pool_edge)
                && last_switch.elapsed() >= Duration::from_millis(SWITCH_COOLDOWN_MS)
                && try_pool_edge_switch(
                    px,
                    py,
                    &pool,
                    pool_w,
                    pool_h,
                    edge,
                    &local,
                    state,
                    &local_kvm_info,
                    &mut blocked_edges,
                    &out_tx,
                    &mut focus,
                    &mut remote_x,
                    &mut remote_y,
                    &mut input_grab,
                    true,
                )
            {
                last_switch = Instant::now();
                last_phys = (px, py);
                thread::sleep(poll);
                continue;
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

            if try_pool_edge_switch(
                px,
                py,
                &pool,
                pool_w,
                pool_h,
                edge,
                &local,
                state,
                &local_kvm_info,
                &mut blocked_edges,
                &out_tx,
                &mut focus,
                &mut remote_x,
                &mut remote_y,
                &mut input_grab,
                false,
            ) {
                last_switch = Instant::now();
            }
        }

        last_phys = (px, py);
        state.set_kvm_focus(&focus);
        thread::sleep(poll);
    }
}

/// Grab souris avant d'envoyer SwitchTo — sinon le distant reçoit le curseur
/// alors que le master n'arrive pas à capturer l'entrée (split-brain).
fn ensure_remote_grab(pool: &KvmDisplay, input_grab: &mut Option<InputGrab>) -> bool {
    if input_grab.is_some() {
        return true;
    }
    match InputGrab::begin(pool.width, pool.height) {
        Ok(grab) => {
            *input_grab = Some(grab);
            true
        }
        Err(err) => {
            tracing::warn!("grab KVM avant switch: {err:#}");
            set_cursor_visible_best_effort(true);
            false
        }
    }
}

/// Détection bords pool (moniteur primaire). `claim_master` = reprendre l'input si esclave.
fn try_pool_edge_switch(
    px: i32,
    py: i32,
    pool: &KvmDisplay,
    pool_w: i32,
    pool_h: i32,
    edge: i32,
    local: &str,
    state: &AgentState,
    local_kvm_info: &KvmDesktopInfo,
    blocked_edges: &mut Vec<(BlockedEdge, Instant)>,
    out_tx: &mpsc::UnboundedSender<String>,
    focus: &mut String,
    remote_x: &mut i32,
    remote_y: &mut i32,
    input_grab: &mut Option<InputGrab>,
    claim_master: bool,
) -> bool {
    if !pool.contains(px, py) {
        return false;
    }
    let (lx, ly) = pool.to_local(px, py);

    if lx >= pool_w - edge && !is_blocked(blocked_edges, BlockedEdge::Right) {
        if let Some(target) = neighbor(state, Direction::Right) {
            if target != local && !ensure_remote_grab(pool, input_grab) {
                return false;
            }
            let ty = map_coord(ly, pool.height, target_screen(state, &target).height);
            let (rx, ry) = pool_to_root(
                state,
                &target,
                entry_inset_from_left(edge),
                ty,
                local_kvm_info,
            );
            if claim_master {
                state.set_kvm_input_node(local);
                state.set_master(local);
                send_master_claim(local, out_tx);
            }
            do_switch(
                local,
                &target,
                rx,
                ry,
                state,
                local_kvm_info,
                blocked_edges,
                out_tx,
                focus,
                remote_x,
                remote_y,
            );
            block_edge(blocked_edges, BlockedEdge::Left);
            let (rx, ry) = pool.to_root(entry_inset_from_right(pool_w, edge), ly);
            let _ = warp_mouse(rx, ry);
            return true;
        }
    } else if lx < edge && !is_blocked(blocked_edges, BlockedEdge::Left) {
        if let Some(target) = neighbor(state, Direction::Left) {
            if target != local && !ensure_remote_grab(pool, input_grab) {
                return false;
            }
            let tw = target_screen(state, &target).width as i32;
            let ty = map_coord(ly, pool.height, target_screen(state, &target).height);
            let entry_x = entry_inset_from_right(tw, edge);
            let (rx, ry) = pool_to_root(state, &target, entry_x, ty, local_kvm_info);
            if claim_master {
                state.set_kvm_input_node(local);
                state.set_master(local);
                send_master_claim(local, out_tx);
            }
            do_switch(
                local,
                &target,
                rx,
                ry,
                state,
                local_kvm_info,
                blocked_edges,
                out_tx,
                focus,
                remote_x,
                remote_y,
            );
            block_edge(blocked_edges, BlockedEdge::Right);
            let (rx, ry) = pool.to_root(entry_inset_from_left(edge), ly);
            let _ = warp_mouse(rx, ry);
            return true;
        }
    } else if ly < edge && !is_blocked(blocked_edges, BlockedEdge::Up) {
        if let Some(target) = neighbor(state, Direction::Up) {
            if target != local && !ensure_remote_grab(pool, input_grab) {
                return false;
            }
            let th = target_screen(state, &target).height as i32;
            let tx = map_coord(lx, pool.width, target_screen(state, &target).width);
            let (rx, ry) = pool_to_root(
                state,
                &target,
                tx,
                entry_inset_from_right(th, edge),
                local_kvm_info,
            );
            if claim_master {
                state.set_kvm_input_node(local);
                state.set_master(local);
                send_master_claim(local, out_tx);
            }
            do_switch(
                local,
                &target,
                rx,
                ry,
                state,
                local_kvm_info,
                blocked_edges,
                out_tx,
                focus,
                remote_x,
                remote_y,
            );
            block_edge(blocked_edges, BlockedEdge::Down);
            let (rx, ry) = pool.to_root(lx, entry_inset_from_left(edge));
            let _ = warp_mouse(rx, ry);
            return true;
        }
    } else if ly >= pool_h - edge && !is_blocked(blocked_edges, BlockedEdge::Down) {
        if let Some(target) = neighbor(state, Direction::Down) {
            if target != local && !ensure_remote_grab(pool, input_grab) {
                return false;
            }
            let tx = map_coord(lx, pool.width, target_screen(state, &target).width);
            let (rx, ry) = pool_to_root(
                state,
                &target,
                tx,
                entry_inset_from_left(edge),
                local_kvm_info,
            );
            if claim_master {
                state.set_kvm_input_node(local);
                state.set_master(local);
                send_master_claim(local, out_tx);
            }
            do_switch(
                local,
                &target,
                rx,
                ry,
                state,
                local_kvm_info,
                blocked_edges,
                out_tx,
                focus,
                remote_x,
                remote_y,
            );
            block_edge(blocked_edges, BlockedEdge::Up);
            let (rx, ry) = pool.to_root(lx, entry_inset_from_right(pool_h, edge));
            let _ = warp_mouse(rx, ry);
            return true;
        }
    }
    false
}

fn block_edge(blocked: &mut Vec<(BlockedEdge, Instant)>, edge: BlockedEdge) {
    blocked.retain(|(e, _)| *e != edge);
    blocked.push((edge, Instant::now() + Duration::from_millis(EDGE_BLOCK_MS)));
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
    blocked.iter().any(|(e, until)| *e == edge && *until > now)
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
    local_info: &KvmDesktopInfo,
    blocked_edges: &mut Vec<(BlockedEdge, Instant)>,
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
    if target != input {
        block_entry_edge(blocked_edges, state, target, x, y, local_info);
    }
}

fn block_entry_edge(
    blocked: &mut Vec<(BlockedEdge, Instant)>,
    state: &AgentState,
    target: &str,
    x: i32,
    y: i32,
    local_info: &KvmDesktopInfo,
) {
    let edge = state.config.edge_px as i32;
    let primary = target_screen(state, target);
    let pool = kvm_layout_for(state, target, local_info).primary_bounds(primary);
    if !pool.contains(x, y) {
        return;
    }
    let (plx, ply) = pool.to_local(x, y);
    if plx <= edge + EDGE_ARM_PX {
        block_edge(blocked, BlockedEdge::Left);
    } else if plx >= pool.width as i32 - edge - EDGE_ARM_PX {
        block_edge(blocked, BlockedEdge::Right);
    }
    if ply <= edge + EDGE_ARM_PX {
        block_edge(blocked, BlockedEdge::Up);
    } else if ply >= pool.height as i32 - edge - EDGE_ARM_PX {
        block_edge(blocked, BlockedEdge::Down);
    }
}

fn try_physical_claim(
    px: i32,
    py: i32,
    last_phys: (i32, i32),
    last_switch: Instant,
    reason: PhysicalClaimReason,
    local: &str,
    state: &AgentState,
    local_kvm_info: &KvmDesktopInfo,
    blocked_edges: &mut Vec<(BlockedEdge, Instant)>,
    out_tx: &mpsc::UnboundedSender<String>,
    focus: &mut String,
    remote_x: &mut i32,
    remote_y: &mut i32,
    input_grab: &mut Option<InputGrab>,
) -> bool {
    let cooldown = match reason {
        PhysicalClaimReason::Motion => Duration::from_millis(PHYSICAL_CLAIM_COOLDOWN_MS),
        PhysicalClaimReason::Key | PhysicalClaimReason::Button => {
            Duration::from_millis(PHYSICAL_INSTANT_COOLDOWN_MS)
        }
    };
    if last_switch.elapsed() < cooldown {
        return false;
    }

    match reason {
        PhysicalClaimReason::Motion => {
            let dx = (px - last_phys.0).abs();
            let dy = (py - last_phys.1).abs();
            if dx + dy < PHYSICAL_CLAIM_PX {
                return false;
            }
        }
        PhysicalClaimReason::Key | PhysicalClaimReason::Button => {}
    }

    info!(
        "KVM primary → {local} ({})",
        match reason {
            PhysicalClaimReason::Motion => "mouvement souris",
            PhysicalClaimReason::Key => "clavier",
            PhysicalClaimReason::Button => "clic souris",
        }
    );
    state.set_kvm_input_node(local);
    state.set_master(local);
    send_master_claim(local, out_tx);
    do_switch(
        local,
        local,
        px,
        py,
        state,
        local_kvm_info,
        blocked_edges,
        out_tx,
        focus,
        remote_x,
        remote_y,
    );
    state.set_kvm_focus(local);
    *input_grab = None;
    blocked_edges.clear();
    true
}

/// Ctrl+Alt+Shift+M : cette machine reprend clavier/souris (master + focus local).
fn apply_hotkey_master_claim(
    local: &str,
    state: &AgentState,
    local_kvm_info: &KvmDesktopInfo,
    blocked_edges: &mut Vec<(BlockedEdge, Instant)>,
    out_tx: &mpsc::UnboundedSender<String>,
    focus: &mut String,
    remote_x: &mut i32,
    remote_y: &mut i32,
    input_grab: &mut Option<InputGrab>,
) {
    *input_grab = None;
    set_cursor_visible_best_effort(true);
    blocked_edges.clear();
    let (px, py) = kvm_x11::mouse_location().unwrap_or((0, 0));
    info!("KVM master réclamé via raccourci → {local}");
    state.set_kvm_input_node(local);
    state.set_master(local);
    send_master_claim(local, out_tx);
    do_switch(
        local,
        local,
        px,
        py,
        state,
        local_kvm_info,
        blocked_edges,
        out_tx,
        focus,
        remote_x,
        remote_y,
    );
    state.set_kvm_focus(local);
}

fn local_kvm_info_from_config(state: &AgentState) -> KvmDesktopInfo {
    state
        .topology_node(&state.config.node)
        .map(|n| KvmDesktopInfo {
            monitor_x: n.monitor_x,
            monitor_y: n.monitor_y,
            desktop_x: n.desktop_x,
            desktop_y: n.desktop_y,
            desktop_width: n.desktop_width,
            desktop_height: n.desktop_height,
        })
        .unwrap_or_default()
}

fn kvm_layout_for(
    state: &AgentState,
    node: &str,
    local_info: &KvmDesktopInfo,
) -> KvmDesktopInfo {
    if node == state.config.node {
        return *local_info;
    }
    state
        .topology_node(node)
        .map(|n| KvmDesktopInfo {
            monitor_x: n.monitor_x,
            monitor_y: n.monitor_y,
            desktop_x: n.desktop_x,
            desktop_y: n.desktop_y,
            desktop_width: n.desktop_width,
            desktop_height: n.desktop_height,
        })
        .unwrap_or_default()
}

fn local_display_from_info(info: &KvmDesktopInfo, primary: ScreenInfo) -> KvmDisplay {
    let r = info.primary_bounds(primary);
    KvmDisplay {
        x: r.x,
        y: r.y,
        width: r.width,
        height: r.height,
    }
}

fn pool_to_root(
    state: &AgentState,
    node: &str,
    lx: i32,
    ly: i32,
    local_info: &KvmDesktopInfo,
) -> (i32, i32) {
    let primary = target_screen(state, node);
    let layout = kvm_layout_for(state, node, local_info);
    let pool = layout.primary_bounds(primary.clone());
    pool.to_root(
        lx.clamp(0, primary.width as i32 - 1),
        ly.clamp(0, primary.height as i32 - 1),
    )
}

fn local_display_from_config(state: &AgentState) -> KvmDisplay {
    KvmDisplay::from_screen_at_origin(local_screen_from_config(state))
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

pub async fn detect_kvm_desktop() -> Option<KvmDesktopInfo> {
    tokio::task::spawn_blocking(|| kvm_x11::kvm_layout_snapshot().ok())
        .await
        .ok()
        .flatten()
}

pub async fn detect_screen() -> Option<ScreenInfo> {
    tokio::task::spawn_blocking(|| kvm_x11::kvm_display().ok().map(|d| d.screen_info()))
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
                // Voisin clip-only (ex. gbs-p2) : ne pas bloquer le fallback config.
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
    let mapped = ((ratio * to.max(1) as f64) - 0.5).round() as i32;
    mapped.clamp(0, to.max(1) as i32 - 1)
}

fn announce_screen_to_hub(
    state: &AgentState,
    screen: &ScreenInfo,
    kvm_desktop: &KvmDesktopInfo,
    out_tx: &mpsc::UnboundedSender<String>,
) {
    let payload = match encode_message(&Message::Hello {
        node: state.config.node.clone(),
        mode: state.config.mode,
        screen: screen.clone(),
        neighbors: state.config.neighbors.clone(),
        kvm_enabled: state.kvm_effective(),
        kvm_desktop: *kvm_desktop,
        clipboard_sync: state.clipboard_sync_enabled(),
        local_active: state.local_poolsync_active(),
        monitors: crate::kvm_x11::described_monitors().unwrap_or_default(),
    }) {
        Ok(p) => p,
        Err(err) => {
            tracing::warn!("encode Hello screen update: {err:#}");
            return;
        }
    };
    if out_tx.send(payload).is_ok() {
        info!(
            "écran annoncé au hub: {}x{} (bureau {}x{})",
            screen.width, screen.height, kvm_desktop.desktop_width, kvm_desktop.desktop_height
        );
    }
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

pub async fn inject_input(kind: &poolsync_core::InputKind) -> Result<()> {
    let kind = kind.clone();
    tokio::task::spawn_blocking(move || inject_input_sync(&kind)).await?
}

fn inject_input_sync(kind: &poolsync_core::InputKind) -> Result<()> {
    kvm_x11::set_injecting(true);
    let result = inject_input_sync_inner(kind);
    kvm_x11::set_injecting(false);
    result
}

fn inject_input_sync_inner(kind: &poolsync_core::InputKind) -> Result<()> {
    let desktop = kvm_x11::kvm_desktop()
        .ok()
        .or_else(|| kvm_x11::kvm_display().ok());
    let clamp = |x: i32, y: i32| {
        if let Some(d) = desktop {
            (
                x.clamp(d.x, d.x + d.width as i32 - 1),
                y.clamp(d.y, d.y + d.height as i32 - 1),
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

#[cfg(test)]
mod tests {
    use super::map_coord;

    #[test]
    fn map_coord_scales_between_screens() {
        // Milieu d'un écran 1000 → milieu d'un écran 500.
        assert_eq!(map_coord(500, 1000, 500), 250);
        // Bords bornés dans l'écran cible.
        assert_eq!(map_coord(0, 1000, 500), 0);
        assert_eq!(map_coord(999, 1000, 500), 499);
    }

    #[test]
    fn map_coord_same_size_keeps_position() {
        assert_eq!(map_coord(0, 800, 800), 0);
        assert_eq!(map_coord(400, 800, 800), 400);
        assert_eq!(map_coord(799, 800, 800), 799);
    }
}
