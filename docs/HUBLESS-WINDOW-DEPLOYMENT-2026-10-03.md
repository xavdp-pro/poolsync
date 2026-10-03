# Hubless clipboard window: qualification and fleet deployment

## Executing production version

All five computers execute `2.1.0-dev.3`, exact SHA-256
`6e9b49804eb4b48f7f1a31698bedc9d503751573cd9aef6e74148be9743208cd`.
The runtime source is `e1e5339`; subsequent qualification/migration documentation
does not rebuild that artifact. All 147 Rust tests, release Clippy with warnings
denied and the release build passed. The discarded native image-owner experiment
is never installed on a physical computer.

| Computer | Mode | Executing PID after deployment | Display |
| --- | --- | --- | --- |
| Asus | KVM and clipboard | 543002 | `:0` |
| Acer | KVM and clipboard | 1546397 | `:0` |
| gbs-p2 | Clipboard only | 2067349 | `:10` |
| gbs-p3 | Clipboard only | 1426082 | `:10` |
| zaza-desktop | Clipboard only | 372605 | `:10` |

These are observed deployment PIDs, not permanent identifiers. Actual executable
hashes, account IDs, modes, displays, configuration and saved-layout hashes are
verified independently on each host. See
`qualification/window-cohort-fleet-deployment-20261003.json`.

## DEV and native production evidence

The dedicated gbs-test/LXD/Podman lab uses separate Xorg displays, HOME, DBus,
credentials and profiles. Original desktop agents remain unchanged.

| Campaign | Scope | Result |
| --- | --- | --- |
| 76 | Three-peer relay, TLS, 20 Mbit/s directions, real Flameshot, recovery/privacy/screens | 97 checks; 634.672 seconds mixed traffic; 192 native Chromium image pastes and 326 fresh native keys |
| 80 | Updated three-peer triangle, TLS, slow/refused PNG, BMP, deferred copy, input/recovery/screens | 112 checks; 135.182 seconds mixed traffic; 36 native browser image pastes |
| 81 | Old-to-new paused cohort, configuration/private-copy preservation, constrained triangle and full selected scenarios | 124 checks; all original agents/configurations/absence and proxy cleanup preserved |
| 83 | Fresh isolated desktops, two complete container start orders, TLS triangle, native GTK, recovery/privacy/screens and prolonged copies | 100 checks; 1,205.105 seconds, 290 alternating image/text rounds; candidate cleanup and independent original-desktop preservation pass |
| Physical subset | Actual P2/P3/zaza-desktop GTK and Chrome, rotating copy source over three rounds, hub stopped | Four checks and 12 convergence observations; median 2.638 seconds, maximum 3.684 seconds |

The convergence values include fixture orchestration and receiver sampling;
they are not pure network latency. Native production proof is
`qualification/window-native-three-hub-stopped-20261003.json`.
The DEV campaigns use synthetic X11 input and dummy display changes. They do
not establish physical keyboard/mouse or monitor-cable acceptance.

A subsequent read-only production interval takes 13 process samples on every
computer over 60 seconds, without generating input or copying clipboard content.
All five exact agents and configurations remain unchanged, and every sample
retains five active peers. Interval measurements are:

| Computer | CPU, percent of one core | RSS at start/end, KiB |
| --- | --- | --- |
| Asus | 3.0493 | 50,676 / 50,676 |
| Acer | 4.2820 | 57,904 / 57,904 |
| gbs-p2 | 1.3498 | 54,220 / 54,348 |
| gbs-p3 | 1.0332 | 27,888 / 28,144 |
| zaza-desktop | 0.5000 | 44,704 / 44,704 |

These measure the agent process, not total machine power or battery drain.
This finite interval does not prove full-day memory stability. The desktop's
process-start and uptime clock origins are incompatible, so its lifetime CPU
estimate is explicitly unavailable; the monotonic interval remains valid.
See `qualification/window-fleet-resources-20261003.json`.

