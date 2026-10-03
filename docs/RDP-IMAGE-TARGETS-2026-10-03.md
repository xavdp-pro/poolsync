# RDP image target regression: DEV diagnosis and candidate

## Observed fault and scope

The production follow-up reproduces a missing local image on both full-mode
computers with their native RDP clients active. Their local PoolSync clipboard
pause policy remains enabled. Read-only route inspection confirms that each
client reaches the same graphical session as its destination PoolSync agent.
The destination servers and the isolated fixture use XRDP
`0.10.1-3.1+deb13u2` and xorgxrdp `1:0.10.2-1`.

A new disposable Podman fixture inside the dedicated test environment runs
XRDP, FreeRDP 3.15.0, an isolated Xorg server desktop and an Xvfb client desktop.
It creates a separate `rdplab` account, random disposable login credentials and
private session environments. No real node identity, key, session or connection
is copied into it. Password input uses a pipe, not process arguments or reports.
The container has no published RDP port. PoolSync does not run in the format
comparison; this separates native RDP conversion from mesh transport.

XRDP's upstream [feature description](https://github.com/neutrinolabs/xrdp)
includes two-way bitmap clipboard transfer. That capability claim is not a
substitute for the native comparisons below.

## Comparison evidence

| Remote image owner | Local native GTK image paste through RDP |
| --- | --- |
| Ordinary GTK `set_image`, after native text-channel readiness | Pass |
| PNG/BMP plus empty UTF8_STRING/STRING/TEXT targets, modeled on the deployed PoolSync offer | Fail; local clipboard advertises text and keeps the preceding text |
| Same modeled offer with only PNG/BMP targets | Pass, repeated in two separate disposable sessions |

The expected 193 x 127 RGBA image hash is
`6fe0ae12112e3fad3f195ae5e9d46e0aa91272a9a347c68b540127ec610a4fd9`.
The fixture's synthetic text and images are intentionally non-private.
[Bounded comparison artifacts](qualification/rdp-image-format-comparison-20261003.json)
retain successes and harness failures. Early attempts exposed a startup race,
a missing absolute path to `runuser` under the remote environment, and a fixture
script error. They do not qualify a runtime. The repaired control campaign first
checks native text delivery, then compares image pixels.

The modeled BMP uses Pillow's RGB encoder; PoolSync uses Rust's RGBA BMP encoder.
Consequently this comparison identifies the target-advertisement regression but
is **not yet an executing-candidate or transparent-alpha acceptance test**.
It is also a native GTK paste-handler test, not browser or physical-input proof.

## Candidate change

The Rust image offer now advertises only formats which actually contain the
image: PNG and, when conversion succeeds, BMP aliases. It no longer advertises
empty text or answers them as though the image were a text copy. Real text/rich
text offers, image generation guards, native-owner preservation, peer encryption,
identities, layouts and the RDP pause policy are unchanged.

The candidate version is `2.1.0-dev.4`, SHA-256
`20f5168a32e446fe1edd94b678ed5ee24c10b6dc1a3336addd13c2bcdf48ffe9`.
It is built in a separate build directory with an existing compatible compiler;
no production library or installed agent is changed. All 147 release workspace
tests pass; formatting, release workspace/all-targets Clippy with warnings denied
and the agent release build pass.

**The candidate is not deployed.** The following DEV campaigns have completed.
A read-only fleet upgrade preflight passes after the exact candidate and installer
are staged in root-only directories on all five computers. No maintenance pause,
installation or RDP option change is applied. See the
[fleet preflight](qualification/rdp-dev4-fleet-preflight-20261003.json). The reviewed installer hash is
`4d31b5cf04f804a75bf21e23d0e4d424b4fbd16a24e3cd880b7c268a9bb1c49b`.
An earlier dry-run with an incorrect installer pin is rejected before any
maintenance; its private report is retained. The actual native-RDP clipboard
policy choice remains pending because the user requested preservation of settings.
The fleet continues executing `2.1.0-dev.3` with its original configuration.
Production physical edge recovery and extra-monitor acceptance remain open.

## Reproducing the format comparison safely

The two repository fixtures are:

- [RDP baseline runner](../deploy/tests/rdp-clipboard-baseline.py)
- [Modeled image owner](../deploy/tests/rdp-image-offer-owner.py)

The runner refuses to execute outside a container explicitly provisioned with
`/opt/poolsync-rdp-lab/.isolated-fixture`. Use only a newly created disposable
container whose normal entry point has been replaced by an idle process, with
no host home, credentials, X11 socket, device or production volume mounted.
Provision XRDP, xorgxrdp, FreeRDP 3, Xvfb, Python GTK/Pillow, xclip and dbus-x11.
Copy these two fixtures and `clipboard-receiver.py` into
`/opt/poolsync-rdp-lab`, then explicitly create the marker there. The runner
creates and changes only the disposable account and services inside that
container. A new clean container start is required for each comparison.

