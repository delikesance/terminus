// Copyright (c) 2026-present, Terminus Contributors.
//! Lucide icon geometry, as real SVG path data.
//!
//! Icons are *vector* artwork: Lucide draws them as stroked outlines on a
//! 24x24 grid (2-unit stroke, round caps and round joins), never as
//! bitmaps. This module owns that geometry and nothing else — no colors, no
//! pixels — so it stays GPU-free and unit-testable, like the rest of
//! `terminus-ui`.
//!
//! How an icon reaches the screen:
//!
//! 1. [`Icon::d`] holds the icon's own path data, regenerated verbatim from
//!    the Lucide sources by `scripts/gen-lucide-icons.py --write`.
//! 2. [`commands`] parses it into absolute [`Cmd`]s on the 24x24 grid, arcs
//!    included — nothing is flattened and nothing is coarser than the
//!    source, which is what makes step 3 possible.
//! 3. The front end rasterizes those commands at the *device* size with
//!    `tiny-skia` (the vector rasterizer behind `resvg`) and hands the
//!    resulting coverage mask to `sugarloaf`, which samples it 1:1.
//!
//! Step 3 is why icons stay sharp. Painting the outline with primitives
//! means approximating a curve with quads whose edges land between pixels,
//! and no amount of snapping recovers the coverage they lose; a rasterized
//! mask carries the rasterizer's own sub-pixel coverage instead.

/// Lucide's artwork grid: every coordinate is in `0.0..=24.0`.
pub const LUCIDE_GRID: f32 = 24.0;

/// Lucide's stroke weight, in grid units (`24`-unit box → 2-unit stroke).
pub const LUCIDE_STROKE: f32 = 2.0;

/// The icons the chrome draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    Server,
    ArrowRightLeft,
    Folder,
    SlidersHorizontal,
    Plus,
    Monitor,
    SquareTerminal,
    Check,
    Globe,
    Lock,
    Minus,
    Square,
    Copy,
    X,
    Search,
    LayoutGrid,
    CodeXml,
    CloudUpload,
    Settings,
    KeyRound,
    Database,
    ChevronRight,
    ChevronDown,
    Eye,
    EyeOff,
}

impl Icon {
    /// Every icon, in the order the generator emits them.
    pub const ALL: [Icon; 25] = [
        Icon::Server,
        Icon::ArrowRightLeft,
        Icon::Folder,
        Icon::SlidersHorizontal,
        Icon::Plus,
        Icon::Monitor,
        Icon::SquareTerminal,
        Icon::Check,
        Icon::Globe,
        Icon::Lock,
        Icon::Minus,
        Icon::Square,
        Icon::Copy,
        Icon::X,
        Icon::Search,
        Icon::LayoutGrid,
        Icon::CodeXml,
        Icon::CloudUpload,
        Icon::Settings,
        Icon::KeyRound,
        Icon::Database,
        Icon::ChevronRight,
        Icon::ChevronDown,
        Icon::Eye,
        Icon::EyeOff,
    ];

    /// The icon's drawing elements: one SVG `d` per shape, verbatim from
    /// Lucide.
    ///
    /// They are kept apart rather than merged into one string because
    /// each element is drawn from the user-space origin — merging would
    /// silently re-anchor a relative moveto onto the previous element's
    /// end point.
    pub fn d(self) -> &'static [&'static str] {
        match self {
            Icon::Server => SERVER,
            Icon::ArrowRightLeft => ARROW_RIGHT_LEFT,
            Icon::Folder => FOLDER,
            Icon::SlidersHorizontal => SLIDERS_HORIZONTAL,
            Icon::Plus => PLUS,
            Icon::Monitor => MONITOR,
            Icon::SquareTerminal => SQUARE_TERMINAL,
            Icon::Check => CHECK,
            Icon::Globe => GLOBE,
            Icon::Lock => LOCK,
            Icon::Minus => MINUS,
            Icon::Square => SQUARE,
            Icon::Copy => COPY,
            Icon::X => X,
            Icon::Search => SEARCH,
            Icon::LayoutGrid => LAYOUT_GRID,
            Icon::CodeXml => CODE_XML,
            Icon::CloudUpload => CLOUD_UPLOAD,
            Icon::Settings => SETTINGS,
            Icon::KeyRound => KEY_ROUND,
            Icon::Database => DATABASE,
            Icon::ChevronRight => CHEVRON_RIGHT,
            Icon::ChevronDown => CHEVRON_DOWN,
            Icon::Eye => EYE,
            Icon::EyeOff => EYE_OFF,
        }
    }

    /// A key for this icon, stable within a process. Callers rasterizing
    /// into a cache or an atlas namespace their entries with it.
    pub fn id(self) -> u64 {
        self as u64
    }

    /// The icon's outline in one command list, flattened no further than
    /// SVG arcs — see [`commands`].
    pub fn path(self) -> Vec<Cmd> {
        subpaths(self.d())
    }
}

