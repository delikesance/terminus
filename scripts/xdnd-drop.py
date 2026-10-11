#!/usr/bin/env python3
#
# Drop a local file on a running Terminus window with a real XDND exchange, so
# the app receives a genuine WindowEvent::DroppedFile. Pairs with
# scripts/screenshot.sh, which starts the app on Xvfb with no window manager.
#
#   scripts/xdnd-drop.py --file "/path/with space/report.pdf"
#   scripts/xdnd-drop.py --file ./big.bin --x 400 --y 300 --display :93
#
# Needs python-xlib (`pip install python-xlib`). The target is the first
# top-level window that advertises XdndAware, which winit sets on every app
# window. The script owns XdndSelection, runs XdndEnter, XdndPosition,
# XdndDrop, answers the target's selection request with a text/uri-list
# holding the file:// URI, and waits for XdndFinished. Exit status is 0 when
# the target accepted the drop and finished it.
#
# Options:
#   --file PATH      file to drop (required)
#   --x X --y Y      pointer position, window-relative (default: centre)
#   --display DISP   X display (default: $DISPLAY)
#   --timeout SEC    how long to wait for each protocol step (default 5)

import argparse
import os
import sys
import time
import urllib.parse

from Xlib import X, display
from Xlib.protocol import event

XDND_VERSION = 5
COPY_ACTION_INDEX = 0
URI_LIST = "text/uri-list"


class DropSource:
    def __init__(self, disp, timeout):
        self.disp = disp
        self.timeout = timeout
        self.root = disp.screen().root
        self.window = self.root.create_window(0, 0, 1, 1, 0, disp.screen().root_depth)
        self.atoms = {
            name: disp.intern_atom(name)
            for name in (
                "XdndAware",
                "XdndEnter",
                "XdndPosition",
                "XdndStatus",
                "XdndDrop",
                "XdndFinished",
                "XdndSelection",
                "XdndActionCopy",
                "TARGETS",
                URI_LIST,
            )
        }
        self.payload = b""

    def find_target(self):
        for child in self.root.query_tree().children:
            if child.get_full_property(self.atoms["XdndAware"], X.AnyPropertyType):
                return child
        raise SystemExit("no XdndAware top-level window found; is the app running?")

    def send(self, target, name, data):
        message = event.ClientMessage(
            window=target,
            client_type=self.atoms[name],
            data=(32, [self.window.id, *data, *([0] * (4 - len(data)))]),
        )
        target.send_event(message, event_mask=0)
        self.disp.flush()

    def wait_for(self, predicate):
        deadline = time.monotonic() + self.timeout
        while time.monotonic() < deadline:
            while self.disp.pending_events():
                ev = self.disp.next_event()
                if predicate(ev):
                    return ev
                self.answer_selection(ev)
            time.sleep(0.01)
        return None

    def answer_selection(self, ev):
        if ev.type != X.SelectionRequest or ev.selection != self.atoms["XdndSelection"]:
            return
        prop = ev.property or ev.target
        if ev.target == self.atoms["TARGETS"]:
            ev.requestor.change_property(prop, X.AnyPropertyType, 32,
                                         [self.atoms["TARGETS"], self.atoms[URI_LIST]])
        elif ev.target == self.atoms[URI_LIST]:
            ev.requestor.change_property(prop, self.atoms[URI_LIST], 8, self.payload)
        else:
            prop = X.NONE
        reply = event.SelectionNotify(
            time=ev.time,
            requestor=ev.requestor,
            selection=ev.selection,
            target=ev.target,
            property=prop,
        )
        ev.requestor.send_event(reply, event_mask=0)
        self.disp.flush()

    def drop(self, target, path, x, y):
        self.payload = ("file://" + urllib.parse.quote(path) + "\r\n").encode()
        self.window.set_selection_owner(self.atoms["XdndSelection"], X.CurrentTime)
        self.disp.flush()

        self.send(target, "XdndEnter", [XDND_VERSION << 24, self.atoms[URI_LIST], 0, 0])
        self.send(target, "XdndPosition", [0, (x << 16) | y, X.CurrentTime, self.atoms["XdndActionCopy"]])
        status = self.wait_for(lambda ev: is_client(ev, self.atoms["XdndStatus"]))
        if status is None:
            raise SystemExit("no XdndStatus from the target")

        self.send(target, "XdndDrop", [0, X.CurrentTime])
        finished = self.wait_for(lambda ev: is_client(ev, self.atoms["XdndFinished"]))
        if finished is None:
            raise SystemExit("no XdndFinished from the target")
        accepted = bool(finished.data[1][1] & 1)
        return accepted


def is_client(ev, atom):
    return ev.type == X.ClientMessage and ev.client_type == atom


def parse_args():
    parser = argparse.ArgumentParser(description="Drop a file on a Terminus window over XDND.")
    parser.add_argument("--file", required=True)
    parser.add_argument("--x", type=int)
    parser.add_argument("--y", type=int)
    parser.add_argument("--display")
    parser.add_argument("--timeout", type=float, default=5.0)
    return parser.parse_args()


def main():
    args = parse_args()
    path = os.path.abspath(args.file)
    if not os.path.exists(path):
        raise SystemExit(f"no such path: {path}")
    disp = display.Display(args.display)
    source = DropSource(disp, args.timeout)
    target = source.find_target()
    geometry = target.get_geometry()
    x = args.x if args.x is not None else geometry.width // 2
    y = args.y if args.y is not None else geometry.height // 2
    accepted = source.drop(target, path, x, y)
    print(f"dropped {path} at {x},{y}: accepted={accepted}")
    return 0 if accepted else 1


if __name__ == "__main__":
    sys.exit(main())
