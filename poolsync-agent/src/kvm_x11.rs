use anyhow::{Context, Result};
use poolsync_core::ScreenInfo;
use std::cell::{Cell, RefCell};
use tracing::warn;
use x11rb::connection::Connection;
use x11rb::protocol::randr;
use x11rb::protocol::xfixes;
use x11rb::protocol::xproto::{ChangeWindowAttributesAux, ConnectionExt as XprotoExt, EventMask};
use x11rb::protocol::xtest;
use x11rb::protocol::Event;

thread_local! {
    static XDO: RefCell<Option<libxdo::XDo>> = const { RefCell::new(None) };
    static INJECTING: Cell<bool> = const { Cell::new(false) };
    static PASSIVE_EVENTS: Cell<bool> = const { Cell::new(false) };
}

/// Entrée physique locale (clavier / souris) — pas une injection KVM distante.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicalInput {
    Key,
    Button,
}

pub fn set_injecting(active: bool) {
    INJECTING.with(|c| c.set(active));
}

/// Détecte une touche ou un clic physique (pour reprendre le rôle master sur un nœud esclave).
pub fn poll_physical_input() -> Option<PhysicalInput> {
    if INJECTING.with(|c| c.get()) {
        return None;
    }
    with_x11_conn(|conn, screen_num| {
        let root = conn.setup().roots[screen_num].root;
        if !PASSIVE_EVENTS.with(|c| c.get()) {
            conn.change_window_attributes(
                root,
                &ChangeWindowAttributesAux::new()
                    .event_mask(EventMask::KEY_PRESS | EventMask::BUTTON_PRESS),
            )?;
            conn.flush()?;
            PASSIVE_EVENTS.with(|c| c.set(true));
        }
        loop {
            match conn.poll_for_event() {
                Ok(Some(Event::KeyPress(ev))) if ev.detail != 0 => {
                    return Ok(Some(PhysicalInput::Key));
                }
                Ok(Some(Event::ButtonPress(_))) => return Ok(Some(PhysicalInput::Button)),
                Ok(Some(_)) => continue,
                Ok(None) => return Ok(None),
                Err(err) => return Err(err.into()),
            }
        }
    })
    .ok()
    .flatten()
}

/// Écran « pool » KVM : moniteur primaire RandR (pas le bureau X11 étendu).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KvmDisplay {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl KvmDisplay {
    pub fn from_screen_at_origin(screen: ScreenInfo) -> Self {
        Self {
            x: 0,
            y: 0,
            width: screen.width,
            height: screen.height,
        }
    }

    pub fn screen_info(&self) -> ScreenInfo {
        ScreenInfo {
            width: self.width,
            height: self.height,
        }
    }

    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x
            && py >= self.y
            && px < self.x + self.width as i32
            && py < self.y + self.height as i32
    }

    pub fn to_local(&self, px: i32, py: i32) -> (i32, i32) {
        (px - self.x, py - self.y)
    }

    pub fn to_root(&self, lx: i32, ly: i32) -> (i32, i32) {
        (lx + self.x, ly + self.y)
    }

    pub fn clamp_local(&self, lx: i32, ly: i32) -> (i32, i32) {
        (
            lx.clamp(0, self.width as i32 - 1),
            ly.clamp(0, self.height as i32 - 1),
        )
    }

    /// Pixel at the geometric center of this monitor (root coordinates).
    pub fn center_root(&self) -> (i32, i32) {
        (
            self.x + self.width as i32 / 2,
            self.y + self.height as i32 / 2,
        )
    }
}

fn with_xdo<F>(f: F) -> Result<()>
where
    F: FnOnce(&libxdo::XDo) -> libxdo::OpResult,
{
    XDO.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot =
                Some(libxdo::XDo::new(None).map_err(|e| anyhow::anyhow!("libxdo init: {e:?}"))?);
        }
        f(slot.as_ref().expect("xdo")).map_err(|e| anyhow::anyhow!("{e:?}"))
    })
}

fn with_x11_conn<F, T>(f: F) -> Result<T>
where
    F: FnOnce(&x11rb::rust_connection::RustConnection, usize) -> Result<T>,
{
    thread_local! {
        static CONN: RefCell<Option<(x11rb::rust_connection::RustConnection, usize)>> =
            const { RefCell::new(None) };
    }
    CONN.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            let (conn, screen) = x11rb::connect(None).context("connexion X11")?;
            *slot = Some((conn, screen));
        }
        let (conn, screen) = slot.as_mut().expect("x11");
        f(conn, *screen)
    })
}

pub fn mouse_location() -> Result<(i32, i32)> {
    with_x11_conn(|conn, screen_num| {
        let root = conn.setup().roots[screen_num].root;
        let reply = conn.query_pointer(root)?.reply()?;
        Ok((reply.root_x as i32, reply.root_y as i32))
    })
}