/// Parse a list of drawing elements into one command list.
///
/// Each element is parsed on its own, so its relative coordinates start
/// from the origin the way SVG says they do; the lists are then
/// concatenated, which is safe because every element opens with a
/// moveto.
pub fn subpaths(elements: &[&str]) -> Vec<Cmd> {
    elements.iter().flat_map(|d| commands(d)).collect()
}

/// One drawing command, absolute and in grid units.
///
/// This is SVG's vocabulary minus the convenience spellings: relative
/// commands, `H`/`V` and the smooth variants are all resolved by
/// [`commands`], so a consumer only has to understand four cases.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cmd {
    MoveTo {
        x: f32,
        y: f32,
    },
    LineTo {
        x: f32,
        y: f32,
    },
    /// Cubic Bézier: two control points, then the end point.
    ///
    /// SVG arcs arrive here as cubics — see [`commands`].
    CubicTo {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x: f32,
        y: f32,
    },
    /// Close the current subpath back to its `MoveTo`.
    Close,
}

/// Parse SVG path data into absolute [`Cmd`]s.
///
/// Handles the whole command set Lucide uses — `M L H V A C S Q T Z`, in
/// both cases, with SVG's implicit repetition (`m16 3 4 4-4 4` is a moveto
/// followed by two line-tos) and exponent notation. Quadratic and smooth
/// commands are converted to cubics, and arcs to cubic approximations of at
/// most 90 degrees each, so the output never needs a special case.
///
/// The input is generated by `scripts/gen-lucide-icons.py` and exercised by
/// the tests below; an unreadable command panics rather than silently
/// drawing the wrong shape.
pub fn commands(d: &str) -> Vec<Cmd> {
    let mut toks = tokenize(d);
    let mut out = Vec::new();
    let mut i = 0usize;

    // Current point, and the start of the current subpath (for `Z`).
    let mut cx = 0.0f32;
    let mut cy = 0.0f32;
    let (mut sx, mut sy) = (0.0f32, 0.0f32);

    // The verb whose argument group repeats while bare numbers keep coming.
    let mut repeating: Option<char> = None;
    // Control points for `S`/`T`, which reflect them. Cleared by any other
    // command, which is exactly SVG's rule for when reflection applies.
    let mut cubic_ctrl: Option<(f32, f32)> = None;
    let mut quad_ctrl: Option<(f32, f32)> = None;

    while i < toks.len() {
        let verb = match toks[i] {
            Tok::Cmd(c) => {
                i += 1;
                c
            }
            Tok::Num { .. } => match repeating {
                Some(c) => c,
                None => panic!("path data must open with a command: {d:?}"),
            },
        };
        let upper = verb.to_ascii_uppercase();
        let rel = verb.is_ascii_lowercase();
        let mut keep_ctrls = false;

        match upper {
            'M' => {
                let mut x = number(&toks, &mut i, d);
                let mut y = number(&toks, &mut i, d);
                if rel {
                    x += cx;
                    y += cy;
                }
                out.push(Cmd::MoveTo { x, y });
                cx = x;
                cy = y;
                sx = x;
                sy = y;
                // Extra pairs after a moveto are line-tos (SVG 1.1 8.3.2).
                repeating = Some(if rel { 'l' } else { 'L' });
            }
            'L' => {
                let mut x = number(&toks, &mut i, d);
                let mut y = number(&toks, &mut i, d);
                if rel {
                    x += cx;
                    y += cy;
                }
                out.push(Cmd::LineTo { x, y });
                cx = x;
                cy = y;
            }
            'H' => {
                let mut x = number(&toks, &mut i, d);
                if rel {
                    x += cx;
                }
                out.push(Cmd::LineTo { x, y: cy });
                cx = x;
            }
            'V' => {
                let mut y = number(&toks, &mut i, d);
                if rel {
                    y += cy;
                }
                out.push(Cmd::LineTo { x: cx, y });
                cy = y;
            }
            'A' => {
                let rx = number(&toks, &mut i, d);
                let ry = number(&toks, &mut i, d);
                let rot = number(&toks, &mut i, d);
                // SVG allows arc flags to run into the following coordinate
                // (`01-.5` or `012.5`). Peel 0/1 digits from the next token.
                let (large, sweep) = arc_flags(&mut toks, &mut i, d);
                let mut x = number(&toks, &mut i, d);
                let mut y = number(&toks, &mut i, d);
                if rel {
                    x += cx;
                    y += cy;
                }
                arc_to_cubics(&mut out, (cx, cy), rx, ry, rot, large, sweep, (x, y));
                cx = x;
                cy = y;
            }
            'C' => {
                let mut c1x = number(&toks, &mut i, d);
                let mut c1y = number(&toks, &mut i, d);
                let mut c2x = number(&toks, &mut i, d);
                let mut c2y = number(&toks, &mut i, d);
                let mut x = number(&toks, &mut i, d);
                let mut y = number(&toks, &mut i, d);
                if rel {
                    c1x += cx;
                    c1y += cy;
                    c2x += cx;
                    c2y += cy;
                    x += cx;
                    y += cy;
                }
                out.push(Cmd::CubicTo {
                    x1: c1x,
                    y1: c1y,
                    x2: c2x,
                    y2: c2y,
                    x,
                    y,
                });
                cubic_ctrl = Some((c2x, c2y));
                keep_ctrls = true;
                cx = x;
                cy = y;
            }
            'S' => {
                let (c1x, c1y) = reflect(cubic_ctrl, (cx, cy));
                let mut c2x = number(&toks, &mut i, d);
                let mut c2y = number(&toks, &mut i, d);
                let mut x = number(&toks, &mut i, d);
                let mut y = number(&toks, &mut i, d);
                if rel {
                    c2x += cx;
                    c2y += cy;
                    x += cx;
                    y += cy;
                }
                out.push(Cmd::CubicTo {
                    x1: c1x,
                    y1: c1y,
                    x2: c2x,
                    y2: c2y,
                    x,
                    y,
                });
                cubic_ctrl = Some((c2x, c2y));
                keep_ctrls = true;
                cx = x;
                cy = y;
            }
            'Q' => {
                let mut qx = number(&toks, &mut i, d);
                let mut qy = number(&toks, &mut i, d);
                let mut x = number(&toks, &mut i, d);
                let mut y = number(&toks, &mut i, d);
                if rel {
                    qx += cx;
                    qy += cy;
                    x += cx;
                    y += cy;
                }
                out.push(quad_to_cubic((cx, cy), (qx, qy), (x, y)));
                quad_ctrl = Some((qx, qy));
                keep_ctrls = true;
                cx = x;
                cy = y;
            }
            'T' => {
                let (qx, qy) = reflect(quad_ctrl, (cx, cy));
                let mut x = number(&toks, &mut i, d);
                let mut y = number(&toks, &mut i, d);
                if rel {
                    x += cx;
                    y += cy;
                }
                out.push(quad_to_cubic((cx, cy), (qx, qy), (x, y)));
                quad_ctrl = Some((qx, qy));
                keep_ctrls = true;
                cx = x;
                cy = y;
            }
            'Z' => {
                out.push(Cmd::Close);
                cx = sx;
                cy = sy;
                repeating = None;
            }
            other => panic!("unsupported SVG path command {other:?} in {d:?}"),
        }

        if !keep_ctrls {
            cubic_ctrl = None;
            quad_ctrl = None;
        }
        if upper != 'M' && upper != 'Z' {
            repeating = Some(verb);
        }
    }

    out
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Cmd(char),
    /// Parsed value plus the original substring so arc flags can peel
    /// leading `0`/`1` digits without losing glued coordinates (`012.5`).
    Num {
        value: f32,
        raw: String,
    },
}

