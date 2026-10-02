# Daily-use robustness review — 2026-10-01

## Scope and decision

This candidate starts at the deployed v2.0.3 baseline (`2713acb`). It does not
merge the separate, unfinished discovery/enrollment/mTLS worktree. It includes
the RDP clipboard fix already qualified on real desktop screenshots.

The product is viable for the existing X11 desktop pool, but **is not fully
serverless**: clipboard data can travel directly between peers; KVM sessions,
master coordination and topology still use the hub. Stopping a hub must return
input locally, rather than promise KVM availability that the implementation
cannot provide.

## What works and what changed

| Area | Existing capability | Improvement in this candidate |
|---|---|---|
| Clipboard | Direct peer mesh, encrypted payloads, local history, image handling | Native RDP clients no longer turn old clipboard callbacks into new pool copies; incoming history remains available during native RDP ownership |
| Temporary absence | A session-only pause and a separate clipboard toggle | Persistent local absence from the systray or CLI; no automatic sharing, new history retention or relay while absent; positions and credentials stay intact |
| Rejoining | Devices reconnect | A private copy made while absent becomes a baseline, rather than being replayed as a new copy on return |
| Master | Local input and a master hotkey | Physical activity also claims master when a locally focused device still names itself as input owner after another device's claim |
| Departure | Hub removes normally closed sessions | Error/abrupt session termination also unregisters the device, releases master and removes unavailable KVM routes |
| Reconnection | Agent retries the hub | Old blocking KVM loops exit when their session channel closes; clipboard-only sessions no longer create permanently parked threads; all session exits return focus/input locally |
| Hotplug | Agents announce live RandR geometry and monitor lists | Screen-size changes and new devices recalculate edges; simultaneous joins cannot overwrite another device's saved geometry |
| Mixed resolutions | Coordinate mapping and per-screen dimensions | Deterministic route selection for ambiguous edges; negative coordinates and correct bounding-box scaling in native and web mosaics |
| Web editing | Dragging, keyboard movement, undo/redo | Pointer capture avoids double drag offsets; undo restores the pre-drag position; selected-device settings and explicit unsaved state; read-only computed neighbors replace controls discarded on save |
| Native editing | Map plus every machine's full form | Collapsible device settings, computed neighbors, and saved monitor offsets retained independently of the agent's runtime graph |
| Live interface | Status polling | Geometry updates while not editing; stale-status feedback; hidden browser pages stop polling and refresh on return |
| New installation TLS | Locally generated CA and server certificates | Explicit CA/leaf BasicConstraints and KeyUsage pass strict X.509 server/hostname checks; existing pool keys/certificates are retained |
| Idle overhead | Active polling | Paused/disconnected KVM and inactive/RDP clipboard polling use slower cadences; active input cadence is retained |

## Taking a laptop away

On that laptop, right-click the PoolSync tray icon and enable
**Machine temporairement à l’écart du pool**. Disable it when returning.
Unlike the existing short pause, absence survives process/session restarts and
stops retaining new office copies. Existing history is preserved. This is a
local operational state, not identity revocation.

An agent/human operator can perform the same action without any new password:

```sh
poolsync-agent --config "$HOME/.config/poolsync/agent.toml" --away true
poolsync-agent --config "$HOME/.config/poolsync/agent.toml" --away false
```

The installed agent observes changes within roughly one second, followed by its
normal state announcement. The sibling `agent.away` marker contains only `away`.
No tokens, keys, pool membership or machine positions are rewritten.

## Qualification

Code, automated tests, installed runtime and human acceptance are separate.

- Rust workspace: 110 tests pass; release build, formatting and Clippy with
  warnings denied pass.
- Web: three geometry tests and production build pass. A real browser checks
  mixed resolutions, negative positions, keyboard movement, drag undo,
  undo/redo, save, a narrow viewport and absence of JavaScript runtime errors.
- Hub protocol: a disposable real hub checks alternating master claims,
  clipboard-only exclusion, geometry/monitor announcements, pause, abrupt
  disconnect and position-preserving rejoin. The abrupt-disconnect test exposed
  the error-path cleanup bug; the corrected candidate passes it.
- Dedicated desktop containers: 12 checks pass, including native X11 clipboard,
  alternating input takeover, persistent absence across a restart, private-copy
  suppression on return, real RandR resolution/monitor add/remove events and
  direct clipboard with the disposable hub stopped. Input is generated with
  X11 tools, not a human-operated physical mouse.
- Native GTK editor: visual inspection and drag/save against an isolated mock
  API pass; negative geometry, clipboard-only entries and nonzero monitor/desktop
  offsets survive the save. All device settings are collapsed initially.
