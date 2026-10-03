# PoolSync independent Cursor counter-review

## Scope and evidence

Cursor Agent reviewed branch `codex/daily-use-robustness` at
`ac14c36bb656064526b466cf6c60194224574c3e`, in read-only Ask mode. It used
44 file-read calls, 27 searches and four glob calls, with 40 distinct explicitly
read paths. This was an actual repository inspection, not a prompt-only opinion.
Its attempted shell call was unavailable in Ask mode; Cursor did not execute
binaries, tests, SSH, TLS handshakes or physical input checks.

The scope covered README, pitch, scenarios, installation runbook and SVGs;
Rust peer/control/KVM/clipboard/session sources; security/migration/hotfix
helpers; the test harness and selected qualification metadata. A second,
independent local source check verified the concrete findings before changes.
Raw CLI transcripts remain private and are not repository artifacts.

## Core design assessment

The reviewer found the decentralization and physical-node leadership account
consistent with the code: no permanent PoolSync coordinator, authorized
participating full-mode controllers, deterministic claims, three-second leases,
four-second presence expiry and separation of input ownership from destination
focus. LAN operation without VPN, optional cross-site VPN routes, FreeRDP pause,
Remmina detection limits and temporary departure/private rejoin were also
consistent. This validates the explanation against source; it does not establish
real desk acceptance or all-day runtime stability.

## Findings and dispositions

Severity labels below reproduce the review's prioritization; confirmed source
or documentation findings are distinguished from runtime hypotheses.

| Priority | Finding | Independent check and disposition |
|----------|---------|-----------------------------------|
| P1 | Existing-pool enrollment lacked a concrete single-node certificate issuance procedure. | Confirmed documentation gap. Runbook now gives private existing-CA signing commands, a new token and a reciprocal membership/layout/maintenance checklist. Isolated issuance and CA preservation were executed successfully. Configuration rollout remains an explicitly reviewed procedure, not an automatic enrollment tool. |
| P1 | Docs described primary-only KVM geometry although hubless code uses full desktop bounds. | Confirmed in `kvm.rs::pool_bounds` and `KvmDesktopInfo::desktop_bounds`. README/scenarios now describe virtual desktop geometry, origin and primary fallback. No runtime change or physical acceptance claim. |
| P2 | Copying `tls/ca.crt` does not install native-TLS outbound trust. | Confirmed in `peer_mesh.rs` and crate TLS features. Runbook now states this and gives guarded Debian trust-store commands. No system trust store was modified during review. |
| P2 | Cohort quiescence requires `xinput` on full-mode hosts after the pool is made absent. | Confirmed in `apply-agent-cohort-hotfix.py`. Runbook now requires a separate pre-maintenance executable/XTEST query check. Coordinator dry run still does not implement this dependency preflight; that tooling improvement remains open. |
| P2 | First migration does not replace required session-selection/start helpers. | Confirmed `FILES` mapping omits them while the launcher needs the picker. Runbook now requires helper version/presence checks and a separately backed-up replacement when needed. Expanding the installer's tracked file set remains open. |
| P2 | Empty-layout initialization seeds neighbor layout KVM flags as true. | Confirmed initialization behavior. Actual control still checks peer presence/mode/participation. Runbook explicitly requires a reviewed nonempty initial topology. Changing automatic seeding remains open; this finding alone does not prove clipboard-only input injection. |
| P3 | Cohort hotfix targets a particular fleet. | Confirmed and already documented. Generic inventory-driven upgrade support remains open; do not present the script as universal enrollment. |
| P3 | Migration backfills transport peers with rightward directions. | Confirmed; runbook now calls these placeholders and identifies the saved topology as authoritative for hubless adjacency. |
| Hypothesis | ED25519 compatibility on older native-TLS/OpenSSL stacks. | Not confirmed as a runtime defect. Certificate issuance/verification passed in the isolated local fixture, but target-stack native TLS handshake qualification remains required. |

Additional independent checks found two issues not asserted by Cursor:

- Bootstrap/new-pool shell guards did not guarantee abort on failure. In a
  temporary reproducer, `test ! -e` failed but the next command ran and the shell
  returned success. Relevant runbook blocks now run in fail-fast subshells.
  The actual revised bootstrap refuses an existing fixture config before any
  binary installation; the revised signing block refuses an existing output
  directory without changing its files.
- Received layouts currently have a 64-entry bound. Scenarios now distinguish
  this implementation bound from evidence of a qualified large deployment.

## Verification performed after corrections

The local reviewer executed the runbook's single-node signing example using a
fresh disposable ED25519 CA and fictitious node/SANs. Certificate verification
passed, existing CA key/certificate hashes stayed identical, no CA serial file
was created, and private token/key permissions were checked. A repeated issuance
attempt was refused without overwriting the first result. The actual bootstrap
block was tested against an existing temporary configuration and stopped before
installation. Fixtures were removed afterward; no real identities were accessed.

Boolean proof metadata is in
[the isolated documentation check](qualification/cursor-runbook-check-20261003.json).
Markdown links, shell-block syntax, TOML/JSON examples and documented script
options were checked independently. Existing SVGs are unchanged by this review.
No Rust runtime, deployment helpers, live node configuration, VPN, system trust
or running agent was changed. No CI/CD, GitHub Actions or Vercel was enabled.

## Remaining boundaries

Read the [current deployment report](HUBLESS-WINDOW-DEPLOYMENT-2026-10-03.md) for
executing artifacts and physical gates. The reported full-node edge blockage and
return loop, physical takeover/crossing, local emergency recovery and real
monitor-cable changes remain unaccepted. Cursor inspection and these isolated
command checks cannot close those gates. Application/tooling findings above are
recorded for separately qualified fixes; this documentation correction is not a
new production runtime release.