/// Split path data into command letters and numbers.
///
/// Numbers may run together without separators (`4 4-4 4`), and may use a
/// leading `.` or an exponent — SVG allows all of it.
fn tokenize(d: &str) -> Vec<Tok> {
    let bytes = d.as_bytes();
    let mut toks = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_whitespace() || c == ',' {
            i += 1;
            continue;
        }
        if c.is_ascii_alphabetic() {
            toks.push(Tok::Cmd(c));
            i += 1;
            continue;
        }

        let start = i;
        if c == '+' || c == '-' {
            i += 1;
        }
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        }
        if i < bytes.len() && (bytes[i] | 0x20) == b'e' {
            i += 1;
            if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
                i += 1;
            }
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        }
        let raw = d[start..i].to_string();
        match raw.parse::<f32>() {
            Ok(value) => toks.push(Tok::Num { value, raw }),
            Err(_) => panic!("unreadable number {raw:?} in {d:?}"),
        }
    }

    toks
}

fn number(toks: &[Tok], i: &mut usize, d: &str) -> f32 {
    match toks.get(*i) {
        Some(Tok::Num { value, .. }) => {
            *i += 1;
            *value
        }
        other => panic!("expected a number, found {other:?} in {d:?}"),
    }
}

/// Read one SVG arc flag (`0` or `1`), peeling a glued digit when needed.
fn take_arc_flag(toks: &mut Vec<Tok>, i: &mut usize, d: &str) -> bool {
    let Some(Tok::Num { value, raw }) = toks.get(*i).cloned() else {
        panic!("expected an arc flag in {d:?}");
    };
    let bytes = raw.as_bytes();
    match bytes.first().copied() {
        Some(b @ (b'0' | b'1')) => {
            let flag = b == b'1';
            let rest = &raw[1..];
            if rest.is_empty() {
                *i += 1;
            } else {
                match rest.parse::<f32>() {
                    Ok(v) => {
                        toks[*i] = Tok::Num {
                            value: v,
                            raw: rest.to_string(),
                        };
                    }
                    Err(_) => panic!("bad remainder {rest:?} after arc flag in {d:?}"),
                }
            }
            flag
        }
        _ if value == 0.0 => {
            *i += 1;
            false
        }
        _ if value == 1.0 => {
            *i += 1;
            true
        }
        _ => panic!("expected an arc flag, found {raw:?} in {d:?}"),
    }
}

