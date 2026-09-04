//! Gestionnaire de presse-papiers X11 : recueille le contenu des applications
//! qui se ferment, via le protocole `CLIPBOARD_MANAGER` / `SAVE_TARGETS`.
//!
//! Sous X11 le presse-papiers n'est pas un stockage central : le contenu vit
//! dans la mémoire de l'application qui a copié, et meurt avec elle. La
//! convention prévoit un remède — une application qui se ferme propose sa
//! sélection au gestionnaire déclaré, qui la reprend. GTK et Qt le font tout
//! seuls, à condition qu'un gestionnaire possède la sélection
//! `CLIPBOARD_MANAGER`. Jusqu'ici personne ne la possédait sur ces machines.
//!
//! On complète ainsi la reprise par sondage (`reclaim_orphaned_selection`) :
//! le sondage rattrape après coup, ce protocole prévient — l'application nous
//! donne son contenu avant de disparaître, sans fenêtre d'oubli possible.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    Atom, AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, PropMode, SelectionNotifyEvent,
    SelectionRequestEvent, Window, WindowClass, SELECTION_NOTIFY_EVENT,
};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::CURRENT_TIME;

static OWNED: AtomicBool = AtomicBool::new(false);

/// Le gestionnaire est-il actif sur cette machine ?
pub fn is_active() -> bool {
    OWNED.load(Ordering::SeqCst)
}

/// Démarre le gestionnaire dans un fil dédié. Sans effet si un autre programme
/// (clipman, klipper…) possède déjà la sélection : deux gestionnaires sur un
/// même affichage se disputeraient le contenu, exactement comme PoolSync et
/// xrdp-chansrv se le disputaient.
pub fn spawn() {
    std::thread::Builder::new()
        .name("clipboard-manager".into())
        .spawn(|| {
            if let Err(err) = run() {
                tracing::warn!("gestionnaire de presse-papiers : {err:#}");
            }
            OWNED.store(false, Ordering::SeqCst);
        })
        .ok();
}

struct Atoms {
    manager: Atom,
    save_targets: Atom,
    clipboard: Atom,
    targets: Atom,
    utf8: Atom,
}

fn intern(conn: &RustConnection, name: &str) -> Result<Atom> {
    Ok(conn.intern_atom(false, name.as_bytes())?.reply()?.atom)
}

fn run() -> Result<()> {
    let (conn, screen_num) = x11rb::connect(None).context("connexion X11")?;
    let screen = &conn.setup().roots[screen_num];
    let atoms = Atoms {
        manager: intern(&conn, "CLIPBOARD_MANAGER")?,
        save_targets: intern(&conn, "SAVE_TARGETS")?,
        clipboard: intern(&conn, "CLIPBOARD")?,
        targets: intern(&conn, "TARGETS")?,
        utf8: intern(&conn, "UTF8_STRING")?,
    };

    let existing = conn.get_selection_owner(atoms.manager)?.reply()?.owner;
    if existing != x11rb::NONE {
        tracing::info!(
            "gestionnaire de presse-papiers déjà en place (fenêtre {existing:#x}) — PoolSync s'abstient"
        );
        return Ok(());
    }

    let win = conn.generate_id()?;
    conn.create_window(
        x11rb::COPY_DEPTH_FROM_PARENT,
        win,
        screen.root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_OUTPUT,
        screen.root_visual,
        &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    )?;
    conn.set_selection_owner(win, atoms.manager, CURRENT_TIME)?;
    conn.flush()?;

    if conn.get_selection_owner(atoms.manager)?.reply()?.owner != win {
        tracing::warn!("gestionnaire de presse-papiers : sélection non obtenue");
        return Ok(());
    }
    OWNED.store(true, Ordering::SeqCst);
    tracing::info!(
        "gestionnaire de presse-papiers actif : les applications qui se ferment nous confient leur copie"
    );

    loop {
        match conn.wait_for_event() {
            Ok(Event::SelectionRequest(req)) => handle_request(&conn, &atoms, &req)?,
            Ok(Event::SelectionClear(_)) => {
                tracing::info!("gestionnaire de presse-papiers : un autre programme a pris le relais");
                return Ok(());
            }
            Ok(_) => {}
            Err(err) => return Err(err).context("boucle d'événements X11"),
        }
    }
}

