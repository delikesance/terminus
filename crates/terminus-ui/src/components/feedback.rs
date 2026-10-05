//! Feedback: status dot, connection steps, progress bar and toast.
//!
//! Pure state and geometry (logical pixels); the painter in
//! `rioterm::renderer::components::feedback` walks these same functions.

use crate::connection::{NodeVisual, STEP_COUNT, TRACK_LINE_HEIGHT};
use crate::geom::Rect;
use crate::theme::ChromeTheme;
use std::time::Duration;

// ---------------------------------------------------------------- status dot

pub const DOT_SIZE: f32 = 8.0;
/// Gap between a dot and its label.
pub const DOT_LABEL_GAP: f32 = 10.0;
/// Stroke of the hollow "idle" ring.
pub const IDLE_RING_WIDTH: f32 = 1.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Running,
    Connected,
    Idle,
    Warning,
    Error,
}

impl StatusKind {
    pub const ALL: [StatusKind; 5] = [
        StatusKind::Running,
        StatusKind::Connected,
        StatusKind::Idle,
        StatusKind::Warning,
        StatusKind::Error,
    ];
}

/// How a dot is drawn: a filled disc or a hollow ring.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DotPaint {
    Fill([f32; 4]),
    Ring { color: [f32; 4], width: f32 },
}

pub fn status_dot_paint(kind: StatusKind, theme: &ChromeTheme) -> DotPaint {
    match kind {
        StatusKind::Running => DotPaint::Fill(theme.success),
        StatusKind::Connected => DotPaint::Fill(theme.accent),
        StatusKind::Idle => DotPaint::Ring {
            color: theme.idle_ring,
            width: IDLE_RING_WIDTH,
        },
        StatusKind::Warning => DotPaint::Fill(theme.warning),
        StatusKind::Error => DotPaint::Fill(theme.danger_fill),
    }
}

/// Dot box vertically centred on a row of height `row_h` starting at `(x, y)`.
pub fn status_dot_rect(x: f32, y: f32, row_h: f32) -> Rect {
    Rect::new(x, y + (row_h - DOT_SIZE) * 0.5, DOT_SIZE, DOT_SIZE)
}

/// Left edge of the label that follows a dot placed at `x`.
pub fn status_label_x(x: f32) -> f32 {
    x + DOT_SIZE + DOT_LABEL_GAP
}

// ----------------------------------------------------------- connection steps

pub const STEP_NODE: f32 = 40.0;
/// Width of the translucent halo around the active node.
pub const STEP_HALO: f32 = 6.0;
/// Alpha of the halo (accent at 20 %).
pub const STEP_HALO_ALPHA: f32 = 0.2;
pub const STEP_LABELS: [&str; STEP_COUNT] = ["Local", "Network", "Handshake", "Shell"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepState {
    Pending,
    Active,
    Done,
    Failed,
}

impl From<NodeVisual> for StepState {
    fn from(v: NodeVisual) -> Self {
        match v {
            NodeVisual::Pending => StepState::Pending,
            NodeVisual::Active => StepState::Active,
            NodeVisual::Done => StepState::Done,
        }
    }
}

/// Colours of one node: fill, glyph (number or icon), optional halo.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepPaint {
    pub fill: [f32; 4],
    pub glyph: [f32; 4],
    pub halo: Option<[f32; 4]>,
}

pub fn step_paint(state: StepState, theme: &ChromeTheme) -> StepPaint {
    match state {
        StepState::Pending => StepPaint {
            fill: theme.surface,
            glyph: theme.text_faint.map(|c| c as f32 / 255.0),
            halo: None,
        },
        StepState::Active => StepPaint {
            fill: theme.accent,
            glyph: theme.on_accent,
            halo: Some([
                theme.accent[0],
                theme.accent[1],
                theme.accent[2],
                STEP_HALO_ALPHA,
            ]),
        },
        StepState::Done => StepPaint {
            fill: theme.step_done_bg,
            glyph: theme.success,
            halo: None,
        },
        StepState::Failed => StepPaint {
            fill: theme.step_failed_bg,
            glyph: theme.danger_text.map(|c| c as f32 / 255.0),
            halo: None,
        },
    }
}

