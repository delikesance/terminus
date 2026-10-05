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
FRAMES = []
FRAME_WIDTH, FRAME_HEIGHT = 1000, 800
LAST_GEOMETRY = None
DIALOG_LEFT, DIALOG_WIDTH = 276, 448


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
    return sum(predicate(*data[(row * FRAME_WIDTH + col) * 3: (row * FRAME_WIDTH + col) * 3 + 3])
               for row in range(y, y + height) for col in range(x, x + width))


def geometry(rows, baseline=False, data=None):
    """Read the dialog shell from the framebuffer, without predicting its height."""
    global LAST_GEOMETRY, DIALOG_LEFT, DIALOG_WIDTH
    if baseline:
        height = 274 + (rows - 2) * 64
        top = (800 - height) // 2
        return top, height, top + height - 24 - 38
    if data is not None:
        def dialog_pixel(x, y):
            at = (y * FRAME_WIDTH + x) * 3
            return tuple(data[at:at + 3]) == (17, 17, 19)
        expected_width = min(448, FRAME_WIDTH)
        expected_left = (FRAME_WIDTH - expected_width) // 2
        # Require both padding columns to agree: a sidebar card can share the
        # modal color, but cannot span both sides of this centered dialog.
        ys = [y for y in range(FRAME_HEIGHT)
              if dialog_pixel(expected_left + 12, y)
              and dialog_pixel(expected_left + expected_width - 13, y)]
        assert ys, "No dialog shell in framebuffer"
        top, bottom = min(ys), max(ys) + 1
        while top > 0 and dialog_pixel(FRAME_WIDTH // 2, top - 1):
            top -= 1
        while bottom < FRAME_HEIGHT and dialog_pixel(FRAME_WIDTH // 2, bottom):
            bottom += 1
        assert bottom - top > 100 and top > 40, "No modal dialog shell in framebuffer"
        # Only follow the connected interior around the center. Sidebar cards can
        # have the same color; the dialog border separates them from this run.
        left = right = FRAME_WIDTH // 2
        while left > 0 and dialog_pixel(left - 1, top + 20):
            left -= 1
        while right + 1 < FRAME_WIDTH and dialog_pixel(right + 1, top + 20):
            right += 1
        DIALOG_LEFT, DIALOG_WIDTH = left, right - left + 1
        LAST_GEOMETRY = top, bottom - top, bottom - 24 - 32
    assert LAST_GEOMETRY, "Capture a dialog before asking for its geometry"
    return LAST_GEOMETRY


def inspect(path, rows, step, empty_row=None, error=False, baseline=False):
    data = rgb(path)
    check(len(data) == FRAME_WIDTH * FRAME_HEIGHT * 3,
          f"{path.stem}: real {FRAME_WIDTH}x{FRAME_HEIGHT} framebuffer")
    top, height, footer = geometry(rows, baseline, data)
    left = DIALOG_LEFT + 24
    content_width = DIALOG_WIDTH - 48
    if not baseline:
        FRAMES.append({"name": path.stem, "viewport": [FRAME_WIDTH, FRAME_HEIGHT],
                       "dialog": [DIALOG_LEFT, top, DIALOG_WIDTH, height],
                       "footer_y": footer, "error": error})
    light = lambda r, g, b: r > 130 and g > 130 and b > 130
    check(count(data, (left, top + 27, min(220, content_width), 24), light) > 80,
          f"{path.stem}: wizard title is present")
    pill_width = (content_width - 12) / 3
    pill_x = left + step * (pill_width + 6)
    fill = tuple(data[((top + 70) * FRAME_WIDTH + int(pill_x + pill_width - 12)) * 3:
                      ((top + 70) * FRAME_WIDTH + int(pill_x + pill_width - 12)) * 3 + 3])
    check(fill[2] < 100, f"{path.stem}: active pill has a dark fill")
    blue = lambda r, g, b: b > 120 and 40 < g < 180 and r < 100
    check(count(data, (pill_x + 7, top + 60, int(pill_width - 14), 16), blue) > 20,
          f"{path.stem}: active step label is visible")
    if empty_row is not None:
        placeholder = lambda r, g, b: r > 60 and g > 70 and 90 < b < 210
        check(count(data, (left + 15, top + (117 if baseline else 123) + empty_row * 64, content_width - 30, 16), placeholder) > 10,
              f"{path.stem}: focused empty field retains its placeholder")
    red = lambda r, g, b: r > 100 and r > g * 1.4 and abs(g - b) < 30
    if error:
        last_input_bottom = top + (140 if baseline else 146) + (rows - 1) * 64
        check(count(data, (left, last_input_bottom, content_width, footer - last_input_bottom), red) > 20,
              f"{path.stem}: error is visible below inputs")
        check(count(data, (left, footer, content_width, 32), red) == 0,
              f"{path.stem}: no error pixels overlap action buttons")
    else:
        check(count(data, (left, top + 146 + (rows - 1) * 64, content_width, footer - top - 146 - (rows - 1) * 64), red) == 0,
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

                def capture(path):
                    subprocess.run([tool("xorg_xwd", "xwd"), "-silent", "-id", window,
                                    "-out", str(SHOTS / "frame.xwd")], env=env, check=True)
                    subprocess.run([MAGICK, str(SHOTS / "frame.xwd"), str(path)], check=True)

                def current_geometry(rows):
                    path = SHOTS / "current.png"
                    capture(path)
                    return geometry(rows, data=rgb(path))

                def field(rows, row):
                    top, _, _ = current_geometry(rows)
                    click(DIALOG_LEFT + 74, top + 130 + row * 64)

                def next_button(rows):
                    _, _, footer = current_geometry(rows)
                    click(DIALOG_LEFT + DIALOG_WIDTH - 64, footer + 16)

                def back(rows):
                    _, _, footer = current_geometry(rows)
                    click(DIALOG_LEFT + 54, footer + 16)

                def text(value):
                    action("type", "--clearmodifiers", "--delay", 40, value)

                def shot(name, rows, step, **kwargs):
                    path = SHOTS / f"{name}.png"
                    capture(path)
                    top, height, _ = geometry(rows, data=rgb(path))
                    crop_x = max(0, DIALOG_LEFT - 1)
                    subprocess.run([MAGICK, str(path), "-crop",
                                    f"{min(DIALOG_WIDTH + 2, FRAME_WIDTH)}x{height + 2}+{crop_x}+{top - 1}",
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
                    check(count(data, (315, geometry(2)[0] + 123, 200, 16), light) > 80,
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
                    check(count(data, (315, geometry(3)[0] + 251, 200, 16), light) > 20,
                          "Masked password glyphs remain visible while focused")
                    click(680, geometry(3)[0] + 258)
                    data = shot("09-visible-password", 3, 1)
                    check(count(data, (315, geometry(3)[0] + 251, 200, 16), light) > 80,
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
                    check(count(data, (315, geometry(2)[0] + 187, 200, 16), light) > 20,
                          "Invalid port value is preserved on backward navigation")
                    field(2, 1)
                    action("key", "End", "BackSpace", "BackSpace", "BackSpace")
                    text("2222")
                    next_button(2)
                    next_button(3)
                    corrected = shot("14-corrected-details", 1, 2)
                    def name_text(data):
                        name_y = geometry(1, data=data)[0] + 123
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
                text("invalid" * 9)
                next_button(2)
                field(3, 2)
                shot("20-edit-password-placeholder", 3, 1, empty_row=2)
                next_button(3)
                shot("21-edit-details", 1, 2)
                next_button(1)
                data = shot("22-wrapped-port-error", 1, 2, error=True)
                red = lambda r, g, b: r > 100 and r > g * 1.4 and abs(g - b) < 30
                top, height, footer = geometry(1)
                red_rows = [y for y in range(top + 146, footer)
                            if count(data, (300, y, 400, 1), red) > 1]
                line_starts = [y for i, y in enumerate(red_rows) if i == 0 or y > red_rows[i - 1] + 1]
                check(len(line_starts) == 2,
                      "Long validation message wraps onto two lines above Save")
                wide_height = height
                global FRAME_WIDTH, FRAME_HEIGHT
                FRAME_WIDTH, FRAME_HEIGHT = 320, 800
                action("windowsize", window, FRAME_WIDTH, FRAME_HEIGHT)
                narrow = shot("23-narrow-wrapped-error", 1, 2, error=True)
                narrow_top, narrow_height, narrow_footer = geometry(1)
                check(narrow_height > wide_height,
                      "Narrowing the window grows the measured validation block")
                check(DIALOG_LEFT == 0 and DIALOG_WIDTH == 320,
                      "Dialog components fit the narrow viewport")
                # Click the recomputed Back hitbox while the notice is visible.
                back(1)
                field(3, 2)
                shot("24-narrow-auth", 3, 1, empty_row=2)
                next_button(3)
                shot("25-narrow-details", 1, 2)
                FRAME_WIDTH, FRAME_HEIGHT = 1000, 800
                action("windowsize", window, FRAME_WIDTH, FRAME_HEIGHT)
                shot("26-restored-details", 1, 2)
                check(geometry(1)[1] < wide_height,
                      "Cleared notice releases its space after widening the window")
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
    (SHOTS / "results.json").write_text(json.dumps({"checks": CHECKS, "failures": FAILURES, "frames": FRAMES}, indent=2))
print(f"{len(CHECKS) - len(FAILURES)}/{len(CHECKS)} checks passed; {len(FAILURES)} failures")
raise SystemExit(bool(FAILURES))
