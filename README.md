# PoolSync

Shared **clipboard + keyboard/mouse** across multiple Linux desktops — a modern alternative to Barrier.

→ **[Product pitch](PITCH.md)** (overview, elevator pitch, Barrier comparison)

- **Hub** — lightweight central coordinator (any host reachable over your VPN)
- **Agent** — one daemon per machine in the pool (XFCE / X11)
- **Transport** — WebSocket/TLS over WireGuard, LAN or another trusted network
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
hub_url = "wss://hub.example:9470/ws"
token = "YOUR_TOKEN"
node_token = "THIS_NODE_TOKEN"
e2e_key = "BASE64_32_BYTE_GROUP_KEY"
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

Secure deployment after generating the security directory:

```bash
POOLSYNC_TOKEN=admin POOLSYNC_SECURITY_DIR=/secure/path/poolsync \
  ./deploy/install-hub-gbs-p3.sh
POOLSYNC_TOKEN=admin POOLSYNC_SECURITY_DIR=/secure/path/poolsync \
  ./deploy/install-agent.sh ssh-host my-node-name
```

Agents start via **systemd user** + **XFCE autostart** after graphical login.

## Keyboard shortcuts

Full detail: **[docs/keyboard-shortcuts.md](docs/keyboard-shortcuts.md)**

| Shortcut | What it does |
|----------|----------------|
| **Ctrl+Alt+Shift+P** | Pause or resume PoolSync **on this machine only** (KVM + clipboard). Other nodes are unchanged. |
| **Ctrl+Alt+Shift+M** | **Claim KVM master** on this machine: keyboard and mouse return here (`MasterClaim` + local focus). Ignored on clipboard-only agents. |
| **Ctrl+Alt+Shift+C** | Warp the pointer to the **center of the monitor** that currently contains it (this machine). |
| **Ctrl+Alt+Shift+L** | **Locate** the pointer: ripple on that screen + notification with the computer name. |

macOS: **Ctrl+Option+Shift+P / M / C / L**. Systray: **Devenir maître KVM** is the same as M.

## Security model

- All HTTP and WebSocket credentials use `Authorization: Bearer`; PoolSync no longer accepts or emits secrets in URL query parameters.
- The hub supports native TLS with `--tls-cert` and `--tls-key`; peer listeners support `peer_tls_cert`/`peer_tls_key`, and `wss://` clients validate the system trust store. Peer certificate SANs must match the hostnames used in `peer_url`.
- `--node-tokens-file` enables one independently revocable identity per node. The JSON entry accepts `token`, `previous_tokens` for zero-downtime rotation, and `revoked`; secure agent installs do not retain the dashboard administrator token.
- `e2e_key` enables XChaCha20-Poly1305 clipboard encryption. The hub relays opaque authenticated ciphertext and therefore cannot populate its central clipboard history in this mode. Use `--require-e2e` after every agent has migrated to prevent downgrade.
- `/health` is deliberately public and contains only `ok`; every status, topology and clipboard API is private.

Generate a local CA, hub certificate, node identities and E2E key outside the repository:

```bash
./deploy/generate-security.sh /secure/path/poolsync hub.example desk-a desk-b
```

For native Wayland, install `wl-clipboard`; text, HTML and image synchronization uses `wl-paste`/`wl-copy`. KVM injection uses `ydotool`/`uinput`. Global input capture remains intentionally receive-only on Wayland until a user-authorized RemoteDesktop/libei portal session is available; set `kvm_capture = false` on such nodes.

## License

MIT — see [LICENSE](LICENSE) if present, otherwise MIT as stated in project metadata.

## Roadmap

See **[ROADMAP.md](ROADMAP.md)** — next up: native systray window (server config + logs/settings).
