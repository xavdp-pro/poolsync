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
| Physical subset | Actual P2/P3/zaza-desktop GTK and Chrome, rotating copy source over three rounds, hub stopped | Four checks and 12 convergence observations; median 2.638 seconds, maximum 3.684 seconds |

The convergence values include fixture orchestration and receiver sampling;
they are not pure network latency. Native production proof is
`qualification/window-native-three-hub-stopped-20261003.json`.
The DEV campaigns use synthetic X11 input and dummy display changes. They do
not establish physical keyboard/mouse or monitor-cable acceptance.

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
