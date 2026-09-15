#!/usr/bin/env python3
"""Regenerate the Lucide icon path data in `crates/terminus-ui/src/icons.rs`.

Lucide icons are SVG *outlines*: a 24x24 grid, stroked (never filled) with
a 2-unit width, round caps and round joins. This script keeps their real
path data — `path`/`line`/`rect`/`circle`/`polyline`/`polygon` markup,
verbatim from the Lucide sources (MIT) — normalized into one SVG path
string per icon.

Nothing is flattened here. The path is rasterized at the exact device size
by `terminus-ui`'s consumer (the rio front end, through `tiny-skia`), so an
icon keeps its vector edges and its anti-aliasing at every size instead of
being approximated with pixel-snapped quads.

The SVG source of each icon is kept inline in `ICONS` below, verbatim from
https://github.com/lucide-icons/lucide (MIT). Adding an icon means pasting
its markup here, adding a variant to `Icon` in icons.rs, and running:

    scripts/gen-lucide-icons.py --write
    cargo fmt -p terminus-ui

Without `--write` the block is printed to stdout for inspection.
"""

import argparse
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
    "monitor": '<rect width="20" height="14" x="2" y="3" rx="2"/>'
    '<line x1="8" x2="16" y1="21" y2="21"/>'
    '<line x1="12" x2="12" y1="17" y2="21"/>',
    "square-terminal": '<path d="m7 11 2-2-2-2"/><path d="M11 13h4"/>'
    '<rect width="18" height="18" x="3" y="3" rx="2" ry="2"/>',
    "check": '<path d="M20 6 9 17l-5-5"/>',
    "globe": '<circle cx="12" cy="12" r="10"/>'
    '<path d="M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20"/>'
    '<path d="M2 12h20"/>',
    "lock": '<rect width="18" height="11" x="3" y="11" rx="2" ry="2"/>'
    '<path d="M7 11V7a5 5 0 0 1 10 0v4"/>',
    "minus": '<path d="M5 12h14"/>',
    "square": '<rect width="18" height="18" x="3" y="3" rx="2"/>',
    "copy": '<rect width="14" height="14" x="8" y="8" rx="2" ry="2"/>'
    '<path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>',
    "x": '<path d="M18 6 6 18"/><path d="m6 6 12 12"/>',
    "search": '<circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/>',
    "layout-grid": '<rect width="7" height="7" x="3" y="3" rx="1"/>'
    '<rect width="7" height="7" x="14" y="3" rx="1"/>'
    '<rect width="7" height="7" x="14" y="14" rx="1"/>'
    '<rect width="7" height="7" x="3" y="14" rx="1"/>',
    "code-xml": '<path d="m18 16 4-4-4-4"/><path d="m6 8-4 4 4 4"/><path d="m14.5 4-5 16"/>',
    "cloud-upload": '<path d="M12 13v8"/><path d="M4 14.899A7 7 0 1 1 15.71 8h1.79a4.5 4.5 0 0 1 2.5 8.242"/><path d="m8 17 4-4 4 4"/>',
    "settings": '<path d="M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915"/><circle cx="12" cy="12" r="3"/>',
    "key-round": '<path d="M2.586 17.414A2 2 0 0 0 2 18.828V21a1 1 0 0 0 1 1h3a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h1a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h.172a2 2 0 0 0 1.414-.586l.814-.814a6.5 6.5 0 1 0-4-4z"/><circle cx="16.5" cy="7.5" r=".5" fill="currentColor"/>',
    "database": '<path d="M12 5c-4.97 0-9 1.34-9 3v11c0 1.66 4.03 3 9 3s9-1.34 9-3V8c0-1.66-4.03-3-9-3"/><path d="M3 8c0 1.66 4.03 3 9 3s9-1.34 9-3"/><path d="M3 13c0 1.66 4.03 3 9 3s9-1.34 9-3"/>',
    "chevron-right": '<path d="m9 18 6-6-6-6"/>',
    "chevron-down": '<path d="m6 9 6 6 6-6"/>',
}

ELEMENT_RE = re.compile(r"<(\w+)((?:\s+[\w:-]+=\"[^\"]*\")*)\s*/?>")
ATTR_RE = re.compile(r"([\w:-]+)=\"([^\"]*)\"")

# Elements that carry no geometry of their own.
IGNORED_TAGS = {"svg", "g", "title", "desc", "defs", "style", "metadata"}


def attrs_of(raw):
    return dict(ATTR_RE.findall(raw))


