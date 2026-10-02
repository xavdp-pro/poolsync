# Hubless peer control: development qualification

## Status and boundaries

The development agent can run presence, layout, input ownership, KVM and
clipboard over authenticated direct peer links. `hubless = true` prevents
starting the legacy hub session or its KVM loop. An absent hub is not a fallback
condition; it is the qualified startup condition. No production machine has
been migrated by the qualification described here. This is not a release or
physical daily-use acceptance report.

The existing WireGuard VPN is retained by explicit user choice. Hubless here
means no PoolSync hub process or indispensable PoolSync computer. It does not
claim independence from the existing cross-site VPN gateway.

The first full native qualification candidate is SHA-256
`1ac3a6e1ad0fed7f3d4da4c25520c97cbc57b03b60bbf3bec559928be9e3b1eb`.
Its package version remains 2.0.3; that version alone cannot identify the
development build. Later source changes must receive their own qualification.

The current development candidate is version **2.1.0-dev.3**, SHA-256
`c1c38e2f957e32a8e2a5e35d533c82a2f414f2ed1234dedba8e4f2e421090b93`.
Its 124 Rust tests, workspace Clippy checks and release build pass. Attempt 27
passed a 20-minute sequence with GTK native copy providers and real Flameshot
captures. Attempt 27 later failed an immediate private-copy departure scenario;
this candidate is not eligible for production. Departure acknowledgment and
selection release protection are being qualified in a later candidate.

## Protocol and recovery

- Each authorized peer advertises its mode, participation, screen geometry,
  monitors and process boot identifier every second. Presence expires locally
  after four seconds, independently of any central process.
- A full, participating KVM peer can claim control. Claims use a Lamport term
  and node-name tie break; every peer applies the same ordering. A controller
  renews its three-second input lease. Clipboard-only peers cannot claim or
  receive KVM input.
- Switch and input packets carry the controller's term, boot and target.
  Retired boots, wrong owners, stale switches and reordered old key/button
  events are rejected. Motion ordering does not suppress an unrelated key
  release. Input ownership/focus changes release all remotely held keys and
  buttons. The existing native capture loop remains the source of input.
- Ctrl+Alt+Shift+M is handled inside an active grab as an emergency local
  return. A failed grab returns locally before sending remote input.
- Control, presence and layout packets use XChaCha20-Poly1305, the existing
  pool E2E key and a distinct authenticated domain. Clipboard retains its
  existing encrypted wire format. Node bearer tokens authenticate direct
  connections. The deliberate absence of another user password is preserved.
- Control packets outside a five-second wall-clock tolerance are rejected;
  process-local monotonic time determines presence and lease expiry. Hosts
  must have synchronized clocks. Partitioned peers retain local input; they
  reconcile claims after reconnecting.
- A latest-document layout uses `(revision, author)` ordering and is persisted
  beside `agent.toml` as `agent.topology.json`. Heartbeats gossip positions,
  never clipboard history. Production migration must import saved positions
  and authorize every member before enabling the new mode.
- Leaving the pool keeps configuration and positions, excludes the peer from
  KVM and ignores new shared clipboard contents. Rejoin adopts a local
  clipboard baseline rather than publishing private contents copied while
  absent.

There is no elected permanent coordinator. Relays are ordinary peers. An
explicit A–B–C chain loses A–C communication while B is absent; production
routes must provide alternate paths if that availability is required.

## Clipboard corrections

Forwarding precedes local clipboard application. A separate, coalescing
selection writer prevents delayed X11 verification from blocking control or
relay traffic. Pending writes preserve total clipboard ordering; a local
copy or newer remote copy invalidates an older deferred write. Delayed text
checks are tied to the current GTK offer rather than awaited inline.

Image repair does not overwrite a native text selection. The actual XRDP owner
must be recognized before recovering a stripped offer. A BMP is read before
recovery: a valid new BMP is normalized to PNG, never replaced by cached pixels.
Queued GTK offers and repair callbacks retain selection owner, XFixes epoch and
offer generation guards. A superseded native read is cancelled within its next
40 ms selection check, killing its pending xclip child. XFixes selection
epochs distinguish fresh native images when an application reuses its owner
window and provides no TIMESTAMP target. A failed or superseded image read
does not permanently poison the read cache. The source pointer stays centered
after an edge grab; a warp back to the source edge must not become spurious
remote motion.

