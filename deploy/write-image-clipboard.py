#!/usr/bin/env python3
"""Écrit une image PNG/JPEG dans le presse-papiers via GTK (apps GTK / XFCE)."""
from __future__ import annotations

import sys
import time

try:
    import gi

    gi.require_version("Gdk", "3.0")
    gi.require_version("GdkPixbuf", "2.0")
    gi.require_version("Gtk", "3.0")
    from gi.repository import Gdk, GdkPixbuf, Gtk
except ImportError:
    sys.exit(2)


def main() -> int:
    raw = sys.stdin.buffer.read()
    if not raw:
        return 1

    Gtk.init([])
    loader = GdkPixbuf.PixbufLoader.new()
    loader.write(raw)
    loader.close()
    pixbuf = loader.get_pixbuf()
    if pixbuf is None:
        return 1

    clipboard = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
    clipboard.set_image(pixbuf)
    clipboard.store()
    deadline = time.time() + 0.4
    while time.time() < deadline:
        while Gtk.events_pending():
            Gtk.main_iteration_do(False)
        time.sleep(0.02)

    return 0


if __name__ == "__main__":
    sys.exit(main())
