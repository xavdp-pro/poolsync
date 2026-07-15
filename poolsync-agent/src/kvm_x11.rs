use anyhow::{Context, Result};
use std::cell::RefCell;
use tracing::warn;
use x11rb::connection::Connection;
use x11rb::protocol::xfixes::{self, ConnectionExt as _};
use x11rb::protocol::xproto::ConnectionExt as _;
use x11rb::protocol::xtest::{self, ConnectionExt as _};

thread_local! {
    static XDO: RefCell<Option<libxdo::XDo>> = const { RefCell::new(None) };
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

pub fn display_size() -> Result<(u32, u32)> {
    with_x11_conn(|conn, screen_num| {
        let screen = &conn.setup().roots[screen_num];
        Ok((
            screen.width_in_pixels as u32,
            screen.height_in_pixels as u32,
        ))
    })
}
