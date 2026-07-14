#!/usr/bin/env python3
"""Écrit image/png dans le presse-papiers via GTK (comme xfce4-screenshooter)."""
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
    png = sys.stdin.buffer.read()
    if not png:
        return 1

    Gtk.init([])
    loader = GdkPixbuf.PixbufLoader.new_with_type("png")
    loader.write(png)
    loader.close()
    pixbuf = loader.get_pixbuf()
    if pixbuf is None:
        return 1

    clipboard = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
    clipboard.set_image(pixbuf)
    clipboard.store()

    time.sleep(0.2)
    while Gtk.events_pending():
        Gtk.main_iteration_do(False)
    return 0


if __name__ == "__main__":
    sys.exit(main())