- Reconnection endurance sample: three abrupt hub terminations/restarts with
  both full and clipboard-only native agents retain 13 threads per process in
  all four samples. Original test hub and desktop agents are restored afterward.
- Idle sample: an absent virtual-desktop agent used 0.2% of one CPU core over
  five seconds and 34.9 MiB RSS. This is a short sample, not a power measurement
  or a full-day endurance qualification.
- Existing RDP fix: two different real Flameshot captures survive delayed native
  RDP callbacks, and both reach all four active histories. A repeat run uses
  unique output paths so Flameshot does not leave an older reference file in place.

Reproduce unit/build checks with `cargo test --workspace`,
`cargo clippy --workspace --all-targets -- -D warnings`, and `npm ci && npm test &&
npm run build` in `web` (Node 20+).

Run `deploy/tests/daily-use-hub-test.py` against a **disposable** hub, setting
`POOLSYNC_TEST_TOKEN`; it creates test devices and exercises their departure.
For browser tests, serve the built web bundle and run `web/tests/ui-smoke.cjs`
with Playwright installed. `POOLSYNC_TEST_URL`, `POOLSYNC_CHROME`,
`POOLSYNC_PLAYWRIGHT_MODULE` and `POOLSYNC_UI_ARTIFACTS` select the local tools and
output directory. Its API fixtures do not access a production pool.

## Installed runtime qualification

Initial deployment on 2026-10-01: installed on the current desktop, Asus, Acer
and P3 with binary/config backups;
the hub and web bundle on P3 are updated with a rollback backup. P2's agent was
not active and is not started by this change. Node tokens, clipboard E2E key,
peer credentials, TLS material, systemd configuration and saved desk positions
are preserved. Real fresh full/region screenshots are stable after 0.2, 3 and
7 seconds and reach the four active desktop histories.

- Agent binary SHA-256: `6f7663458a9b150af9ead0805c5359b4ca8c6c65ae89087daf0942a4a372e63c`.
- Hub binary SHA-256: `43c34c71f11a82e1753bdebbd1e543b1bcc920f276543170b0d0b0a3ac59fae8`.
- Installed servers/current desktop are clipboard-only; the physical Asus/Acer
  agents remain full mode. Native RDP clipboard ownership is respected.

The existing CA lacks KeyUsage and is rejected by strict X.509 clients. The
first deployment verification rolled back safely; the final check retains
chain/hostname validation using the existing clients' compatibility policy.
New PKI generation now passes strict validation. Renewing and distributing the
installed CA/certificates is a separate migration, not performed here.

Physical-device and full-day acceptance are still outstanding.

### Production reconciliation — 2026-10-02

The requested production roles are now installed and checked against the
running processes and hub announcements:

| Machine | Requested role | Verified state |
|---|---|---|
| Asus | Full KVM and clipboard | Qualified agent running; KVM enabled; hub online |
| Acer | Full KVM and clipboard | Qualified agent running; KVM enabled; hub online |
| P3 | Clipboard only | Qualified agent and hub running; KVM disabled; hub online |
| zaza-desktop | Clipboard only | Qualified agent running; KVM disabled; hub online |
| P2 | Clipboard only | Compatible agent installed and enabled; KVM disabled; waiting for a graphical zaza session |

The four active agents already matched the qualified binary and requested roles,
so their working sessions were preserved. P3's running hub still matches the
qualified hub SHA-256 above. Rollback snapshots were created on all five machines
under `~/.local/state/poolsync/deployment-backups/20261002-5338971` with private
permissions. Node credentials, encryption keys, TLS files, saved positions and
existing clipboard history were preserved.

P2 requires glibc 2.36. The exact source commit `5338971` was built in an isolated
directory on P2 with the locked dependencies and one build job. Its release
agent starts successfully for the version check, and all **81 agent tests** pass
on P2. Compatible-agent SHA-256:
`9aa5df8bb08c06870a2ee73b1ea23ee25a46aa5be2f9866788894ce47305a867`.
P2's configured node identity also successfully authenticated to the production
hub through its existing TLS trust and credentials.

No live XFCE session existed for zaza on P2 during this check. Its user service
therefore waits instead of repeatedly failing to acquire a clipboard. The
optional `deploy/systemd/poolsync-agent-graphical-session.conf` drop-in is
installed as `~/.config/systemd/user/poolsync-agent.service.d/50-graphical-session.conf`.
Session autostart and the existing watchdog can start the agent when the user
opens the graphical/RDP session. **P2's live clipboard acceptance remains
pending that session; installation is not proof of a live clipboard.**