/// Read the two SVG elliptical-arc flags (`large-arc`, `sweep`).
///
/// Flags are only `0`/`1` and may be glued to the next coordinate without a
/// separator (`a 10 10 0 012.5 0` ≡ large=0 sweep=1 x=2.5). When the next
/// token is a multi-digit number starting with flag digits, peel them off
/// and leave the remainder as the next number token.
fn arc_flags(toks: &mut Vec<Tok>, i: &mut usize, d: &str) -> (bool, bool) {
    let large = take_arc_flag(toks, i, d);
    let sweep = take_arc_flag(toks, i, d);
    (large, sweep)
}

/// Mirror `ctrl` through `current` — SVG's rule for `S` and `T`.
///
/// Callers clear the stored control point whenever the previous command was
/// not the matching kind, so a `None` here already means "use the current
/// point".
fn reflect(ctrl: Option<(f32, f32)>, current: (f32, f32)) -> (f32, f32) {
    match ctrl {
        Some((x, y)) => (2.0 * current.0 - x, 2.0 * current.1 - y),
        None => current,
    }
}

/// A quadratic Bézier as an equivalent cubic — an exact conversion, not an
/// approximation.
fn quad_to_cubic(from: (f32, f32), control: (f32, f32), to: (f32, f32)) -> Cmd {
    let mix = |a: f32, b: f32| a + 2.0 / 3.0 * (b - a);
    Cmd::CubicTo {
        x1: mix(from.0, control.0),
        y1: mix(from.1, control.1),
        x2: mix(to.0, control.0),
        y2: mix(to.1, control.1),
        x: to.0,
        y: to.1,
    }
}

/// Append the cubic approximation of an SVG elliptical arc.
///
/// Endpoint to centre parameterization per SVG 1.1 F.6.5, then the standard
/// `4/3 * tan(sweep/4)` control-point construction, on sub-arcs of at most
/// 90 degrees so the error stays far below a pixel at any icon size. This is
/// what lets the rasterizer stay a plain Bézier rasterizer — `tiny-skia`
/// strokes cubics but has no arc primitive.
#[allow(clippy::too_many_arguments)]
fn arc_to_cubics(
    out: &mut Vec<Cmd>,
    from: (f32, f32),
    rx: f32,
    ry: f32,
    rot_deg: f32,
    large: bool,
    sweep: bool,
    to: (f32, f32),
) {
    // A zero radius is a straight line, by definition (SVG 1.1 F.6.2).
    if rx == 0.0 || ry == 0.0 || from == to {
        out.push(Cmd::LineTo { x: to.0, y: to.1 });
        return;
    }

    let (rx, ry) = (rx.abs(), ry.abs());
    let phi = rot_deg.to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();

    // F.6.5.1 — the endpoint delta in the ellipse's own frame.
    let (dx, dy) = ((from.0 - to.0) / 2.0, (from.1 - to.1) / 2.0);
    let x1p = cos_phi * dx + sin_phi * dy;
    let y1p = -sin_phi * dx + cos_phi * dy;

    // F.6.5.2 — scale up radii that are too small to span the endpoints.
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    let (rx, ry) = if lambda > 1.0 {
        let s = lambda.sqrt();
        (rx * s, ry * s)
    } else {
        (rx, ry)
    };

    // F.6.5.2 — centre, in the ellipse's frame then rotated back.
    let sign = if large == sweep { -1.0 } else { 1.0 };
    let numerator =
        (rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p).max(0.0);
    let denominator = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let coef = sign * (numerator / denominator).sqrt();
    let (cxp, cyp) = (coef * rx * y1p / ry, -coef * ry * x1p / rx);
    let centre = (
        cos_phi * cxp - sin_phi * cyp + (from.0 + to.0) / 2.0,
        sin_phi * cxp + cos_phi * cyp + (from.1 + to.1) / 2.0,
    );

    // F.6.5.5 / F.6.5.6 — start angle and sweep, normalised so a `sweep`
    // arc always turns positively and the centre landing above is the one
    // the flags asked for.
    let angle = |ux: f32, uy: f32, vx: f32, vy: f32| -> f32 {
        let dot = ux * vx + uy * vy;
        let len = ((ux * ux + uy * uy) * (vx * vx + vy * vy)).sqrt();
        let mut a = (dot / len).clamp(-1.0, 1.0).acos();
        if ux * vy - uy * vx < 0.0 {
            a = -a;
        }
        a
    };
    let u = ((x1p - cxp) / rx, (y1p - cyp) / ry);
    let v = ((-x1p - cxp) / rx, (-y1p - cyp) / ry);
    let theta = angle(1.0, 0.0, u.0, u.1);
    let mut sweep_angle = angle(u.0, u.1, v.0, v.1);
    if !sweep && sweep_angle > 0.0 {
        sweep_angle -= 2.0 * std::f32::consts::PI;
    } else if sweep && sweep_angle < 0.0 {
        sweep_angle += 2.0 * std::f32::consts::PI;
    }

    // A point and its tangent, both in device/window space.
    let point = |t: f32| {
        let (sin_t, cos_t) = t.sin_cos();
        (
            centre.0 + cos_phi * rx * cos_t - sin_phi * ry * sin_t,
            centre.1 + sin_phi * rx * cos_t + cos_phi * ry * sin_t,
        )
    };
    let tangent = |t: f32| {
        let (sin_t, cos_t) = t.sin_cos();
        (
            -cos_phi * rx * sin_t - sin_phi * ry * cos_t,
            -sin_phi * rx * sin_t + cos_phi * ry * cos_t,
        )
    };

    let steps = (sweep_angle.abs() / (std::f32::consts::FRAC_PI_2))
        .ceil()
        .max(1.0) as usize;
    let step = sweep_angle / steps as f32;
    let k = 4.0 / 3.0 * (step / 4.0).tan();

    let mut t0 = theta;
    let mut p0 = from;
    for _ in 0..steps {
        let t1 = t0 + step;
        let p1 = point(t1);
        let (d0x, d0y) = tangent(t0);
        let (d1x, d1y) = tangent(t1);
        out.push(Cmd::CubicTo {
            x1: p0.0 + k * d0x,
            y1: p0.1 + k * d0y,
            x2: p1.0 - k * d1x,
            y2: p1.1 - k * d1y,
            x: p1.0,
            y: p1.1,
        });
        t0 = t1;
        p0 = p1;
    }

    // Land exactly on the requested endpoint rather than wherever the
    // parameterization rounded to.
    if let Some(Cmd::CubicTo { x, y, .. }) = out.last_mut() {
        *x = to.0;
        *y = to.1;
    }
}