Fresh cold-fixture attempt 82 independently repeats complete container restarts
in orders A/B/C and B/C/A on this exact artifact. Both restore native mesh text
delivery without an accessible hub. Subsequent image/text, screenshot, lossless
retrieval, deferred-owner and control checks pass, but the campaign stops after
39 checks because the separate old cloned image lacks `xinput`. The target button
query cannot run; no target-release assertion or prolonged soak is qualified by
this incomplete campaign. Its failure and complete candidate cleanup are retained
in `qualification/window-cold-fixture-failure-82-20261003.json`. The disposable
clones receive that missing package, and the harness now checks xinput/xprop
availability before changing any lab session.

The subsequent complete attempt 83 passes all 100 checks, including both cold
restart orders, full-HD images, a real XFCE screenshot, slow/refused lossless
offers, stale-owner cancellation, alternate-path continuity, target SIGTERM and
SIGKILL recovery, network/controller loss, emergency return, simulated monitor
changes, concurrent claims and private departure/rejoin. Its 1,205.105-second
soak finishes 290 actual image/text rounds. The complete campaign records 627
paste-convergence observations: median 1.803 seconds, p95 2.968 seconds and
maximum 5.604 seconds. These include fault injection, receiver sampling and
orchestration; they are not pure wire latency or a browser soak.

All three PIDs stay stable during the soak. Agent CPU is approximately 4.02%,
3.99% and 3.97% of one core. RSS moves from 39,656 to 51,156 KiB on A, 95,620 to
99,388 KiB on B and 35,648 to 48,172 KiB on C. The last quarter varies by 808,
500 and 872 KiB respectively; the following 30-second idle interval has unchanged
RSS on every peer. Report the observed increase and finite duration rather than
claiming full-day memory or battery stability. See
`qualification/window-cold-soak-pass-83-20261003.json`.

Independent post-run verification compares neko-desk-a/b/c against the previous
retained original snapshots: exact PIDs, executable hashes, configuration bytes
and participation markers are preserved. All three candidate agents and private
Xorg servers are gone. Their disposable cold clones are stopped after preserving
the artifacts; the original Neko desktops keep running. The separate evidence is
`qualification/window-cold-soak-preservation-83-20261003.json`.

Firefox 140.16.0esr times out in this lab even with an ordinary GTK clipboard
owner and no PoolSync instance on the test display (attempt 77). The equivalent
Chromium control passes 36 image pastes and three texts (attempt 79). Preserve
both results. This does not identify a specific upstream bug or prove Firefox
behavior on physical computers. Details and all retained failures are in
[HUBLESS-TRANSPORT-2026-10-03.md](HUBLESS-TRANSPORT-2026-10-03.md).

## Migration and rollback

Active mixed-version bulk KVM fails attempt 78: the legacy complete-message
writer times out toward old C and control is interrupted. Wire-format fallback
and the preceding native clipboard checks pass, but active bulk coexistence is
not qualified. Production therefore leaves the whole pool temporarily before
replacing any executable. Full-mode nodes leave first and injected input is
verified released. Replacements are sequential; participation resumes only
after every executing binary is verified to be the new artifact.

The coordinator defaults to a read-only dry run and pins candidate, installed
and per-host installer hashes. Failure inspects every host, including an unknown
installation outcome, and attempts full-cohort rollback while paused. An
unknown/incomplete cohort remains absent rather than resuming mixed-version
KVM. Local input remains available. Configuration is never rolled back over
later user edits.

Every host retains its previous `1591699f` executable and complete configuration
in `~/.local/state/poolsync/deployment-backups/20261003-window-6e9b4980`.
All five backup/executable/configuration/layout/security checks pass; backups
are mode 0700, manifests mode 0600 and their saved absence reflects original
participation. See `qualification/window-fleet-backup-verification-20261003.json`.