## Native qualification

Run on the dedicated Podman test host, inside `poolsync-test`. The three
desktops use private dummy Xorg display 110, HOME, runtime directories,
configuration, keys, selections, receiver logs and a Firefox profile. Existing
agents continue on their own displays and are checked byte-for-byte afterward.
Every candidate has loopback port 1 as an unreachable hub URL.

```sh
python3 deploy/tests/no-hub-desktop-test.py \
  --candidate /path/to/candidate/poolsync-agent \
  --expected-sha256 1ac3a6e1ad0fed7f3d4da4c25520c97cbc57b03b60bbf3bec559928be9e3b1eb \
  --addresses 10.89.2.2 10.89.2.3 10.89.2.5 \
  --hubless --clipboard-rounds 20 --native-browser \
  --output /path/to/private/qualification.json
```

`--kvm-only` runs the native input/recovery subset. It is a diagnostic shortcut,
not a replacement for the full sequence. The GTK receiver samples its actual
paste handler once per second. Firefox uses real desktop Ctrl+V and records
the actual paste event, not a virtual browser clipboard.

The full run passes **33 checks**, including 20 repeated image/text rounds:

| Area | Container evidence |
|---|---|
| Startup | Reverse-order startup, no reachable hub, direct authenticated listeners |
| Three peers | Presence and encrypted clipboard relay across A–B–C in both directions |
| Images and text | Distinct PNG RGBA pixels, image-to-text and text-to-image transitions |
| Browser | Native Firefox pastes two different PNGs, fresh text and another PNG |
| KVM | Source edge crossing, target pointer at `(30,450)`, motion and real GTK key receipt |
| Local recovery | Emergency return and remote modifier release, controller loss and target absence |
| Ownership | B claims control and crosses to A after A restarts |
| Absence | Private content not shared or replayed; office content neither applied nor cached |
| Recovery | Fresh copies after relay restart, absent-node restart and reverse-order agent restart |
| Preservation | Original configuration, absence markers, PIDs and executable fingerprints unchanged |

Observed native paste convergence is **0.623–1.916 seconds** over 59 recorded
operations. It includes receiver sampling and orchestration, not just transport
latency. During that run, a one-time lifetime CPU sample was 2.1–10.0% and RSS
33,300–34,768 KiB; this is not an idle/resource endurance qualification.

## Expanded native campaign

The later candidate SHA-256
`4d8dc5411f750171f6ef444f272c0c1df364bdefa7095afe2137b61b38a57f69`
passes **57 checks** on the original dedicated desktops, with their existing
agents, configurations and participation preserved. Its package version is
still 2.0.3. The subsequently named 2.1.0-dev.1 candidate and visible startup
window require separate qualification; do not identify them by this result.

| Added scenario | Observed result |
|---|---|
| Candidate TCP partition | Held remote Shift released; native source and target keyboards usable; KVM resumes after reconnect |
| Two local displays | Presence advertises both; internal monitor boundary remains local; outer edge crosses to B |
| Unplug during a grab | Remote modifier released and source native input recovered |
| Different resolutions | 1600×900 source enters 1366×768 target at `(30,384)` |
| Concurrent claims | Ten rounds converge on one eligible controller; both native keyboards remain usable |
| Valid new BMP | Two different BMP copies supersede the old PNG, with exact RGBA checks and native Firefox Ctrl+V |
| Delayed image owner | Fresh text converges in 1.834 seconds before the six-second image reply; no stale capture after another seven seconds |

The campaign records 62 native paste operations, **0.991–2.307 seconds** to
convergence including receiver sampling and orchestration. Native GTK KVM key
receipt takes 0.2863 seconds including command launch and orchestration. These
are not isolated wire latency or a full-day resource qualification.

XI2.1 raw keyboard, button and motion source devices distinguish physical
activity from XTEST remote injection. Queues are drained while locally owning
input, preventing old events from reclaiming it after handoff. Device hierarchy
changes refresh the injection-device inventory. Compatibility core events keep
a conservative fallback when XI2 is unavailable; real hardware acceptance
remains required.

Runtime geometry follows the full desktop and clamps coordinates to real
monitors, including holes between staggered outputs. Saved positions remain
unchanged by docking. A changed source desktop drops a stale grab and parks a
clamped edge cursor locally before another edge crossing can strand input.