/// Where an icon goes and how big it is, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IconPlacement {
    /// Top-left of the icon's box.
    pub x: f32,
    pub y: f32,
    /// Side of the box. Lucide artwork is square.
    pub size: f32,
}

impl IconPlacement {
    pub fn new(x: f32, y: f32, size: f32) -> Self {
        Self { x, y, size }
    }

    /// Side of the coverage mask to rasterize for this box, in device
    /// pixels.
    ///
    /// The mask is drawn 1:1 against the device grid, so its side has to
    /// be a whole number of device pixels; the artwork inside it is then
    /// scaled to fill that side exactly, which keeps the stroke weight
    /// and the mask in step. Never zero, so a degenerate size still
    /// rasterizes something.
    pub fn device_size(&self, device_scale: f32) -> u16 {
        (self.size * device_scale).round().max(1.0) as u16
    }

    /// A grid point in logical window coordinates.
    pub fn map(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.x + x * self.size / LUCIDE_GRID,
            self.y + y * self.size / LUCIDE_GRID,
        )
    }

    /// Device pixels per grid unit for a mask `device_size` wide.
    pub fn unit(device_size: u16) -> f32 {
        device_size as f32 / LUCIDE_GRID
    }
}

// ---- generated: do not edit ----
// Regenerated by `scripts/gen-lucide-icons.py --write` from the Lucide
// sources; run `cargo fmt -p terminus-ui` afterwards.

/// `server`
const SERVER: &[&str] = &[
    "M4 2H20A2 2 0 0 1 22 4V8A2 2 0 0 1 20 10H4A2 2 0 0 1 2 8V4A2 2 0 0 1 4 2Z",
    "M4 14H20A2 2 0 0 1 22 16V20A2 2 0 0 1 20 22H4A2 2 0 0 1 2 20V16A2 2 0 0 1 4 14Z",
    "M6 6L6.01 6",
    "M6 18L6.01 18",
];

/// `arrow-right-left`
const ARROW_RIGHT_LEFT: &[&str] =
    &["m16 3 4 4-4 4", "M20 7H4", "m8 21-4-4 4-4", "M4 17h16"];

/// `folder`
const FOLDER: &[&str] = &[
    "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z",
];

/// `sliders-horizontal`
const SLIDERS_HORIZONTAL: &[&str] = &[
    "M10 5H3",
    "M12 19H3",
    "M14 3v4",
    "M16 17v4",
    "M21 12h-9",
    "M21 19h-5",
    "M21 5h-7",
    "M8 10v4",
    "M8 12H3",
];

/// `plus`
const PLUS: &[&str] = &["M5 12h14", "M12 5v14"];

/// `monitor`
const MONITOR: &[&str] = &[
    "M4 3H20A2 2 0 0 1 22 5V15A2 2 0 0 1 20 17H4A2 2 0 0 1 2 15V5A2 2 0 0 1 4 3Z",
    "M8 21L16 21",
    "M12 17L12 21",
];

/// `square-terminal`
const SQUARE_TERMINAL: &[&str] = &[
    "m7 11 2-2-2-2",
    "M11 13h4",
    "M5 3H19A2 2 0 0 1 21 5V19A2 2 0 0 1 19 21H5A2 2 0 0 1 3 19V5A2 2 0 0 1 5 3Z",
];

/// `check`
const CHECK: &[&str] = &["M20 6 9 17l-5-5"];