/// States of the four nodes for a sequence stopped at `active` (0-based).
/// `failed` marks the active node as failed instead.
pub fn steps_at(active: usize, failed: bool) -> [StepState; STEP_COUNT] {
    let mut out = [StepState::Pending; STEP_COUNT];
    for (i, s) in out.iter_mut().enumerate() {
        *s = if i < active {
            StepState::Done
        } else if i == active {
            if failed {
                StepState::Failed
            } else {
                StepState::Active
            }
        } else {
            StepState::Pending
        };
    }
    out
}

/// Node + connector geometry of the four-node line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepLine {
    pub nodes: [Rect; STEP_COUNT],
    /// `segments[i]` joins node `i` to node `i + 1` (edge to edge).
    pub segments: [Rect; STEP_COUNT - 1],
}

impl StepLine {
    /// From node centre x positions (e.g. the output of
    /// [`crate::connection::track_layout_from_labels`] plus the dialog's x)
    /// and the shared centre line `cy`.
    pub fn from_centers(node_cx: &[f32; STEP_COUNT], cy: f32) -> Self {
        let node = |cx: f32| {
            Rect::new(
                cx - STEP_NODE * 0.5,
                cy - STEP_NODE * 0.5,
                STEP_NODE,
                STEP_NODE,
            )
        };
        let nodes = node_cx.map(node);
        let mut segments = [Rect::new(0.0, 0.0, 0.0, 0.0); STEP_COUNT - 1];
        for (i, seg) in segments.iter_mut().enumerate() {
            let x0 = nodes[i].right();
            let x1 = nodes[i + 1].x;
            *seg = Rect::new(
                x0,
                cy - TRACK_LINE_HEIGHT * 0.5,
                (x1 - x0).max(0.0),
                TRACK_LINE_HEIGHT,
            );
        }
        Self { nodes, segments }
    }

    /// Evenly spaced across `[x, x + width]` with node centres at the
    /// midpoints of four equal columns.
    pub fn evenly(x: f32, width: f32, cy: f32) -> Self {
        let col = width / STEP_COUNT as f32;
        let mut cx = [0.0; STEP_COUNT];
        for (i, c) in cx.iter_mut().enumerate() {
            *c = x + col * (i as f32 + 0.5);
        }
        Self::from_centers(&cx, cy)
    }

    /// Halo box around node `i` (grown by [`STEP_HALO`]).
    pub fn halo(&self, i: usize) -> Rect {
        let n = self.nodes[i];
        Rect::new(
            n.x - STEP_HALO,
            n.y - STEP_HALO,
            n.width + 2.0 * STEP_HALO,
            n.height + 2.0 * STEP_HALO,
        )
    }

    /// A segment is "lit" once the node it leaves is Done.
    pub fn segment_lit(states: &[StepState; STEP_COUNT], i: usize) -> bool {
        states[i] == StepState::Done
    }
}

// ------------------------------------------------------------------ progress

pub const PROGRESS_TRANSFER_H: f32 = 4.0;
pub const PROGRESS_UPDATE_H: f32 = 6.0;
/// Gap between the bar and its caption.
pub const PROGRESS_CAPTION_GAP: f32 = 8.0;
pub const PROGRESS_CAPTION_H: f32 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressKind {
    Transfer,
    Update,
    Failed,
}

impl ProgressKind {
    pub fn bar_height(self) -> f32 {
        match self {
            ProgressKind::Update => PROGRESS_UPDATE_H,
            _ => PROGRESS_TRANSFER_H,
        }
    }
}

/// Fraction clamped to `0..=1`; NaN reads as 0.
pub fn clamp_fraction(f: f32) -> f32 {
    if f.is_nan() {
        0.0
    } else {
        f.clamp(0.0, 1.0)
    }
}