The session watchdog skips all hub probes and hub/VPN-triggered restarts in
hubless mode. Graphical-session attachment remains active. Agent replacement
no longer kills copying applications' independent xclip workers. CLI departure,
rejoin and local history clearing preserve the absence of a central service.
An isolated command-fixture check verifies four watchdog passes issue neither
hub requests nor healthy-agent restarts, local history clearing makes no hub
request, and a graphical-session mismatch still requests attachment. Run
`python3 deploy/tests/hubless-scripts-test.py` to reproduce it.

## Retained failures

Private raw reports and desktop logs retain failed attempts as well as the
passing campaign. Synthetic fixture contents can be hashed publicly; inherited
environments and credentials remain private.

| Candidate / attempt | Failure and disposition |
|---|---|
| `3c2b08a7…`, 1 | Native text after an image was replaced by delayed PNG repair; corrected text-target protection |
| `70645852…`, 2 | Reused owner without TIMESTAMP hid a fresh image; corrected with XFixes epochs |
| `36640dbc…`, 3 | Clipboard and native Firefox pass; KVM edge landing fails after captured source warp |
| `1ac3a6e1…`, 4 | Fixture searched for the GTK key window before it appeared; added bounded readiness wait |
| `1ac3a6e1…`, 5 | KVM/return pass; fixture keydown argument conflicted with the operation parameter; corrected harness |
| `1ac3a6e1…`, 6 | Native KVM recovery subset passes |
| `1ac3a6e1…`, 7 | Full 33-check campaign passes; original agents preserved |
| `db6bef5a…`, 8 | Second full 33-check campaign passes |
| `4f02a696…`, 9 | Dock/network recovery pass; undock-clamped edge cursor crosses unintentionally before the mixed-resolution assertion |
| `3e27b315…`, 10 | Unplugging during a grab exposes false ownership from queued synthetic core events |
| `3e27b315…`, 11 | Two complete container restart orders and native clipboard/browser pass; local key fixture searches before its GTK window appears |
| `2c3d87af…`, 12 | Unplug recovery passes; injected entry motion races takeover on the resized target |
| `4d8dc541…`, 13 | Full 57-check expanded campaign passes with XTEST raw motion excluded |
| `cdba2f1e…`, 14 | Cold starts, complete container restart orders and expanded scenarios pass. Planned 20-minute soak fails after about 620 seconds: source selection has no owner and receivers retain old text. UI drag also fails because RefCell clones separate motion/release state; shared Rc state fixes that defect. Neither failure is counted as a passing endurance result. |
| `fb472898…`, 15 | Native layout save propagates but reconnect-time presence changes an offline peer's saved KVM flag and the intended tile is missing. Preserve saved permissions while overlaying live geometry. |
| `2be588a1…`, 16 | Expanded native clipboard/KVM scenarios pass; the full campaign cannot find the configuration window after recovery scenarios. |
| `2be588a1…`, 17 | Isolated native layout drag/save, gossip and restart persistence pass. |
| `e8efc920…`, 18 | Full 62-check original-desktop campaign passes, including native layout opening through local IPC and SAVE_TARGETS close/supersession tests. |
| `e8efc920…`, 19 | Two complete cold-start orders pass; optional TIMESTAMP instrumentation blocks on a full-HD native owner. Bound the harness probe. |
| `e8efc920…`, 20 | First distinct full-HD PNG pastes through GTK and native Firefox. Second full-HD PNG fails on all receivers; small-image campaign success does not cover this. Unsupported metadata requests can strand the native owner's INCR transfer. Replaced owner TIMESTAMP probes with XFixes timestamps; new qualification is pending. |
| `e8efc920…`, 21 | Six native layout/permission checks pass. Disabling B persists and prevents its KVM claim while its local keyboard works; re-enabling preserves saved positions. |
| `28eb5f05…`, 22 | Full cold starts, direct alternate paths, two full-HD pastes and expanded KVM checks pass. The sustained paste campaign fails after 102 observed paste operations. The new image owner exists, so this failure cannot be dismissed as the earlier owner-close fixture issue. Resource samples are partial, not a passing 20-minute result. |
| Launch only, 23 | Candidate fingerprint differs before functional qualification begins. Transfer had not completed; the launcher rejects the mismatch. No behavior is qualified by this attempt. |
| `28eb5f05…`, 24 | Large images and a real XFCE screenshot pass. Browser instrumentation later cannot find its window by transient title; capture and retain the native window identifier instead. |
| `28eb5f05…`, 25 | Full 67-check original-desktop campaign passes, including two different full-HD images, a real XFCE capture, native Firefox paste, screen changes, claims and layout editing. Maximum measured convergence is 4.162 seconds including orchestration. No prolonged soak is included. |
| `20b65773…`, 26 | Complete cold starts and nine native checks pass, then the first browser image check fails on all three desktops. Retained live sessions prove an old local GTK selection is promoted with a fresh sequence while a newer incoming offer is being applied. This is a PoolSync race, not a passing run. |
| `c1c38e2f…`, 27 | The 1,203.957-second soak passes 284 image/text rounds, with 636 recorded paste observations including earlier scenarios. A later immediate private-copy departure fails locally on C: the CLI returns before the live absence gate changes, and a delayed release clears another application's selection. The full campaign fails and fresh sessions are retained for diagnosis. |
| `c1c38e2f…`, 28 | Six native layout and permission regression checks pass on neko-desk-a/b/c. Original configurations, participation, running PIDs and binaries are preserved. This is UI/control coverage, not the sustained clipboard campaign. |
| `3253861f…`, 29 | All 41 immediate-departure/private-rejoin checks pass, covering 20 concurrent return rounds and 61 native paste operations. Neither office receiver history contains private contents; original agents/configurations are preserved. This candidate adds live command acknowledgment and safe release, before resource optimization. |
| `90bc1ddc…`, 30 | Complete cold-desktop campaign in progress with the acknowledged departure fixes, XFixes idle polling and launcher-equivalent allocator limits. Planned 20-minute paste soak and interval resource comparison remain pending. |

