#!/usr/bin/env python3
"""Native X11 selection fixture: keep metadata responsive, delay image replies."""
import argparse
import ctypes as c
from pathlib import Path
import time


class Request(c.Structure):
    _fields_ = [('type', c.c_int), ('serial', c.c_ulong), ('send_event', c.c_int),
                ('display', c.c_void_p), ('owner', c.c_ulong), ('requestor', c.c_ulong),
                ('selection', c.c_ulong), ('target', c.c_ulong), ('property', c.c_ulong),
                ('time', c.c_ulong)]


class Selection(c.Structure):
    _fields_ = [('type', c.c_int), ('serial', c.c_ulong), ('send_event', c.c_int),
                ('display', c.c_void_p), ('requestor', c.c_ulong), ('selection', c.c_ulong),
                ('target', c.c_ulong), ('property', c.c_ulong), ('time', c.c_ulong)]


class Event(c.Union):
    _fields_ = [('type', c.c_int), ('request', Request), ('selection', Selection),
                ('padding', c.c_long * 24)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--image', type=Path, required=True)
    parser.add_argument('--requested', type=Path, required=True)
    parser.add_argument('--delay', type=float, default=6)
    parser.add_argument('--save-targets', action='store_true', help='Request application-close handoff and exit after acknowledgement')
    args = parser.parse_args()
    x = c.CDLL('libX11.so.6')
    signatures = {
        'XOpenDisplay': ([c.c_char_p], c.c_void_p),
        'XDefaultRootWindow': ([c.c_void_p], c.c_ulong),
        'XCreateSimpleWindow': ([c.c_void_p, c.c_ulong, c.c_int, c.c_int, c.c_uint,
                                c.c_uint, c.c_uint, c.c_ulong, c.c_ulong], c.c_ulong),
        'XInternAtom': ([c.c_void_p, c.c_char_p, c.c_int], c.c_ulong),
        'XConvertSelection': ([c.c_void_p, c.c_ulong, c.c_ulong, c.c_ulong, c.c_ulong, c.c_ulong], c.c_int),
        'XSetSelectionOwner': ([c.c_void_p, c.c_ulong, c.c_ulong, c.c_ulong], c.c_int),
        'XGetSelectionOwner': ([c.c_void_p, c.c_ulong], c.c_ulong),
        'XChangeProperty': ([c.c_void_p, c.c_ulong, c.c_ulong, c.c_ulong, c.c_int,
                             c.c_int, c.c_void_p, c.c_int], c.c_int),
        'XSendEvent': ([c.c_void_p, c.c_ulong, c.c_int, c.c_long, c.POINTER(Event)], c.c_int),
        'XNextEvent': ([c.c_void_p, c.POINTER(Event)], c.c_int),
        'XFlush': ([c.c_void_p], c.c_int),
        'XCloseDisplay': ([c.c_void_p], c.c_int),
    }
    for name, (arguments, result) in signatures.items():
        function = getattr(x, name)
        function.argtypes, function.restype = arguments, result
    display = x.XOpenDisplay(None)
    assert display, 'isolated X11 display unavailable'
    window = x.XCreateSimpleWindow(display, x.XDefaultRootWindow(display), 0, 0, 1, 1, 0, 0, 0)
    atom = lambda name: x.XInternAtom(display, name.encode(), 0)
    clipboard, targets, png, atoms = map(atom, ('CLIPBOARD', 'TARGETS', 'image/png', 'ATOM'))
    raw = args.image.read_bytes()
    x.XSetSelectionOwner(display, clipboard, window, 0)
    manager, save, saved = map(atom, ('CLIPBOARD_MANAGER', 'SAVE_TARGETS', 'POOLSYNC_TEST_SAVED'))
    if args.save_targets:
        assert x.XGetSelectionOwner(display, manager), 'no native clipboard manager'
        x.XConvertSelection(display, manager, save, saved, window, 0)
    x.XFlush(display)
    try:
        while True:
            event = Event()
            x.XNextEvent(display, c.byref(event))
            if args.save_targets and event.type == 31 and event.selection.selection == manager:
                args.requested.with_suffix('.ack').write_text('acknowledged')
                return
            if event.type == 29:  # SelectionClear
                return
            if event.type != 30:  # SelectionRequest
                continue
            request = event.request
            prop = request.property or request.target
            ok = False
            if request.target == targets:
                available = (c.c_ulong * 2)(targets, png)
                x.XChangeProperty(display, request.requestor, prop, atoms, 32, 0,
                                  c.cast(available, c.c_void_p), 2)
                ok = True
            elif request.target == png:
                args.requested.write_text('requested')
                time.sleep(args.delay)
                if x.XGetSelectionOwner(display, clipboard) == window:
                    data = c.create_string_buffer(raw)
                    x.XChangeProperty(display, request.requestor, prop, png, 8, 0,
                                      c.cast(data, c.c_void_p), len(raw))
                    ok = True
            if x.XGetSelectionOwner(display, clipboard) != window:
                return
            reply = Event()
            reply.selection = Selection(31, 0, 1, display, request.requestor,
                                        request.selection, request.target, prop if ok else 0,
                                        request.time)
            x.XSendEvent(display, request.requestor, 0, 0, c.byref(reply))
            x.XFlush(display)
    finally:
        x.XCloseDisplay(display)


if __name__ == '__main__':
    main()
