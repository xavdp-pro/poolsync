//! Capture souris/clavier style Barrier (XWindowsScreen::leave / grabMouseAndKeyboard).
//! Fenêtre InputOnly plein écran + curseur pixmap vide → le curseur disparaît sur le master.

use anyhow::{Context, Result};
use std::thread;
use std::time::{Duration, Instant};
use tracing::warn;
use x11rb::connection::Connection;
use x11rb::protocol::xfixes;
use x11rb::protocol::xproto::{
    Bool32, ConfigureWindowAux, ConnectionExt as _, CreateGCAux, CreateWindowAux, Cursor,
    EventMask, GrabMode, GrabStatus, ImageFormat, InputFocus, Pixmap, StackMode, Window,
    WindowClass,
};
use x11rb::protocol::Event;
use x11rb::{COPY_DEPTH_FROM_PARENT, COPY_FROM_PARENT, CURRENT_TIME, NONE};

const GRAB_TIMEOUT: Duration = Duration::from_secs(1);
const GRAB_RETRY: Duration = Duration::from_millis(50);

#[derive(Debug, Clone)]
pub enum GrabEvent {
    Motion { dx: i32, dy: i32 },
    Button { button: u8, pressed: bool },
    Key { keycode: u8, pressed: bool },
}

pub struct InputGrab {
    conn: x11rb::rust_connection::RustConnection,
    screen_num: usize,
    grab_window: Window,
    blank_cursor: Cursor,
    cursor_pixmap: Pixmap,
    screen_w: u16,
    screen_h: u16,
    last_x: i16,
    last_y: i16,
    active: bool,
}

impl InputGrab {
    pub fn begin(_screen_w: u32, _screen_h: u32) -> Result<Self> {
        let (conn, screen_num) = x11rb::connect(None).context("X11 grab")?;
        let root = conn.setup().roots[screen_num].root;
        let setup = &conn.setup().roots[screen_num];
        let w = setup.width_in_pixels.max(1);
        let h = setup.height_in_pixels.max(1);
        let cx = (w / 2) as i16;
        let cy = (h / 2) as i16;

        let (blank_cursor, cursor_pixmap) = create_blank_cursor(&conn, root)?;
        let grab_window = create_grab_window(&conn, root, w, h, blank_cursor)?;

        release_stale_grabs(&conn);

        conn.map_window(grab_window)?;
        conn.configure_window(
            grab_window,
            &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
        )?;
        let _ = xfixes::hide_cursor(&conn, root);
        if let Err(err) = grab_mouse_and_keyboard(&conn, grab_window) {
            let _ = conn.ungrab_pointer(CURRENT_TIME);
            let _ = conn.ungrab_keyboard(CURRENT_TIME);
            let _ = xfixes::show_cursor(&conn, root);
            let _ = conn.unmap_window(grab_window);
            let _ = conn.destroy_window(grab_window);
            let _ = conn.flush();
            return Err(err);
        }
        let _ = conn.set_input_focus(InputFocus::POINTER_ROOT, grab_window, CURRENT_TIME);
        conn.warp_pointer(NONE, root, 0, 0, 0, 0, cx, cy)?;
        conn.flush()?;

        Ok(Self {
            conn,
            screen_num,
            grab_window,
            blank_cursor,
            cursor_pixmap,
            screen_w: w,
            screen_h: h,
            last_x: cx,
            last_y: cy,
            active: true,
        })
    }

    pub fn end(&mut self) {
        if !self.active {
            return;
        }
        let root = self.conn.setup().roots[self.screen_num].root;
        let _ = self.conn.ungrab_pointer(CURRENT_TIME);
        let _ = self.conn.ungrab_keyboard(CURRENT_TIME);
        let _ = self.conn.unmap_window(self.grab_window);
        let _ = xfixes::show_cursor(&self.conn, root);
        let _ = self.conn.destroy_window(self.grab_window);
        let _ = self.conn.free_cursor(self.blank_cursor);
        let _ = self.conn.free_pixmap(self.cursor_pixmap);
        let _ = self.conn.flush();
        self.active = false;
    }

    pub fn recenter(&mut self, _screen_w: u32, _screen_h: u32) {
        if !self.active {
            return;
        }
        let root = self.conn.setup().roots[self.screen_num].root;
        let cx = (self.screen_w / 2) as i16;
        let cy = (self.screen_h / 2) as i16;
        let _ = self.conn.warp_pointer(NONE, root, 0, 0, 0, 0, cx, cy);
        let _ = self.conn.flush();
        self.last_x = cx;
        self.last_y = cy;
    }

    /// Recentre la souris physique si elle s'éloigne du centre (Barrier s_size = 32).
    pub fn needs_recenter(&self, threshold: i32) -> bool {
        if !self.active {
            return false;
        }
        let cx = (self.screen_w / 2) as i16;
        let cy = (self.screen_h / 2) as i16;
        i32::from(self.last_x - cx).abs() > threshold
            || i32::from(self.last_y - cy).abs() > threshold
    }

