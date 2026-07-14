use crate::state::AgentState;
use anyhow::{Context, Result};
use poolsync_core::{encode_message, InputKind, Message};
use std::thread;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::warn;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt as _, GrabMode};
use x11rb::protocol::Event;

/// Capture clavier sur le master quand le focus KVM est sur un autre écran.
pub fn keyboard_relay_loop(state: &AgentState, out_tx: mpsc::UnboundedSender<String>) {
    let idle = Duration::from_millis(50);
    loop {
        if !state.is_connected() || !state.kvm_enabled() || !state.is_input_owner() {
            thread::sleep(idle);
            continue;
        }
        let local = state.config.node.clone();
        if state.kvm_focus() == local {
            thread::sleep(idle);
            continue;
        }
        if let Err(err) = grab_and_relay(state, &out_tx) {
            warn!("relai clavier KVM: {err:#}");
            thread::sleep(Duration::from_millis(300));
        }
    }
}

fn grab_and_relay(state: &AgentState, out_tx: &mpsc::UnboundedSender<String>) -> Result<()> {
    let (conn, screen_num) = x11rb::connect(None).context("X11 clavier")?;
    let root = conn.setup().roots[screen_num].root;
    conn.grab_keyboard(
        false,
        root,
        x11rb::CURRENT_TIME,
        GrabMode::ASYNC,
        GrabMode::ASYNC,
    )?
    .reply()?;
    conn.flush()?;

    let wait = Duration::from_millis(8);
    loop {
        let local = state.config.node.clone();
        if !state.is_connected()
            || !state.kvm_enabled()
            || !state.is_input_owner()
            || state.kvm_focus() == local
        {
            break;
        }
        let focus = state.kvm_focus();
        if conn.poll_for_event()?.is_none() {
            thread::sleep(wait);
            continue;
        }
        while let Some(event) = conn.poll_for_event()? {
            match event {
                Event::KeyPress(e) => send_key(&focus, e.detail, true, out_tx),
                Event::KeyRelease(e) => send_key(&focus, e.detail, false, out_tx),
                _ => {}
            }
        }
    }

    let _ = conn.ungrab_keyboard(x11rb::CURRENT_TIME)?;
    conn.flush()?;
    Ok(())
}

fn send_key(target: &str, keycode: u8, pressed: bool, out_tx: &mpsc::UnboundedSender<String>) {
    if keycode == 0 {
        return;
    }
    if let Ok(payload) = encode_message(&Message::Input {
        target: target.to_string(),
        kind: InputKind::Key {
            keycode: u32::from(keycode),
            pressed,
        },
    }) {
        let _ = out_tx.send(payload);
    }
}
