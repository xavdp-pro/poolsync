# KVM reverse-crossing motion regression

## Physical failure and capture diagnosis

Manual acceptance on Asus and Acer failed with the notification-hotfix binary
(`2.1.0-dev.3`, SHA-256
`ef4798a183e7adb85749eea496eecfe148a3d969d042a31acb01fbee6d161823`).
Asus is left of Acer. Acer-to-Asus crossing repeatedly returned the pointer to
the right of Asus rather than providing stable control. Runtime switch and
cursor-entry timestamps showed repeated transitions near the existing edge
cooldown. This invalidates physical KVM acceptance of that binary.

Captured motion was computed from core X11 pointer-coordinate differences.
Software pointer warps therefore became apparent device movement. An isolated
native capture harness reproduced a **770-pixel** forwarded movement from
another client's pointer warp. The unchanged relative-device test moved
exactly -160 pixels; a simple successful crossing had missed the warp defect.
The original agents and configuration were preserved in both capture runs.

## Corrective behavior

For relative pointer devices, captured motion now uses XI2.1 relative device
events and retains fractional deltas. Core pointer coordinates still determine
when the source needs recentering, but those coordinates no longer generate
forwarded movement. Software warps for recentering, cursor entry or other
applications cannot masquerade as captured relative motion. The existing core
fallback is retained when XI2.1 is unavailable or an absolute pointer device
is present; this campaign does not qualify absolute tablets/touchscreens.
The actual Asus/Acer pointer axes were inventoried as relative.

The native mesh fixture also changes: `xdotool mousemove_relative` uses a
software warp and cannot prove relative device capture. Motion injection now
uses `XTestFakeRelativeMotionEvent`; explicit `move` operations continue to
test software positioning and warp rejection. Input remains synthetic X11
input, not physical keyboard/mouse acceptance.

The candidate executable SHA-256 is
`fcd71e99668bf9189b3469eda8b5933eed83927087aad7350f8424b43f4bdee3`,
still version `2.1.0-dev.3`. All 130 Rust workspace tests, all-target Clippy
with warnings denied, and the release build pass.

## Native isolated evidence

`qualification/grab-motion-baseline-20261002.json` retains the failing
770-pixel software-warp observation.
`qualification/grab-motion-fixed-20261002.json` passes both cases: zero
forwarded warp motion and exactly -160 relative pixels with recentering,
without a positive-direction jump. The harness directly includes the shipping
`InputGrab` implementation. It runs on private Xorg display 114 in neko-desk-a
with separate HOME and authority. Original configuration and agent PID pass
preservation checks; the private display is stopped afterward.

Control attempt 45 failed before functional testing because a required native
copy-owner fixture was absent from staging. Its failure is retained in
`qualification/grab-motion-control-45-20261002.json`. This setup failure does
not provide KVM evidence. The corrected staging is used for attempt 46.

Attempt 46 passes all **33 checks** on isolated display 110 across
neko-desk-a/b/c with direct triangle links and no accessible hub. Repeated
software warps while A captures B and B captures A leave the target pointer
and focus unchanged; relative device events still move each actual target.
Native GTK key reception measures 0.2082 seconds including orchestration.
Cold start, emergency recovery, held-modifier release, network partition and
reconnect, controller disappearance/restart, temporary departure, mixed
resolutions, monitor docking/removal and ten concurrent control claims pass.
Original configuration, agent PID, away state and exact candidate cleanup
pass on all three desktops. See
`qualification/grab-motion-control-46-20261002.json`. This focused control
campaign does not repeat the preceding native-reader clipboard soak.

## Authorized production installation

The exact qualified candidate is installed sequentially on Asus, Acer,
gbs-p3, gbs-p2 and zaza-desktop. `/proc/PID/exe`, configured modes, graphical
accounts and direct presence are verified on every machine. Each configuration
hash remains unchanged. The account-specific backup identifier is
`20261002-grab-motion-fcd71e99`; binary-only rollback uses
`deploy/apply-agent-hotfix.py --rollback BACKUP_DIRECTORY` as root, preserving
later configuration edits. See
`qualification/grab-motion-fleet-deployment-20261002.json`.

Asus and Acer continue to permit KVM; the other three remain clipboard only.
All five advertise active direct presence. Physical acceptance of this new
binary remains pending; installation and a passing native fixture cannot
turn the preceding manual failure into a physical success.

During a subsequent 45.056-second stop of the legacy hub, all five exact
executables retain fresh active direct presence with the authorized modes.
The hub is then restored and its prior enablement is preserved. This window
qualifies live presence only; no new native paste or physical KVM acceptance
is claimed. See `qualification/grab-motion-fleet-hub-stopped-20261002.json`.

In a separate hub-stopped window, the exact new binary passes three alternating
image/text rounds through native GTK and Chrome on P2/P3/zaza-desktop, rotating
the source across those three computers. All four checks, twelve convergence
observations and agent/configuration preservation pass. The legacy hub is
restored afterward with unchanged enablement. See
`qualification/grab-motion-native-three-hub-stopped-20261002.json`.

## Physical observation and remaining acceptance

`deploy/tests/physical-acceptance-observer.py` observes evdev-backed input
counts and control/screen state. It never saves keycodes, event values, typed
content or clipboard data. XTEST and other non-evdev events are counted
separately and cannot count as physical acceptance. The tool makes no input,
configuration or service changes. Its `hardware_acceptance_passed` field is
always null: a human must confirm successful use.

Sixty-second observation with the preceding binary recorded no physical
events on Asus and 65 key presses, 65 releases, 277 motion events and one
button press/release on Acer. Both running executables and configurations were
preserved. These counts establish observation of actual hardware activity,
not successful cross-machine delivery. A private DEV observer setup failed
because the restricted container root could not inspect a different user's
diagnostic process; actual physical-host observations completed.
The sanitized count/preservation evidence is retained in
`qualification/physical-event-observation-20261002.json`.

After the qualified correction is installed, repeat Acer-to-Asus and
Asus-to-Acer crossing with the departure machine's actual mouse and keyboard.
Confirm stable remote pointing/typing, local Ctrl+Alt+Shift+M recovery and
monitor addition/removal during sharing. Clipboard qualification on Asus/Acer
also remains subject to their deliberately retained RDP clipboard guard.
Do not retire the rollback hub or mark the daily-use goal complete before
the outstanding physical behavior is verified.
