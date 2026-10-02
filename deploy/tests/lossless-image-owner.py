#!/usr/bin/env python3
"""Offer PNG and JPEG while native PNG or TARGETS conversion is temporarily slow.

Replies are scheduled independently so one delayed request cannot freeze other
paste clients. Expired requestor windows are harmless after bounded read cancellation.
This fixture owns only the explicitly selected isolated X11 display.
"""
import argparse
import ctypes as c
from pathlib import Path
import runpy
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--image', type=Path, required=True)
    parser.add_argument('--jpeg', type=Path, required=True)
    parser.add_argument('--metadata-delay', type=float, default=0)
    parser.add_argument('--png-delay', type=float, default=0.8)
    parser.add_argument('--refuse-png-count', type=int, default=0)
    parser.add_argument('--log', type=Path, required=True)
    args = parser.parse_args()
    structures = runpy.run_path(str(Path(__file__).with_name('slow-image-owner.py')))
    Event, Selection = structures['Event'], structures['Selection']
    x = c.CDLL('libX11.so.6')
    signatures = {
        'XOpenDisplay': ([c.c_char_p], c.c_void_p),
        'XDefaultRootWindow': ([c.c_void_p], c.c_ulong),
        'XCreateSimpleWindow': ([c.c_void_p, c.c_ulong, c.c_int, c.c_int, c.c_uint,
                                c.c_uint, c.c_uint, c.c_ulong, c.c_ulong], c.c_ulong),
        'XInternAtom': ([c.c_void_p, c.c_char_p, c.c_int], c.c_ulong),
        'XSetSelectionOwner': ([c.c_void_p, c.c_ulong, c.c_ulong, c.c_ulong], c.c_int),
        'XGetSelectionOwner': ([c.c_void_p, c.c_ulong], c.c_ulong),
        'XChangeProperty': ([c.c_void_p, c.c_ulong, c.c_ulong, c.c_ulong, c.c_int,
                             c.c_int, c.c_void_p, c.c_int], c.c_int),
        'XSendEvent': ([c.c_void_p, c.c_ulong, c.c_int, c.c_long, c.POINTER(Event)], c.c_int),
        'XNextEvent': ([c.c_void_p, c.POINTER(Event)], c.c_int),
        'XPending': ([c.c_void_p], c.c_int),
        'XFlush': ([c.c_void_p], c.c_int),
        'XCloseDisplay': ([c.c_void_p], c.c_int),
    }
    for name, (arguments, result) in signatures.items():
        getattr(x, name).argtypes, getattr(x, name).restype = arguments, result
    handler_type = c.CFUNCTYPE(c.c_int, c.c_void_p, c.c_void_p)
    handler = handler_type(lambda display, error: 0)
    x.XSetErrorHandler.argtypes, x.XSetErrorHandler.restype = [handler_type], c.c_void_p
    x.XSetErrorHandler(handler)
    display = x.XOpenDisplay(None)
    assert display, 'isolated X11 display unavailable'
    window = x.XCreateSimpleWindow(display, x.XDefaultRootWindow(display), 0, 0, 1, 1, 0, 0, 0)
    atom = lambda name: x.XInternAtom(display, name.encode(), 0)
    clipboard, targets, png, jpeg, atoms = map(atom, ('CLIPBOARD', 'TARGETS', 'image/png', 'image/jpeg', 'ATOM'))
    image, compressed = args.image.read_bytes(), args.jpeg.read_bytes()
    pending = []
    refused = 0
    x.XSetSelectionOwner(display, clipboard, window, 0)
    x.XFlush(display)
    try:
        with args.log.open('a', buffering=1) as log:
            while x.XGetSelectionOwner(display, clipboard) == window:
                while x.XPending(display):
                    event = Event()
                    x.XNextEvent(display, c.byref(event))
                    if event.type == 29:
                        return
                    if event.type != 30:
                        continue
                    request = structures['Request'].from_buffer_copy(event.request)
                    delay = 0
                    payload = None
                    if request.target == targets:
                        delay, payload = args.metadata_delay, (c.c_ulong * 3)(targets, png, jpeg)
                    elif request.target == png:
                        if refused < args.refuse_png_count:
                            refused += 1
                            log.write('PNG refused temporarily\n')
                        else:
                            delay, payload = args.png_delay, c.create_string_buffer(image)
                            log.write('PNG requested\n')
                    elif request.target == jpeg:
                        payload = c.create_string_buffer(compressed)
                        log.write('JPEG requested\n')
                    pending.append((time.monotonic() + delay, request, payload))
                ready = [item for item in pending if item[0] <= time.monotonic()]
                pending = [item for item in pending if item[0] > time.monotonic()]
                for _, request, payload in ready:
                    prop = request.property or request.target
                    if payload is not None:
                        metadata = request.target == targets
                        x.XChangeProperty(display, request.requestor, prop, atoms if metadata else request.target,
                                          32 if metadata else 8, 0, c.cast(payload, c.c_void_p),
                                          3 if metadata else len(image) if request.target == png else len(compressed))
                    reply = Event()
                    reply.selection = Selection(31, 0, 1, display, request.requestor,
                                                request.selection, request.target, prop if payload is not None else 0,
                                                request.time)
                    x.XSendEvent(display, request.requestor, 0, 0, c.byref(reply))
                x.XFlush(display)
                time.sleep(.005)
    finally:
        x.XCloseDisplay(display)


if __name__ == '__main__':
    main()
