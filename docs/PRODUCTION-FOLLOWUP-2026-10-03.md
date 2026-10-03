# Production follow-up: native paste and controller recovery

## Scope and acceptance

This follow-up exercises the existing production artifact; it does not build or
install a replacement. All five agents execute `2.1.0-dev.3`, SHA-256
`6e9b49804eb4b48f7f1a31698bedc9d503751573cd9aef6e74148be9743208cd`.
Asus and Acer remain full-mode nodes; gbs-p2, gbs-p3 and zaza-desktop remain
clipboard-only. The old PoolSync hub is inactive, disabled and has no listener
on port 9470. The existing VPN remains in use.

**Production acceptance remains incomplete.** Automated native applications are
useful production evidence, but do not prove physical edge crossing, physical
input takeover, monitor-cable changes, an end-to-end RDP image path or full-day
stability. The previously reported physical edge failures remain open.

The bounded evidence, including failed attempts, is retained in
[production-followup-20261003.json](qualification/production-followup-20261003.json).
The earlier [deployment report](HUBLESS-WINDOW-DEPLOYMENT-2026-10-03.md) records
DEV qualification and the original fleet rollout separately.

## Native clipboard campaigns

The repository's `deploy/tests/physical-fleet-clipboard-test.py` runs isolated
GTK copy/receiver applications and private Chrome paste receivers in the actual
agents' graphical sessions. It validates fresh image/text hashes after real
native paste requests, rotates copy sources and cleans up its test workers.
Input is synthetic native GTK/XTest input, not a physical keyboard or a
production Flameshot capture.

| Campaign | Conditions | Result |
| --- | --- | --- |
| Initial five-node attempt | Existing FreeRDP clients active; local RDP clipboard pause detected on Asus/Acer | First image does not converge to the two local full-mode desktops; failed evidence retained |
| Reopened-RDP five-node attempt | User-reopened native RDP clients; original pause policy retained | First image again fails to converge to Asus/Acer; agent/configuration preservation passes |
| Three clipboard-only nodes | Existing RDP sessions retained | Nine alternating rounds; GTK and Chrome paste checks pass; 36 convergence samples |
| Five-node direct PoolSync cohort | Native FreeRDP clients temporarily stopped; server graphical sessions retained | Ten alternating rounds; GTK and Chrome paste checks pass on all five nodes; 40 convergence samples |

The three-node convergence median is 2.8835 seconds, p95 4.293 seconds and
maximum 4.483 seconds. For the five-node direct cohort, the corresponding values
are 4.912, 7.388 and 8.413 seconds. The p95 uses the sorted sample at index
`floor(0.95 * sample_count)`, capped at the final index. These measurements include
fixture orchestration and receiver sampling; they are not pure transport latency.
No stale image is observed in the successful alternating sequences. This finite
campaign does not establish that stale captures are impossible in daily use.

The failure reproduces after both native clients are reopened. RDP clipboard pause provides a relevant explanation for the initial test's
conditions, but that failure does not independently establish that every RDP
clipboard route works. A direct cohort pass must not be presented as an
end-to-end FreeRDP image acceptance result.

## RDP maintenance and restoration

The direct-cohort test temporarily closes only the two native FreeRDP clients.
Their remote graphical sessions are retained. Original client arguments and
session environments are kept in root-only private files, outside Git; passwords,
clipboard contents and complete process arguments are excluded from the report.

Automatic reopening succeeds on Asus. Acer's original client uses `/from-stdin`
and obtains its password through a native Zenity prompt each time it starts.
The maintenance procedure did not preflight this authentication dependency, so
its automatic restoration fails. This is a test-maintenance failure, separate
from the successful clipboard checks. The user reopens the RDP clients; the
agent's additional password prompt is cancelled to avoid a duplicate connection.
Independent inspection then finds one visible native client on each full-mode
computer. Original RDP clipboard pause policy is again detected on both nodes.

Future isolated production tests must preflight reopening credentials and active
keyboard grabs before changing any client window. Prefer the dedicated DEV lab;
do not claim an unattended restoration path for a client needing human input.

## Automated controller checks

The initial synthetic emergency-claim attempt times out. Centering alone does
not consistently resolve it. A separate diagnostic observes all five peers
agreeing on one Asus-owned lease, but one success is not accepted as stability.

With native RDP windows temporarily minimized around each local shortcut, the
complete controller campaign passes six checks:

1. Asus local claim converges across all five peers.
2. Acer local claim converges across all five peers.
3. Asus can claim again.
4. Simultaneous claims agree on one owner, focus and term.
5. The current controller can temporarily leave the pool; its shared lease is
   relinquished and the XTEST keyboard/pointer have no held input on either
   full-mode node.
6. The controller returns with its membership and configuration preserved.

The windows are restored after the synthetic shortcut. The observation is
consistent with active RDP keyboard grabs intercepting synthetic shortcuts; it
is not proof that physical emergency recovery works while RDP owns the keyboard.
The test finishes by restoring an Acer-owned local claim. No agent binary,
configuration, saved screen layout or participation state is changed.

## Final preservation and resource interval

Independent final inspection compares all five agents against the baseline:
PIDs, installed/running hashes, configuration hashes, saved-layout hashes and
participation markers match. Graphical access is available on every node; all
five peers are active. Neither full-mode node has a held XTEST key/button. RDP
clients are visible again. No new runtime deployment is performed.

Thirteen samples over approximately 60 seconds show stable PIDs and five active
peers throughout:

| Node | Agent CPU, percent of one core | RSS start/end, KiB |
| --- | --- | --- |
| Asus | 3.134 | 52,844 / 52,844 |
| Acer | 3.967 | 61,152 / 61,152 |
| gbs-p2 | 1.350 | 118,276 / 118,404 |
| gbs-p3 | 0.926 | 27,188 / 27,188 |
| zaza-desktop | 0.450 | 65,932 / 65,932 |

This is an agent-process interval, not total-machine power or battery usage.
The higher absolute RSS on some clipboard-only nodes is recorded rather than
hidden; the interval alone cannot diagnose its lifetime growth or prove a full
day of stability.

## Remaining proof

- Physical Asus/right-edge to Acer and Acer/left-edge to Asus, including the
  previously reported blocked edge and looping return.
- Takeover from the actual keyboard/mouse on either full-mode node and emergency
  recovery when a full-screen RDP client owns the keyboard.
- Actual extra-monitor connection/removal and heterogeneous physical layouts.
- Fresh production Flameshot captures and end-to-end image paste through the
  currently used RDP routes, with the existing pause policy retained.
- Full-day resource and clipboard stability; production disruption scenarios
  remain distinct from the already qualified isolated-lab fault injections.

These requirements remain unverified. The goal must not be marked complete on
the strength of the automated native clipboard and controller checks alone.
