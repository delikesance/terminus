#!/usr/bin/env python3
"""Regenerate the Lucide icon geometry in `crates/terminus-ui/src/icons.rs`.

Sugarloaf has no SVG renderer and no stroke API, so each Lucide icon is
decomposed here into the exact 2px-wide stroke it would cover: line
segments, plus circular arcs for the `A` commands, all on Lucide's 24x24
grid. The output is the block between the `---- generated ----` markers in
`crates/terminus-ui/src/icons.rs`.

The SVG source of each icon is kept inline in `ICONS` below, verbatim from
https://github.com/lucide-icons/lucide (MIT). Adding an icon means pasting
its markup here, adding a variant to `Icon` in icons.rs, and running:

    scripts/gen-lucide-icons.py --write
    cargo fmt -p terminus-ui

Without `--write` the block is printed to stdout for inspection.
"""

import argparse
import math
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TARGET = ROOT / "crates" / "terminus-ui" / "src" / "icons.rs"
BEGIN = "// ---- generated: do not edit ----"
END = "// ---- end generated ----"

# Lucide, MIT licensed. Keys are Lucide's file names; the Rust constant is
# derived from them.
ICONS = {
    "server": '<rect width="20" height="8" x="2" y="2" rx="2" ry="2"/>'
    '<rect width="20" height="8" x="2" y="14" rx="2" ry="2"/>'
    '<line x1="6" x2="6.01" y1="6" y2="6"/>'
    '<line x1="6" x2="6.01" y1="18" y2="18"/>',
    "arrow-right-left": '<path d="m16 3 4 4-4 4"/><path d="M20 7H4"/>'
    '<path d="m8 21-4-4 4-4"/><path d="M4 17h16"/>',
    "folder": '<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9'
    'L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>',
    "sliders-horizontal": '<path d="M10 5H3"/><path d="M12 19H3"/>'
    '<path d="M14 3v4"/><path d="M16 17v4"/><path d="M21 12h-9"/>'
    '<path d="M21 19h-5"/><path d="M21 5h-7"/><path d="M8 10v4"/>'
    '<path d="M8 12H3"/>',
    "plus": '<path d="M5 12h14"/><path d="M12 5v14"/>',
}


def arc_center(p0, p1, rx, ry, large_arc, sweep):
    """SVG endpoint -> center parameterization (SVG 1.1 F.6.5)."""
    x1, y1 = p0
    x2, y2 = p1
    rx, ry = abs(rx), abs(ry)
    dx2, dy2 = (x1 - x2) / 2.0, (y1 - y2) / 2.0
    x1p, y1p = dx2, dy2  # no x-axis rotation in Lucide icons
    lam = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry)
    if lam > 1:
        s = math.sqrt(lam)
        rx, ry = rx * s, ry * s
    num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p
    den = rx * rx * y1p * y1p + ry * ry * x1p * x1p
    coef = math.sqrt(max(num / den, 0.0))
    if large_arc == sweep:
        coef = -coef
    cxp = coef * rx * y1p / ry
    cyp = -coef * ry * x1p / rx
    return cxp + (x1 + x2) / 2.0, cyp + (y1 + y2) / 2.0, rx, ry


def angle_deg(cx, cy, x, y):
    return math.degrees(math.atan2(y - cy, x - cx)) % 360.0


TOKEN_RE = re.compile(r"([MmLlHhVvAaZz])|(-?\d*\.?\d+(?:e-?\d+)?)", re.IGNORECASE)


def tokenize(d):
    out = []
    for cmd, num in TOKEN_RE.findall(d):
        out.append(cmd if cmd else float(num))
    return out


def parse(d):
    """Yield ('line', p0, p1) and ('arc', (cx, cy), r, a0, a1, large)."""
    toks = tokenize(d)
    i = 0
    cur = (0.0, 0.0)
    start = (0.0, 0.0)
    cmd = None
    items = []

    while i < len(toks):
        if isinstance(toks[i], str):
            cmd = toks[i]
            i += 1
        rel = cmd.islower()
        up = cmd.upper()

        if up == "M":
            x, y = toks[i], toks[i + 1]
            i += 2
            if rel:
                x, y = cur[0] + x, cur[1] + y
            cur = (x, y)
            start = cur
            cmd = "l" if rel else "L"
        elif up == "L":
            x, y = toks[i], toks[i + 1]
            i += 2
            if rel:
                x, y = cur[0] + x, cur[1] + y
            items.append(("line", cur, (x, y)))
            cur = (x, y)
        elif up == "H":
            x = toks[i]
            i += 1
            if rel:
                x = cur[0] + x
            items.append(("line", cur, (x, cur[1])))
            cur = (x, cur[1])
        elif up == "V":
            y = toks[i]
            i += 1
            if rel:
                y = cur[1] + y
            items.append(("line", cur, (cur[0], y)))
            cur = (cur[0], y)
        elif up == "A":
            rx, ry, _rot, large, sweep = toks[i : i + 5]
            x, y = toks[i + 5], toks[i + 6]
            i += 7
            if rel:
                x, y = cur[0] + x, cur[1] + y
            cx, cy, rx, _ry = arc_center(cur, (x, y), rx, ry, large, sweep)
            a0 = angle_deg(cx, cy, cur[0], cur[1])
            a1 = angle_deg(cx, cy, x, y)
            items.append(("arc", (cx, cy), rx, a0, a1, large))
            cur = (x, y)
        elif up == "Z":
            if cur != start:
                items.append(("line", cur, start))
            cur = start
        else:
            raise SystemExit(f"unhandled SVG command {cmd!r} in {d!r}")
    return items


