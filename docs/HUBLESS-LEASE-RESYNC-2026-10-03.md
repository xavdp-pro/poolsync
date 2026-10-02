# Hubless control recovery after peer reconnection

## Failure and corrective behavior

The preceding motion-capture binary, SHA-256
`fcd71e99668bf9189b3469eda8b5933eed83927087aad7350f8424b43f4bdee3`,
passes native A-to-B-to-C control and return with a mandatory relay and no
direct A/C TCP connection: attempt 47, 27 checks. Attempt 49 adds relay
partition/recovery and passes 32 checks. Original configurations, agent PIDs,
participation and candidate cleanup pass. Attempt 48's retained failure is a
fixture assertion: source Shift was still held on A, so local GTK received
uppercase Z while the test expected lowercase z. The corrected fixture
releases its source modifier before that local-keyboard assertion.

Attempt 50 demonstrates a protocol gap in a triangle with an alternate path.
A/C continue working while B is unavailable. On return, B has expired its
previously observed session and rejects the same-term renewals that cannot
resurrect an expired lease. It remains without the current session. The
fixture also raised on the null lease; a bounded convergence wait now exposes
the actual recovery requirement. This is not a complete passing campaign.
Its two-minute active-capture subset does pass: 6,572 native motion events,
53 software warps and fresh C GTK key reception, with stable agent PIDs and
less than 1 MiB RSS growth per agent. The failure and success evidence remain
separate in `qualification/three-kvm-*-20261002.json`.

The corrected protocol asks the current owner for a fresh, correlated answer
when an expired observer receives the continuing owner's renewal. The request
uses a process-local random challenge, requester/owner boot identities and a
three-second monotonic deadline. Only an owner with a currently valid lease
answers. Acceptance requires that challenge, the expected owner process,
active authorized owner/target, and the existing claim ordering. An unsolicited,
expired or previous-process answer cannot restore a session. Input queued
before the answer's sequence cannot revive old keys or switches.

The answer restores only the requesting observer's state. The uninterrupted
owner/target keep the same term, focus and held keys. Requests and answers are
optional encrypted presence metadata, retaining the existing envelope/domain
and direct authentication. Older peers can deserialize/relay the presence
without understanding the additional fields. No identity, key, password,
saved position or clipboard format changes are required.

## Candidate and scoped native evidence

Candidate: `2.1.0-dev.3`, exact executable SHA-256
`214d24134ddd42c7786f24a3416ec9db540d7233dbd42f733ce0dc83b143b54d`.
All 137 Rust workspace tests, all-target Clippy with warnings denied and the
release build pass. Seven new model tests cover correlation, unsolicited and
timed-out answers, requester/owner restart, newer claims, uninterrupted
sessions, queued input fencing and legacy presence defaults.

Attempt 51 passes **46 native checks** on private Xorg/HOME/keys in
neko-desk-a/b/c, with the hub unreachable throughout. A controls C through
the peer mesh. B's partition leaves A/C active; reconnect restores the exact
same epoch on all three and preserves C's held modifier until its actual
release. Network/controller loss, local emergency recovery, temporary
departure, monitor docking/removal, different resolutions and ten concurrent
claims also pass. Original configuration, agent PID, away state and exact
candidate cleanup pass on all three desktops.

Active capture lasts 61.751 seconds with 3,348 motion events in native XTEST
bursts, 27 software warps and six fresh GTK key observations (0.1599–0.2016
seconds, including fixture orchestration). Measured agent CPU is 3.57%, 2.98%
and 5.54% of one core; RSS grows by 128, 512 and 768 KiB. These measurements
describe this isolated workload, not physical device latency or long-term
daily acceptance. See `qualification/lease-resync-triangle-51-20261003.json`.

Physical reverse-crossing, real monitor hotplug and the Asus/Acer native
clipboard campaign with their RDP guard remain separate acceptance gates.
Keep the daily-use goal open until those requirements are verified.

Attempt 52 passes 32 native checks with a mandatory B relay, no direct A/C
TCP connection and relay partition/reconnection. See
`qualification/lease-resync-chain-52-20261003.json`.

Attempt 53 fails an additional target-process restart check: C's X server
retains held remote input after its agent is
terminated and restarted. A recovers locally, but this does not qualify target
recovery. The retained failure is
`qualification/target-restart-53-20261003.json`. Consequently the `214d2413`
candidate was **not promoted**. Production still runs the preceding `fcd71e99`
motion fix. The scoped passing campaigns above do not supersede this failure.