pub fn warp_mouse(x: i32, y: i32) -> Result<()> {
    with_xdo(|xdo| xdo.move_mouse(x, y, 0))
}

pub fn move_mouse_absolute(x: i32, y: i32) -> Result<()> {
    warp_mouse(x, y)
}

/// Positionne la souris en coordonnées pool (relatives au moniteur primaire KVM).
pub fn move_mouse_pool(lx: i32, ly: i32) -> Result<()> {
    let display = kvm_display()?;
    let (lx, ly) = display.clamp_local(lx, ly);
    let (rx, ry) = display.to_root(lx, ly);
    warp_mouse(rx, ry)
}

pub fn move_mouse_relative(dx: i32, dy: i32) -> Result<()> {
    if dx == 0 && dy == 0 {
        return Ok(());
    }
    with_xdo(|xdo| xdo.move_mouse_relative(dx, dy))
}

pub fn mouse_button(button: u8, pressed: bool) -> Result<()> {
    let b = i32::from(button);
    with_xdo(|xdo| {
        if pressed {
            xdo.mouse_down(b)
        } else {
            xdo.mouse_up(b)
        }
    })
}

pub fn key_event(keycode: u32, pressed: bool) -> Result<()> {
    let code = keycode as u8;
    if code == 0 {
        return Ok(());
    }
    with_x11_conn(|conn, screen_num| {
        let root = conn.setup().roots[screen_num].root;
        let event_type = if pressed { 2u8 } else { 3u8 };
        xtest::fake_input(conn, event_type, code, 0, root, 0, 0, 0)?;
        conn.flush()?;
        Ok(())
    })
}

pub fn click_wheel_button(button: i32) -> Result<()> {
    with_xdo(|xdo| xdo.click(button))
}

pub fn set_cursor_visible(visible: bool) -> Result<()> {
    with_x11_conn(|conn, screen_num| {
        let root = conn.setup().roots[screen_num].root;
        if visible {
            xfixes::show_cursor(conn, root)?;
        } else {
            xfixes::hide_cursor(conn, root)?;
        }
        conn.flush()?;
        Ok(())
    })
}

pub fn set_cursor_visible_best_effort(visible: bool) {
    if let Err(err) = set_cursor_visible(visible) {
        warn!("curseur X11 (visible={visible}): {err:#}");
    }
}

/// Taille du bureau X11 complet (tous écrans) — éviter pour le KVM pool.
pub fn display_size() -> Result<(u32, u32)> {
    with_x11_conn(|conn, screen_num| {
        let screen = &conn.setup().roots[screen_num];
        Ok((
            screen.width_in_pixels as u32,
            screen.height_in_pixels as u32,
        ))
    })
}

/// Moniteur primaire RandR : bords KVM + coordonnées pool (style Barrier).
pub fn kvm_display() -> Result<KvmDisplay> {
    with_x11_conn(|conn, screen_num| {
        let root = conn.setup().roots[screen_num].root;
        let primary_out = randr::get_output_primary(conn, root)
            .ok()
            .and_then(|c| c.reply().ok())
            .map(|r| r.output);

        let res = randr::get_screen_resources_current(conn, root)
            .context("randr get_screen_resources")?
            .reply()?;

        let mut fallback: Option<KvmDisplay> = None;

        for &crtc_id in &res.crtcs {
            let crtc = match randr::get_crtc_info(conn, crtc_id, res.config_timestamp) {
                Ok(cookie) => match cookie.reply() {
                    Ok(info) => info,
                    Err(_) => continue,
                },
                Err(_) => continue,
            };
            if crtc.width == 0 || crtc.height == 0 {
                continue;
            }
            let disp = KvmDisplay {
                x: crtc.x as i32,
                y: crtc.y as i32,
                width: crtc.width as u32,
                height: crtc.height as u32,
            };
            if let Some(primary) = primary_out {
                if crtc.outputs.contains(&primary) {
                    return Ok(disp);
                }
            }
            let area = disp.width.saturating_mul(disp.height);
            if fallback
                .map(|b| area > b.width.saturating_mul(b.height))
                .unwrap_or(true)
            {
                fallback = Some(disp);
            }
        }

        fallback.context("aucun moniteur actif (RandR)")
    })
}