pub fn progress_track_rect(x: f32, y: f32, width: f32, kind: ProgressKind) -> Rect {
    Rect::new(x, y, width, kind.bar_height())
}

pub fn progress_fill_rect(track: &Rect, fraction: f32) -> Rect {
    Rect::new(
        track.x,
        track.y,
        track.width * clamp_fraction(fraction),
        track.height,
    )
}

/// Caption line below the bar.
pub fn progress_caption_rect(track: &Rect) -> Rect {
    Rect::new(
        track.x,
        track.bottom() + PROGRESS_CAPTION_GAP,
        track.width,
        PROGRESS_CAPTION_H,
    )
}

/// Bar + gap + caption.
pub fn progress_height(kind: ProgressKind) -> f32 {
    kind.bar_height() + PROGRESS_CAPTION_GAP + PROGRESS_CAPTION_H
}

/// Fill colour for a bar kind.
pub fn progress_fill_color(kind: ProgressKind, theme: &ChromeTheme) -> [f32; 4] {
    match kind {
        ProgressKind::Failed => theme.danger_fill,
        _ => theme.accent,
    }
}

// --------------------------------------------------------------------- toast

pub const TOAST_WIDTH: f32 = 400.0;
pub const TOAST_PAD: f32 = 18.0;
pub const TOAST_GAP: f32 = 14.0;
pub const TOAST_RADIUS: f32 = 14.0;
pub const TOAST_DOT_TOP: f32 = 6.0;
pub const TOAST_DISMISS: f32 = 30.0;
pub const TOAST_TITLE_H: f32 = 20.0;
pub const TOAST_BODY_LINE_H: f32 = 19.5;
/// Gap between title, body and the action row.
pub const TOAST_TEXT_GAP: f32 = 6.0;
pub const TOAST_ACTIONS_TOP: f32 = 4.0;
pub const TOAST_ACTION_GAP: f32 = 14.0;
pub const TOAST_ACTION_H: f32 = 19.5;
/// Screen margin and gap between stacked toasts.
pub const TOAST_MARGIN: f32 = 20.0;
pub const TOAST_STACK_GAP: f32 = 12.0;
pub const TOAST_TTL: Duration = Duration::from_secs(5);
pub const TOAST_WARNING_TTL: Duration = Duration::from_secs(8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Error,
    Warning,
    Success,
    Info,
}

/// Colour set of a toast kind.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToastColors {
    pub bg: [f32; 4],
    pub border: [f32; 4],
    pub dot: [f32; 4],
    pub body: [f32; 4],
}

fn hex(r: u8, g: u8, b: u8) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
}

impl ToastKind {
    pub const ALL: [ToastKind; 4] = [
        ToastKind::Error,
        ToastKind::Warning,
        ToastKind::Success,
        ToastKind::Info,
    ];

    pub fn colors(self, theme: &ChromeTheme) -> ToastColors {
        match self {
            ToastKind::Error => ToastColors {
                bg: hex(0x2A, 0x16, 0x1C),
                border: hex(0x5A, 0x25, 0x33),
                dot: theme.danger_fill,
                body: hex(0xE6, 0xC8, 0xD0),
            },
            ToastKind::Warning => ToastColors {
                bg: hex(0x2A, 0x24, 0x16),
                border: hex(0x5A, 0x4A, 0x25),
                dot: theme.warning,
                body: hex(0xE9, 0xDC, 0xC2),
            },
            ToastKind::Success => ToastColors {
                bg: hex(0x16, 0x26, 0x1F),
                border: hex(0x25, 0x52, 0x3F),
                dot: theme.success,
                body: hex(0xC8, 0xE6, 0xD6),
            },
            ToastKind::Info => ToastColors {
                bg: hex(0x16, 0x1E, 0x2A),
                border: hex(0x25, 0x3F, 0x5A),
                dot: theme.info,
                body: hex(0xC8, 0xD8, 0xE6),
            },
        }
    }

