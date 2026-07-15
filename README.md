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

## License

MIT — see [LICENSE](LICENSE) if present, otherwise MIT as stated in project metadata.

## Roadmap

See **[ROADMAP.md](ROADMAP.md)** — next up: native systray window (server config + logs/settings).
