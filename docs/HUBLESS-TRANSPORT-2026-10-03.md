# Clipboard transport under constrained peer bandwidth

## Retained qualification failure

Candidate `1591699f` passes the preceding input-recovery and fast-link clipboard
campaigns was installed on the five computers before this transport qualification. An additional network
qualification reveals a remaining limitation: attempt 57 caps each private
peer TCP direction at 20 Mbit/s through a disposable proxy owned by that
desktop. No central proxy, accessible hub, production network rule or original
configuration is introduced. Small text/images and native Firefox paste pass
eleven checks, but the first random full-HD PNG never reaches B/C's GTK paste
receivers within the bounded observation. The peer writer sends a complete
large clipboard WebSocket message with a one-second timeout, sharing the
connection with control traffic.

This is not a complete passing campaign. Original agent PIDs, configurations,
absence markers and private candidate/proxy cleanup pass on all three nodes.
See `qualification/bandwidth-failure-57-20261003.json`. Attempt 56 is a fixture
argument rejection before any desktop mutation: its original guard disallowed
three KVM-capable nodes in a combined clipboard/control campaign. The guard
now requires hubless mode, allowing the intended combined qualification.

## DEV correction

New peers negotiate an optional clipboard-fragment capability during the
existing authenticated WebSocket handshake. Legacy peers keep their original
wire format. Large clipboard envelopes are split into complete 16 KiB binary
messages; keyboard/mouse/presence messages retain their existing format and
can be scheduled between them. This is application framing, not interleaved
WebSocket fragments; the [WebSocket specification](https://datatracker.ietf.org/doc/html/rfc6455#section-5.4)
distinguishes complete messages from continuation frames.

The reassembled envelope still requires the existing clipboard authentication
and decryption before forwarding/application. Assembly validates the marker,
version, identity, length and contiguous offsets, caps one partial transfer at
64 MiB per link and expires stalled state. A fresh copy supersedes an unfinished
transfer. Departure/pause generations fence pending outgoing data, incoming
assembly and deferred clipboard application, including a short departure/rejoin
between scheduling turns. No completed/private clipboard history is replayed.

Application priority must also survive socket buffering. The per-socket
`TCP_NOTSENT_LOWAT` limit bounds unsent bulk bytes without reducing the TCP
flight/receive windows or changing any global setting; see the
[Linux kernel documentation](https://docs.kernel.org/networking/ip-sysctl.html).
It does not bound bytes already acknowledged by a buffering transport endpoint.
Attempt 59 demonstrates that remaining gap: native image and browser checks
pass, but a fresh GTK key misses its two-second deadline during the first large
image transfer. The isolated peer status files retain the same A-owner/C-focus
lease, and C's retained receiver eventually records the synthetic `h` key.
This failure is preserved in `qualification/fragment-mixed-failure-59-20261003.json`.

The subsequent DEV candidate adds cumulative fragment acknowledgements and a
256 KiB application window. Control messages are still sent between complete
messages, while bulk emission waits for remote application progress once its
window is full. Acknowledgements from a superseded transfer cannot grant credit
to a newer transfer; future offsets are rejected. A stalled transfer expires
without taking down the control session. This change remains under qualification.
Existing identities, certificates, encryption keys, saved layout and deliberate
password policy remain unchanged.

Attempt 58 is a scoped success (52 checks) for large native GTK/Firefox paste,
three-peer KVM, network loss, controller loss, temporary departure and restart
order on the constrained link. It does not include simultaneous sustained input
and large-copy traffic; attempt 59's failure prevents promotion despite that
earlier success. See `qualification/fragment-scoped-pass-58-20261003.json`.

Attempt 60's windowed candidate passes simultaneous native input and large-copy
traffic for 127.775 seconds (six image/text rounds, 66 fresh GTK keys, maximum
0.6041 seconds including orchestration). Transfer logs show no send failures
or ended sessions. Supersession, private departure and both target-process
restart modes pass. Its later source-partition assertion fails because the
existing partition fixture blocks only backend port 19676, while the newly
introduced shaping proxy is reached through port 19686. Source A's outgoing
connection to B:19686 therefore remains outside that simulated partition.
The fixture now blocks both private ports. Attempt 60 remains a failed campaign
and is retained in `qualification/window-network-fixture-failure-60-20261003.json`;
the partition scope also limits earlier constrained-link recovery evidence.

Attempt 61 verifies all three exact executables and private TLS listeners, then
passes native GTK/Firefox text/image and three-peer control checks. The first
mixed-workload browser paste is not recorded despite GTK image convergence and
continued target keyboard input. It is retained as an unresolved browser failure,
not a complete transport success, in
`qualification/window-tls-browser-failure-61-20261003.json`. The lab browser hook
now restores editable-target focus when its native window gains focus and
records bounded focus/shortcut/paste-dispatch metadata separately. No virtual
clipboard or artificial paste event is introduced. A fresh campaign checks
this behavior with the same executable and corrected partition fixture.

Attempt 62 repeats that browser failure and confirms the diagnostic array is
empty: the updated server was staged in the Podman host but not copied into
the desktop containers that actually launch it. Its exact failure and successful
original-agent restoration are retained in
`qualification/window-browser-diagnostic-fixture-failure-62-20261003.json`.
The desktop fixture was updated for attempt 63. Subsequent runs copy their
browser server and page into each private test root and record their source
fingerprints, avoiding dependence on an older globally installed test helper.
The synthetic key receiver lifetime is also extended beyond the ten-minute
mixed workload; cleanup still terminates only fixture-owned processes.

TLS campaigns generate a private short-lived CA and per-peer certificates under
their isolated artifact directory. Each process receives its own certificate,
private key and process-local CA trust path; system trust and production keys
are untouched. A separate normal-link compatibility fixture can run legacy C
with an explicit required executable hash while A/B negotiate the new framing.

## Current DEV evidence

The windowed executable is version `2.1.0-dev.3`, exact SHA-256
`6e9b49804eb4b48f7f1a31698bedc9d503751573cd9aef6e74148be9743208cd`.
All 147 Rust tests (101 agent, 36 core, 10 legacy hub), all-target release Clippy
with warnings denied and the release build pass. The local checkout and build
tree fingerprints match for 43 Rust/manifest/lock files.

Attempt 63 passes all 96 checks on the mandatory A-B-C chain with hub unreachable,
private TLS and every TCP direction limited to 20 Mbit/s. It includes six large
image/text rounds over 127.777 seconds while A controls C: 64 fresh GTK key
observations, median 0.25785 seconds and maximum 0.5214 seconds, including process
orchestration. Native browser paste and exact RGBA hashes pass during that
traffic. Superseded image cancellation, private departure, both target restart
modes, actual source/relay partitions, controller loss, emergency recovery,
synthetic display changes, simultaneous ownership requests, private rejoin and
reverse startup order pass. Original PIDs/configurations/absence and exact private
candidate/proxy cleanup pass on all three desktops. See
`qualification/window-tls-mixed-pass-63-20261003.json`.

This is synthetic native X11 input, not physical keyboard/mouse or monitor-cable
acceptance. Attempt 64 extends the mixed workload to ten minutes and measures
idle resources; compatibility and staged production qualification follow only
after satisfactory results.

Attempt 64 fails its first mixed browser paste before completing endurance.
Its new diagnostics prove that the paste target is focused and the native
shortcut and paste dispatch arrive, but no matching image record follows.
GTK convergence and fresh C keyboard input continue. The complete failure is
preserved in `qualification/window-native-browser-failure-64-20261003.json`.
The earlier attempt-63 success is therefore not sufficient for promotion.

The focused diagnostic skips the already qualified clipboard primer and ends
after its mixed-traffic scope. Attempt 65 rejects an incorrect fixture operation
name before opening its browser; cleanup passes and the failure is retained in
`qualification/window-browser-startup-fixture-failure-65-20261003.json`.
Attempt 66 passes 14 focused checks, six large image/text rounds over 125.1
seconds, 67 fresh native keys and original-agent restoration, without that
primer. It is not a full recovery/endurance campaign. The browser trace now
also records image-file counts/byte sizes and image-decoder error names, without
clipboard payloads. Attempt 67 restores the complete primer to investigate
the failing sequence. See `qualification/window-focused-mixed-pass-66-20261003.json`.

Attempts 67 and 69 retain the full primer and fail the first mixed native
browser paste. Their paste events contain zero image files while the GTK
receivers paste matching pixels and fresh C keyboard input still arrives.
Attempt 69 additionally runs Firefox 140.16.0esr with a private WidgetClipboard
trace. Firefox requests `image/png`, exceeds its clipboard read deadline, falls
back to empty text and later receives the image callback after the timeout.
The failing shortcut has no Shift/Alt/Meta modifiers. PoolSync offers and serves
the expected PNG; these observations narrow the failure to native retrieval,
without proving its cause or resolving it. See
`qualification/window-native-browser-timeout-69-20261003.json` and
`qualification/window-browser-primer-failure-67-20261003.json`.

Attempt 68 passes 96 full selected checks, seven mixed rounds over 139.367
seconds and 72 fresh native keys (maximum 0.4519 seconds). Attempt 70 passes
56 selected checks and six mixed rounds while its browser X11 connection goes
through the diagnostic xtrace proxy. That proxy changes I/O timing; its success
is diagnostic evidence, not proof that uninstrumented Firefox is repaired.
Attempt 71 also passes 56 selected checks with main-process syscall tracing
without an X11 proxy; its six mixed rounds span 130.014 seconds. Instrumentation
can still perturb scheduling and does not establish a repair. Its independent
selection-protocol observer records no data bytes or keyboard/mouse events.
See `qualification/window-syscall-diagnostic-pass-71-20261003.json`.
All five original production binaries remain unchanged. Endurance, legacy
compatibility and the unresolved native paste gate still prevent promotion.
See `qualification/window-native-mixed-pass-68-20261003.json` and
`qualification/window-x11-proxy-diagnostic-pass-70-20261003.json`.

Attempt 72 runs the normal native browser with twelve requested pastes per
mixed round and a metadata-only XRecord observer on the private display.
It fails the first mixed paste; fresh native C keys continue during browser
retrieval. The server delivers a new `GDK_SELECTION` property notification to
Firefox at 1790990935.6270094, but Firefox issues its next property read only
at 1790990936.5454676, 0.918458 seconds later. Earlier chunks were read promptly.
The PNG callback arrives after Firefox's one-second retrieval deadline. This
is a mid-INCR receiver stall while the property is already available, not a
missing owner response. No clipboard bytes or input events enter the protocol
report; all original desktop state is restored. See
`qualification/window-native-incr-stall-72-20261003.json`.

Attempt 73 tests an experimental native image owner using single BIG-REQUESTS
PNG properties (`51cf0508`). Its first three mixed pastes succeed but the fourth
still times out; the private browser log also contains a TARGETS metadata read
timeout. Native GTK pixels and fresh C keyboard input continue. A single PNG
property therefore does not resolve Firefox's native-retrieval issue. The
experimental Rust implementation is removed from the working candidate;
its binary, local patch and exact failure are retained. It is never installed
on a physical computer. See
`qualification/native-image-experiment-failure-73-20261003.json`.

Attempt 74 compares Chromium 153.0.8010.52 through real native Ctrl+V with
`6e9b4980`, independent of the discarded experiment. It passes 27 checks and
records 39 image pastes, including two complete mixed image/text rounds with
12 browser pastes each. Its later window-change fixture cannot find the visible
keyboard-receiver title; no failed native image retrieval is established.
The fixture now retains the actual window ID, validates that it still exists
and maps/raises it before focus. This failure remains in
`qualification/window-chromium-window-fixture-failure-74-20261003.json`.

Attempt 75 passes 17 checks, then discovers that Flameshot is missing from the
disposable capture desktop before endurance starts. The application is installed
only there. New read-only prerequisite checks record the selected native browser
version and reject a missing screenshot executable before modifying test
sessions. The failed campaign and successful restoration remain in
`qualification/window-flameshot-prerequisite-failure-75-20261003.json`.
Attempt 76 exercises the corrected fixture with a ten-minute mixed workload,
12 actual browser pastes per round, real Flameshot and the selected recovery
scenarios. It uses a disposable Chromium profile/user process. No physical
browser, sandbox setting, identity or production configuration is changed.
Firefox's failure remains explicit even if this comparison passes.

Attempt 76 passes all 97 selected checks with hub unavailable, private TLS,
the mandatory A-B-C relay and the 20 Mbit/s per-direction limit. The mixed
workload spans 634.672 seconds: 16 full-HD image/text rounds, 192 actual Chromium
image pastes and 326 fresh native GTK key observations. Native key reception
has median 0.2316 seconds and maximum 0.6817 seconds, including orchestration.
Agent CPU during that workload is 4.85%, 6.31% and 5.64% of one core; ending
RSS is 94,852, 109,436 and 93,528 KiB. All three measured PIDs remain stable.
A subsequent 60-second idle interval measures 1.79–1.91% of one core with
unchanged RSS during that interval. Receivers and private transport diagnostics
remain active; these are lab-process measurements, not laptop battery data.

Real Flameshot application capture, superseded fragments, private departure,
source/relay loss, SIGTERM/SIGKILL target recovery, controller disappearance,
emergency release, synthetic docking/undocking, mixed resolutions, concurrent
claims, private rejoin and reverse startup pass. Cleanup restores all original
PIDs, configuration bytes and absence state and stops candidate proxies.
See `qualification/window-chromium-endurance-pass-76-20261003.json`.
This is repeated finite native-browser evidence, not physical input/monitor
acceptance or a guarantee of all-day operation across every application.

The selection observer's bounded timer and display-loss shutdown are also
checked on a separately started private display with no PoolSync candidate.
Both exit cleanly and preserve the original desktop; see
`qualification/selection-observer-cleanup-20261003.json`.
A separate native GTK-to-Firefox control uses display 112, its own HOME/DBus
and loopback port, without starting PoolSync on that display. This narrows the
remaining compatibility investigation independently of the transport candidate.
Attempt 77 reproduces the Firefox 140.16.0esr failure there: nine actual full-HD
image pastes succeed, then the tenth has zero image files and the native browser
trace reports a clipboard timeout. The clipboard owner is an ordinary Python
GTK `set_image` process; no PoolSync instance runs on that private display.
Original agent/configuration/absence and all test workers are preserved/cleaned.
See `qualification/native-gtk-firefox-control-failure-77-20261003.json`.
This establishes that the lab anomaly also occurs outside PoolSync's clipboard
path. It does not identify a specific upstream defect or qualify Firefox on
physical computers. The independently passing Chromium campaign matches the
browser family used for the user's ChatGPT/Codex paste workflow. Firefox's
native-X11 limitation remains recorded rather than covered by that success.


## Transition and additional qualification

Attempt 79 passes the same independent native control with Chromium:
36 full-HD image pastes, three intervening fresh texts and original-state/
worker cleanup pass. See
`qualification/native-gtk-chromium-control-pass-79-20261003.json`.

Attempt 78 negotiates A/B fragments and legacy C fallback correctly, then
passes 45 native clipboard/input checks, including lossless PNG, BMP conversion
and delayed-copy supersession. Its first mixed bulk/input round interrupts C
control. A's trace records two complete-message send timeouts specifically to
legacy C; B/C lose their active lease. This fails active mixed-version KVM
qualification even on the normal link. Preserve the failure in
`qualification/window-legacy-mixed-failure-78-20261003.json`. A staggered
replacement must keep all participants absent until the full cohort runs the
new executable. A failure must restore the complete old cohort before resuming;
binary-only rollback of one active KVM peer is not a qualified bulk-use plan.

Attempt 80 repeats the selected triangle/TLS/race/recovery scenarios with all
three peers on `6e9b4980`: 112 checks pass. Its 135.182-second mixed workload
contains nine image/text rounds, 36 native Chromium image pastes and 70 fresh
GTK keys (median 0.21355 seconds, maximum 0.4072). No measured PID changes.
Original-state and worker cleanup pass. See
`qualification/window-triangle-native-pass-80-20261003.json`.

Attempt 81 starts every private peer on `1591699f`, pauses the whole cohort,
proves local native keyboard usability, replaces B/C/A sequentially and verifies
the exact new executable while each remains absent. Configurations and private
TLS material stay in place. A private C copy made during maintenance is not
replayed after all three rejoin; a fresh public copy crosses the updated mesh.
The complete constrained triangle/TLS/race/native-paste/recovery campaign then
passes 124 checks. Its six mixed rounds span 126.89 seconds, with 24 native
browser pastes and 68 fresh keys (median 0.21815 seconds, maximum 0.3627).
All original desktop PIDs/configurations/absence and proxy cleanup pass. See
`qualification/window-cohort-upgrade-pass-81-20261003.json`.

`deploy/apply-agent-cohort-hotfix.py` applies this maintenance rule to the five
physical hosts. It defaults to a read-only dry run, requires exact installed,
candidate and installer hashes, checks identities/modes/displays/settings,
pauses full-mode hosts first, verifies no injected input remains held and uses
the existing per-host atomic installer/backup. Participation resumes only after
a verified uniform new cohort or complete rollback. Unknown/incomplete cohorts
remain absent with local input available. Original participation is reflected
in each backup; rollback preserves later user settings. Its read-only preflight
passes on all five original production agents before authorized installation.

## Negotiated framing

Both peers must advertise `x-poolsync-clipboard-fragments: 1`. Otherwise the
existing complete encrypted text message is retained. A clipboard data message
is `PSCF1` (5 bytes), a random transfer UUID (16), total envelope size (4), byte
offset (4) and 1..16384 envelope bytes. An acknowledgement is `PSCA1` (5), the
same UUID (16) and cumulative next-byte offset (4). Integers are big-endian.
This framing is transport metadata; completed contents still require the
existing E2E authentication and ordering checks. One link keeps at most one
partial envelope and 256 KiB unacknowledged bulk, with the stated size/stall
limits. A new copy or participation generation replaces/cancels pending work.

The qualified windowed executable is now installed on all five computers via
the verified paused-cohort procedure. Its physical P2/P3/zaza-desktop GTK/Chrome
clipboard campaign passes with the hub still stopped. See
[HUBLESS-WINDOW-DEPLOYMENT-2026-10-03.md](HUBLESS-WINDOW-DEPLOYMENT-2026-10-03.md).
Physical Asus/Acer input/monitor acceptance and their native paste campaign
after safe RDP closure remain independent, open requirements. The daily-use
goal is not complete while these facts remain unverified.
