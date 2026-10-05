#!/usr/bin/env python3
"""Real-window regression test of the J5 "Add a server" wizard.

Drives the real app on its own Xvfb display, in a throw-away data dir, and
checks both what is drawn (validation error, step indicator) and what is
stored (the host row the wizard writes). No vault or user database is
touched: a throw-away data dir holds the test key as a plaintext managed key.
Saving probes the server and signs in, so --port/--user/--key must name a
local sshd that accepts that key (port 2200 is assumed closed, to check the
probe error).

Run inside the devshell after a build, with the capture tools resolved once
by scripts/screenshot.sh:

    nix develop --command python3 scripts/test-host-wizard.py \
        --key ~/.ssh/test_ed25519 [--port 22] [--user $USER] \\
        [--bin target/debug/terminus] [--display :117]
"""
import argparse
import json
import os
from pathlib import Path
import re
import sqlite3
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
TOOLS = dict(re.findall(r"export TERMINUS_TOOL_(\w+)=(\S+)",
                        (ROOT / ".dev/screenshot-tools.env").read_text()))
W, H = 1440, 900
SHOTS = ROOT / ".dev/shots/host-wizard"
CHECKS, FAILURES = [], []

# Geometry of the wizard at 1440x900 (dialog centred in the window).
ADD_SERVER = (86, 828)
ADDRESS, USER, PORT, NAME = (720, 361), (634, 449), (891, 449), (720, 537)
CONTINUE_1 = (918, 624)       # step 1 footer
ERROR_AREA = (468, 612, 290, 26)
STEP_LINES = [(470, 263), (641, 263), (811, 263)]
CONTINUE_2 = (918, 594)       # step 2 footer, SSH key picked
BACK_2 = (818, 594)
KEY_ID = "5a1d0c2e-7b7e-4c3a-9f00-00000000c0de"
TAGS = (720, 449)
SAVE_3 = (910, 670)
BACK_3 = (803, 670)
PROBE_ERROR_AREA = (468, 652, 280, 36)


def tool(package, name):
    return str(Path(TOOLS[package]) / "bin" / name)


def check(ok, message):
    CHECKS.append(message)
    if not ok:
        FAILURES.append(message)
    print("PASS:" if ok else "FAIL:", message, flush=True)


class App:
    def __init__(self, binary, display, data_dir, log):
        self.env = os.environ.copy()
        for k in ("WAYLAND_DISPLAY", "WAYLAND_SOCKET"):
            self.env.pop(k, None)
        config = Path(data_dir) / "config"
        config.mkdir(exist_ok=True)
        (config / "config.toml").write_text("[fonts]\nsize = 16\n")
        self.env.update(DISPLAY=display, TERMINUS_DATA_DIR=str(data_dir),
                        TERMINUS_CONFIG_HOME=str(config), TERMINUS_NO_UPDATE_CHECK="1",
                        TERMINUS_HISTORY="0")
        self.xvfb = subprocess.Popen([tool("xvfb", "Xvfb"), display, "-screen", "0",
                                      f"{W}x{H}x24", "-nolisten", "tcp"],
                                     stdout=log, stderr=log)
        time.sleep(2)
        assert self.xvfb.poll() is None, "Xvfb failed"
        self.proc = subprocess.Popen([binary], env=self.env, cwd=ROOT,
                                     stdout=log, stderr=log)
        self.window = None
        for _ in range(80):
            assert self.proc.poll() is None, "the app exited; see app.log"
            found = subprocess.run([tool("xdotool", "xdotool"), "search", "--onlyvisible",
                                    "--pid", str(self.proc.pid)],
                                   env=self.env, capture_output=True, text=True)
            if found.returncode == 0 and found.stdout.strip():
                self.window = found.stdout.splitlines()[0]
                break
            time.sleep(0.5)
        assert self.window, "no window appeared"
        self.xdo("windowsize", self.window, W, H)
        self.xdo("windowfocus", self.window)
        time.sleep(3)

    def xdo(self, *args, wait=0.6):
        subprocess.run([tool("xdotool", "xdotool"), *map(str, args)],
                       env=self.env, check=True)
        time.sleep(wait)

    def click(self, at, wait=1.0):
        self.xdo("mousemove", "--window", self.window, *at, wait=0.2)
        self.xdo("click", 1, wait=wait)

    def type(self, text):
        self.xdo("type", "--clearmodifiers", "--delay", 40, text)

    def key(self, *keys):
        self.xdo("key", "--clearmodifiers", *keys)

    def shot(self, name):
        path = SHOTS / f"{name}.png"
        xwd = SHOTS / "frame.xwd"
        subprocess.run([tool("xorg_xwd", "xwd"), "-silent", "-id", self.window,
                        "-out", str(xwd)], env=self.env, check=True)
        subprocess.run([tool("imagemagick", "magick"), str(xwd), str(path)], check=True)
        raw = subprocess.check_output([tool("imagemagick", "magick"), str(path),
                                       "-alpha", "off", "-depth", "8", "rgb:-"])
        return raw

    def close(self):
        for p in (self.proc, self.xvfb):
            if p.poll() is None:
                p.terminate()
                try:
                    p.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    p.kill()


def px(data, x, y):
    i = (y * W + x) * 3
    return tuple(data[i:i + 3])


def count(data, rect, pred):
    x, y, w, h = rect
    return sum(pred(*px(data, c, r)) for r in range(y, y + h) for c in range(x, x + w))