    /// Time before auto-dismiss; `None` = persistent until dismissed.
    pub fn default_ttl(self) -> Option<Duration> {
        match self {
            ToastKind::Error => None,
            ToastKind::Warning => Some(TOAST_WARNING_TTL),
            ToastKind::Success | ToastKind::Info => Some(TOAST_TTL),
        }
    }
}

/// One toast: content plus its lifetime. Times are offsets on a monotonic
/// clock chosen by the caller (e.g. time since app start).
#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    pub kind: ToastKind,
    pub title: String,
    pub body: String,
    pub actions: Vec<String>,
    pub width: f32,
    pub created: Duration,
    pub ttl: Option<Duration>,
}

impl Toast {
    pub fn new(
        kind: ToastKind,
        title: impl Into<String>,
        body: impl Into<String>,
        actions: Vec<String>,
        created: Duration,
    ) -> Self {
        Self {
            kind,
            title: title.into(),
            body: body.into(),
            actions,
            width: TOAST_WIDTH,
            created,
            ttl: kind.default_ttl(),
        }
    }

    pub fn is_persistent(&self) -> bool {
        self.ttl.is_none()
    }

    /// True once the auto-dismiss time has passed. Persistent toasts never expire.
    pub fn is_expired(&self, now: Duration) -> bool {
        match self.ttl {
            Some(ttl) => now.saturating_sub(self.created) >= ttl,
            None => false,
        }
    }

    /// Width available to title / body text.
    pub fn text_width(&self) -> f32 {
        (self.width - 2.0 * TOAST_PAD - DOT_SIZE - 2.0 * TOAST_GAP - TOAST_DISMISS)
            .max(0.0)
    }
}

/// Greedy word wrap using a caller-supplied width measure. Always returns
/// at least one line; a single over-long word stays on its own line.
pub fn wrap_lines(
    text: &str,
    max_width: f32,
    mut measure: impl FnMut(&str) -> f32,
) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if cur.is_empty() {
            cur.push_str(word);
            continue;
        }
        let candidate = format!("{cur} {word}");
        if measure(&candidate) <= max_width {
            cur = candidate;
        } else {
            lines.push(std::mem::take(&mut cur));
            cur.push_str(word);
        }
    }
    if !cur.is_empty() || lines.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Hit target inside a toast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastHit {
    Dismiss,
    Action(usize),
    /// Anywhere else on the toast (consume the click).
    Body,
}

/// Resolved geometry of one toast at a given origin.
#[derive(Debug, Clone, PartialEq)]
pub struct ToastLayout {
    pub rect: Rect,
    pub dot: Rect,
    pub title: Rect,
    pub body_lines: Vec<Rect>,
    pub actions: Vec<Rect>,
    pub dismiss: Rect,
}

/// Total height for `body_line_count` wrapped lines and optional actions.
pub fn toast_height(body_line_count: usize, has_actions: bool) -> f32 {
    let mut h = 2.0 * TOAST_PAD
        + TOAST_TITLE_H
        + TOAST_TEXT_GAP
        + body_line_count.max(1) as f32 * TOAST_BODY_LINE_H;
    if has_actions {
        h += TOAST_TEXT_GAP + TOAST_ACTIONS_TOP + TOAST_ACTION_H;
    }
    h
}