def sweep_pair(a0, a1, large):
    """The traversed (start, end) angle pair for one arc.

    SVG's sweep flag is direction, which a symmetric stroke does not care
    about; the large-arc flag is what separates a 90-degree corner from
    its 270-degree complement, so the sweep length is chosen to match it.
    """
    d = (a1 - a0) % 360.0
    if d == 0.0:
        d = 360.0
    if (not large and d > 180.0) or (large and d < 180.0):
        d = 360.0 - d
    return a0, a0 + d


def rect_path(x, y, w, h, rx):
    """A rounded rect as an SVG path, so one parser handles every shape."""
    return (
        f"M{x + rx} {y}H{x + w - rx}A{rx} {rx} 0 0 1 {x + w} {y + rx}"
        f"V{y + h - rx}A{rx} {rx} 0 0 1 {x + w - rx} {y + h}"
        f"H{x + rx}A{rx} {rx} 0 0 1 {x} {y + h - rx}V{y + rx}"
        f"A{rx} {rx} 0 0 1 {x + rx} {y}Z"
    )


def svg_paths(svg):
    """Every drawable in a Lucide `<svg>` body, as SVG path data."""
    paths = re.findall(r'd="([^"]+)"', svg)
    for x1, x2, y1, y2 in re.findall(
        r'x1="([\d.]+)" x2="([\d.]+)" y1="([\d.]+)" y2="([\d.]+)"', svg
    ):
        paths.append(f"M{x1} {y1}L{x2} {y2}")
    for w, h, x, y, rx in re.findall(
        r'width="([\d.]+)" height="([\d.]+)" x="([\d.]+)" y="([\d.]+)" rx="([\d.]+)"',
        svg,
    ):
        paths.append(rect_path(float(x), float(y), float(w), float(h), float(rx)))
    if not paths:
        raise SystemExit(f"no shapes found in {svg!r}")
    return paths


def num(value):
    """A Rust float literal: `6` would be an integer and fail to compile."""
    text = f"{float(value):g}"
    return text if "." in text or "e" in text or "E" in text else text + ".0"


def emit_const(name, items):
    const = name.upper().replace("-", "_")
    out = [f"/// `{name}`", f"const {const}: &[Seg] = &["]
    for item in items:
        if item[0] == "line":
            (x1, y1), (x2, y2) = item[1], item[2]
            out.append(
                "    Seg::Line { "
                f"x1: {num(x1)}, y1: {num(y1)}, x2: {num(x2)}, y2: {num(y2)} "
                "},"
            )
        else:
            (cx, cy), r, a0, a1, large = item[1], item[2], item[3], item[4], item[5]
            lo, hi = sweep_pair(a0, a1, large)
            out.append(
                "    Seg::Arc { "
                f"cx: {num(cx)}, cy: {num(cy)}, r: {num(r)}, "
                f"start_deg: {lo:.2f}, end_deg: {hi:.2f} "
                "},"
            )
    out.append("];")
    return "\n".join(out)


def generate():
    blocks = []
    for name, svg in ICONS.items():
        items = []
        for d in svg_paths(svg):
            items.extend(parse(d))
        blocks.append(emit_const(name, items))
    header = (
        f"{BEGIN}\n"
        "// Regenerated by `scripts/gen-lucide-icons.py --write` from the Lucide\n"
        "// sources; run `cargo fmt -p terminus-ui` afterwards.\n"
    )
    return header + "\n" + "\n\n".join(blocks) + f"\n\n{END}"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--write",
        action="store_true",
        help=f"rewrite {TARGET.relative_to(ROOT)} in place (default: print)",
    )
    args = parser.parse_args()
    block = generate()

    if not args.write:
        print(block)
        return

    source = TARGET.read_text()
    start = source.find(BEGIN)
    stop = source.find(END)
    if start < 0 or stop < 0:
        sys.exit(f"{TARGET}: generated markers not found")
    TARGET.write_text(source[:start] + block + source[stop + len(END) :])
    print(f"wrote {TARGET.relative_to(ROOT)}; now run: cargo fmt -p terminus-ui")


if __name__ == "__main__":
    main()