## Further clipboard and permission corrections

SAVE_TARGETS application handoffs retain owner, copy epoch and GTK offer
generation guards. Closing an application may destroy its owner, but any newer
copy invalidates the handoff. The local poll consumes a handoff before orphan
recovery. Orphan recovery checks the copy epoch associated with remembered
data and declines to restore a previous capture after an uncaptured newer copy.

Unsupported TIMESTAMP probes use the XFixes selection timestamp instead. Some
native owners answer unknown targets with the entire image; cancelling that
unexpected INCR exchange can obstruct subsequent image reads.

Local intent is tracked independently from the last shared clipboard hash.
Receiving a new remote copy can update that shared hash before GTK replaces the
old native owner. An unchanged, already observed native selection must not then
be republished with a fresh clock. The current candidate records both the
native copy epoch and its content hash to reject that stale promotion. Incoming
ordering still protects deferred writes from newer local copies.

Rejoin also excludes the specific selection copied while absent, including a
delayed SAVE_TARGETS handoff after its owner closes. The exclusion is bound to
the X11 copy epoch before participation resumes. A subsequent real copy may
publish identical bytes. This exclusion is being tested against concurrent
office copies; Wayland bridge behavior is not established by the X11 fixtures.

Saved layout permissions now constrain advertised KVM capability, runtime
edges and input leases. Disabling a controller or focus releases held remote
input; enabling a clipboard-only machine in the map cannot override its mode.
Opening a native configuration window uses a user-owned runtime socket to
open the running agent, rather than a disconnected state unable to persist edits.
The local socket is mode 0600 and accepts only its agent's configuration path.

The 62-check campaign on SHA-256
`e8efc920a24b1e040d66c1de5195f887d3cecb1d9f43b2753ec2599d74fff656`
records 65 native paste observations, with a maximum of 4.376 seconds including
sampling and a deliberately delayed source. Its complete restore checks pass.
It does not establish full-HD repeatability, interval resource endurance,
actual XRDP bridge behavior or physical device acceptance.

A separate native-copy baseline with no PoolSync process passes 450 alternating
copies. It does not reproduce the source-owner loss in attempt 14 and therefore
does not establish that the earlier failure was only a harness defect.

Migration rendering is now explicit and checked: it preserves all existing
identity, TLS, encryption and unrelated settings, retains existing LAN/VPN
fallbacks, fills missing peer authorization from the already enrolled identities,
and adds alternate direct routes. Four migration fixture checks and the native
watchdog command fixture pass. Rendered production configurations remain private;
no production installation has occurred in this report.