/// Lay a toast out at `(x, y)`. `body_line_count` comes from [`wrap_lines`];
/// `action_widths` are the measured widths of the action labels.
pub fn toast_layout(
    toast: &Toast,
    x: f32,
    y: f32,
    body_line_count: usize,
    action_widths: &[f32],
) -> ToastLayout {
    let lines = body_line_count.max(1);
    let height = toast_height(lines, !toast.actions.is_empty());
    let rect = Rect::new(x, y, toast.width, height);
    let dot = Rect::new(
        x + TOAST_PAD,
        y + TOAST_PAD + TOAST_DOT_TOP,
        DOT_SIZE,
        DOT_SIZE,
    );
    let text_x = dot.right() + TOAST_GAP;
    let tw = toast.text_width();
    let title = Rect::new(text_x, y + TOAST_PAD, tw, TOAST_TITLE_H);
    let body_top = title.bottom() + TOAST_TEXT_GAP;
    let body_lines = (0..lines)
        .map(|i| {
            Rect::new(
                text_x,
                body_top + i as f32 * TOAST_BODY_LINE_H,
                tw,
                TOAST_BODY_LINE_H,
            )
        })
        .collect::<Vec<_>>();
    let actions_top =
        body_top + lines as f32 * TOAST_BODY_LINE_H + TOAST_TEXT_GAP + TOAST_ACTIONS_TOP;
    let mut ax = text_x;
    let actions = toast
        .actions
        .iter()
        .enumerate()
        .map(|(i, _)| {
            let w = action_widths.get(i).copied().unwrap_or(0.0);
            let r = Rect::new(ax, actions_top, w, TOAST_ACTION_H);
            ax += w + TOAST_ACTION_GAP;
            r
        })
        .collect();
    let dismiss = Rect::new(
        rect.right() - TOAST_PAD - TOAST_DISMISS,
        y + TOAST_PAD,
        TOAST_DISMISS,
        TOAST_DISMISS,
    );
    ToastLayout {
        rect,
        dot,
        title,
        body_lines,
        actions,
        dismiss,
    }
}

impl ToastLayout {
    pub fn hit_test(&self, px: f32, py: f32) -> Option<ToastHit> {
        if !self.rect.contains(px, py) {
            return None;
        }
        if self.dismiss.contains(px, py) {
            return Some(ToastHit::Dismiss);
        }
        if let Some(i) = self.actions.iter().position(|r| r.contains(px, py)) {
            return Some(ToastHit::Action(i));
        }
        Some(ToastHit::Body)
    }
}

