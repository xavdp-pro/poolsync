# AI agent runbook: build, install and configure PoolSync nodes

## Work as a human–AI agent tandem

The human chooses the pool members, KVM permissions, screen arrangement and RDP
clipboard policy. The AI agent inventories the environment, prepares a reviewed
configuration, compiles and qualifies an exact executable, installs reversibly
within the authorized scope, and records evidence. Real keyboard/mouse and
monitor-cable acceptance belongs to the human at the desk.

Read [concept and example computer/RDP scenarios](CONCEPT-AND-SCENARIOS.md) first.
This runbook explains the existing tools; it is not permission to replace keys,
reconfigure production or close a user's RDP session without authorization.
The AI assistant is not a required runtime service. Do not introduce an extra
user password, a new central hub, GitHub Actions, CI/CD or Vercel.

## 1. Inventory before changing anything

Record the checkout/branch/commit, dirty files and applicable `AGENTS.md`; read
[the current qualification report](HUBLESS-WINDOW-DEPLOYMENT-2026-10-03.md).
For each intended node record:

- SSH route, actual login user/UID/home, OS/architecture/glibc and available tools.
- Installed and executing agent SHA-256, package version, launcher/service paths.
- Intended graphical session and actual `DISPLAY`, X authority and user bus.
- Node identity, mode, saved topology, participation marker and file fingerprints.
- Authorized peer routes, listen port, TLS certificate SAN/expiry and CA trust.
- Presence of node tokens and the shared E2E key, without printing their values.
- Active RDP clients, their clipboard policy and existing clipboard managers.

A copied home can carry another computer's identity. Do not rename `node` alone:
identity, node token, TLS SAN, peer authorization and routes must agree. Preserve
existing group keys, settings and positions unless the human authorizes a change.
Do not paste raw configs, process arguments, private keys or environment dumps
into public reports. RDP arguments can contain passwords.

