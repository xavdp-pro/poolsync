# Atomic native keyboard and pointer capture: DEV candidate

## Reproduced failure and change

A foreign X11 client can own the keyboard while leaving the pointer available.
The existing capture routine accepts that condition as a mouse-only takeover.
This may send pointer input to a PoolSync peer while keyboard input stays with
another application, including a native RDP client. Read-only production process
inspection finds no explicit grab-disabling flags on either full-mode RDP client;
this does not prove the cause of the reported physical edge failure.

The isolated native fixture reproduces the partial-capture failure on the old
routine. The revised routine requires both keyboard and pointer capture before
reporting success. It releases its pointer before retrying a busy keyboard,
bounds retries by the existing one-second timeout, and uses the existing failure
cleanup/local-return path when complete capture is unavailable. It never steals
or clears the other client's grab.

[Before/after native proof](qualification/grab-contention-dev5-20261003.json)
records the old failure and all four corrected checks: refuse partial takeover,
release the pointer, preserve the foreign keyboard owner, and recover complete
capture after that owner releases it. This is a synthetic Xvfb display, not a
physical keyboard, RDP focus or screen-edge acceptance test.

## Candidate status

Version `2.1.0-dev.5`, executable SHA-256:
`6e29bd70d965b76d7dbfcab668ed31a84a29656958f14089fa9ffd930722969d`.
It includes the [image-target fix](RDP-IMAGE-TARGETS-2026-10-03.md).
All 147 release workspace tests, formatting, release all-targets Clippy with
warnings denied and the release build pass in the isolated builder directory.
The complete three-desktop encrypted mesh campaign is started separately for
this exact executable. Its results must be inspected before any promotion;
the 116 successful dev.4 checks do not qualify the changed dev.5 runtime.

Production remains on dev.3. The earlier staged dev.4 binary and preflight are
historical evidence, not authorization to skip dev.5 qualification or stage an
unverified binary. No RDP flags, installed agents, keys, layout or configuration
are changed by this work. The human's native RDP clipboard policy decision,
physical edge crossings, emergency local return, monitor cable changes and
full-day stability remain open.

## Native fixture

[Grab contention fixture](../poolsync-agent/examples/grab-contention-qualification.rs)
includes the real capture module. Build it with:

```sh
cargo build --locked --release -p poolsync-agent --example grab-contention-qualification
```

Run only in a disposable container with a newly started private Xvfb display
`:196`; the fixture requires both the container marker and explicit matching
display acknowledgement. No production X socket, device or home may be mounted.

```sh
DISPLAY=:196 POOLSYNC_ISOLATED_GRAB_DISPLAY=:196 \
  ./grab-contention-qualification
```

It emits bounded JSON checks and a nonzero exit code on failure. Stop the private
X server and preserve both failing and successful reports after each comparison.
