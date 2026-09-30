#!/usr/bin/env python3
"""Real-window wizard regression test; run inside `nix develop`.

Requires the tools resolved by scripts/screenshot.sh and a built terminus.
Example: nix develop --command bash -c \
  'source .dev/gpu-env.sh; python3 scripts/test-host-wizard.py'
--baseline checks the saved pre-fix captures against the same acceptance rules.
No network connection or persistent user database is used.
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


def tool(package, name):
    return str(Path(TOOLS[package]) / "bin" / name)


MAGICK = tool("imagemagick", "magick")
XDOTOOL = tool("xdotool", "xdotool")
SHOTS = ROOT / ".dev/shots/host-wizard"
FAILURES = []
CHECKS = []


def check(condition, message):
    CHECKS.append(message)
    if not condition:
        FAILURES.append(message)
        print("FAIL:", message, flush=True)
    else:
        print("PASS:", message, flush=True)


def rgb(path):
    return subprocess.check_output([MAGICK, str(path), "-alpha", "off",
                                    "-depth", "8", "rgb:-"])


def count(data, rect, predicate):
    x, y, width, height = map(int, rect)
    return sum(predicate(*data[(row * 1000 + col) * 3: (row * 1000 + col) * 3 + 3])
               for row in range(y, y + height) for col in range(x, x + width))


def geometry(rows, baseline=False):
    height = (274 if baseline else 326) + (rows - 2) * 64
    top = (800 - height) // 2
    footer = top + height - 24 - (38 if baseline else 32)
    return top, height, footer


def inspect(path, rows, step, empty_row=None, error=False, baseline=False):
    data = rgb(path)
    check(len(data) == 1000 * 800 * 3, f"{path.stem}: real 1000x800 framebuffer")
    top, height, footer = geometry(rows, baseline)
    light = lambda r, g, b: r > 130 and g > 130 and b > 130
    check(count(data, (300, top + 27, 220, 24), light) > 80,
          f"{path.stem}: wizard title is present")
    pill_x = 300 + step * (406 / 3)
    fill = tuple(data[((top + 70) * 1000 + int(pill_x + 115)) * 3:
                      ((top + 70) * 1000 + int(pill_x + 115)) * 3 + 3])
    check(fill[2] < 100, f"{path.stem}: active pill has a dark fill")
    blue = lambda r, g, b: b > 120 and 40 < g < 180 and r < 100
    check(count(data, (pill_x + 7, top + 60, 105, 16), blue) > 20,
          f"{path.stem}: active step label is visible")
    if empty_row is not None:
        placeholder = lambda r, g, b: r > 60 and g > 70 and 90 < b < 210
        check(count(data, (315, top + 117 + empty_row * 64, 370, 16), placeholder) > 10,
              f"{path.stem}: focused empty field retains its placeholder")
    red = lambda r, g, b: r > 100 and r > g * 1.4 and abs(g - b) < 30
    if error:
        last_input_bottom = top + 140 + (rows - 1) * 64
        check(count(data, (300, last_input_bottom, 400, footer - last_input_bottom), red) > 20,
              f"{path.stem}: error is visible below inputs")
        check(count(data, (300, footer, 400, 32), red) == 0,
              f"{path.stem}: no error pixels overlap action buttons")
    else:
        check(count(data, (300, top + height - 110, 400, 86), red) == 0,
              f"{path.stem}: no stale validation error")
    return data


def run_gui(edit_only=False):
    SHOTS.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env.pop("WAYLAND_DISPLAY", None)
    env.pop("WAYLAND_SOCKET", None)
    env["DISPLAY"] = os.environ.get("TERMINUS_E2E_DISPLAY", ":117")
    with tempfile.TemporaryDirectory(prefix="terminus-wizard-") as temporary:
        config = Path(temporary) / "config"
        config.mkdir()
        (config / "config.toml").write_text('[fonts]\nsize = 16\n[shell]\nargs = []\n')
        env.update(TERMINUS_DATA_DIR=temporary, TERMINUS_CONFIG_HOME=str(config),
                   TERMINUS_LOG_LEVEL="info")
        with (SHOTS / "app.log").open("w") as log:
            display = subprocess.Popen([tool("xvfb", "Xvfb"), env["DISPLAY"],
                                        "-screen", "0", "1000x800x24", "-nolisten", "tcp"],
                                       stdout=log, stderr=log)
            app = None
            try:
                time.sleep(2)
                assert display.poll() is None, "Xvfb failed; inspect app.log"
                app = subprocess.Popen([str(ROOT / "target/debug/terminus")], env=env,
                                       cwd=ROOT, stdout=log, stderr=log)
                window = None
                for _ in range(60):
                    assert app.poll() is None, "Application exited; inspect app.log"
                    found = subprocess.run([XDOTOOL, "search", "--onlyvisible", "--pid", str(app.pid)],
                                           env=env, capture_output=True, text=True)
                    if found.returncode == 0 and found.stdout.strip():
                        window = found.stdout.splitlines()[0]
                        break
                    time.sleep(0.5)
                assert window, "No application window appeared"

                def action(*args):
                    subprocess.run([XDOTOOL, *map(str, args)], env=env, check=True)
                    time.sleep(0.6)

                def click(x, y):
                    action("mousemove", "--window", window, x, y)
                    action("click", 1)

                def field(rows, row):
                    click(350, geometry(rows)[0] + 124 + row * 64)

                def next_button(rows):
                    click(660, geometry(rows)[2] + 16)

                def back(rows):
                    click(330, geometry(rows)[2] + 16)

                def text(value):
                    action("type", "--clearmodifiers", "--delay", 40, value)

                def shot(name, rows, step, **kwargs):
                    path = SHOTS / f"{name}.png"
                    subprocess.run([tool("xorg_xwd", "xwd"), "-silent", "-id", window,
                                    "-out", str(SHOTS / "frame.xwd")], env=env, check=True)
                    subprocess.run([MAGICK, str(SHOTS / "frame.xwd"), str(path)], check=True)
                    top, height, _ = geometry(rows)
                    subprocess.run([MAGICK, str(path), "-crop", f"450x{height + 2}+275+{top - 1}",
                                    "+repage", str(SHOTS / f"{name}-dialog.png")], check=True)
                    return inspect(path, rows, step, **kwargs)

                initial = rgb(SHOTS / "01-focused-hostname.png") if edit_only else None
                if not edit_only:
                    action("windowsize", window, 1000, 800)
                    action("windowfocus", window)
                    time.sleep(2)
                    click(180, 175)
                    initial = shot("01-focused-hostname", 2, 0, empty_row=0)
                    action("key", "Tab")
                    shot("02-focused-port", 2, 0, empty_row=1)
                    next_button(2)
                    shot("03-empty-host-error", 2, 0, error=True)
                    field(2, 0)
                    text("127.0.0.1")
                    action("key", "Home", "Right", "Right")
                    data = shot("04-hostname-cursor", 2, 0)
                    light = lambda r, g, b: r > 130 and g > 130 and b > 130
                    check(count(data, (315, geometry(2)[0] + 117, 200, 16), light) > 80,
                          "Populated hostname remains visible around the cursor")
                    field(2, 1)
                    text("abc")
                    next_button(2)
                    shot("05-auth-username", 3, 1, empty_row=0)
                    text("root")
                    next_button(3)
                    shot("06-password-error", 3, 1, error=True)
                    field(3, 2)
                    shot("07-focused-password", 3, 1, empty_row=2)
                    text("e2e-pass")
                    data = shot("08-masked-password", 3, 1)
                    check(count(data, (315, geometry(3)[0] + 245, 200, 16), light) > 20,
                          "Masked password glyphs remain visible while focused")
                    click(680, geometry(3)[0] + 252)
                    data = shot("09-visible-password", 3, 1)
                    check(count(data, (315, geometry(3)[0] + 245, 200, 16), light) > 80,
                          "Revealed password is rendered")
                    next_button(3)
                    shot("10-focused-details", 1, 2, empty_row=0)
                    text("Wizard E2E")
                    next_button(1)
                    details = shot("11-invalid-port-error", 1, 2, error=True)
                    back(1)
                    shot("12-back-auth", 3, 1)
                    back(3)
                    data = shot("13-back-target", 2, 0)
                    check(count(data, (315, geometry(2)[0] + 181, 200, 16), light) > 20,
                          "Invalid port value is preserved on backward navigation")
                    field(2, 1)
                    action("key", "End", "BackSpace", "BackSpace", "BackSpace")
                    text("2222")
                    next_button(2)
                    next_button(3)
                    corrected = shot("14-corrected-details", 1, 2)
                    name_y = geometry(1)[0] + 117
                    def name_text(data):
                        return b"".join(data[(y * 1000 + 315) * 3:(y * 1000 + 500) * 3]
                                        for y in range(name_y, name_y + 16))
                    check(name_text(details) == name_text(corrected),
                          "Name text is preserved exactly after correcting the port")
                    back(1)
                    # Keyboard-select other auth methods, without saved keys or network calls.
                    field(3, 1)
                    action("key", "Left")
                    shot("15-key-auth", 3, 1)
                    next_button(3)
                    shot("16-missing-key-error", 3, 1, error=True)
                    field(3, 1)
                    action("key", "Left")
                    shot("17-gssapi-auth", 2, 1)
                    next_button(2)
                    shot("18-gssapi-details", 1, 2)

                # Seed a host in this disposable database, then reopen through
                # the real context menu. No SSH probe or user vault is involved.
                app.terminate()
                app.wait(timeout=5)
                with sqlite3.connect(Path(temporary) / "terminus.db") as db:
                    db.execute("INSERT INTO hosts (id,name,hostname,port,username,auth_method,created_at,updated_at) "
                               "VALUES (?,?,?,?,?,?,?,?)",
                               ("4c40b3a0-58d6-4e66-a357-e230c5f439c0", "EditFixture", "127.0.0.1", 22,
                                "root", "password", "2026-09-30T00:00:00Z", "2026-09-30T00:00:00Z"))
                app = subprocess.Popen([str(ROOT / "target/debug/terminus")], env=env,
                                       cwd=ROOT, stdout=log, stderr=log)
                window = None
                for _ in range(60):
                    assert app.poll() is None, "Application exited while reopening fixture"
                    found = subprocess.run([XDOTOOL, "search", "--onlyvisible", "--pid", str(app.pid)],
                                           env=env, capture_output=True, text=True)
                    if found.returncode == 0 and found.stdout.strip():
                        window = found.stdout.splitlines()[0]
                        break
                    time.sleep(0.5)
                assert window, "Fixture window did not appear"
                action("windowsize", window, 1000, 800)
                action("windowfocus", window)
                time.sleep(2)
                click(150, 104)
                text("EditFixture")
                action("mousemove", "--window", window, 180, 270)
                action("click", 3)
                click(210, 386)  # Edit host, fourth context menu action.
                edited = shot("19-edit-target", 2, 0)
                footer = geometry(2)[2]
                def next_label(data):
                    return b"".join(data[(y * 1000 + 630) * 3:(y * 1000 + 694) * 3]
                                    for y in range(footer + 7, footer + 25))
                check(next_label(initial) == next_label(edited),
                      "Edit Target has one readable Next label, without a Save label over it")
                field(2, 1)
                action("key", "End", "BackSpace", "BackSpace")
                text("invalid" * 7)
                next_button(2)
                field(3, 2)
                shot("20-edit-password-placeholder", 3, 1, empty_row=2)
                next_button(3)
                shot("21-edit-details", 1, 2)
                next_button(1)
                data = shot("22-wrapped-port-error", 1, 2, error=True)
                red = lambda r, g, b: r > 100 and r > g * 1.4 and abs(g - b) < 30
                top, height, footer = geometry(1)
                check(count(data, (300, footer - 48, 400, 14), red) > 20
                      and count(data, (300, footer - 34, 400, 14), red) > 20,
                      "Long validation message wraps onto two lines above Save")
            finally:
                for process in (app, display):
                    if process is not None and process.poll() is None:
                        process.terminate()
                        try:
                            process.wait(timeout=5)
                        except subprocess.TimeoutExpired:
                            process.kill()
                            process.wait()


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--baseline", action="store_true")
parser.add_argument("--edit-only", action="store_true")
args = parser.parse_args()
if args.baseline:
    inspect(ROOT / ".dev/shots/wizard-before.png", 2, 0, empty_row=0, baseline=True)
    inspect(ROOT / ".dev/shots/wizard-error-before.png", 2, 0, error=True, baseline=True)
else:
    run_gui(args.edit_only)
    (SHOTS / "results.json").write_text(json.dumps({"checks": CHECKS, "failures": FAILURES}, indent=2))
print(f"{len(CHECKS) - len(FAILURES)}/{len(CHECKS)} checks passed; {len(FAILURES)} failures")
raise SystemExit(bool(FAILURES))
