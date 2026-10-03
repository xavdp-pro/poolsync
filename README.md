# PoolSync

## Install and configure with a human–AI agent tandem

PoolSync is designed to be installed and configured by **a human working with an
AI agent**. The human chooses the computers, permissions and screen positions;
the AI agent helps inspect the environment, preserve identities and keys,
configure the agents, test changes and prepare rollback. The human validates
physical keyboard/mouse and monitor behavior. The AI agent is an installation
and maintenance assistant, not a required runtime service.

PoolSync shares **clipboard + keyboard/mouse** across Linux desktops through
authenticated, encrypted peer connections, without a permanent PoolSync hub.

Its two distinguishing design choices relative to the documented server/client
model of Synergy and Barrier are **decentralized peer coordination** and
**temporary KVM leadership driven by physical use of an authorized node**.
Use desk-a's own keyboard/mouse and desk-a can take the lead; use desk-b's own
devices and desk-b can take over. The controller coordinates input only, not
membership, layout or clipboard as a central server. See
[the comparison and leadership example](docs/CONCEPT-AND-SCENARIOS.md#two-distinguishing-design-choices).

![PoolSync: human–AI setup and direct peer operation](docs/images/poolsync-concept.svg)

Read **[how it works and example computer/RDP scenarios](docs/CONCEPT-AND-SCENARIOS.md)**.
For the AI assistant: **[build, install and configure nodes](docs/AI-AGENT-INSTALLATION.md)**.

Computer names in these diagrams and scenarios are fictitious. The illustrated
five-node pool is one possibility: add an authorized node, temporarily take one
away while retaining its settings, or retire it through a reviewed membership
change. See [hypothetical pool changes](docs/CONCEPT-AND-SCENARIOS.md#hypothetical-pool-changes).

→ **[Product pitch](PITCH.md)** (overview and share text)

- **Agent** — one local daemon per user, attached to the selected graphical session (XFCE / X11)
- **Peer mesh** — direct encrypted clipboard, presence, layout and KVM control
- **Network** — no VPN needed on a reachable LAN; a VPN is useful for connecting different networks or sites
- **Temporary controller** — eligible full-mode nodes claim and renew control
- **Example pool (fictitious names)** — desk-a/desk-b: KVM + clipboard; work-a/work-b/work-c: clipboard only

The deployed hubless development build has passing isolated qualification.
Physical crossings between full-mode computers and monitor changes still require
acceptance; the reported edge blockage/loop remains open. See the
[current qualification and deployment report](docs/HUBLESS-WINDOW-DEPLOYMENT-2026-10-03.md).
An [independent Cursor counter-review](docs/CURSOR-COUNTER-REVIEW-2026-10-03.md)
records documentation corrections and remaining application/tooling limits.

## Rust workspace

| Crate | Role |
|-------|------|
| `poolsync-core` | JSON protocol, TOML config |
| `poolsync-hub` | Legacy coordinator + web dashboard, retained for rollback |
| `poolsync-agent` | X11 client (clipboard, KVM, systray) |

## Build

```bash
cargo build --release
```

Binaries: `target/release/poolsync-hub`, `target/release/poolsync-agent`

## Legacy hub (optional compatibility / rollback)

The following hub instructions and agent example describe the legacy deployment.
They are not the setup path for the current hubless fleet. In hubless mode,
`hubless = true` bypasses the hub session; authorized peer routes and the existing
pool key must be configured together. See the
[hubless implementation record](docs/HUBLESS-IMPLEMENTATION-2026-10-02.md).

```bash
poolsync-hub --listen 0.0.0.0:9470 --token YOUR_TOKEN
```

Run on any node your agents can reach (VPS, home server, container, etc.).

## Legacy agent configuration example

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

## Legacy deployment helpers

These helpers predate the hubless migration. They are retained for compatibility;
do not use the hub install command to configure the current hubless fleet.

```bash
POOLSYNC_TOKEN=your_token ./deploy/install-agent-local.sh my-node-name
# remote host:
POOLSYNC_TOKEN=your_token ./deploy/install-agent.sh ssh-host my-node-name
```

Legacy secure agent deployment after generating the security directory:

```bash
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

## Daily use

Right-click the tray icon and choose **Machine temporairement à l’écart du pool**
to take a laptop away; uncheck it on return. This persists across restarts,
preserves its layout/identity and prevents replay of copies made while away.
Operators can use `poolsync-agent --config PATH --away true` / `--away false`.

See [daily-use review and qualification](docs/DAILY-USE-REVIEW-2026-10-01.md).
Clipboard, KVM control, presence and layout operate over the peer mesh in hubless
mode. Hubless KVM uses full virtual desktop bounds across monitors, including
their desktop origin, with primary-monitor fallback when desktop geometry is
unavailable. Legacy hub KVM uses primary bounds. Actual monitor attachment/removal
remains a physical acceptance gate.

When native FreeRDP clipboard redirection is detected and
`pause_clipboard_when_rdp = true`, PoolSync leaves the client's local clipboard
to RDP. It continues receiving/relaying pool history. See the
[RDP scenarios and detection limits](docs/CONCEPT-AND-SCENARIOS.md#rdp-scenarios).

## Security model

- All HTTP and WebSocket credentials use `Authorization: Bearer`; PoolSync no longer accepts or emits secrets in URL query parameters.
- The hub supports native TLS with `--tls-cert` and `--tls-key`; peer listeners support `peer_tls_cert`/`peer_tls_key`, and `wss://` clients validate the system trust store. Peer certificate SANs must match the hostnames used in `peer_url`.
- `--node-tokens-file` enables one independently revocable identity per node. The JSON entry accepts `token`, `previous_tokens` for zero-downtime rotation, and `revoked`; secure agent installs do not retain the dashboard administrator token.
- `e2e_key` provides XChaCha20-Poly1305 payload encryption. Hubless control, presence and layout use a distinct authenticated domain; clipboard uses its encrypted wire format. Existing keys and identities are preserved. In legacy mode, the hub relays opaque ciphertext; `--require-e2e` is a legacy hub option.
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
