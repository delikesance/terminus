//! Feedback components: status dot, connection steps, progress bar, toast.
//!
//! Geometry comes from `terminus_ui::components::feedback`; this file only
//! paints it, plus the gallery section.

use crate::renderer::chrome::{draw_icon, paint_surface_stroke};
use crate::renderer::ui_text::{draw_ui_text, measure_ui_text, UiWeight};
use rio_backend::sugarloaf::Sugarloaf;
use std::time::Duration;
use terminus_ui::components::feedback::*;
use terminus_ui::connection::STEP_COUNT;
use terminus_ui::geom::Rect;
use terminus_ui::icons::{Icon, IconPlacement};
use terminus_ui::theme::ChromeTheme;
use terminus_ui::tokens::{font_size, radius};

/// Draw `text` with its box vertically centred on `cy`.
fn text_at_center(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    cy: f32,
    text: &str,
    size: f32,
    color: [u8; 4],
    weight: UiWeight,
) -> f32 {
    draw_ui_text(sugarloaf, x, cy - size * 0.6, text, size, color, weight)
}

/// `chrome::paint_surface` is private; this is the same thing via the
/// public stroke variant.
fn paint_surface(
    sugarloaf: &mut Sugarloaf,
    rect: &Rect,
    bg: [f32; 4],
    border: Option<[f32; 4]>,
    radius: f32,
    depth: f32,
    order: u8,
    _composite_alpha: bool,
) {
    paint_surface_stroke(
        sugarloaf, rect, bg, border, radius, 1.0, depth, order, false,
    );
}

fn disc(sugarloaf: &mut Sugarloaf, r: &Rect, color: [f32; 4], depth: f32) {
    paint_surface(sugarloaf, r, color, None, r.width * 0.5, depth, 0, false);
}

/// Status dot at `rect`; the idle ring is punched out over `backdrop`.
pub fn paint_status_dot(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    rect: &Rect,
    kind: StatusKind,
    backdrop: [f32; 4],
) {
    match status_dot_paint(kind, theme) {
        DotPaint::Fill(c) => disc(sugarloaf, rect, c, 0.1),
        DotPaint::Ring { color, width } => paint_surface_stroke(
            sugarloaf,
            rect,
            backdrop,
            Some(color),
            rect.width * 0.5,
            width,
            0.1,
            0,
            false,
        ),
    }
}

/// One step node: halo, disc and number / check / cross.
pub fn paint_step_node(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    node: &Rect,
    halo: &Rect,
    state: StepState,
    number: usize,
) {
    let paint = step_paint(state, theme);
    if let Some(h) = paint.halo {
        disc(sugarloaf, halo, h, 0.1);
    }
    disc(sugarloaf, node, paint.fill, 0.2);
    let (cx, cy) = (node.x + node.width * 0.5, node.y + node.height * 0.5);
    match state {
        StepState::Done | StepState::Failed => {
            let icon = if state == StepState::Done {
                Icon::Check
            } else {
                Icon::X
            };
            let scale = sugarloaf.scale_factor();
            draw_icon(
                sugarloaf,
                icon,
                IconPlacement::new(cx - 8.0, cy - 8.0, 16.0),
                paint.glyph,
                scale,
            );
        }
        _ => {
            let label = number.to_string();
            let w = measure_ui_text(sugarloaf, &label, 14.0, UiWeight::SemiBold);
            let color = terminus_ui::theme::text_color(paint.glyph);
            text_at_center(
                sugarloaf,
                cx - w * 0.5,
                cy,
                &label,
                14.0,
                color,
                UiWeight::SemiBold,
            );
        }
    }
}