Example names throughout the public documentation are fictional: desk-a/desk-b
are `full`; work-a/work-b/work-c are `clipboard_only`. Map these roles to the
actual authorized inventory; do not infer real hostnames from the examples.
These are illustrative roles, not a fixed membership list or capacity limit.
See [hypothetical pool changes](CONCEPT-AND-SCENARIOS.md#hypothetical-pool-changes)
for adding, pausing, retiring and changing a node's role.
Do not hard-code UID 1000 or display `:10`. The current
launcher selects this user's live XRDP session preferentially, otherwise an
XFCE session. It does not simultaneously bridge every graphical session. Verify
that this selection matches the intended desktop before starting an agent.

## 2. Compile an exact Rust candidate

Use a build environment compatible with the oldest target's glibc and CPU.
`deploy/build-portable-p2.sh` combines compilation with old deployment behavior;
**do not use it as a build-only command** for this fleet. Compile in DEV or an
isolated build environment, not by changing production libraries.

On a Debian-family DEV builder, inspect installed packages first. The agent uses
GTK3, appindicator, X11/XTest and TLS. A representative dependency installation
is below; adapt to the actual distribution and authorized DEV environment:

```sh
sudo apt-get install build-essential pkg-config clang cmake libssl-dev \
  libgtk-3-dev libayatana-appindicator3-dev libxdo-dev libnotify-dev
```

Use an installed Rust toolchain compatible with `Cargo.lock`, then, from the
reviewed checkout:

```sh
rustc --version
cargo --version
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo build --locked --release -p poolsync-agent
sha256sum target/release/poolsync-agent
target/release/poolsync-agent --version
ldd target/release/poolsync-agent
```

Keep the source commit, compiler version and executable hash together. A package
version such as `2.1.0-dev.3` does not identify a particular candidate. On every
target check `ldd` for missing libraries before replacement; copying an executable
does not make its dynamically linked GTK/TLS libraries portable.

## 3. Prepare identities and peer configuration privately

### Existing pool

Reuse the existing per-node token, pool E2E key, TLS identity, trust and layout.
Add an authorized node to both sides' route/authorization configuration; do not
regenerate the whole pool's security directory just to enroll one machine.
A new node needs its own identity/certificate and explicit authorization from
existing peers. Never give it another node's copied identity or the CA private key.

### Entirely new isolated pool

`deploy/generate-security.sh` creates a CA, node tokens, certificates and a shared
E2E key. Use it only with a new private output directory and node names/routes
reviewed for that new pool. It overwrites security material in its output path.
Certificate SANs are inferred from `agent.<node>.toml` files in the configuration
directory; those routes must be prepared first.

```sh
umask 077
# Set these to reviewed absolute paths outside the checkout.
: "${POOLSYNC_PRIVATE_CONFIG_DIR:?private seed configurations required}"
: "${POOLSYNC_NEW_SECURITY_DIR:?new private security directory required}"
test ! -e "$POOLSYNC_NEW_SECURITY_DIR"
POOLSYNC_CONFIG_DIR="$POOLSYNC_PRIVATE_CONFIG_DIR" \
  ./deploy/generate-security.sh "$POOLSYNC_NEW_SECURITY_DIR" \
  unused-hub.invalid desk-a desk-b desk-c
```

The generator still creates unused legacy hub certificate files; this does not
require installing or starting a hub. Distribute only the target's own node
certificate/key/config fragment and CA certificate. Keep the CA signing key
private/offline. Provision the CA certificate into each client's trust store
with the human's approval; do not disable TLS certificate verification.

### Configuration shape

This is a **schema example**, not a ready-to-run configuration. Replace the
placeholders privately; use actual peer URLs and TLS SANs. Put all root settings
before TOML table headers. The existing parser still requires `hub_url` and
`token`, even though `hubless = true` bypasses the legacy hub session.

```toml
node = "desk-a"
hubless = true
hub_url = "ws://127.0.0.1:1/ws" # intentionally unreachable in a new test pool
# For an existing pool, preserve its existing hub_url and token fields.
token = "MIGRATION_DISABLED"
node_token = "REPLACE_WITH_THIS_NODES_PRIVATE_TOKEN"
e2e_key = "REPLACE_WITH_THE_POOLS_BASE64_32_BYTE_KEY"
mode = "full" # clipboard_only for nodes excluded from KVM
kvm_enabled = true
kvm_capture = true
pause_clipboard_when_rdp = true
peer_listen_port = 9472
peer_direct_clipboard = true
hub_clipboard = false
peer_tls_cert = "/home/USER/.config/poolsync/tls/desk-a.crt"
peer_tls_key = "/home/USER/.config/poolsync/tls/desk-a.key"

[peer_tokens]
"desk-b" = "REPLACE_WITH_DESK_BS_PRIVATE_TOKEN"
"desk-c" = "REPLACE_WITH_DESK_CS_PRIVATE_TOKEN"

[screen]
width = 1920
height = 1080

[[neighbors]]
node = "desk-b"
direction = "right"
peer_url = "wss://desk-b.example:9472/ws"

[[neighbors]]
node = "desk-c"
direction = "down"
peer_url = "wss://desk-c.example:9472/ws"
```

On each receiving node, `[peer_tokens]` maps the **sending peer's node name** to
that peer's token. Reciprocal authorization and routes must cover the intended
members. For a clipboard-only node set `mode = "clipboard_only"`,
`kvm_enabled = false` and `kvm_capture = false`. Preserve the agreed RDP policy;
the current detector directly covers FreeRDP, not Remmina in general.

For computers on the same reachable LAN, configure local peer addresses; no VPN
is required. For computers on different networks/sites, a VPN is useful to
provide private peer routes. Mixed pools can use LAN routes locally and VPN
routes for distant peers, with `peer_url_vpn` as an optional configured fallback.
Verify name resolution, permitted connectivity to the configured peer port,
TLS trust/SANs and synchronized clocks. Do not expose the listener publicly or
silently change firewall/VPN policy. Preserve the recorded deployment's existing
cross-site VPN; do not install one merely to connect an already reachable LAN.
Provide alternate routes where loss of one relay must not isolate the pool.

Saved layout lives in `agent.topology.json`, with `revision`, `origin` and
`topology.nodes`. Network reachability is not screen adjacency. Preserve the
existing document during upgrades. For a new pool, prepare one consistent layout
for all members or review it in the configuration window before accepting edge
KVM. Do not rely on independent per-node default layouts to describe a real desk.

For the entirely new three-node example above, this minimal initial layout
places the two full nodes side by side and excludes the third from KVM. Copy the
same reviewed document to all three private bundles. Adapt dimensions/positions
to the real screens; never replace an existing pool's saved document with it:

```json
{
  "revision": 1,
  "origin": "desk-a",
  "topology": {
    "nodes": {
      "desk-a": {"x": 0, "y": 0, "width": 1920, "height": 1080, "kvm_enabled": true},
      "desk-b": {"x": 1920, "y": 0, "width": 1920, "height": 1080, "kvm_enabled": true},
      "desk-c": {"x": 0, "y": 1080, "width": 1920, "height": 1080, "kvm_enabled": false}
    }
  }
}
```

### First migration from a legacy hub

Only for a legacy pool not already migrated: collect private `<node>.toml`
inputs, explicit node-to-`wss://host:port/ws` routes and the saved `PoolTopology`
object (`nodes`, without the outer layout-document wrapper). Direct clipboard
must already be enabled, hub clipboard disabled, and existing TLS identities
and peer authorizations consistent. Then render a private bundle:

```sh
python3 deploy/migrate-hubless-config.py \
  --input-directory "$POOLSYNC_PRIVATE_INPUT_DIR" \
  --routes "$POOLSYNC_PRIVATE_ROUTES_JSON" \
  --saved-topology "$POOLSYNC_PRIVATE_TOPOLOGY_JSON" \
  --output-directory "$POOLSYNC_NEW_RENDER_DIR" \
  --layout-author "$POOLSYNC_LAYOUT_AUTHOR"
```

The output directory must not exist. This tool renders only; it does not contact
hosts or install. It preserves existing settings and adds missing peer entries;
existing route URLs are not rewritten, so review their reachability separately.
Do not run it over an already deployed hubless layout to reset its revision.

## 4. Qualify on the dedicated test host before promotion

Use `poolsync-test` and dedicated `neko-desk-a/b/c` desktops on your dedicated test host (fictional alias `lab-host`), with
private configurations, tokens, CA, HOME, graphical display and browser profiles.
Prefer disposable clones for complete cold reboots; keep the original desktops'
state and record preservation afterward. The commands below run **inside the
lab environment where Podman owns the three selected desktop containers**,
not on an arbitrary production desktop. Inventory their actual addresses first.

```sh
python3 deploy/tests/no-hub-desktop-test.py --help
python3 deploy/tests/no-hub-desktop-test.py \
  --candidate "$POOLSYNC_QUALIFIED_CANDIDATE" \
  --expected-sha256 "$POOLSYNC_CANDIDATE_SHA256" \
  --containers "$POOLSYNC_TEST_A" "$POOLSYNC_TEST_B" "$POOLSYNC_TEST_C" \
  --addresses "$POOLSYNC_TEST_A_ADDR" "$POOLSYNC_TEST_B_ADDR" "$POOLSYNC_TEST_C_ADDR" \
  --hubless --links triangle --peer-tls --native-owner gtk \
  --clipboard-rounds 20 --native-browser --native-browser-engine chromium \
  --large-images --clipboard-races --network-loss --screen-changes \
  --simultaneous-claims --target-restart --bounded-allocator \
  --idle-seconds 30 --soak-seconds 1200 \
  --output "$POOLSYNC_PRIVATE_LAB_REPORT"
```

For **fresh disposable clones only**, add `--fresh-desktops --reboot-desktops` to
exercise cold restarts. Preflight required native tools, including `xinput` and
`xprop` for target-restart checks. Run separate chain/relay and paused old-to-new
cohort campaigns when relevant. Preserve failures and native receiver artifacts;
a single successful transfer is not daily-use acceptance. Firefox has a recorded
native clipboard control limitation; do not silently claim all-browser coverage.

Measure actual paste contents, stale-image exclusion, local recovery, relay
loss/reconnection, temporary departure/private rejoin, CPU/RSS and elapsed time.
Keep original desktop state intact. The finite campaign does not establish
all-day stability or physical monitor-cable behavior.

## 5. Install the correct kind of node

### Brand-new node: explicit bootstrap, not an overwrite

Existing `install-agent*.sh` helpers overwrite configuration from legacy
fleet templates. Do not use them blindly for a new hubless node or an existing
identity. Prepare a private per-node bundle with a qualified `poolsync-agent`,
resolved `agent.toml`, consistent `agent.topology.json` and that node's TLS files.
Before starting, existing peers must authorize the new identity.

After reviewing runtime dependencies (`xclip`, `xdotool`, GTK3, appindicator,
Python GI/GTK3, notifications and X11 tools), run the following **as the target
user in its intended graphical session**, from the reviewed source checkout.
These bootstrap commands refuse an existing configuration:

```sh
: "${POOLSYNC_PRIVATE_NODE_BUNDLE:?reviewed node bundle required}"
test ! -e "$HOME/.config/poolsync/agent.toml"
install -d -m 700 "$HOME/.config/poolsync" "$HOME/.config/poolsync/tls"
install -d "$HOME/.local/bin" "$HOME/.local/share/poolsync" \
  "$HOME/.config/systemd/user" "$HOME/.config/autostart"
install -m 755 "$POOLSYNC_PRIVATE_NODE_BUNDLE/poolsync-agent" \
  "$HOME/.local/bin/poolsync-agent"
install -m 600 "$POOLSYNC_PRIVATE_NODE_BUNDLE/agent.toml" \
  "$HOME/.config/poolsync/agent.toml"
install -m 600 "$POOLSYNC_PRIVATE_NODE_BUNDLE/agent.topology.json" \
  "$HOME/.config/poolsync/agent.topology.json"
: "${POOLSYNC_NODE_NAME:?reviewed node identity required}"
# The private bundle's resolved config must name these exact destination paths.
install -m 644 "$POOLSYNC_PRIVATE_NODE_BUNDLE/tls/$POOLSYNC_NODE_NAME.crt" \
  "$HOME/.config/poolsync/tls/$POOLSYNC_NODE_NAME.crt"
install -m 600 "$POOLSYNC_PRIVATE_NODE_BUNDLE/tls/$POOLSYNC_NODE_NAME.key" \
  "$HOME/.config/poolsync/tls/$POOLSYNC_NODE_NAME.key"
install -m 644 "$POOLSYNC_PRIVATE_NODE_BUNDLE/tls/ca.crt" \
  "$HOME/.config/poolsync/tls/ca.crt"
# CA trust installation is a separate reviewed system change; never copy ca.key.
for helper in poolsync-agent-launch.sh poolsync-pick-session.sh \
  poolsync-session-start.sh poolsync-watchdog.sh; do
  install -m 755 "deploy/$helper" "$HOME/.local/bin/$helper"
done
install -m 755 deploy/poolsync-ctl.sh "$HOME/.local/bin/poolsync-ctl"
install -m 755 deploy/poolsync-logs.sh "$HOME/.local/bin/poolsync-logs"
install -m 644 poolsync-agent/icons/poolsync-tray.png \
  "$HOME/.local/share/poolsync/poolsync-tray.png"
install -m 644 deploy/systemd/poolsync-agent.service \
  deploy/systemd/poolsync-watchdog.service deploy/systemd/poolsync-watchdog.timer \
  "$HOME/.config/systemd/user/"
```

Create a per-user autostart entry from `deploy/autostart/poolsync-agent.desktop`,
resolving its `Exec`, `TryExec` and `Icon` to the actual home. That template
currently embeds an account-specific absolute home path; do not copy it
unchanged for another account.
For a home path without spaces, this prepares the adapted entry. For paths with
spaces, quote desktop-entry command paths correctly and review them separately:

```sh
python3 - <<'PY'
from pathlib import Path
home = Path.home()
assert not any(c.isspace() for c in str(home)), "review desktop-entry path quoting"
source = Path("deploy/autostart/poolsync-agent.desktop").read_text()
target = home / ".config/autostart/poolsync-agent.desktop"
assert not target.exists(), "existing autostart entry must be preserved/reviewed"
exec_line = next(line for line in source.splitlines() if line.startswith("Exec="))
source_command = Path(exec_line.split("=", 1)[1])
assert source_command.name == "poolsync-session-start.sh"
source_home = source_command.parents[2]
target.write_text(source.replace(str(source_home), str(home)))
target.chmod(0o644)
PY
```

Validate private TOML/JSON, permissions, TLS files and target binary hash before
starting. From the same target user's graphical session:

```sh
"$HOME/.local/bin/poolsync-pick-session.sh"
ldd "$HOME/.local/bin/poolsync-agent"
"$HOME/.local/bin/poolsync-agent" --version
systemctl --user daemon-reload
systemctl --user enable poolsync-agent.service poolsync-watchdog.timer
systemctl --user start poolsync-agent.service poolsync-watchdog.timer
```

The session-start helper/autostart connects startup to graphical login. Do not
turn on user linger to pretend a missing graphical session is available.
No new PoolSync hub unit is installed.

### Existing legacy node: preserve and qualify migration

Use `deploy/apply-hubless-upgrade.py` with a reviewed per-node bundle, original
config hash, candidate hash and unique backup ID. Read `--help`; run
`--check-only` first as root on the selected host. The bundle includes the binary,
rendered `agent.toml`/`agent.topology.json`, and the helper files listed by the
script's `FILES` mapping. This installer refuses an existing hubless layout;
it is for first migration, not routine upgrades. Its backups support `--rollback`.

### Current hubless fleet: paused cohort upgrade

Use `deploy/apply-agent-cohort-hotfix.py` for a qualified binary update. It is
fleet-specific: inspect `TARGETS`, SSH aliases and account prerequisites first.
Stage the identical candidate and `deploy/apply-agent-hotfix.py` at the reviewed
absolute paths on **every host**, including the local node. Pin their hashes.
The coordinator defaults to a read-only dry run:

```sh
python3 deploy/apply-agent-cohort-hotfix.py \
  --user "$POOLSYNC_AGENT_USER" \
  --candidate "$POOLSYNC_STAGED_CANDIDATE" \
  --installer "$POOLSYNC_STAGED_INSTALLER" \
  --expected-current-sha256 "$POOLSYNC_CURRENT_SHA256" \
  --expected-candidate-sha256 "$POOLSYNC_CANDIDATE_SHA256" \
  --expected-installer-sha256 "$POOLSYNC_INSTALLER_SHA256" \
  --backup-id "$POOLSYNC_UNIQUE_BACKUP_ID" \
  --output "$POOLSYNC_PRIVATE_DEPLOYMENT_REPORT"
```

Only after qualification and within authorized deployment scope, repeat with
`--apply`. The tool makes the entire pool temporarily absent, releases input,
replaces agents sequentially and resumes only after a uniform candidate cohort
is verified. Active mixed-version KVM upgrades have a recorded failure; do not
substitute an uncoordinated sequence of restarts. On failure it attempts rollback
while the cohort stays absent; an unknown/incomplete cohort must remain absent
until inspected. Local input should remain usable. Reports/backups stay private.

## 6. Verify installation and human acceptance separately

From the intended target user's session, inspect service PID and exact running
executable rather than trusting the copied file or version label alone:

```sh
systemctl --user is-active poolsync-agent.service
POOLSYNC_AGENT_PID="$(systemctl --user show poolsync-agent.service --property=MainPID --value)"
sha256sum "$HOME/.local/bin/poolsync-agent" "/proc/$POOLSYNC_AGENT_PID/exe"
"$HOME/.local/bin/poolsync-pick-session.sh"
journalctl --user -u poolsync-agent.service -n 40 --no-pager
```

Read only the necessary session variables from the agent process; do not dump
its whole environment. Check private `agent.status.json` for fresh intended peer
presence, and compare config/layout fingerprints against the inventory. Verify
one agent on the intended display, no held injected keys/buttons, and no hub
connection or hub-triggered watchdog restart. Keep logs reviewed for secrets.

Test fresh text/images and real paste on each intended session, distinguishing
native RDP redirection from PoolSync delivery. Close RDP clients normally before
an independent desk-a/desk-b native paste campaign, with the human's agreement.
Ask the human for both physical crossings, destination typing, emergency return
and real monitor changes. Record failures as well as successes. Do not mark a
blocked hardware requirement complete from synthetic tests.

Keep the old hub stopped/disabled and its rollback backup until dependencies
and acceptance are validated. Commit reviewed code/docs/evidence in English,
push within the user's authorized scope, and keep GitHub automation disabled.
Report **developed / container-tested / deployed / physically accepted** separately.