def red(r, g, b):
    return r > 150 and r > g * 1.5 and r > b * 1.3


def lit(r, g, b):  # the violet accent of an active / done step line
    return b > 150 and r > 110 and b > g


def steps_lit(data):
    return [lit(*px(data, x, y)) for x, y in STEP_LINES]


def seed_key(tmp, key_path):
    """Store the test key as a managed (plaintext, vault-less) identity."""
    pem = Path(key_path).read_text()
    public = Path(str(key_path) + ".pub").read_text().strip()
    stamp = "2026-10-05T00:00:00+00:00"
    with sqlite3.connect(Path(tmp) / "terminus.db") as db:
        db.execute("INSERT INTO identities (id,name,kind,public_key,private_key,"
                   "created_at,updated_at) VALUES (?,?,?,?,?,?,?)",
                   (KEY_ID, "wizard key", "key", public, pem, stamp, stamp))


def run(binary, display, key_path):
    SHOTS.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="terminus-wizard-") as tmp, \
            (SHOTS / "app.log").open("w") as log:
        # First launch creates the database; the key is seeded, then the
        # app starts again and loads it.
        App(binary, display, tmp, log).close()
        seed_key(tmp, key_path)
        app = App(binary, display, tmp, log)
        try:
            app.click(ADD_SERVER, wait=1.5)
            data = app.shot("01-address")
            check(steps_lit(data) == [True, False, False], "step 1: only Address is lit")
            check(count(data, ERROR_AREA, red) == 0, "step 1: no error before Continue")

            app.click(CONTINUE_1)
            data = app.shot("02-empty-address-error")
            check(count(data, ERROR_AREA, red) > 20,
                  "Continue with no address shows the error in the footer")
            check(steps_lit(data) == [True, False, False], "an error keeps step 1")

            app.click(ADDRESS)
            app.type("127.0.0.1")
            app.click(USER)
            app.type(args.user)
            app.click(PORT)
            app.key("End", *["BackSpace"] * 6)
            app.type("2200")
            app.click(NAME)
            app.type("Wizard E2E")
            app.click(CONTINUE_1)
            data = app.shot("03-sign-in")
            check(steps_lit(data) == [True, True, False], "step 2: Sign in is lit")

            app.click(BACK_2)
            data = app.shot("04-back-to-address")
            check(steps_lit(data) == [True, False, False], "Back returns to step 1")
            app.click(CONTINUE_1)
            app.click(CONTINUE_2)
            data = app.shot("05-organise")
            check(steps_lit(data) == [True, True, True], "step 3: Organise is lit")

            app.click(TAGS)
            app.type("e2e, wizard")
            # Port 2200: nothing listens, the reachability probe refuses.
            app.click(SAVE_3, wait=3.0)
            data = app.shot("06-unreachable")
            check(count(data, PROBE_ERROR_AREA, red) > 20,
                  "Save against a closed port shows the probe error")
            check(steps_lit(data) == [True, True, True], "the wizard stays open on step 3")

            # Back twice, fix the port (the local test sshd), save again.
            app.click(BACK_3)
            app.click(BACK_2)
            app.click(PORT)
            app.key("End", *["BackSpace"] * 6)
            app.type(str(args.port))
            app.click(CONTINUE_1)
            app.click(CONTINUE_2)
            data = app.shot("07-organise-again")
            check(steps_lit(data) == [True, True, True], "Continue keeps the sign-in choice")
            app.click(SAVE_3, wait=4.0)
            data = app.shot("08-saved")
            check(steps_lit(data) == [False, False, False], "Save closes the wizard")
        finally:
            app.close()

        db = sqlite3.connect(Path(tmp) / "terminus.db")
        rows = db.execute("SELECT name, hostname, port, username, auth_method, tags, "
                          "identity_id "
                          "FROM hosts WHERE deleted_at IS NULL").fetchall()
        check(len(rows) == 1, f"exactly one host stored (got {len(rows)})")
        if rows:
            name, host, port, user, auth, tags, identity = rows[0]
            check(identity == KEY_ID, f"the picked key is stored (got {identity})")
            check((name, host, port, user)
                  == ("Wizard E2E", "127.0.0.1", args.port, args.user),
                  f"address step values survive Back/Continue: {rows[0][:4]}")
            check(auth == "key", f"sign-in method stored (got {auth})")
            check(sorted(json.loads(tags)) == ["e2e", "wizard"], f"tags stored (got {tags})")


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--bin", default=str(ROOT / "target/debug/terminus"))
parser.add_argument("--display", default=os.environ.get("TERMINUS_E2E_DISPLAY", ":117"))
parser.add_argument("--port", type=int, default=22,
                    help="local sshd port; saving probes and signs in")
parser.add_argument("--user", default=os.environ.get("USER", "root"))
parser.add_argument("--key", required=True,
                    help="private key (no passphrase) authorised on that sshd; "
                         "KEY.pub must sit next to it")
args = parser.parse_args()
run(args.bin, args.display, args.key)
(SHOTS / "results.json").write_text(json.dumps({"checks": CHECKS, "failures": FAILURES},
                                               indent=2))
print(f"{len(CHECKS) - len(FAILURES)}/{len(CHECKS)} checks passed; {len(FAILURES)} failures")
raise SystemExit(bool(FAILURES))
