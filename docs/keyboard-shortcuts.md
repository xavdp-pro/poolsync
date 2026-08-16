# PoolSync keyboard shortcuts

Global hotkeys run **on the machine where you press them**. They are registered by `poolsync-agent` (`global-hotkey`, X11).

| Shortcut | Action |
|----------|--------|
| **Ctrl+Alt+Shift+P** | Pause / resume PoolSync on this machine only (KVM + clipboard) |
| **Ctrl+Alt+Shift+M** | Claim **KVM master** on this machine (keyboard and mouse come back here) |

On macOS the same combos are **Ctrl+Option+Shift+P** and **Ctrl+Option+Shift+M** (Option = Alt).

Avoid **Ctrl+Tab** (app tab switch) and **Ctrl+Escape** (Start menu on Windows).

---

## Ctrl+Alt+Shift+P — local pause

Toggles KVM and clipboard sync **only on this node**. Other pool members keep running.

| State | Notification | Effect |
|-------|----------------|--------|
| On | PoolSync — ACTIVÉ | KVM + clipboard sync again |
| Off | PoolSync — DÉSACTIVÉ | This node stops sending/receiving KVM and clipboard; X11 cursor is shown again if a grab had hidden it |

Press the shortcut again to resume, or re-enable clipboard from the systray.

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
```

(Agent logs are still in French; the shortcuts themselves are the same on every locale.)

---

## Code

| File | Role |
|------|------|
| `poolsync-agent/src/hotkey.rs` | Global listener |
| `poolsync-agent/src/state.rs` | `toggle_local_poolsync()`, `request_master_claim()` |
| `poolsync-agent/src/kvm.rs` | `apply_hotkey_master_claim` |
| `poolsync-agent/src/notify_util.rs` | Desktop notifications |
| `poolsync-agent/src/tray.rs` | Systray menu + hotkey hints |