/// `globe`
const GLOBE: &[&str] = &[
    "M2 12a10 10 0 1 0 20 0a10 10 0 1 0 -20 0Z",
    "M12 2a14.5 14.5 0 0 0 0 20 14.5 14.5 0 0 0 0-20",
    "M2 12h20",
];

/// `lock`
const LOCK: &[&str] = &[
    "M5 11H19A2 2 0 0 1 21 13V20A2 2 0 0 1 19 22H5A2 2 0 0 1 3 20V13A2 2 0 0 1 5 11Z",
    "M7 11V7a5 5 0 0 1 10 0v4",
];

/// `minus`
const MINUS: &[&str] = &["M5 12h14"];

/// `square`
const SQUARE: &[&str] =
    &["M5 3H19A2 2 0 0 1 21 5V19A2 2 0 0 1 19 21H5A2 2 0 0 1 3 19V5A2 2 0 0 1 5 3Z"];

/// `copy`
const COPY: &[&str] = &[
    "M10 8H20A2 2 0 0 1 22 10V20A2 2 0 0 1 20 22H10A2 2 0 0 1 8 20V10A2 2 0 0 1 10 8Z",
    "M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2",
];

/// `x`
const X: &[&str] = &["M18 6 6 18", "m6 6 12 12"];

/// `search`
const SEARCH: &[&str] = &["M3 11a8 8 0 1 0 16 0a8 8 0 1 0 -16 0Z", "m21 21-4.3-4.3"];

/// `layout-grid`
const LAYOUT_GRID: &[&str] = &[
    "M4 3H9A1 1 0 0 1 10 4V9A1 1 0 0 1 9 10H4A1 1 0 0 1 3 9V4A1 1 0 0 1 4 3Z",
    "M15 3H20A1 1 0 0 1 21 4V9A1 1 0 0 1 20 10H15A1 1 0 0 1 14 9V4A1 1 0 0 1 15 3Z",
    "M15 14H20A1 1 0 0 1 21 15V20A1 1 0 0 1 20 21H15A1 1 0 0 1 14 20V15A1 1 0 0 1 15 14Z",
    "M4 14H9A1 1 0 0 1 10 15V20A1 1 0 0 1 9 21H4A1 1 0 0 1 3 20V15A1 1 0 0 1 4 14Z",
];

/// `code-xml`
const CODE_XML: &[&str] = &["m18 16 4-4-4-4", "m6 8-4 4 4 4", "m14.5 4-5 16"];

/// `cloud-upload`
const CLOUD_UPLOAD: &[&str] = &[
    "M12 13v8",
    "M4 14.899A7 7 0 1 1 15.71 8h1.79a4.5 4.5 0 0 1 2.5 8.242",
    "m8 17 4-4 4 4",
];

/// `settings`
const SETTINGS: &[&str] = &[
    "M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915",
    "M9 12a3 3 0 1 0 6 0a3 3 0 1 0 -6 0Z",
];

/// `key-round`
const KEY_ROUND: &[&str] = &[
    "M2.586 17.414A2 2 0 0 0 2 18.828V21a1 1 0 0 0 1 1h3a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h1a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h.172a2 2 0 0 0 1.414-.586l.814-.814a6.5 6.5 0 1 0-4-4z",
    "M16 7.5a0.5 0.5 0 1 0 1 0a0.5 0.5 0 1 0 -1 0Z",
];

/// `database`
const DATABASE: &[&str] = &[
    "M12 5c-4.97 0-9 1.34-9 3v11c0 1.66 4.03 3 9 3s9-1.34 9-3V8c0-1.66-4.03-3-9-3",
    "M3 8c0 1.66 4.03 3 9 3s9-1.34 9-3",
    "M3 13c0 1.66 4.03 3 9 3s9-1.34 9-3",
];

/// `chevron-right`
const CHEVRON_RIGHT: &[&str] = &["m9 18 6-6-6-6"];

/// `chevron-down`
const CHEVRON_DOWN: &[&str] = &["m6 9 6 6 6-6"];

/// `eye`
const EYE: &[&str] = &[
    "M2.062 12.348a1 1 0 0 1 0-.696 10.75 10.75 0 0 1 19.876 0 1 1 0 0 1 0 .696 10.75 10.75 0 0 1-19.876 0",
    "M9 12a3 3 0 1 0 6 0a3 3 0 1 0 -6 0Z",
];

/// `eye-off`
const EYE_OFF: &[&str] = &[
    "M10.733 5.076a10.744 10.744 0 0 1 11.205 6.575 1 1 0 0 1 0 .696 10.747 10.747 0 0 1-1.444 2.49",
    "M14.084 14.158a3 3 0 0 1-4.242-4.242",
    "M17.479 17.499a10.75 10.75 0 0 1-15.417-5.151 1 1 0 0 1 0-.696 10.75 10.75 0 0 1 4.446-5.143",
    "m2 2 20 20",
];