/// The 4-node line with connecting segments and (optional) labels below.
pub fn paint_step_line(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    line: &StepLine,
    states: &[StepState; STEP_COUNT],
    labels: bool,
) {
    for (i, seg) in line.segments.iter().enumerate() {
        let c = if StepLine::segment_lit(states, i) {
            theme.success
        } else {
            theme.raised
        };
        paint_surface(sugarloaf, seg, c, None, seg.height * 0.5, 0.0, 0, false);
    }
    for i in 0..STEP_COUNT {
        paint_step_node(
            sugarloaf,
            theme,
            &line.nodes[i],
            &line.halo(i),
            states[i],
            i + 1,
        );
        if labels {
            let n = line.nodes[i];
            let w = measure_ui_text(
                sugarloaf,
                STEP_LABELS[i],
                font_size::CAPTION,
                UiWeight::Regular,
            );
            draw_ui_text(
                sugarloaf,
                n.x + n.width * 0.5 - w * 0.5,
                n.bottom() + STEP_HALO + 4.0,
                STEP_LABELS[i],
                font_size::CAPTION,
                theme.text_muted,
                UiWeight::Regular,
            );
        }
    }
}

/// Progress bar with caption; returns the height used.
pub fn paint_progress(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    origin: (f32, f32),
    width: f32,
    kind: ProgressKind,
    fraction: f32,
    caption: &str,
) -> f32 {
    let track = progress_track_rect(origin.0, origin.1, width, kind);
    paint_surface(
        sugarloaf,
        &track,
        theme.raised,
        None,
        track.height,
        0.0,
        0,
        false,
    );
    let fill = progress_fill_rect(&track, fraction);
    if fill.width > 0.5 {
        let color = progress_fill_color(kind, theme);
        paint_surface(sugarloaf, &fill, color, None, fill.height, 0.1, 0, false);
    }
    let cap = progress_caption_rect(&track);
    let color = if kind == ProgressKind::Failed {
        theme.danger_text
    } else {
        theme.text_muted
    };
    draw_ui_text(
        sugarloaf,
        cap.x,
        cap.y,
        caption,
        font_size::CAPTION,
        color,
        UiWeight::Regular,
    );
    progress_height(kind)
}

/// Measured wrapped body lines and action widths for a toast.
pub fn measure_toast(
    sugarloaf: &mut Sugarloaf,
    toast: &Toast,
) -> (Vec<String>, Vec<f32>) {
    let lines = wrap_lines(&toast.body, toast.text_width(), |s| {
        measure_ui_text(sugarloaf, s, font_size::LABEL, UiWeight::Regular)
    });
    let widths = toast
        .actions
        .iter()
        .map(|a| measure_ui_text(sugarloaf, a, font_size::LABEL, UiWeight::Regular))
        .collect();
    (lines, widths)
}

/// Paint a toast at `(x, y)`; returns its layout (for hit-testing).
pub fn paint_toast(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    toast: &Toast,
    x: f32,
    y: f32,
) -> ToastLayout {
    let (lines, widths) = measure_toast(sugarloaf, toast);
    let layout = toast_layout(toast, x, y, lines.len(), &widths);
    let c = toast.kind.colors(theme);
    paint_surface(
        sugarloaf,
        &layout.rect,
        c.bg,
        Some(c.border),
        radius::CARD,
        0.0,
        0,
        false,
    );
    disc(sugarloaf, &layout.dot, c.dot, 0.2);
    let t = &layout.title;
    draw_ui_text(
        sugarloaf,
        t.x,
        t.y,
        &toast.title,
        font_size::BODY_SM,
        theme.text,
        UiWeight::SemiBold,
    );
    for (line, r) in lines.iter().zip(&layout.body_lines) {
        draw_ui_text(
            sugarloaf,
            r.x,
            r.y,
            line,
            font_size::LABEL,
            terminus_ui::theme::text_color(c.body),
            UiWeight::Regular,
        );
    }
    let link = terminus_ui::theme::text_color(theme.accent);
    for (a, r) in toast.actions.iter().zip(&layout.actions) {
        draw_ui_text(
            sugarloaf,
            r.x,
            r.y,
            a,
            font_size::LABEL,
            link,
            UiWeight::Regular,
        );
    }
    let d = layout.dismiss;
    let scale = sugarloaf.scale_factor();
    draw_icon(
        sugarloaf,
        Icon::X,
        IconPlacement::new(
            d.x + (d.width - 13.0) * 0.5,
            d.y + (d.height - 13.0) * 0.5,
            13.0,
        ),
        c.body,
        scale,
    );
    layout
}

