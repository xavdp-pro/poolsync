# Clipboard paste receivers

Use these receivers in the dedicated desktop test containers. They display what
an application actually receives, including image dimensions and an RGBA pixel
SHA-256, without uploading clipboard contents.

## Native GTK receiver

Run as the logged-in desktop user in that user's X11 session:

```sh
python3 clipboard-receiver.py
```

Click **Paste clipboard** or press **Ctrl+V**. For bounded automated clipboard
checks, `--watch` invokes the same GTK image/text receive handler once per second:

```sh
python3 clipboard-receiver.py --watch --quit-after 60 --log /tmp/paste-results.json
```

The log contains hashes, dimensions and text lengths, not clipboard payloads.
Watching is a test mode; the default manual receiver has no polling timer.
Python GObject introspection and GTK 3 are required.

## Browser receiver

Serve the directory on loopback and open the page in the desktop browser:

```sh
python3 -m http.server 19480 --bind 127.0.0.1
```

Open `http://127.0.0.1:19480/clipboard-paste.html`, focus the paste area and press
**Ctrl+V**. **Read current clipboard** tests the browser clipboard API separately.
The page displays the image, received format, dimensions, file hash and pixel
hash. Files and text remain in the page; there is no upload endpoint.

A browser automation virtual clipboard is a separate path. Passing a virtual
paste test does not prove the desktop X11 clipboard or ChatGPT paste works.

For the isolated automated Firefox or Chromium fixture, `browser-paste-server.py` serves
the same page on loopback port 19580. Its lab-only hook records the result of
the actual **Ctrl+V paste event**. It posts hashes, dimensions and text lengths
to its own loopback server, never image or text payloads. The manual HTML page
keeps its original behavior and has no upload endpoint. Each browser uses a
separate profile, HOME and X11 display; neither the in-app browser's virtual
clipboard nor `navigator.clipboard.write` is used to fabricate delivery.
The lab hook refocuses the editable paste target when its native browser window
regains focus. It separately records focus, native paste-shortcut arrival and
paste dispatch as bounded metadata, without logging key contents. This lets a
missing native paste be distinguished from a shortcut sent to an unfocused
browser widget; image/text delivery still requires the real paste handler.

The no-hub desktop harness selects the implementation with
`--native-browser-engine firefox` (default) or `chromium`. Use
`--mixed-browser-pastes 12` with a mixed workload to repeat native Ctrl+V after
changing windows. Each paste must produce a new matching record; generated
remote input continues while browser retrieval is awaited. The selected engine
is recorded explicitly. The disposable Chromium process uses `--no-sandbox`
inside its isolated test container only; this does not change a physical
browser or machine-wide setting.

For a failing Firefox retrieval, the optional `--browser-x11-trace` proxy and
`--browser-syscall-trace` main-process trace are diagnostic modes. They are
mutually exclusive and can change scheduling; a passing instrumented run does
not qualify the normal browser. Syscall string output is disabled. Private
WidgetClipboard metadata distinguishes timeout from decoder failures.

`clipboard-selection-observer.py` provides a bounded, non-proxy XRecord trace
on display `:110` only. It requires Python Xlib and an existing isolated test
root supplied as `--root /tmp/poolsync-no-hub-IDENTIFIER`. It records selection
requests, property identifiers and notification times, excluding property
contents and keyboard/mouse events. `--seconds` defaults to 180 and is capped
at 1,200; the output is private and capped at 50,000 records. Original desktop
clients and display configuration are not modified.

`native-browser-control-test.py` provides a separate GTK-to-browser comparison
without starting or contacting a PoolSync candidate. Run it on the disposable
Podman host with `--engine firefox` or `chromium` and `--output PRIVATE_JSON`.
It reuses the native helpers on private display `:112`, a separate HOME/DBus
session and loopback port 19581. Three rounds of twelve image pastes with window
changes and intervening fresh text are the defaults. Existing agent PID,
executable, configuration and absence marker must remain unchanged. The report
separates native retrieval failures from fixture/startup failures and records
cleanup, including workers started before an assertion fails. This comparison
cannot replace mesh or physical acceptance.

## Regression sequence

1. Copy an image, paste it, and compare the pixel hash with the source.
2. Copy a distinct second image, paste immediately and after a delay, and check
   that the second image remains current.
3. Copy ordinary text and verify that an older image is not restored.
4. Copy another image and check the image-after-text transition.
5. In an isolated agent regression, replace PNG with an empty BMP-only selection
   to reproduce a degraded RDP callback, then verify recovery of the latest PNG.

Keep production credentials and hub endpoints out of these container tests.

For `--target-restart`, every disposable desktop also needs `xinput` and `xprop`
to query held XTEST buttons and the native recovery marker. The harness checks
these executables before changing any test session. Fresh cold-desktop images
must include them as well; their presence in the original Neko desktops does
not cover a separately cloned image. `--fresh-desktops --reboot-desktops` is
limited to disposable desktops with no original agent, session or shared mounts.

## Paused cohort upgrade regression

Use `--rolling-upgrade-from OLD_BINARY --expected-starting-sha256 OLD_HASH`
with the three-peer hubless harness. Every private peer starts on that exact
binary, leaves before any replacement, keeps native local input and restarts
sequentially on `--candidate`. The test preserves each configuration and absence
marker, resumes only after all executing hashes match, checks that a private
maintenance copy is not replayed and then runs the selected normal scenarios.
This exercises protocol/session behavior in isolated containers; physical
systemd installation and hardware acceptance remain separate.
