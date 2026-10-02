# Hubless fleet migration and acceptance

## Current gate

All five production agents are installed and run in hubless mode. This is an
installation and qualification record; physical daily-use acceptance is still
incomplete. The existing WireGuard VPN is intentionally retained. Identities,
certificates, pool encryption keys and saved positions were preserved.

The staged candidate is `poolsync-agent 2.1.0-dev.3`, source commit `2415168`,
executable SHA-256
`208d67cb42a48e34037dcefda46f1ec84c1510f73acbd425bd4daa6e6fc0bec4`.
Package version alone does not identify it. Its 130 Rust tests, Clippy and
release build pass. Native focused qualification passes 52 lossless-copy
checks, six layout/permission checks and 41 checks across twenty private
departure/rejoin cycles. The full cold-start/recovery/endurance campaign,
attempt 41, passes all 86 checks, including 296 image/text rounds over
1,202.746 seconds. Configuration restoration and exact candidate cleanup pass.
DEV qualification permitted the authorized staged installation. A subsequent
notification-only hotfix is now the running candidate, version 2.1.0-dev.3,
SHA-256 `ef4798a183e7adb85749eea496eecfe148a3d969d042a31acb01fbee6d161823`.
It passes 130 Rust tests, all-target Clippy and seven native XFCE notification
checks on a private Xorg/DBus/HOME session in neko-desk-a. Package version alone
does not identify either binary. Clipboard/control code is unchanged by this
hotfix; the complete attempt-41 qualification belongs to the preceding binary.

| Computer | Authorized mode | Candidate preservation/dependency preflight | Installed/runtime validation | Physical input/screen acceptance |
|---|---|---|---|---|
| gbs-p3 | Clipboard only | Pass | Exact hotfix executing on :10; native GTK/Chrome paste passes | Not a KVM target |
| gbs-p2 | Clipboard only | Pass; graphical session open | Exact hotfix executing on :10; native GTK/Chrome paste passes | Not a KVM target |
| zaza-desktop | Clipboard only | Pass; graphical session open | Exact hotfix executing on :10; native GTK/Chrome paste passes | Not a KVM target |
| Asus | KVM and clipboard | Pass | Exact hotfix executing on :0; direct presence passes; clipboard RDP guard active | Pending |
| Acer | KVM and clipboard | Pass; account UID 1001 | Exact hotfix executing on :0; direct presence passes; clipboard RDP guard active | Pending |

Private bundles are staged at
`/opt/poolsync/staging/hubless-20261002-native-reader`. Staging does not change
the installed executable or configuration. Each computer has its own rendered
configuration and imported topology; existing peer routes and credentials are
retained, with authorized alternate direct routes added.

## Installation and rollback

After satisfactory DEV qualification, install sequentially on gbs-p3, gbs-p2,
zaza-desktop, Asus and Acer. `deploy/apply-hubless-upgrade.py` requires the
inventoried original configuration hash and the exact candidate executable
hash. Before stopping an agent, it saves the complete configuration, changed
executables/scripts/units, ownership and service state in a private deployment
backup. Replacement is atomic and exceptions trigger automatic restoration.

The verified migration backup identifier on every computer is
`20261002-nohub-208d67cb`. Backups reside under the selected account's
`.local/state/poolsync/deployment-backups/` directory. Explicit rollback uses
`apply-hubless-upgrade.py --user ACCOUNT --rollback BACKUP_DIRECTORY` as root.
Keep the exact backup path returned by each successful installation.

The binary-only notification hotfix has a second verified backup on all five:
`20261002-notifications-ef4798a1`. `deploy/apply-agent-hotfix.py` checks both
installed and executing fingerprints, backs up the previous binary and complete
configuration, then replaces only the executable. The live configuration hash
is unchanged on every computer. Rollback with this tool restores the binary
and service state without overwriting later user configuration edits.

## Native production tests and notification correction

Four unsuccessful fixture attempts are preserved with the successful evidence.
Initial failures exposed browser startup dependence, an unauthorized root X11
clipboard query, Acer's Gdk 4/Gtk 3 import conflict, and a repeated fixture image
already recorded before the fresh copy timestamp. The fixture now uses the
graphical account's cookie, explicit Gdk 3, distinct image pixels per run, and a
private Chrome profile forced to the verified X11/loopback receiver.