const WELL_PAD: f32 = 28.0;
const WELL_RADIUS: f32 = 16.0;
const CELL_GAP: f32 = 24.0;

/// Paint a titled well; `content_h` is the height of the body, `body`
/// paints it at the given origin.
fn section(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    origin: (f32, f32),
    width: f32,
    title: &str,
    desc: &str,
    content_h: f32,
    body: impl FnOnce(&mut Sugarloaf, (f32, f32), f32),
) -> f32 {
    let head_h = 18.0 + 6.0 + 20.0;
    let h = WELL_PAD * 2.0 + head_h + 20.0 + content_h;
    paint_surface(
        sugarloaf,
        &Rect::new(origin.0, origin.1, width, h),
        theme.canvas,
        None,
        WELL_RADIUS,
        0.0,
        0,
        false,
    );
    let x = origin.0 + WELL_PAD;
    let y = origin.1 + WELL_PAD;
    draw_ui_text(sugarloaf, x, y, title, 18.0, theme.text, UiWeight::SemiBold);
    draw_ui_text(
        sugarloaf,
        x,
        y + 24.0,
        desc,
        font_size::LABEL,
        theme.text_muted,
        UiWeight::Regular,
    );
    body(sugarloaf, (x, y + head_h + 20.0), width - 2.0 * WELL_PAD);
    h
}

fn caption(sugarloaf: &mut Sugarloaf, theme: &ChromeTheme, x: f32, y: f32, text: &str) {
    draw_ui_text(
        sugarloaf,
        x,
        y,
        text,
        font_size::CAPTION,
        theme.text_muted,
        UiWeight::Medium,
    );
}

