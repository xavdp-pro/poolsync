# No-hub desktop qualification — 2026-10-02

This report records the clipboard-only direct mesh at commit `19a15fd`.
Subsequent peer-control implementation and its separate qualification are
tracked in [HUBLESS-IMPLEMENTATION-2026-10-02.md](HUBLESS-IMPLEMENTATION-2026-10-02.md).
The historical limitations below must not be read as a test of that newer code.

## Scope

This development test creates a private dummy Xorg display inside each of the
three disposable desktop containers in the dedicated test environment. A and B
use full mode; C uses clipboard-only mode. They form
an A–B–C chain with authenticated direct WebSockets and a fresh shared E2E key.
There is no direct A–C link, so an A–C paste proves forwarding through B.

Every agent points at its own loopback port 1, checked unreachable. No hub is
started or used by the candidate, including during startup or recovery. Existing
desktop agents keep running on their own display; their configuration, absence
markers, PIDs and executable fingerprints are checked afterward. Candidate
credentials, HOME, history, runtime locks, X11 selections and logs use separate
private files. Candidate agents, receivers and private X servers are removed
from the running process set in `finally`.

The candidate has the native screenshot recovery fix from `3b24ab6`; its agent
SHA-256 is `7cdb6c8cad30abae068daa6190a3ddc20b06734375672d3722607a16162306c8`.
The displayed package version is still 2.0.3. This qualification changes the test
harness and documentation, not the production runtime or protocol.

## Reproduction

Install the native receiver as described in `deploy/tests/PASTE-RECEIVERS.md` on
each disposable desktop. Each desktop must have one existing agent for the
harness to snapshot and verify, plus GTK 3, Python GI, xclip, xdotool, Xorg and
the dummy video driver. Display 110 must be unused, or choose `--display`.
Run as root on their Podman host:

```sh
python3 deploy/tests/no-hub-desktop-test.py \
  --candidate /path/to/qualified/poolsync-agent \
  --expected-sha256 7cdb6c8cad30abae068daa6190a3ddc20b06734375672d3722607a16162306c8 \
  --containers neko-desk-a neko-desk-b neko-desk-c \
  --addresses 10.89.2.2 10.89.2.3 10.89.2.5 \
  --output /path/to/private/no-hub-result.json
```

The receiver exercises its real GTK image/text paste handler once per second.
It records hashes, dimensions and timestamps; it does not upload clipboard
content. The report includes observed paste convergence time. Receiver sampling
and process orchestration are part of that time, so it is not a pure transport
latency measurement.

## Recorded result

The final isolated-display run passes **18 checks** with the hub unreachable
from every candidate for the entire run:

| Area | Observed result |
|---|---|
| Cold startup | Authenticated direct listeners and the A–B–C chain become available without a hub |
| Native clipboard | Text in both directions, two distinct PNGs with matching RGBA pixels, text after image and image after text pass |
| Temporary absence | New office copies are neither applied nor cached; private copies are not shared; absence survives an agent restart |
| Rejoin | The private copy is not replayed, and a fresh shared copy arrives |
| Peer loss and recovery | Local paste remains usable while B is stopped; delivery returns after B restarts and after A restarts |
| KVM without hub | No remote edge crossing; a local GTK window still receives keyboard input |
| Cleanup | All three original agents retain their PIDs, executable fingerprints, configuration bytes and absence markers |

Observed paste convergence ranges from **1.024 to 11.208 seconds** in this run.
Passing the delivery checks therefore does not qualify interactive latency.
No production binary, configuration or hub is deployed or changed by this test.

Exploratory runs also missed an image or an image-to-text transition. Some
earlier runs had fixture problems: inherited HOME, competing desktop clipboard
managers, and an unreaped worker PID preventing a restart. The final harness
isolates X11 and HOME, supervises/reaps its workers, and waits for a new direct
link after restarting. The last full run passes; the earlier transition
failures are retained as an unresolved repeatability concern, not silently
converted into a full-day reliability claim.

## Capability boundary

The direct peer protocol currently handles clipboard messages. The KVM loop is
started inside an established hub session and uses hub-delivered master/focus
and topology state. Moving across an edge with no hub must leave local input
available; it cannot provide cross-desktop KVM in this implementation.

Successful clipboard checks must therefore be read separately from the
`serverless_kvm` result. Full serverless KVM still requires peer presence and
expiry, shared topology, master coordination, authenticated input transport and
recovery when the current input owner disappears.

These checks use synthetic X11 events and agent process restarts. They do not
qualify physical keyboard/mouse takeover, a full container or workstation
reboot, HDMI docks, Wayland, peer TLS deployment or full-day endurance.
They also do not qualify the original desktop's competing clipboard managers
or its Neko clipboard bridge.

Leaving the pool deliberately releases the agent's GTK clipboard ownership.
The old selection can therefore become empty on a private display with no
other clipboard manager. The absence gate checks that a new office copy is
not applied or cached, and that a new private copy is not shared; it does not
require the old pool copy to remain selected.

## Latency finding

An initial ten-second convergence limit exposed a delayed reverse paste across
the chain. The native text arrived eventually, but the receiver deadline expired.
`write_clipboard()` awaits `ensure_text_is_actually_served()`, which checks at
250, 1,500 and 4,000 ms. `serve_peer_session()` waits for local application to
finish before forwarding an incoming clipboard. These delayed checks can add
roughly 5.75 seconds at each relay even when the text is already pasteable.

The qualification allows up to 25 seconds to establish whether delivery recovers
and reports the observed delay explicitly. This does not make that delay suitable
for daily use. A follow-up change should keep verification and stale-owner
protection while removing its wait from forwarding and serial message handling.