The previous 110 workspace tests and 12 dedicated desktop-container checks
remain the functional qualification for this unchanged Rust candidate. Real
physical keyboard/mouse, HDMI docks, mixed DPI and full-day acceptance remain
separate checks. This deployment does not add serverless KVM or independent
per-monitor routing, and it does not enable GitHub CI/CD.

## Remaining product limits and next gates

### Native screenshot recovery — 2026-10-02

A production regression on the current desktop reproduced an older screenshot
replacing a new Flameshot copy within three seconds. Removing Clipman alone did
not fix it: XFCE's built-in clipboard manager was also active, and PoolSync's
BMP-only recovery still cached the last image it had offered through GTK rather
than the latest image copied by a native application.

Every accepted local image now refreshes the recovery PNG without taking X11
ownership from the copying application. The existing PRIMARY text becomes a
baseline so it cannot displace that screenshot. A native text copy discards the
old image recovery cache entirely.

On the current desktop, Clipman and its panel package are removed. Other XFCE
applications are retained. XFCE's built-in clipboard manager is disabled through
`XFSETTINGSD_NO_CLIPBOARD=1` in the user's `.xsessionrc`, which the installed X11
session launcher sources. The running settings daemon uses the same setting,
and PoolSync owns `CLIPBOARD_MANAGER`. The setting is supported by
[the XFCE 4.20.1 settings daemon](https://github.com/xfce-mirror/xfce4-settings/blob/xfce4-settings-4.20.1/xfsettingsd/main.c#L149).

Qualification: 111 release workspace tests and formatting pass. Four checks in
the dedicated desktop container cover successive native PNG copies, recovery of
the latest image after an empty BMP-only callback, an actual GTK paste consumer,
and cancellation of stale image recovery after a native text copy. The original
container agent is restored afterward; the production hub is not used.

The corrected agent is installed only on the current desktop, preserving its
clipboard-only role and byte-identical configuration. A real full-screen capture
and a distinct region capture pass eight pixel comparisons, immediately and up
to 50 seconds after copying. The installed and running executable SHA-256 is
`7cdb6c8cad30abae068daa6190a3ddc20b06734375672d3722607a16162306c8`.
The displayed version remains 2.0.3. Browser paste acceptance in ChatGPT still
requires the user's retry because that browser is not connected to the UI tools.

1. **Complete serverless control first.** Direct authenticated KVM transport,
   peer presence/expiry, bounded master claims and shared topology must work
   without the hub. Next gate: stop the hub, use either desktop's physical
   keyboard/mouse, cross an edge, remove the current master, then reboot peers in
   a different order. Clipboard survival alone does not satisfy this gate.
2. **Multi-monitor routing is still primary-monitor routing.** Extra monitors
   are announced and stay locally usable; they are not independently draggable
   pool surfaces. Add per-monitor edge segments before claiming arbitrary
   monitor-wall support. One target per direction cannot represent all layouts;
   this candidate chooses one deterministically. After a primary resolution
   change, a saved physical layout may need realignment; stale edges are removed
   instead of silently moving other machines.
3. **Qualify physical devices and mixed scaling.** Virtual RandR changes and
   numeric mapping do not prove HDMI docks, fractional DPI, suspend/resume,
   physical input takeover or days of usage. Include local plus remote RDP,
   LAN/VPN transitions, unexpected unplugging and native Wayland portal capture.
4. **Simplify setup for public users.** Show connection and RDP ownership clearly;
   move advanced credentials behind setup; keep identity enrollment distinct from
   temporary absence. Adding/removing a revoked member needs the separate
   security migration, not a temporary pause.

## Flutter/Dart and mobile

Recommendation: retain the Rust daemon/protocol and consider a **Flutter UI
written in Dart**. Flutter supports desktop platforms and native platform
integration; UI replacement does not by itself supply low-level clipboard,
input capture or serverless control. Keep the daemon running independently of
an open window and use a small local IPC/platform interface.
[Flutter desktop](https://docs.flutter.dev/platform-integration/desktop),
[platform channels](https://docs.flutter.dev/platform-integration/platform-channels).

A mobile clipboard-only companion is realistic. Start with explicit actions:
**share clipboard**, **receive/copy**, history and system share-sheet integration.
On Android 10+, ordinary background apps cannot read clipboard contents without
being the focused app or default IME. iOS provides intentional paste controls
and restricts programmatic cross-app reads. Avoid promising continuous silent
mobile clipboard synchronization.
[Android clipboard access](https://developer.android.com/about/versions/10/privacy/changes#clipboard-data),
[Apple UIPasteControl](https://developer.apple.com/documentation/uikit/uipastecontrol).

No Flutter rewrite or mobile application is implemented by this candidate.
