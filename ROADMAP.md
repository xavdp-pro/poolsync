# PoolSync — Roadmap

## Done

### Native window from systray (config + logs) ✅

Implemented in `poolsync-agent/src/config_window.rs`. The systray entry
**“Configuration du pool…”** opens a reusable GTK window (same `OPEN_WINDOW`
singleton pattern as `logs_viewer.rs`) with two tabs:

| Tab | Content |
|-----|---------|
| **Configuration** | Pool topology editor: per-node `kvm_enabled`, screen size, and left/right/up/down neighbours, fetched from and saved back to the hub via `GET`/`POST /api/topology` (using the agent's token). |
| **Logs** | Read-only `journalctl` view, reusing `logs_viewer::fetch_journal_logs`. |

Native desktop equivalent of `web/src/pages/Config.jsx`, no browser required.

**Still open for this window:** editing `~/.config/poolsync/agent.toml` local
fields (`node`, `mode`, `screen`, `neighbors`) with optional agent restart, and
a drag mosaic like the web page (positions are currently preserved as-is).

---

## Later ideas

- Mouse wheel (`MouseWheel`) in KVM grab (`kvm_input.rs`)
- Packaging / install script polish for multi-host deploy