/// Bottom-right stack: `heights[0]` is the newest and sits lowest; each
/// following toast stacks above with [`TOAST_STACK_GAP`]. Returns one rect
/// per toast (width `width`).
pub fn stack_rects(
    viewport_w: f32,
    viewport_h: f32,
    width: f32,
    heights: &[f32],
) -> Vec<Rect> {
    let x = viewport_w - TOAST_MARGIN - width;
    let mut bottom = viewport_h - TOAST_MARGIN;
    heights
        .iter()
        .map(|&h| {
            let r = Rect::new(x, bottom - h, width, h);
            bottom -= h + TOAST_STACK_GAP;
            r
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> ChromeTheme {
        ChromeTheme::default()
    }

    fn s(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn dot_is_8px_and_centred_in_row() {
        let r = status_dot_rect(10.0, 100.0, 20.0);
        assert_eq!((r.width, r.height), (8.0, 8.0));
        assert_eq!(r.y, 106.0);
        assert_eq!(status_label_x(10.0), 28.0);
    }

    #[test]
    fn idle_is_hollow_ring_others_filled() {
        let t = theme();
        assert_eq!(
            status_dot_paint(StatusKind::Idle, &t),
            DotPaint::Ring {
                color: t.idle_ring,
                width: 1.5
            }
        );
        assert_eq!(
            status_dot_paint(StatusKind::Running, &t),
            DotPaint::Fill(t.success)
        );
        assert_eq!(
            status_dot_paint(StatusKind::Connected, &t),
            DotPaint::Fill(t.accent)
        );
        assert_eq!(
            status_dot_paint(StatusKind::Warning, &t),
            DotPaint::Fill(t.warning)
        );
        assert_eq!(
            status_dot_paint(StatusKind::Error, &t),
            DotPaint::Fill(t.danger_fill)
        );
    }

    #[test]
    fn step_paint_matches_board() {
        let t = theme();
        assert_eq!(step_paint(StepState::Pending, &t).fill, t.surface);
        let a = step_paint(StepState::Active, &t);
        assert_eq!(a.fill, t.accent);
        assert_eq!(a.glyph, t.on_accent);
        assert_eq!(a.halo.unwrap()[3], 0.2);
        assert_eq!(step_paint(StepState::Done, &t).fill, t.step_done_bg);
        assert_eq!(step_paint(StepState::Done, &t).glyph, t.success);
        assert_eq!(step_paint(StepState::Failed, &t).fill, t.step_failed_bg);
        assert!(step_paint(StepState::Pending, &t).halo.is_none());
    }

    #[test]
    fn steps_at_marks_done_active_pending_and_failed() {
        use StepState::*;
        assert_eq!(steps_at(2, false), [Done, Done, Active, Pending]);
        assert_eq!(steps_at(2, true), [Done, Done, Failed, Pending]);
        assert_eq!(StepState::from(NodeVisual::Done), Done);
    }

    #[test]
    fn step_line_nodes_40px_segments_join_edges() {
        let line = StepLine::from_centers(&[50.0, 150.0, 250.0, 350.0], 100.0);
        for n in line.nodes {
            assert_eq!((n.width, n.height), (40.0, 40.0));
            assert_eq!(n.y, 80.0);
        }
        assert_eq!(line.segments[0].x, line.nodes[0].right());
        assert_eq!(line.segments[0].right(), line.nodes[1].x);
        assert_eq!(line.segments[0].height, TRACK_LINE_HEIGHT);
        assert_eq!(line.segments[0].y, 100.0 - TRACK_LINE_HEIGHT / 2.0);
        assert_eq!(line.halo(1).width, 52.0);
    }

    #[test]
    fn step_line_accepts_connection_layout() {
        let (w, cx) = crate::connection::track_layout_from_labels(&[40.0; 4]);
        let line = StepLine::from_centers(&cx.map(|c| c + 100.0), 0.0);
        assert!(line.nodes[3].right() <= 100.0 + w);
        assert!(line.segments.iter().all(|s| s.width > 0.0));
    }

    #[test]
    fn evenly_spaces_four_nodes() {
        let line = StepLine::evenly(0.0, 400.0, 20.0);
        assert_eq!(line.nodes[0].x + 20.0, 50.0);
        assert_eq!(line.nodes[3].x + 20.0, 350.0);
    }

    #[test]
    fn segment_lit_after_done_node() {
        use StepState::*;
        let st = [Done, Active, Pending, Pending];
        assert!(StepLine::segment_lit(&st, 0));
        assert!(!StepLine::segment_lit(&st, 1));
    }

    #[test]
    fn progress_heights_and_fraction_clamp() {
        assert_eq!(ProgressKind::Transfer.bar_height(), 4.0);
        assert_eq!(ProgressKind::Update.bar_height(), 6.0);
        assert_eq!(ProgressKind::Failed.bar_height(), 4.0);
        let tr = progress_track_rect(10.0, 10.0, 200.0, ProgressKind::Transfer);
        assert_eq!(progress_fill_rect(&tr, 0.62).width, 124.0);
        assert_eq!(progress_fill_rect(&tr, 2.0).width, 200.0);
        assert_eq!(progress_fill_rect(&tr, -1.0).width, 0.0);
        assert_eq!(progress_fill_rect(&tr, f32::NAN).width, 0.0);
        let cap = progress_caption_rect(&tr);
        assert_eq!(cap.y, 22.0);
        assert_eq!(progress_height(ProgressKind::Update), 6.0 + 8.0 + 16.0);
    }

    #[test]
    fn failed_bar_uses_danger() {
        let t = theme();
        assert_eq!(progress_fill_color(ProgressKind::Failed, &t), t.danger_fill);
        assert_eq!(progress_fill_color(ProgressKind::Transfer, &t), t.accent);
    }

    #[test]
    fn errors_persist_others_expire() {
        let mk = |k| Toast::new(k, "t", "b", vec![], s(10));
        let e = mk(ToastKind::Error);
        assert!(e.is_persistent());
        assert!(!e.is_expired(s(100_000)));
        let i = mk(ToastKind::Info);
        assert!(!i.is_expired(s(14)));
        assert!(i.is_expired(s(15)));
        assert!(!i.is_expired(s(5))); // clock before creation
        let w = mk(ToastKind::Warning);
        assert!(!w.is_expired(s(17)));
        assert!(w.is_expired(s(18)));
    }

    #[test]
    fn toast_colors_match_board() {
        let t = theme();
        let c = ToastKind::Error.colors(&t);
        assert_eq!(c.bg, hex(0x2A, 0x16, 0x1C));
        assert_eq!(c.dot, t.danger_fill);
        assert_eq!(ToastKind::Info.colors(&t).border, hex(0x25, 0x3F, 0x5A));
        assert_eq!(ToastKind::Success.colors(&t).dot, t.success);
        assert_eq!(ToastKind::Warning.colors(&t).body, hex(0xE9, 0xDC, 0xC2));
    }

    #[test]
    fn wrap_breaks_on_width() {
        let m = |s: &str| s.chars().count() as f32 * 10.0;
        assert_eq!(wrap_lines("aa bb cc", 50.0, m), vec!["aa bb", "cc"]);
        assert_eq!(wrap_lines("", 50.0, m), vec![""]);
        assert_eq!(wrap_lines("longword x", 20.0, m), vec!["longword", "x"]);
    }

    #[test]
    fn toast_text_width_is_298_by_default() {
        let t = Toast::new(ToastKind::Info, "a", "b", vec![], s(0));
        assert_eq!(t.width, 400.0);
        assert_eq!(t.text_width(), 298.0);
    }

    #[test]
    fn toast_layout_geometry_and_hits() {
        let t = Toast::new(
            ToastKind::Error,
            "Couldn't reach",
            "body",
            vec!["Try again".into(), "Edit server".into()],
            s(0),
        );
        let l = toast_layout(&t, 100.0, 200.0, 2, &[60.0, 70.0]);
        assert_eq!(l.rect.height, toast_height(2, true));
        assert_eq!(l.dismiss.width, 30.0);
        assert_eq!(l.dismiss.right(), l.rect.right() - 18.0);
        assert_eq!(l.dot.x, 118.0);
        assert_eq!(l.title.x, 118.0 + 8.0 + 14.0);
        assert_eq!(l.body_lines.len(), 2);
        assert_eq!(l.actions[1].x, l.actions[0].right() + 14.0);
        let c = |r: Rect| (r.x + r.width / 2.0, r.y + r.height / 2.0);
        let (x, y) = c(l.dismiss);
        assert_eq!(l.hit_test(x, y), Some(ToastHit::Dismiss));
        let (x, y) = c(l.actions[1]);
        assert_eq!(l.hit_test(x, y), Some(ToastHit::Action(1)));
        let (x, y) = c(l.title);
        assert_eq!(l.hit_test(x, y), Some(ToastHit::Body));
        assert_eq!(l.hit_test(0.0, 0.0), None);
        assert!(l.actions[0].bottom() <= l.rect.bottom() - 18.0 + 0.01);
    }

    #[test]
    fn toast_without_actions_is_shorter() {
        assert!(toast_height(1, false) < toast_height(1, true));
        assert!(toast_height(2, false) > toast_height(1, false));
    }

    #[test]
    fn stack_grows_upward_from_bottom_right() {
        let r = stack_rects(1000.0, 800.0, 400.0, &[100.0, 80.0]);
        assert_eq!(r[0], Rect::new(580.0, 680.0, 400.0, 100.0));
        assert_eq!(r[1].bottom(), r[0].y - TOAST_STACK_GAP);
        assert_eq!(r[1].x, r[0].x);
        assert!(stack_rects(1000.0, 800.0, 400.0, &[]).is_empty());
    }
}
