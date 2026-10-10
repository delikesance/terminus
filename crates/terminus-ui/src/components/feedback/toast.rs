use super::status::DOT_SIZE;
use crate::geom::Rect;
use crate::theme::ChromeTheme;
use std::time::Duration;

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

pub(super) fn hex(r: u8, g: u8, b: u8) -> [f32; 4] {
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