/// All active RandR CRTCs (each physical monitor).
pub fn active_monitors() -> Result<Vec<KvmDisplay>> {
    with_x11_conn(|conn, screen_num| {
        let root = conn.setup().roots[screen_num].root;
        let res = randr::get_screen_resources_current(conn, root)
            .context("randr get_screen_resources")?
            .reply()?;
        let mut out = Vec::new();
        for &crtc_id in &res.crtcs {
            let crtc = match randr::get_crtc_info(conn, crtc_id, res.config_timestamp) {
                Ok(cookie) => match cookie.reply() {
                    Ok(info) => info,
                    Err(_) => continue,
                },
                Err(_) => continue,
            };
            if crtc.width == 0 || crtc.height == 0 {
                continue;
            }
            out.push(KvmDisplay {
                x: crtc.x as i32,
                y: crtc.y as i32,
                width: crtc.width as u32,
                height: crtc.height as u32,
            });
        }
        if out.is_empty() {
            let screen = &conn.setup().roots[screen_num];
            out.push(KvmDisplay {
                x: 0,
                y: 0,
                width: screen.width_in_pixels as u32,
                height: screen.height_in_pixels as u32,
            });
        }
        Ok(out)
    })
}

/// Warp the pointer to the center of the monitor that currently contains it.
/// Falls back to the KVM primary if the pointer is not on any CRTC.
pub fn center_pointer_on_current_monitor() -> Result<(i32, i32)> {
    let (px, py) = mouse_location()?;
    let monitors = active_monitors()?;
    let mon = monitors
        .iter()
        .find(|m| m.contains(px, py))
        .copied()
        .or_else(|| monitors.into_iter().next())
        .context("aucun moniteur")?;
    let (cx, cy) = mon.center_root();
    warp_mouse(cx, cy)?;
    Ok((cx, cy))
}

/// Rectangle englobant tous les moniteurs actifs (bureau X11 étendu).
pub fn kvm_desktop() -> Result<KvmDisplay> {
    with_x11_conn(|conn, screen_num| {
        let root = conn.setup().roots[screen_num].root;
        let res = randr::get_screen_resources_current(conn, root)
            .context("randr get_screen_resources")?
            .reply()?;

        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;
        let mut any = false;

        for &crtc_id in &res.crtcs {
            let crtc = match randr::get_crtc_info(conn, crtc_id, res.config_timestamp) {
                Ok(cookie) => match cookie.reply() {
                    Ok(info) => info,
                    Err(_) => continue,
                },
                Err(_) => continue,
            };
            if crtc.width == 0 || crtc.height == 0 {
                continue;
            }
            any = true;
            let cx = crtc.x as i32;
            let cy = crtc.y as i32;
            let cw = crtc.width as i32;
            let ch = crtc.height as i32;
            min_x = min_x.min(cx);
            min_y = min_y.min(cy);
            max_x = max_x.max(cx + cw);
            max_y = max_y.max(cy + ch);
        }

        if !any {
            return kvm_display();
        }

        Ok(KvmDisplay {
            x: min_x,
            y: min_y,
            width: (max_x - min_x).max(1) as u32,
            height: (max_y - min_y).max(1) as u32,
        })
    })
}


/// Repousse le curseur a l'interieur du moniteur pool apres un SwitchTo (evite rebond immediat).
pub fn nudge_kvm_enter(x: i32, y: i32, edge: i32) -> Result<(i32, i32)> {
    const ENTRY_ARM_PX: i32 = 24;
    let inset = edge + ENTRY_ARM_PX + 1;
    let pool = kvm_display()?;
    let (mut lx, mut ly) = pool.to_local(x, y);
    let w = pool.width as i32;
    let h = pool.height as i32;
    if lx <= edge {
        lx = inset;
    } else if lx >= w - edge {
        lx = (w - edge - ENTRY_ARM_PX - 1).max(inset);
    }
    if ly <= edge {
        ly = inset;
    } else if ly >= h - edge {
        ly = (h - edge - ENTRY_ARM_PX - 1).max(inset);
    }
    Ok(pool.to_root(lx, ly))
}

pub fn kvm_layout_snapshot() -> Result<poolsync_core::KvmDesktopInfo> {
    let primary = kvm_display()?;
    let desktop = kvm_desktop()?;
    Ok(poolsync_core::KvmDesktopInfo {
        monitor_x: primary.x,
        monitor_y: primary.y,
        desktop_x: desktop.x,
        desktop_y: desktop.y,
        desktop_width: desktop.width,
        desktop_height: desktop.height,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kvm_display_local_root_roundtrip() {
        let d = KvmDisplay {
            x: 1440,
            y: 145,
            width: 1344,
            height: 756,
        };
        assert_eq!(d.center_root(), (1440 + 672, 145 + 378));
        assert!(d.contains(1500, 400));
        assert!(!d.contains(100, 400));
        assert_eq!(d.to_local(1500, 400), (60, 255));
        assert_eq!(d.to_root(60, 255), (1500, 400));
    }
}