The three clipboard-only machines pass 15 alternating image/text rounds through
actual GTK and Chrome paste handlers: 16 checks and 60 measured convergence
observations, median 3.1245 seconds, maximum 4.769 seconds including orchestration
and receiver sampling. This campaign used the preceding native-reader binary
while the legacy hub was available; it is not five-machine hub-down acceptance.
Asus and Acer retain their existing `pause_clipboard_when_rdp = true` setting.
Both currently run FreeRDP with clipboard redirection, so their local selection
is intentionally not replaced by incoming pool copies. Closing those RDP
connections safely remains a human prerequisite for the five-desktop paste test.

The production fixture incorrectly sent the KVM ownership shortcut to
clipboard-only desktops before every browser paste. PoolSync marked the denied
request as critical; XFCE kept it visible despite the requested timeout. The
fixture now sends this shortcut only to full-mode desktops. Routine denial and
suspension messages use normal urgency and a finite timeout. Status messages
replace the preceding status notification using the actual returned ID, scoped
to the current DBus notification daemon owner. Daemon restart cannot cause an
old PoolSync ID to overwrite another application's notice.

Seven isolated native checks verify twenty-request coalescing, five-second
denial expiry, ten-second suspension expiry, preservation of an unrelated
critical notification, and daemon restart with deliberate notification-ID reuse.
On gbs-p3 the installed hotfix was also tested: three ownership shortcuts give
one visible notice and zero after six seconds. Existing stuck PoolSync denial
notices were closed by matching their accessibility text and individual X11
windows; the notification daemon and unrelated notices were preserved.
See `qualification/notification-expiry-20261002.json` and the retained
`qualification/physical-20261002-attempt-*.json` /
`qualification/physical-three-20261002-attempt-*.json` evidence.

The hotfix subsequently passes three additional image/text rounds, with each
of P2, P3 and zaza-desktop acting as copy source, through both GTK and Chrome,
while the legacy hub service is stopped. Four checks, twelve paste convergence
observations and agent/configuration preservation pass. All five computers
continue advertising the five direct peers with their correct KVM permissions
while the hub is unavailable. This proves native clipboard independence for
the selected three machines and direct presence for all five; it does not
replace the pending Asus/Acer native paste and physical input/screen evidence.
The hub is restarted afterward, with its previous enablement preserved.
See `qualification/physical-three-20261002-hub-stopped-hotfix.json`.

After each installation independently verify:

- The systemd main PID is the actual graphical agent, and `/proc/PID/exe`
  hashes to the qualified candidate. The installed package version is recorded.
- The account, graphical display and original KVM/clipboard-only mode are
  correct. Existing saved geometry and security material are preserved.
- Native direct presence converges over authenticated encrypted peer links;
  no legacy hub connection or hub-based watchdog restart is needed.

After all five installations, run
`deploy/tests/physical-fleet-clipboard-test.py` with the exact executable hash,
15 rounds and `--native-browser`. This creates isolated native GTK copiers,
paste receivers and Chrome profiles in the existing graphical sessions. It
rotates the copy source across the five computers and checks actual pasted
fixture text/image hashes. It records agent/configuration preservation and
cleans up only its own workers. These tests establish native application paste
on the physical computers, not physical keyboard-device acceptance.

## Hub retirement and remaining human evidence

The legacy hub on gbs-p3 is retained for rollback and incomplete acceptance.
Its previous automatic dependencies were the five legacy agents and their
watchdog/history scripts. Replacement
scripts use the direct runtime state in hubless mode. An old manual hub API
test utility is not a hubless acceptance tool.

First complete fleet runtime/native-paste validation, then stop the hub
temporarily and repeat the fleet native-paste qualification. Preserve the hub
executable, configuration, credentials and systemd enablement state for rollback.
Reversible disablement follows dependency and independence validation; deletion
is not necessary to achieve hubless operation.

Asus and Acer still require real keyboard/mouse takeover in both directions,
local emergency recovery, distinct Flameshot pastes into the user's application,
and actual monitor addition/removal while input is shared. Synthetic X11 events
and dummy-monitor qualification cannot establish those physical facts. The goal
must remain open while any required physical behavior remains unverified.

No GitHub Actions, CI/CD or Vercel automation is enabled by this procedure.
