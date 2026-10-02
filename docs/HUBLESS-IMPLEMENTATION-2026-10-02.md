# Hubless peer control: development qualification

## Status and boundaries

The development agent can run presence, layout, input ownership, KVM and
clipboard over authenticated direct peer links. `hubless = true` prevents
starting the legacy hub session or its KVM loop. An absent hub is not a fallback
condition; it is the qualified startup condition. No production machine has
been migrated by the qualification described here. This is not a release or
physical daily-use acceptance report.

The full native qualification candidate is SHA-256
`1ac3a6e1ad0fed7f3d4da4c25520c97cbc57b03b60bbf3bec559928be9e3b1eb`.
Its package version remains 2.0.3; that version alone cannot identify the
development build. Later source changes must receive their own qualification.

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

Image repair does not overwrite a native text selection. XFixes selection
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

## Remaining qualification and deployment gates

The goal remains open. Required work not proved by this campaign includes:

1. Multi-monitor outer edges, different resolutions, layouts and hotplug,
   including topology edits and persistence without the hub.
2. Real network loss, alternate direct paths, simultaneous runtime claims,
   suspended peers and complete container cold boots in different orders.
3. Longer repeatability, bounded latency/resource measurements and delayed
   write races; native BMP and actual XRDP recovery require specific coverage.
4. Physical keyboard/mouse takeover, docks and real screen changes on Asus and
   Acer. Synthetic X11 events cannot establish physical-device acceptance.
5. Backed-up staged production migration, running executable/version checks
   and hub-independent native clipboard proof on all five computers.
6. Dependency audit and reversible retirement of the old hub after validation.

No GitHub Actions, CI/CD or Vercel workflow is enabled by this work.