`deploy/apply-hubless-upgrade.py` checks the live configuration and candidate
fingerprints, linked runtime dependencies and preservation of identity, modes,
permissions and existing routes before installation. It saves the complete
configuration, changed files and active service state in a private archive.
Atomic replacement restores the archive automatically on failure. Three
isolated filesystem integration checks pass: check-only does not modify live
files, explicit rollback restores the originals, and failure after replacing
the executable restores the originals. Service commands are fixture mocks;
actual running executable and graphical checks remain deployment requirements.
Check-only also passed on all five physical targets with the earlier
`28eb5f05…` bundle. Staging files is not deployment and must be repeated with
the final qualified candidate.

The pre-migration dependency audit observes exactly the five intended physical
agents connected to the legacy hub on gbs-p3. Every computer still runs in
legacy mode. Installed watchdogs and local history commands reference the hub;
the replacement scripts remove those dependencies in hubless mode. Asus also
has an old manual `poolsync-test` utility which measures hub APIs; it is not a
hubless acceptance tool. No production hub has been stopped. The Acer account
uses UID 1001; migration discovers each account's actual UID rather than assuming
the UID 1000 used on the other computers.

## Participation acknowledgment and resource follow-up

Attempt 27 proves a 1,203.957-second paste soak with 284 native image/text
rounds, including periodic distinct full-HD images. The complete campaign still
fails its later immediate private departure scenario. Across all scenarios it
records 636 paste observations: median 1.895 seconds, p95 2.933 seconds and
maximum 5.736 seconds, including sampling and orchestration.

The 60-second idle intervals measure 3.71–4.83% of one CPU and 395–465 MiB RSS.
Sampled active peak RSS reaches 499 MiB. These observations are a performance
baseline, not evidence that the desired lightweight daily behavior is complete.

The next candidate makes the user-owned control socket acknowledge departure
only after the running agent changes its participation and privacy gates. An
inactive agent may persist the marker for its next start; a running agent which
cannot acknowledge does not report an applied departure. Polling observes the
latest marker under the same transition lock and cannot rewrite a newer command.
Repeated departure is idempotent. A delayed GTK release clears only an offer
still owned by PoolSync, preserving a copying application's private selection.

The resource experiment keeps fast polling for XFixes selection changes, with
a one-second fallback for owners which change their data silently. Unchanged
selections no longer trigger full conversion at every configured poll interval.
The launcher preserves explicit allocator choices and otherwise limits glibc to
two arenas and a static 128-KiB mmap threshold for large buffers. These are
supported [glibc allocator settings](https://sourceware.org/glibc/manual/latest/html_node/Memory-Allocation-Tunables.html).
Container resource qualification must use `--bounded-allocator` to match these
launch conditions. Improvements remain unmeasured until that campaign finishes.
The current source candidate is SHA-256
`90bc1ddc4cde89fed3208d5c93189fb448270641abb1bc59d8068693085bb921`,
version 2.1.0-dev.3, with **126 Rust tests**, Clippy and release build passing.
Attempt 30 is its full functional/resource qualification; no production
installation has occurred.

## Remaining qualification and deployment gates

The goal remains open. Required work not proved by this campaign includes:

1. Native container layout edits, permissions, gossip and persistence pass on
   their recorded candidates. Repeat relevant regression checks on the final
   candidate before promotion.
2. Cold starts in different orders, alternate direct paths, suspended peers,
   TCP loss and simultaneous claims pass before later clipboard failures.
   They do not turn those failed complete campaigns into passing campaigns.
3. A completed sustained native-copy campaign and interval resource measurements.
   Native BMP, SAVE_TARGETS and superseded image-read races pass on the recorded
   candidates; the current native-intent race fix and actual XRDP bridge recovery
   still require qualification.
4. Physical keyboard/mouse takeover, docks and real screen changes on Asus and
   Acer. Synthetic X11 events cannot establish physical-device acceptance.
5. Backed-up staged production migration, running executable/version checks
   and hub-independent native clipboard proof on all five computers.
6. Dependency audit and reversible retirement of the old hub after validation.

No GitHub Actions, CI/CD or Vercel workflow is enabled by this work.