To restore the full previous cohort, run the same coordinator with the saved
old binary as the candidate and a fresh backup ID. It saves the current artifact
before restoring the old one. Run first without `--apply` for its dry run:

```sh
python3 deploy/apply-agent-cohort-hotfix.py \
  --candidate /home/zaza/.local/state/poolsync/deployment-backups/20261003-window-6e9b4980/agent \
  --installer /opt/poolsync/staging/window-20261003/apply-agent-hotfix.py \
  --expected-current-sha256 6e9b49804eb4b48f7f1a31698bedc9d503751573cd9aef6e74148be9743208cd \
  --expected-candidate-sha256 1591699f4a5bd74b2e88639d2b3ed532a8c80082ddaffa6bb7f50c280459a132 \
  --expected-installer-sha256 4d31b5cf04f804a75bf21e23d0e4d424b4fbd16a24e3cd880b7c268a9bb1c49b \
  --backup-id 20261003-window-rollback-1591699f \
  --output /path/to/private/cohort-rollback.json
```

Add `--apply` only for the actual full-cohort rollback. Restoring the old version
also restores its documented bulk-transfer limitation. A one-host active KVM
rollback is not the qualified procedure.

## Hub state and open acceptance

The legacy hub on gbs-p3 stays inactive and disabled; TCP 9470 has no listener.
Its original files and verified `/opt/poolsync/hub-backups/20261002-pre-nohub`
rollback bundle remain. The existing VPN is deliberately retained. No GitHub
Actions, CI/CD or Vercel automation is activated.

The daily-use goal stays open pending real Asus/Acer crossings in both directions,
typing from each source's physical keyboard, Ctrl+Alt+Shift+M local recovery and
actual monitor addition/removal. Their native clipboard campaign also awaits
normal closure of their existing RDP client sessions: the deliberate clipboard
pause setting remains enabled. The selected three-host production success does
not cover these two portable computers. Human requests for those indispensable
actions remain pending.

A human reply reports right-edge obstruction from Asus toward Acer and repeated
return to the Asus right edge when crossing from Acer. The executed artifact
during that attempt is unconfirmed; retain the report as an unresolved physical
failure rather than treating DEV success as acceptance. A fresh read-back verifies
both portable agents still execute `6e9b4980` with mutually consistent left/right
positions. Their current process journals contain no edge switch. Two independent
180-second evdev observations record no physical events and no control lease,
while preserving both executing agents and configurations. This absence of a new
physical attempt proves neither successful crossing nor a reproduced failure on
the current artifact. Repeat both directions with each source's own physical
input, correlate the observed lease with the human result, and retain the gate.
See `qualification/physical-edge-unobserved-20261003.json`.

The final fleet audit after attempt 83 verifies the same five executing PIDs,
exact artifacts, modes and displays as deployment, byte-identical configurations
and saved layouts, and fresh active membership for all five computers. The hub
remains inactive/disabled with no TCP 9470 listener; GitHub Actions remains
disabled. See `qualification/window-final-fleet-audit-20261003.json`.

Physical acceptance remains unverified:
physical Asus/Acer crossings and typing, emergency return, actual monitor-cable
changes, and their native paste campaign after safe closure of the existing RDP
clients. Current process journals still contain no physical edge switch, and the
RDP clients remain open. The reported obstruction/loop stays unresolved, with
the artifact used during that human attempt unconfirmed. Independent DEV,
deployment, backup, native three-host and resource checks are complete to the
recorded finite scope. Further completion now needs those human actions and
their result; repeated synthetic input cannot supply physical acceptance.

## Subsequent production follow-up

See [Production follow-up](PRODUCTION-FOLLOWUP-2026-10-03.md) for the nine-round
three-node and ten-round five-node native paste campaigns, six automated
controller checks, preserved fleet state and retained RDP-active image failures.
These extend the evidence without completing physical acceptance or qualifying
the end-to-end RDP image path. The deployed binary remains unchanged.