```sh
python3 /opt/poolsync-rdp-lab/rdp-clipboard-baseline.py
python3 /opt/poolsync-rdp-lab/rdp-clipboard-baseline.py --custom
python3 /opt/poolsync-rdp-lab/rdp-clipboard-baseline.py --custom --image-only
```

These are three separate fresh-session campaigns, not three commands to run
concurrently. Results contain metadata and pixel/text hashes. Session environments
and worker logs stay private; never publish their complete contents. The runner
terminates its child process groups. Stop the dedicated container afterwards to
remove remaining XRDP session descendants; preserve bounded result files first.

## Actual-agent qualification and remaining RDP choice

The complete real-agent mesh campaign passes 116 checks, including TLS peers,
two complete cold-start orders, real Flameshot capture, native Chromium pastes,
slow/refused image sources, cancellation after a newer copy, held-key recovery,
SIGTERM/SIGKILL target restart, network/controller loss, concurrent claims,
simulated docking/resolutions and departure/privacy/rejoin. Its 602.243-second
soak completes 124 fresh image/text rounds. There are 341 paste-convergence
observations: median 1.798 seconds, p95 9.647 seconds and maximum 11.196 seconds.
These include constrained 20 Mbit/s links, fault injection and orchestration;
they are not pure wire latency or a full-day stability result. The 135.708-second
mixed input/clipboard interval also passes native Chromium pastes. Cleanup and
original-state preservation pass on the disposable cold desktops. See
[Executing-candidate mesh qualification](qualification/rdp-image-only-dev4-mesh-20261003.json).

An actual-agent RDP fixture initially fails because inherited runtime directories
cause a single-instance lock conflict. A headless `--no-tray` attempt then uses
the PNG-only fallback rather than the production GTK clipboard loop. Those
fixture failures are retained. The repaired fixture gives every agent a fresh
HOME/runtime directory and private DBus, runs its normal GTK loop, and verifies
the executing binary hash through each process's own account.

With native RDP clipboard enabled, the actual candidate now delivers correct
pixels to GTK. Chromium still receives a real paste event with zero image files:
FreeRDP advertises BMP/TIFF, without PNG, on the client clipboard. Consequently
the Rust target fix alone does not qualify Chromium through native RDP.

The compared configuration uses `-clipboard` only in the disposable native RDP
client and adds a local PoolSync peer. RDP keeps its remote desktop/input role;
PoolSync handles the clipboard directly. The local agent retains
`pause_clipboard_when_rdp=true`; process detection correctly excludes the
clipboard-disabled client. Nine checks pass: fresh native session, three exact
agents and direct encrypted peers, GTK text/image, Chromium image, three fresh
text/image alternations in both receivers, and local GTK/Chromium clipboard
continuity after the destination's PoolSync agent exits. Eight real browser paste
records contain the expected hashes. The loopback peer fixture uses authenticated
WebSockets with the configured XChaCha20-Poly1305 envelope; the separate complete
mesh campaign establishes TLS. Do not describe the clipboard-disabled mode as
an image travelling through native RDP.

Older comparison artifacts reuse a native-RDP check label in this disabled mode.
The explicit `native_rdp_clipboard_enabled=false` field and peer topology define
the actual route; later runner labels are corrected. A fixture variable-shadowing
failure during the destination-loss extension is likewise retained and repaired.

The user is asked whether to apply the tested clipboard-only RDP option change
on the two full-mode clients. If accepted, stage the qualified cohort with the
existing backup/rollback procedure, change only the reviewed native clipboard
flag, reconnect with the user's existing authentication, and repeat the actual
five-node native/browser qualification with RDP open. If declined, native RDP
Chromium image handling remains unresolved and requires a further separately
qualified solution; do not silently change the option or claim this route works.
No additional user password is introduced by PoolSync.

For the actual-agent fixture, provision the exact candidate at
`/opt/poolsync-rdp-lab/candidate-agent`, plus the existing browser HTML/server
fixtures and Chromium. The hash is mandatory:

```sh
POOLSYNC_EXPECTED_SHA256="$POOLSYNC_CANDIDATE_SHA256" \
  python3 /opt/poolsync-rdp-lab/rdp-clipboard-baseline.py --agent --browser
POOLSYNC_EXPECTED_SHA256="$POOLSYNC_CANDIDATE_SHA256" \
  python3 /opt/poolsync-rdp-lab/rdp-clipboard-baseline.py --agent --pool-clipboard --browser
```

Run these in separate clean container sessions. These commands compare two
architectures; the first remains a retained failing Chromium campaign, the
second is the qualified direct-pool path. Physical keyboard/mouse/edge crossing,
monitor cables and all-day behavior remain separate unverified requirements.

The baseline runner now returns a nonzero shell exit code on retained failure.
A fresh modeled empty-text offer reproduces the missing image and exits with
code 1; this prevents an automated DEV invocation from treating a JSON-reported
failure as a successful command. A fresh read-only fleet audit confirms the
installed/executing dev.3 hashes, configuration and topology fingerprints remain
unchanged, and both full-mode nodes have zero injected keys/buttons held.