/// Répond à une demande `SAVE_TARGETS` : on récupère le contenu courant et on
/// acquitte. **Toujours** acquitter : une application qui se ferme attend cette
/// réponse, et resterait bloquée sans elle.
fn handle_request(conn: &RustConnection, atoms: &Atoms, req: &SelectionRequestEvent) -> Result<()> {
    let mut property = req.property;

    if req.target == atoms.save_targets {
        if let Err(err) = absorb_clipboard(conn, atoms) {
            tracing::warn!("gestionnaire : reprise du presse-papiers impossible: {err:#}");
        }
        // La convention veut une propriété de type NULL, vide.
        let null_atom = intern(conn, "NULL").unwrap_or(AtomEnum::ATOM.into());
        if property != x11rb::NONE {
            conn.change_property8(PropMode::REPLACE, req.requestor, property, null_atom, &[])?;
        }
    } else if req.target == atoms.targets {
        let targets = [atoms.targets, atoms.save_targets];
        if property != x11rb::NONE {
            conn.change_property32(
                PropMode::REPLACE,
                req.requestor,
                property,
                AtomEnum::ATOM,
                &targets,
            )?;
        }
    } else {
        // Cible inconnue : refus explicite, jamais de silence.
        property = x11rb::NONE;
    }

    let notify = SelectionNotifyEvent {
        response_type: SELECTION_NOTIFY_EVENT,
        sequence: 0,
        time: req.time,
        requestor: req.requestor,
        selection: req.selection,
        target: req.target,
        property,
    };
    conn.send_event(false, req.requestor, EventMask::NO_EVENT, notify)?;
    conn.flush()?;
    Ok(())
}

/// Récupère le contenu de CLIPBOARD dans notre tampon avant que l'application
/// ne disparaisse, puis laisse la boucle de poll le réoffrir.
fn absorb_clipboard(conn: &RustConnection, atoms: &Atoms) -> Result<()> {
    let owner = conn.get_selection_owner(atoms.clipboard)?.reply()?.owner;
    if owner == x11rb::NONE {
        return Ok(());
    }
    let text = read_utf8_selection(conn, atoms, owner)?;
    if let Some(text) = text {
        let hash = poolsync_core::hash_text(&text);
        crate::clipboard::remember_clipboard_content("text/plain", &text, &hash);
        tracing::info!(
            "gestionnaire : {} octets recueillis d'une application qui se ferme",
            text.len()
        );
    }
    Ok(())
}

fn read_utf8_selection(
    conn: &RustConnection,
    atoms: &Atoms,
    _owner: Window,
) -> Result<Option<String>> {
    let screen = &conn.setup().roots[0];
    let dest = conn.generate_id()?;
    conn.create_window(
        x11rb::COPY_DEPTH_FROM_PARENT,
        dest,
        screen.root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_OUTPUT,
        screen.root_visual,
        &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    )?;
    let prop = intern(conn, "POOLSYNC_SAVE")?;
    conn.convert_selection(dest, atoms.clipboard, atoms.utf8, prop, CURRENT_TIME)?;
    conn.flush()?;

    // Attente bornée : une application en train de mourir peut ne jamais
    // répondre, et on ne doit pas bloquer la fermeture des autres.
    let deadline = std::time::Instant::now() + Duration::from_millis(400);
    let mut out = None;
    while std::time::Instant::now() < deadline {
        match conn.poll_for_event()? {
            Some(Event::SelectionNotify(ev)) if ev.requestor == dest => {
                if ev.property != x11rb::NONE {
                    let reply = conn
                        .get_property(true, dest, ev.property, AtomEnum::ANY, 0, u32::MAX / 4)?
                        .reply()?;
                    if !reply.value.is_empty() {
                        out = Some(String::from_utf8_lossy(&reply.value).into_owned());
                    }
                }
                break;
            }
            Some(_) => {}
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    }
    conn.destroy_window(dest)?;
    conn.flush()?;
    Ok(out)
}
