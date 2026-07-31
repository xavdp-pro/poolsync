#!/usr/bin/env python3
"""Lit une image du presse-papiers GTK/X11 (ex. xfce4-screenshooter) vers stdout (PNG)."""
from __future__ import annotations

import subprocess
import sys

try:
    import gi

    gi.require_version("Gdk", "3.0")
    gi.require_version("GdkPixbuf", "2.0")
    gi.require_version("Gtk", "3.0")
    from gi.repository import Gdk, Gtk
except ImportError:
    sys.exit(2)


def x11_has_image_target() -> bool:
    try:
        proc = subprocess.run(
            ["xclip", "-selection", "clipboard", "-t", "TARGETS", "-o"],
            capture_output=True,
            text=True,
            timeout=1,
        )
    except (OSError, subprocess.TimeoutExpired):
        return True
    if proc.returncode != 0:
        return True
    return any(line.strip().startswith("image/") for line in proc.stdout.splitlines())


def main() -> int:
    if not x11_has_image_target():
        return 1

    Gtk.init([])
    clipboard = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
    if not clipboard.wait_is_image_available():
        return 1

    pixbuf = clipboard.wait_for_image()
    if pixbuf is None:
        return 1

    success, data = pixbuf.save_to_bufferv("png", [], [])
    if not success or not data:
        return 1
    sys.stdout.buffer.write(data)
    return 0


if __name__ == "__main__":
    sys.exit(main())
