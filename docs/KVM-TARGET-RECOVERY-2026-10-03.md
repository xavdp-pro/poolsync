# Recovering remote input after an agent process disappears

## Retained failure and correction

The lease-resynchronization candidate `214d2413` passes the scoped chain and
triangle campaigns but fails attempt 53: stopping and restarting target C's
agent fails to clear its held injected input in the unchanged X server.
Source A's local recovery alone is insufficient. That candidate was not
deployed. The failure remains in `qualification/target-restart-53-20261003.json`.

The correction records only currently held remote keycodes/buttons in a bounded,
versioned, node-scoped X11 root property. It has no text, completed input
history, filesystem log or physical-device events. Empty state deletes the
property. X11 properties survive the storing client's termination and disappear
on server reset; see the [X.Org protocol specification](https://xorg.freedesktop.org/archive/X11R7.7/doc/xproto/x11protocol.html#ChangeProperty).

The recovery marker is acknowledged before each down event. Up events are
acknowledged before their marker is cleared, including paired wheel clicks.
On startup, the node validates and releases its recorded virtual inputs, then
deletes the property. Duplicate releases after an interrupted cleanup are
idempotent. Invalid, oversized or truncated properties fail validation rather
than injecting arbitrary codes. Failed releases retain their marker for retry.
Normal session/lease changes also release tracked inputs and clear their markers.
Button recovery does not warp the pointer. No identity, encryption key, password,
screen position or clipboard setting is changed.

This implementation and qualification concern the fleet's X11 sessions.
Wayland retains its existing backend and is not claimed as crash-qualified by
these results. An abandoned target's held input is recovered when its agent
restarts; this does not promise input cleanup while that agent stays stopped.

## Candidate and qualification

Exact candidate: `poolsync-agent 2.1.0-dev.3`, SHA-256
`1591699f4a5bd74b2e88639d2b3ed532a8c80082ddaffa6bb7f50c280459a132`.
The formatted workspace, all 140 Rust tests, all-target release Clippy with
warnings denied and release build pass. Native campaigns run on private
Xorg/HOME/runtime/keys in neko-desk-a/b/c, with no accessible hub. Their
generated input is explicitly synthetic XTEST, not physical-device acceptance.

Attempt 54 passes **48 checks** with a mandatory relay and no direct A/C
candidate TCP connection. Attempt 55 passes **62 checks** in a triangle, with
alternate-path continuity, correlated returning-peer resynchronization,
unchanged active epoch and retained target modifiers until their actual release.
Both qualify SIGTERM and SIGKILL target restart: source keyboard recovery,
target key/button release, empty recovery marker, preserved unrelated held key
and pointer, fresh native GTK remote input and normal key-release cleanup.
Controller/network loss, temporary departure, local emergency return and
candidate cleanup pass. Attempt 55 also qualifies simulated monitor docking,
mixed resolution, undocking and ten simultaneous-claim rounds.

The 301.106-second active-capture subset sends 16,492 native motion events,
133 software warps and 27 fresh GTK key observations. Measured key reception is
0.1516–0.2966 seconds, including fixture orchestration. Agent CPU is 3.74%,
3.02% and 5.79% of one core; RSS changes by 0, 916 and 640 KiB respectively.
All three PIDs remain stable. This is a finite synthetic workload, not a daily
physical-use guarantee. Original configurations, PIDs, absence state and exact
candidate cleanup pass in both campaigns. See
`qualification/target-recovery-54-20261003.json` and
`qualification/target-recovery-triangle-55-20261003.json`.

## Authorized fleet deployment

The exact candidate is installed progressively on gbs-p3, gbs-p2,
zaza-desktop, Asus and Acer. All five executing fingerprints match; modes,
saved layouts and active direct membership retain their authorized values.
Asus/Acer temporarily leave the pool before the old, unjournaled binary stops.
Their old virtual inputs are verified released; participation is restored after
replacement. No physical events are injected during this installation.

Every host retains the previous executable and complete configuration in
`~/.local/state/poolsync/deployment-backups/20261003-recovery-1591699f`.
The Asus/Acer backup's transient installer absence marker is removed to reflect
the original participating state. Binary rollback uses
`deploy/apply-agent-hotfix.py --user zaza --rollback BACKUP_DIRECTORY` as root,
without overwriting later user settings. Before a rollback to an unjournaled
binary, leave the pool and verify held input is released; preserve/restore the
user's previous participation state.

Physical Asus/Acer crossings, emergency recovery, real monitor hotplug and
their native clipboard campaign with RDP sessions safely closed remain
separate acceptance gates. The daily-use goal remains open.

## Production without the legacy hub

The current binary passes three image/text rounds, rotating the copy source
across P2/P3/zaza-desktop, through actual GTK and Chrome paste handlers with
the hub stopped throughout. Four checks, twelve convergence observations and
original agent/configuration preservation pass. Median convergence is 2.8345
seconds and maximum 4.236 seconds, including fixture orchestration and receiver
sampling. These values are paste-convergence measurements, not network latency.
The report is `qualification/recovery-native-three-hub-stopped-20261003.json`.

After dependency and saved-backup verification, the hub's automatic startup is
disabled. The service is inactive, TCP 9470 has no listener, and all five exact
agents retain fresh direct presence with correct permissions and saved layouts.
Hub files and the verified `/opt/poolsync/hub-backups/20261002-pre-nohub` rollback
bundle remain on gbs-p3. `systemctl enable --now poolsync-hub` restores its former
service state if needed. The existing VPN is deliberately preserved. Exact
runtime and remaining acceptance gates are recorded in
`qualification/recovery-fleet-nohub-20261003.json`.