def number(attrs, key, default=0.0):
    try:
        return float(attrs.get(key, default))
    except ValueError:
        raise SystemExit(f"{key}={attrs[key]!r} is not a number")


def trim(value):
    """Shortest exact text for a coordinate — SVG accepts `6` and `6.01`."""
    text = f"{value:g}"
    return text


def line_path(attrs):
    return (
        f"M{trim(number(attrs, 'x1'))} {trim(number(attrs, 'y1'))}"
        f"L{trim(number(attrs, 'x2'))} {trim(number(attrs, 'y2'))}"
    )


def rect_path(attrs):
    """A rounded rect as an SVG path, so one parser handles every shape."""
    x, y = number(attrs, "x"), number(attrs, "y")
    w, h = number(attrs, "width"), number(attrs, "height")
    rx = float(attrs.get("rx") or attrs.get("ry") or 0.0)
    if rx <= 0.0:
        return (
            f"M{trim(x)} {trim(y)}H{trim(x + w)}V{trim(y + h)}"
            f"H{trim(x)}Z"
        )
    return (
        f"M{trim(x + rx)} {trim(y)}H{trim(x + w - rx)}"
        f"A{trim(rx)} {trim(rx)} 0 0 1 {trim(x + w)} {trim(y + rx)}"
        f"V{trim(y + h - rx)}"
        f"A{trim(rx)} {trim(rx)} 0 0 1 {trim(x + w - rx)} {trim(y + h)}"
        f"H{trim(x + rx)}"
        f"A{trim(rx)} {trim(rx)} 0 0 1 {trim(x)} {trim(y + h - rx)}"
        f"V{trim(y + rx)}"
        f"A{trim(rx)} {trim(rx)} 0 0 1 {trim(x + rx)} {trim(y)}Z"
    )


def circle_path(attrs):
    cx, cy, r = number(attrs, "cx"), number(attrs, "cy"), number(attrs, "r")
    return (
        f"M{trim(cx - r)} {trim(cy)}"
        f"a{trim(r)} {trim(r)} 0 1 0 {trim(2 * r)} 0"
        f"a{trim(r)} {trim(r)} 0 1 0 {trim(-2 * r)} 0Z"
    )


def poly_path(attrs, close):
    points = [p for p in re.split(r"[\s,]+", attrs.get("points", "").strip()) if p]
    if len(points) % 2:
        raise SystemExit(f"odd point count in {attrs.get('points')!r}")
    pairs = list(zip(points[0::2], points[1::2]))
    out = [f"M{pairs[0][0]} {pairs[0][1]}"]
    out += [f"L{x} {y}" for x, y in pairs[1:]]
    if close:
        out.append("Z")
    return "".join(out)


def svg_paths(svg):
    """Every drawable in a Lucide `<svg>` body, as SVG path data."""
    paths = []
    for tag, raw in ELEMENT_RE.findall(svg):
        if tag in IGNORED_TAGS:
            continue
        attrs = attrs_of(raw)
        if tag == "path":
            paths.append(attrs["d"])
        elif tag == "line":
            paths.append(line_path(attrs))
        elif tag in ("rect", "square"):
            paths.append(rect_path(attrs))
        elif tag == "circle":
            paths.append(circle_path(attrs))
        elif tag == "polyline":
            paths.append(poly_path(attrs, close=False))
        elif tag == "polygon":
            paths.append(poly_path(attrs, close=True))
        else:
            raise SystemExit(f"unhandled SVG element <{tag}> in {svg!r}")
    if not paths:
        raise SystemExit(f"no shapes found in {svg!r}")
    return paths


def emit_const(name, paths):
    """One icon as a list of subpaths, one per drawing element.

    The list is kept flat on purpose: each element of a Lucide `<svg>` is
    drawn from the user-space origin, so a `d` that opens with a relative
    moveto (`m8 21-4-4 4-4` in `arrow-right-left`) means "start at the
    origin plus this offset", and the SVG rule says the pairs that follow
    stay relative to where it lands. Concatenating elements into a single
    path string throws that origin away — the moveto would become
    relative to the previous element's end instead. Parsing each element
    on its own is what keeps the outline where Lucide put it.
    """
    const = name.upper().replace("-", "_")
    for d in paths:
        if '"' in d or "\\" in d:
            raise SystemExit(f"{name}: path data needs escaping: {d!r}")
    body = "".join(f'\n    "{d}",' for d in paths)
    return f"/// `{name}`\nconst {const}: &[&str] = &[{body}\n];"


def generate():
    blocks = [emit_const(name, svg_paths(svg)) for name, svg in ICONS.items()]
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
