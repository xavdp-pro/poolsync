# PoolSync

Shared **clipboard + keyboard/mouse** across multiple Linux desktops — a modern alternative to Barrier.

→ **[Product pitch](PITCH.md)** (overview, elevator pitch, Barrier comparison)

- **Hub** — lightweight central coordinator (any host reachable over your VPN)
- **Agent** — one daemon per machine in the pool (XFCE / X11)
- **Transport** — WebSocket over WireGuard or any private network
- **Dynamic master** — whichever machine you use becomes the input master

## Rust workspace

| Crate | Role |
|-------|------|
| `poolsync-core` | JSON protocol, TOML config |
| `poolsync-hub` | WebSocket server + web dashboard |
| `poolsync-agent` | X11 client (clipboard, KVM, systray) |

## Build

```bash
cargo build --release
```

Binaries: `target/release/poolsync-hub`, `target/release/poolsync-agent`

## Hub

```bash
poolsync-hub --listen 0.0.0.0:9470 --token YOUR_TOKEN
```

Run on any node your agents can reach (VPS, home server, container, etc.).

## Agent

Config file: `~/.config/poolsync/agent.toml`

```toml
node = "laptop-b"
hub_url = "ws://10.0.0.1:9470/ws"
token = "YOUR_TOKEN"
mode = "full"   # or "clipboard_only"

[screen]
width = 1920
height = 1080

[[neighbors]]
direction = "left"
node = "laptop-a"
```

## Deploy (agent)

```bash
POOLSYNC_TOKEN=your_token ./deploy/install-agent-local.sh my-node-name
# remote host:
POOLSYNC_TOKEN=your_token ./deploy/install-agent.sh ssh-host my-node-name
```

Agents start via **systemd user** + **XFCE autostart** after graphical login.

## Keyboard shortcuts

Full detail: **[docs/keyboard-shortcuts.md](docs/keyboard-shortcuts.md)**

| Shortcut | What it does |
|----------|----------------|
| **Ctrl+Alt+Shift+P** | Pause or resume PoolSync **on this machine only** (KVM + clipboard). Other nodes are unchanged. |
| **Ctrl+Alt+Shift+M** | **Claim KVM master** on this machine: keyboard and mouse return here (`MasterClaim` + local focus). Ignored on clipboard-only agents. |

macOS: **Ctrl+Option+Shift+P / M**. Systray: **Devenir maître KVM** is the same as M.

## Security model

PoolSync is designed to run **inside a private network** (WireGuard VPN, LAN). Be aware of the current threat model:

- **Transport is `ws://` (unencrypted).** Clipboard content — text *and* images — and keyboard/mouse events travel in clear text. Confidentiality relies entirely on the underlying VPN/LAN. Run the hub on `wss://` behind a reverse proxy (or over WireGuard) if the link is not already private.
- **The token authenticates, it does not encrypt.** It is passed as a URL query parameter (`/ws?token=…`, `/api/topology?token=…`) and can therefore leak into proxy/access logs. Treat it as a shared secret and rotate it if exposed.
- **No per-node authorization.** Any client presenting a valid token can join the pool, become master, and read/write the shared clipboard.

Do **not** expose the hub port directly on the public internet without a TLS-terminating proxy and network-level access control.

## License

MIT — see [LICENSE](LICENSE) if present, otherwise MIT as stated in project metadata.

## Roadmap

See **[ROADMAP.md](ROADMAP.md)** — next up: native systray window (server config + logs/settings).
