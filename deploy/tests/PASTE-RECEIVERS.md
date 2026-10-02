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

## Regression sequence

1. Copy an image, paste it, and compare the pixel hash with the source.
2. Copy a distinct second image, paste immediately and after a delay, and check
   that the second image remains current.
3. Copy ordinary text and verify that an older image is not restored.
4. Copy another image and check the image-after-text transition.
5. In an isolated agent regression, replace PNG with an empty BMP-only selection
   to reproduce a degraded RDP callback, then verify recovery of the latest PNG.

Keep production credentials and hub endpoints out of these container tests.
