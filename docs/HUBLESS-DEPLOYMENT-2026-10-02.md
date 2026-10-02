# Hubless fleet migration and acceptance

## Current gate

This is an operational record, not a completed deployment report. Production
still runs the previous agents in legacy hub mode. The existing WireGuard VPN
is intentionally retained. No identities, certificates, pool encryption keys
or saved positions are rotated by this migration.

The staged candidate is `poolsync-agent 2.1.0-dev.3`, source commit `2415168`,
executable SHA-256
`208d67cb42a48e34037dcefda46f1ec84c1510f73acbd425bd4daa6e6fc0bec4`.
Package version alone does not identify it. Its 130 Rust tests, Clippy and
release build pass. Native focused qualification passes 52 lossless-copy
checks, six layout/permission checks and 41 checks across twenty private
departure/rejoin cycles. The full cold-start/recovery/endurance campaign,
attempt 41, passes all 86 checks, including 296 image/text rounds over
1,202.746 seconds. Configuration restoration and exact candidate cleanup pass.
DEV qualification permits the authorized staged installation; production and
physical-device acceptance remain pending.

| Computer | Authorized mode | Candidate preservation/dependency preflight | Installed/runtime validation | Physical input/screen acceptance |
|---|---|---|---|---|
| gbs-p3 | Clipboard only | Pass | Pending | Not a KVM target |
| gbs-p2 | Clipboard only | Pass; graphical session open | Pending | Not a KVM target |
| zaza-desktop | Clipboard only | Pass; graphical session open | Pending | Not a KVM target |
| Asus | KVM and clipboard | Pass | Pending | Pending |
| Acer | KVM and clipboard | Pass; account UID 1001 | Pending | Pending |

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

The planned backup identifier is `20261002-nohub-208d67cb`. Until an installation
occurs this is only a reserved name, not evidence that a backup exists. Backups
reside under the selected account's
`.local/state/poolsync/deployment-backups/` directory. Explicit rollback uses
`apply-hubless-upgrade.py --user ACCOUNT --rollback BACKUP_DIRECTORY` as root.
Keep the exact backup path returned by each successful installation.

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

The legacy hub on gbs-p3 remains active. Its observed automatic dependencies
are the five legacy agents and their watchdog/history scripts. Replacement
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
