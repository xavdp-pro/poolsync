# PoolSync — Roadmap

## Next up

### Native window from systray (config + logs)

**Context:** today the systray (`poolsync-agent/src/tray.rs`) shows a context menu plus “View logs…” opening `logs_viewer.rs` (journalctl, read-only). Server / topology config is only available in the web dashboard (`web/src/pages/Config.jsx`).

**Goal:** from the systray icon on **each** agent, open a **native GTK window** (like logs) with **tabs**:

| Tab | Content |
|-----|---------|
| **Configuration** | Local equivalent of the web page: mosaic topology (left/right/up/down neighbours), `kvm_enabled` per node, hub URL, token — via hub API (`/api/topology`) plus editing `~/.config/poolsync/agent.toml` for local fields (`node`, `screen`, `neighbors`, `mode`, etc.) |
| **Logs** | Current log view (`logs_viewer.rs`) **plus** agent settings editor (TOML or form) with Save / optional agent restart |

**Desired behaviour:**

- Systray click (or menu entry “Configuration…”) → single reusable window (same pattern as `OPEN_WINDOW` in `logs_viewer.rs`)
- Tabs: `Configuration` | `Logs & settings`
- Reuse existing patterns: GTK + `glib::MainContext::invoke`, hub API already used by the web app (`web/src/api.js`)
- Does **not** replace the web dashboard — desktop complement for config without a browser

**Likely files:**

- `poolsync-agent/src/tray.rs` — menu entry + open window
- `poolsync-agent/src/logs_viewer.rs` — merge or split into `settings_window.rs` with tabs
- New: `config_viewer.rs` (or `ui/` module) — topology form + agent.toml
- `poolsync-core` — config read/write helpers if needed

**Web reference to mirror (simplified desktop):** `web/src/pages/Config.jsx`, `web/src/api.js` (`fetchTopology`, `saveTopology`)

---

## Later ideas

- Mouse wheel (`MouseWheel`) in KVM grab (`kvm_input.rs`)
- Packaging / install script polish for multi-host deploy
