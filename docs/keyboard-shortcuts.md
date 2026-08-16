# PoolSync keyboard shortcuts

Global hotkeys run **on the machine where you press them**. They are registered by `poolsync-agent` (`global-hotkey`, X11).

| Shortcut | Action |
|----------|--------|
| **Ctrl+Alt+Shift+P** | Pause / resume PoolSync on this machine only (KVM + clipboard) |
| **Ctrl+Alt+Shift+M** | Claim **KVM master** on this machine (keyboard and mouse come back here) |
| **Ctrl+Alt+Shift+C** | Move the pointer to the **exact center** of the monitor that currently contains it |
| **Ctrl+Alt+Shift+L** | **Locate** the pointer: ripple on that screen + notification with the computer name |

On macOS the same combos are **Ctrl+Option+Shift+P / M / C / L** (Option = Alt).

Avoid **Ctrl+Tab** (app tab switch) and **Ctrl+Escape** (Start menu on Windows).

---

## Ctrl+Alt+Shift+P — local pause

Toggles KVM and clipboard sync **only on this node**. Other pool members keep running.

| State | Notification | Effect |
|-------|----------------|--------|
| On | PoolSync — ACTIVÉ | KVM + clipboard sync again. **This node immediately claims KVM master** so you can cross screen edges again. |
| Off | PoolSync — DÉSACTIVÉ | This node leaves the KVM mosaic (other PCs cannot switch onto it). Clipboard and KVM stop here. |

Press the shortcut **on that computer** again to resume. Resuming claims KVM master and **puts the node back** on the mosaic so neighbors can cross onto it.

If you pause **asus** then only resume **acer**, **acer → asus cannot work**: asus is still paused. Unpause asus (Ctrl+Alt+Shift+P on asus).

Press the shortcut again to resume, or re-enable clipboard from the systray.

Using the **physical keyboard or mouse** on a machine that has PoolSync active (and is not currently the input owner) also claims master. After a pause/resume, reclaim is automatic — you should not need Ctrl+Alt+Shift+M except as a fallback.

Master-change desktop notifications are **off by default**. Enable **Notifier le changement de master (debug)** in the systray if you need them.

---

## Ctrl+Alt+Shift+M — claim KVM master

**M = Master.** Same idea as a physical reclaim (moving the mouse on this screen): this node becomes the input owner.

What happens:

1. If local pause was on, PoolSync is turned back on first.
2. The agent drops any remote input grab and shows the local cursor.
3. It sends `MasterClaim` and `SwitchTo` for this node (current pointer position).
4. Focus and input owner are set to this machine.

Clipboard-only nodes (`kvm_enabled = false`, e.g. gbs-p2) ignore the shortcut and show **MASTER unavailable**.

Equivalent systray action: **Devenir maître KVM** (Become KVM master).

---

## Ctrl+Alt+Shift+C — center pointer

**C = Center.** Warps the X11 pointer to the middle of the **physical monitor that currently contains the cursor** (RandR CRTC), on the machine where you press the shortcut.

- Horizontal: `x + width / 2`
- Vertical: `y + height / 2`
- If the pointer is not on any CRTC, the KVM primary monitor is used.
- The cursor is shown again (in case a KVM grab had hidden it).
- No desktop notification (instant, no extra popup).

This is local to that X11 display. It does not move the pointer on a *remote* KVM focus screen.

---

## Ctrl+Alt+Shift+L — locate pointer

**L = Locate.** Finds the mouse on the machine where you press the shortcut:

1. Makes the X11 cursor visible.
2. Draws a short **ripple** (three expanding rings) centered on the pointer, with the **computer name** (PoolSync node, e.g. `asus`) painted on the overlay.
3. Shows a desktop notification titled **PoolSync — {node}** (“The mouse cursor is on this computer: asus”).

Systray: **Localiser le curseur**.

The ripple is click-through and lasts about one second. It appears on the local X11 screen that currently holds the pointer (not a remote KVM focus).

---

## Requirements

- `poolsync-agent` running (systemd user unit after graphical login)
- X11 session (XFCE). Pure Wayland often cannot register global hotkeys
- `notify-send` / `libnotify-bin` for desktop notifications

---

## Verify after deploy

```bash
systemctl --user restart poolsync-agent
journalctl --user -u poolsync-agent --no-pager | grep -iE 'raccourci|hotkey|Ctrl\+Alt'
```

Expected log lines:

```
raccourci Ctrl+Alt+Shift+P enregistré (toggle PoolSync local)
raccourci Ctrl+Alt+Shift+M enregistré (réclamer master KVM)
raccourci Ctrl+Alt+Shift+C enregistré (centrer le curseur)
raccourci Ctrl+Alt+Shift+L enregistré (localiser le curseur)
```

(Agent logs are still in French; the shortcuts themselves are the same on every locale.)

---

## Code

| File | Role |
|------|------|
| `poolsync-agent/src/hotkey.rs` | Global listener |
| `poolsync-agent/src/cursor_ripple.rs` | Locate overlay (Ctrl+Alt+Shift+L) |
| `poolsync-agent/src/state.rs` | `toggle_local_poolsync()`, `request_master_claim()` |
| `poolsync-agent/src/kvm.rs` | `apply_hotkey_master_claim` |
| `poolsync-agent/src/notify_util.rs` | Desktop notifications |
| `poolsync-agent/src/tray.rs` | Systray menu + hotkey hints |
