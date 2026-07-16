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

**Done (v1.2+):** drag mosaic in GTK config window + auto neighbor inference from
screen positions (Barrier-style). Web config page has the same mosaic UX.

**Still open:** editing `~/.config/poolsync/agent.toml` local fields is available
in the Agent local tab; optional snap-to-edge polish.

---

## Later ideas

- Mouse wheel (`MouseWheel`) in KVM grab (`kvm_input.rs`)
- Packaging / install script polish for multi-host deploy