/// Paint the `feedback` gallery at `origin` within `width`; returns the height used.
pub fn paint_gallery(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    origin: (f32, f32),
    width: f32,
) -> f32 {
    let mut y = origin.1;
    let title_h = super::paint_section_title(sugarloaf, theme, origin, "Feedback");
    y += title_h + 12.0;
    let gap = 24.0;

    // ---- status dot
    let words = [
        (StatusKind::Running, "Running"),
        (StatusKind::Connected, "Connected"),
        (StatusKind::Idle, "Not connected"),
        (StatusKind::Warning, "Reconnecting"),
        (StatusKind::Error, "Unreachable"),
    ];
    let col_titles = ["Running", "Connected", "Idle", "Warning", "Error"];
    let h = section(
        sugarloaf,
        theme,
        (origin.0, y),
        width,
        "Status dot",
        "Colour is never alone: a dot always comes with a word, or sits in a row whose position already says what it means.",
        16.0 + 12.0 + 20.0,
        |sl, (x, y), w| {
            let col = (w - 4.0 * CELL_GAP) / 5.0;
            for (i, (kind, word)) in words.iter().enumerate() {
                let cx = x + i as f32 * (col + CELL_GAP);
                caption(sl, theme, cx, y, col_titles[i]);
                let row = Rect::new(cx, y + 28.0, col, 20.0);
                let dot = status_dot_rect(row.x, row.y, row.height);
                paint_status_dot(sl, theme, &dot, *kind, theme.canvas);
                text_at_center(
                    sl,
                    status_label_x(row.x),
                    row.y + row.height * 0.5,
                    word,
                    font_size::LABEL,
                    theme.text,
                    UiWeight::Regular,
                );
            }
        },
    );
    y += h + gap;

    // ---- connection steps
    let step_cases = [
        ("Pending", StepState::Pending, 4),
        ("Active", StepState::Active, 3),
        ("Done", StepState::Done, 1),
        ("Failed", StepState::Failed, 3),
    ];
    let h = section(
        sugarloaf,
        theme,
        (origin.0, y),
        width,
        "Connection steps",
        "Four nodes on a line: Local -> Network -> Handshake -> Shell.",
        16.0 + 12.0 + STEP_NODE + 24.0 + 20.0 + 2.0 * (STEP_NODE + 60.0) + 12.0,
        |sl, (x, y), w| {
            let col = (w - 3.0 * CELL_GAP) / 4.0;
            for (i, (name, state, n)) in step_cases.iter().enumerate() {
                let cx = x + i as f32 * (col + CELL_GAP);
                caption(sl, theme, cx, y, name);
                let node = Rect::new(cx, y + 28.0, STEP_NODE, STEP_NODE);
                let halo = Rect::new(
                    node.x - STEP_HALO,
                    node.y - STEP_HALO,
                    STEP_NODE + 2.0 * STEP_HALO,
                    STEP_NODE + 2.0 * STEP_HALO,
                );
                paint_step_node(sl, theme, &node, &halo, *state, *n);
            }
            let mut ly = y + 28.0 + STEP_NODE + 36.0;
            for (name, active, failed) in [("In progress", 2, false), ("Failed", 2, true)]
            {
                caption(sl, theme, x, ly, name);
                let line = StepLine::evenly(x, w.min(520.0), ly + 20.0 + STEP_NODE * 0.5);
                paint_step_line(sl, theme, &line, &steps_at(active, failed), true);
                ly += STEP_NODE + 60.0;
            }
        },
    );
    y += h + gap;

    // ---- progress
    let h = section(
        sugarloaf,
        theme,
        (origin.0, y),
        width,
        "Progress",
        "Thin bars with a caption: 4 px for transfers, 6 px for updates, danger when stopped.",
        16.0 + 12.0 + progress_height(ProgressKind::Update),
        |sl, (x, y), w| {
            let col = (w - 2.0 * 28.0) / 3.0;
            let cases = [
                ("Transfer \u{b7} 4 px", ProgressKind::Transfer, 0.62, "62 % \u{b7} 11 of 18 MB"),
                ("Update \u{b7} 6 px", ProgressKind::Update, 0.45, "Downloading"),
                ("Failed", ProgressKind::Failed, 0.30, "Upload stopped"),
            ];
            for (i, (name, kind, frac, cap)) in cases.iter().enumerate() {
                let cx = x + i as f32 * (col + 28.0);
                caption(sl, theme, cx, y, name);
                paint_progress(sl, theme, (cx, y + 28.0), col, *kind, *frac, cap);
            }
        },
    );
    y += h + gap;

    // ---- toast
    let mk = |k, t: &str, b: &str, a: &[&str]| {
        Toast::new(
            k,
            t,
            b,
            a.iter().map(|s| s.to_string()).collect(),
            Duration::ZERO,
        )
    };
    let toasts = [
        mk(
            ToastKind::Error,
            "Couldn't reach Host-002",
            "The connection timed out. Check the address and that the server is online.",
            &["Try again", "Edit server"],
        ),
        mk(
            ToastKind::Warning,
            "Reconnecting to jerem prod",
            "The network dropped. Terminus keeps trying for a minute.",
            &["Stop"],
        ),
        mk(
            ToastKind::Success,
            "Public key copied",
            "Paste it into ~/.ssh/authorized_keys on the server.",
            &[],
        ),
        mk(
            ToastKind::Info,
            "Terminus [version] is ready",
            "It installs the next time Terminus starts.",
            &["Restart now"],
        ),
    ];
    let heights: Vec<f32> = toasts
        .iter()
        .map(|t| {
            let (lines, _) = measure_toast(sugarloaf, t);
            toast_height(lines.len(), !t.actions.is_empty())
        })
        .collect();
    let row_h = [heights[0].max(heights[1]), heights[2].max(heights[3])];
    let h = section(
        sugarloaf,
        theme,
        (origin.0, y),
        width,
        "Toast",
        "Errors stay until dismissed; the others leave after a few seconds. Bottom right, above the terminal, never blocking input.",
        row_h[0] + 18.0 + row_h[1],
        |sl, (x, y), w| {
            let col = ((w - 18.0) / 2.0).min(TOAST_WIDTH);
            for (i, t) in toasts.iter().enumerate() {
                let mut t = t.clone();
                t.width = col;
                let (cx, cy) = (
                    x + (i % 2) as f32 * (col + 18.0),
                    y + if i < 2 { 0.0 } else { row_h[0] + 18.0 },
                );
                paint_toast(sl, theme, &t, cx, cy);
            }
        },
    );
    y += h;
    y - origin.1
}
