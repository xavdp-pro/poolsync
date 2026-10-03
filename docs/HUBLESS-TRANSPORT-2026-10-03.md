# Clipboard transport under constrained peer bandwidth

## Retained qualification failure

Candidate `1591699f` passes the preceding input-recovery and fast-link clipboard
campaigns and remains installed on the five computers. An additional network
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

This correction is under DEV qualification. No transport candidate is promoted
until constrained-link, mixed clipboard/control, privacy, restart and native
paste evidence is satisfactory. Physical Asus/Acer acceptance and their native
paste campaign after safe RDP closure remain independent requirements.