    pub fn poll(&mut self) -> Result<Vec<GrabEvent>> {
        let mut out = Vec::new();
        while let Some(event) = self.conn.poll_for_event()? {
            match event {
                Event::MotionNotify(e) => {
                    let dx = i32::from(e.event_x) - i32::from(self.last_x);
                    let dy = i32::from(e.event_y) - i32::from(self.last_y);
                    self.last_x = e.event_x;
                    self.last_y = e.event_y;
                    if dx != 0 || dy != 0 {
                        out.push(GrabEvent::Motion { dx, dy });
                    }
                }
                Event::ButtonPress(e) => {
                    out.push(GrabEvent::Button {
                        button: e.detail,
                        pressed: true,
                    });
                }
                Event::ButtonRelease(e) => {
                    out.push(GrabEvent::Button {
                        button: e.detail,
                        pressed: false,
                    });
                }
                Event::KeyPress(e) => {
                    out.push(GrabEvent::Key {
                        keycode: e.detail,
                        pressed: true,
                    });
                }
                Event::KeyRelease(e) => {
                    out.push(GrabEvent::Key {
                        keycode: e.detail,
                        pressed: false,
                    });
                }
                _ => {}
            }
        }
        Ok(out)
    }
}

impl Drop for InputGrab {
    fn drop(&mut self) {
        self.end();
    }
}

/// Libère un grab clavier/souris orphelin (évite ALREADY_GRABBED après échec précédent).
fn release_stale_grabs(conn: &x11rb::rust_connection::RustConnection) {
    let _ = conn.ungrab_keyboard(CURRENT_TIME);
    let _ = conn.ungrab_pointer(CURRENT_TIME);
    let _ = conn.flush();
}

/// Masque rapide avant que le grab Barrier soit prêt (warp hors écran).
/// Préférer InputGrab::begin seul — n'appeler qu'après grab réussi si besoin.
pub fn hide_cursor_best_effort(screen_w: u32, screen_h: u32) {
    if let Err(err) = hide_and_warp_off(screen_w, screen_h) {
        warn!("masque curseur: {err:#}");
    }
}

fn hide_and_warp_off(screen_w: u32, screen_h: u32) -> Result<()> {
    let (conn, screen_num) = x11rb::connect(None)?;
    let root = conn.setup().roots[screen_num].root;
    let _ = xfixes::hide_cursor(&conn, root);
    conn.warp_pointer(
        NONE,
        root,
        0,
        0,
        0,
        0,
        (screen_w as i16).saturating_add(200),
        (screen_h as i16).saturating_add(200),
    )?;
    conn.flush()?;
    Ok(())
}

fn create_grab_window(
    conn: &x11rb::rust_connection::RustConnection,
    root: Window,
    width: u16,
    height: u16,
    cursor: Cursor,
) -> Result<Window> {
    let win = conn.generate_id()?;
    let event_mask = EventMask::POINTER_MOTION
        | EventMask::BUTTON_MOTION
        | EventMask::BUTTON_PRESS
        | EventMask::BUTTON_RELEASE
        | EventMask::KEY_PRESS
        | EventMask::KEY_RELEASE;
    conn.create_window(
        COPY_DEPTH_FROM_PARENT,
        win,
        root,
        0,
        0,
        width,
        height,
        0,
        WindowClass::INPUT_ONLY,
        COPY_FROM_PARENT,
        &CreateWindowAux::new()
            .override_redirect(Bool32::from(true))
            .event_mask(event_mask)
            .cursor(cursor),
    )?;
    Ok(win)
}

/// Curseur pixmap 1×1 vide — même technique que Barrier createBlankCursor().
fn create_blank_cursor(
    conn: &x11rb::rust_connection::RustConnection,
    root: Window,
) -> Result<(Cursor, Pixmap)> {
    let pixmap = conn.generate_id()?;
    conn.create_pixmap(1, pixmap, root, 1, 1)?;
    let gc = conn.generate_id()?;
    conn.create_gc(gc, pixmap, &CreateGCAux::new())?;
    conn.put_image(ImageFormat::XY_BITMAP, pixmap, gc, 1, 1, 0, 0, 0, 1, &[0u8])?;
    conn.free_gc(gc)?;
    let cursor = conn.generate_id()?;
    conn.create_cursor(cursor, pixmap, pixmap, 0, 0, 0, 0, 0, 0, 0, 0)?;
    Ok((cursor, pixmap))
}

/// Grab pointeur (obligatoire pour le KVM souris), clavier en best-effort.
/// IBus / xcape / menus XFCE tiennent souvent un grab clavier (ALREADY_GRABBED) :
/// on ne doit pas pour autant annuler le basculement souris.
fn grab_mouse_and_keyboard(
    conn: &x11rb::rust_connection::RustConnection,
    window: Window,
) -> Result<()> {
    let event_mask = EventMask::BUTTON_PRESS
        | EventMask::BUTTON_RELEASE
        | EventMask::ENTER_WINDOW
        | EventMask::LEAVE_WINDOW
        | EventMask::POINTER_MOTION;
    let deadline = Instant::now() + GRAB_TIMEOUT;
    loop {
        let ptr = conn
            .grab_pointer(
                false,
                window,
                event_mask,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
                window,
                NONE,
                CURRENT_TIME,
            )?
            .reply()?;
        if ptr.status != GrabStatus::SUCCESS {
            if Instant::now() >= deadline {
                anyhow::bail!("grab pointeur timeout ({:?})", ptr.status);
            }
            thread::sleep(GRAB_RETRY);
            continue;
        }

        let kb = conn
            .grab_keyboard(
                false,
                window,
                CURRENT_TIME,
                GrabMode::ASYNC,
                GrabMode::ASYNC,
            )?
            .reply()?;
        if kb.status != GrabStatus::SUCCESS {
            warn!(
                "grab clavier indisponible ({:?}) — KVM souris seul (IBus/xcape/menu ?)",
                kb.status
            );
        }
        return Ok(());
    }
}