// ---- end generated ----

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn every_icon_has_path_data() {
        for icon in Icon::ALL {
            assert!(!icon.d().is_empty(), "{icon:?} has no drawing elements");
            for d in icon.d() {
                assert!(!d.trim().is_empty(), "{icon:?} has an empty element");
            }
        }
    }

    /// `arrow-right-left` is four elements and two of them open with a
    /// relative moveto. Merged into a single path string, the second
    /// moveto would be measured from the previous element's end point and
    /// the arrow would leave its own 24x24 grid.
    #[test]
    fn elements_keep_their_own_origin() {
        let arrow = Icon::ArrowRightLeft.path();
        assert!(
            arrow.contains(&Cmd::MoveTo { x: 16.0, y: 3.0 }),
            "`m16 3 4 4-4 4` lost its origin: {arrow:?}"
        );
        assert!(
            arrow.contains(&Cmd::MoveTo { x: 8.0, y: 21.0 }),
            "`m8 21-4-4 4-4` lost its origin: {arrow:?}"
        );
    }

    #[test]
    fn every_icon_parses_and_draws() {
        for icon in Icon::ALL {
            let path = icon.path();
            assert!(
                path.iter()
                    .any(|c| matches!(c, Cmd::MoveTo { .. } | Cmd::LineTo { .. })),
                "{icon:?} parsed to no drawable commands: {path:?}"
            );
        }
    }

    #[test]
    fn icon_ids_are_distinct() {
        for (i, icon) in Icon::ALL.iter().enumerate() {
            for other in &Icon::ALL[i + 1..] {
                assert_ne!(icon.id(), other.id(), "{icon:?} and {other:?} share an id");
            }
        }
    }

    /// Nothing may leave the 24x24 box, or `IconPlacement` would clip it.
    #[test]
    fn paths_stay_inside_the_lucide_grid() {
        for icon in Icon::ALL {
            for cmd in icon.path() {
                let (x, y) = match cmd {
                    Cmd::MoveTo { x, y } | Cmd::LineTo { x, y } => (x, y),
                    Cmd::CubicTo { x, y, .. } => (x, y),
                    Cmd::Close => continue,
                };
                assert!(
                    (-0.001..=LUCIDE_GRID + 0.001).contains(&x)
                        && (-0.001..=LUCIDE_GRID + 0.001).contains(&y),
                    "{icon:?} leaves the grid at ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn absolute_commands_are_read_directly() {
        assert_eq!(
            commands("M5 12h14"),
            vec![
                Cmd::MoveTo { x: 5.0, y: 12.0 },
                Cmd::LineTo { x: 19.0, y: 12.0 }
            ]
        );
    }

    /// `m16 3 4 4-4 4`: a moveto and two implicit relative line-tos, with
    /// no separator before the negative coordinate.
    #[test]
    fn implied_commands_and_relative_offsets() {
        assert_eq!(
            commands("m16 3 4 4-4 4"),
            vec![
                Cmd::MoveTo { x: 16.0, y: 3.0 },
                Cmd::LineTo { x: 20.0, y: 7.0 },
                Cmd::LineTo { x: 16.0, y: 11.0 },
            ]
        );
    }

    #[test]
    fn horizontal_and_vertical_reach_the_current_point() {
        assert_eq!(
            commands("M4 2H20V14"),
            vec![
                Cmd::MoveTo { x: 4.0, y: 2.0 },
                Cmd::LineTo { x: 20.0, y: 2.0 },
                Cmd::LineTo { x: 20.0, y: 14.0 },
            ]
        );
    }

    #[test]
    fn close_returns_to_the_subpath_start() {
        assert_eq!(
            commands("M2 2H4V4Z l0 2"),
            vec![
                Cmd::MoveTo { x: 2.0, y: 2.0 },
                Cmd::LineTo { x: 4.0, y: 2.0 },
                Cmd::LineTo { x: 4.0, y: 4.0 },
                Cmd::Close,
                // After `Z` the current point is the subpath start, so `l0
                // 2` lands two units below it.
                Cmd::LineTo { x: 2.0, y: 4.0 },
            ]
        );
    }

    /// A quarter circle of radius 10 from (2, 12) to (12, 2) — one cubic,
    /// whose control point sits `4/3 * tan(pi/8) * 10` along the start
    /// tangent, which points at the top of the circle.
    #[test]
    fn arcs_become_cubics_with_the_right_tangents() {
        let path = commands("M2 12a10 10 0 0 1 10-10");
        assert_eq!(path.len(), 2, "expected a moveto and one cubic: {path:?}");
        let Cmd::CubicTo {
            x1,
            y1,
            x2,
            y2,
            x,
            y,
        } = path[1]
        else {
            panic!("expected a cubic: {path:?}")
        };
        assert!(close(x, 12.0) && close(y, 2.0), "ends at ({x}, {y})");
        let k = 4.0 / 3.0 * (std::f32::consts::FRAC_PI_8).tan() * 10.0;
        assert!(close(x1, 2.0), "control point drifted in x: {x1}");
        assert!(close(y1, 12.0 - k), "control point drifted in y: {y1}");
        // The far control point sits the same distance back along the
        // tangent at the end point, which there runs horizontally.
        assert!(close(x2, 12.0 - k), "control point drifted in x: {x2}");
        assert!(close(y2, 2.0), "control point drifted in y: {y2}");
    }

    #[test]
    fn arcs_are_split_so_no_segment_turns_more_than_ninety_degrees() {
        // A half circle is two quarter-arc cubics.
        let path = commands("M2 12a10 10 0 1 1 20 0");
        let cubics = path
            .iter()
            .filter(|c| matches!(c, Cmd::CubicTo { .. }))
            .count();
        assert_eq!(cubics, 2, "{path:?}");
        assert!(
            matches!(path.last(), Some(Cmd::CubicTo { x, y, .. }) if close(*x, 22.0) && close(*y, 12.0))
        );
    }

    #[test]
    fn a_zero_radius_arc_is_a_line() {
        assert_eq!(
            commands("M2 2A0 0 0 0 1 4 4"),
            vec![
                Cmd::MoveTo { x: 2.0, y: 2.0 },
                Cmd::LineTo { x: 4.0, y: 4.0 }
            ]
        );
    }

    /// Lucide draws its "LED" as a sub-pixel line that only exists because
    /// of round caps — `M6 6L6.01 6`.
    #[test]
    fn degenerate_lines_survive_parsing() {
        assert_eq!(
            commands("M6 6L6.01 6"),
            vec![
                Cmd::MoveTo { x: 6.0, y: 6.0 },
                Cmd::LineTo { x: 6.01, y: 6.0 }
            ]
        );
    }

    #[test]
    fn quadratics_are_exact_cubics() {
        assert_eq!(
            commands("M0 0Q6 6 12 0"),
            vec![
                Cmd::MoveTo { x: 0.0, y: 0.0 },
                Cmd::CubicTo {
                    x1: 4.0,
                    y1: 4.0,
                    x2: 8.0,
                    y2: 4.0,
                    x: 12.0,
                    y: 0.0,
                },
            ]
        );
    }

    #[test]
    fn smooth_cubics_reflect_the_previous_control_point() {
        let path = commands("M0 0C0 4 4 4 4 0S8-4 8 0");
        assert_eq!(path.len(), 3);
        match path[2] {
            Cmd::CubicTo { x1, y1, .. } => {
                assert!(close(x1, 4.0) && close(y1, -4.0), "({x1}, {y1})")
            }
            other => panic!("expected a cubic: {other:?}"),
        }
    }

    #[test]
    fn arc_flags_may_glue_to_the_next_coordinate() {
        // `01-.99` → large=0 sweep=1 x=-0.99
        let path = commands("M2 12a3 3 0 01-.99.05");
        assert!(path.len() >= 2, "{path:?}");
        // `012.5` → large=0 sweep=1 x=2.5
        let path = commands("M0 0a10 10 0 012.5 0");
        assert!(path.len() >= 2, "{path:?}");
        // Separated flags still work.
        let path = commands("M2 12a10 10 0 0 1 10-10");
        assert!(path.len() >= 2, "{path:?}");
    }

    #[test]
    fn exponent_notation_and_leading_dots_parse() {
        assert_eq!(
            commands("M.5 .5L1e0 2"),
            vec![
                Cmd::MoveTo { x: 0.5, y: 0.5 },
                Cmd::LineTo { x: 1.0, y: 2.0 }
            ]
        );
    }

    #[test]
    fn placement_maps_the_grid_onto_the_box() {
        let placement = IconPlacement::new(100.0, 40.0, 20.0);
        assert_eq!(placement.map(0.0, 0.0), (100.0, 40.0));
        assert_eq!(placement.map(LUCIDE_GRID, LUCIDE_GRID), (120.0, 60.0));
        // The centre of the grid is the centre of the box.
        assert_eq!(placement.map(12.0, 12.0), (110.0, 50.0));
    }

    /// The mask is rasterized against the device grid, so its side is the
    /// box's logical size taken up to device pixels.
    #[test]
    fn placement_asks_for_a_whole_device_pixel_mask() {
        let placement = IconPlacement::new(0.0, 0.0, 20.0);
        assert_eq!(placement.device_size(1.0), 20);
        assert_eq!(placement.device_size(2.0), 40);
        // 1.25x is a real WSLg/Windows scale factor: 20 -> 25 exactly.
        assert_eq!(placement.device_size(1.25), 25);
        // Never zero, however small the box is scaled down.
        assert_eq!(IconPlacement::new(0.0, 0.0, 2.0).device_size(0.1), 1);
    }

    /// The mask's side therefore sets the artwork's unit — and Lucide's
    /// 2/24 stroke has to come out of that, not out of the logical size,
    /// or the outline would be rasterized at the wrong weight.
    #[test]
    fn unit_puts_the_lucide_grid_on_the_mask() {
        assert!(close(IconPlacement::unit(24), 1.0));
        assert!(close(IconPlacement::unit(48), 2.0));
        // 20px is a whole number of pixels for a 20/24-unit stroke: 2/24 of
        // the grid is exactly 20/12 px, so the mask keeps real coverage.
        assert!(close(LUCIDE_STROKE * IconPlacement::unit(20), 20.0 / 12.0));
    }
}
