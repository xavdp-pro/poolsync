#!/usr/bin/env python3
"""Receive native GTK image/text pastes without changing the clipboard."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import time

import gi

gi.require_version("Gtk", "3.0")
from gi.repository import Gdk, GLib, Gtk


class Receiver:
    def __init__(self, log_path):
        self.log_path = log_path
        self.records = []
        self.last_key = None
        self.pending = False
        self.request_id = 0
        self.clipboard = Gtk.Clipboard.get(Gdk.SELECTION_CLIPBOARD)
        self.window = Gtk.Window(title="PoolSync image paste receiver")
        self.window.set_default_size(650, 540)
        self.window.connect("destroy", Gtk.main_quit)
        self.window.connect("key-press-event", self.on_key)
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        box.set_border_width(18)
        self.window.add(box)
        box.pack_start(Gtk.Label(label="Real desktop clipboard · image and text paste"), False, False, 0)
        button = Gtk.Button(label="Paste clipboard (Ctrl+V)")
        button.connect("clicked", self.paste)
        box.pack_start(button, False, False, 0)
        self.status = Gtk.Label(label="Ready")
        self.status.set_line_wrap(True)
        box.pack_start(self.status, False, False, 0)
        self.image = Gtk.Image()
        box.pack_start(self.image, True, True, 0)
        self.metrics = Gtk.Label(label="Images: 0 · Texts: 0")
        box.pack_start(self.metrics, False, False, 0)
        self.details = Gtk.Label(label="No paste received")
        self.details.set_selectable(True)
        self.details.set_line_wrap(True)
        box.pack_start(self.details, False, False, 0)
        self.window.show_all()

    def record(self, entry):
        key = (entry["kind"], entry["sha256"])
        if key == self.last_key:
            return
        self.last_key = key
        self.records.append(entry)
        self.records = self.records[-200:]
        images = sum(item["kind"] == "image" for item in self.records)
        texts = sum(item["kind"] == "text" for item in self.records)
        self.metrics.set_text(f"Images: {images} · Texts: {texts}")
        print(json.dumps(entry), flush=True)
        if self.log_path:
            self.log_path.write_text(json.dumps(self.records, indent=2) + "\n")
            os.chmod(self.log_path, 0o600)

    def receive_text(self, _clipboard, text, request_id):
        if request_id != self.request_id:
            return
        self.pending = False
        if not text:
            self.status.set_text("No usable image or text on the clipboard")
            return
        digest = hashlib.sha256(text.encode()).hexdigest()
        self.record({"kind": "text", "sha256": digest, "characters": len(text), "at": round(time.time(), 3)})
        self.image.clear()
        self.status.set_text("Text pasted successfully")
        self.details.set_text(f"Text: {len(text)} characters\nSHA-256: {digest}")

    def receive_image(self, _clipboard, pixbuf, request_id):
        if request_id != self.request_id:
            return
        if pixbuf is None:
            self.clipboard.request_text(self.receive_text, request_id)
            return
        self.pending = False
        rgba = pixbuf if pixbuf.get_has_alpha() else pixbuf.add_alpha(False, 0, 0, 0)
        width, height = rgba.get_width(), rgba.get_height()
        stride, raw = rgba.get_rowstride(), rgba.get_pixels()
        pixels = b"".join(raw[y * stride : y * stride + width * 4] for y in range(height))
        digest = hashlib.sha256(pixels).hexdigest()
        self.record({"kind": "image", "sha256": digest, "width": width, "height": height, "at": round(time.time(), 3)})
        scale = min(1, 580 / width, 330 / height)
        preview = pixbuf.scale_simple(max(1, round(width * scale)), max(1, round(height * scale)), 2)
        self.image.set_from_pixbuf(preview)
        self.status.set_text("Image pasted successfully")
        self.details.set_text(f"{width} × {height}\nRGBA pixel SHA-256: {digest}")

    def timeout(self, request_id):
        if request_id == self.request_id and self.pending:
            self.request_id += 1
            self.pending = False
            self.status.set_text("Clipboard owner did not respond; retry the paste")
        return False

    def paste(self, *_args):
        if not self.pending:
            self.pending = True
            self.request_id += 1
            request_id = self.request_id
            self.clipboard.request_image(self.receive_image, request_id)
            GLib.timeout_add_seconds(3, self.timeout, request_id)
        return True

    def on_key(self, _window, event):
        if event.state & Gdk.ModifierType.CONTROL_MASK and event.keyval in (Gdk.KEY_v, Gdk.KEY_V):
            self.paste()
            return True
        return False


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--watch", action="store_true", help="Exercise the paste handler once per second")
    parser.add_argument("--log", type=Path, help="Write metadata-only receive results")
    parser.add_argument("--quit-after", type=int, default=0, help="Close after this many seconds; 0 keeps the window open")
    args = parser.parse_args()
    Gtk.init([])
    receiver = Receiver(args.log)
    if args.watch:
        GLib.timeout_add(1000, receiver.paste)
    if args.quit_after:
        GLib.timeout_add_seconds(args.quit_after, lambda: (Gtk.main_quit(), False)[1])
    Gtk.main()


if __name__ == "__main__":
    main()
