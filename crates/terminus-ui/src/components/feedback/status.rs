use crate::geom::Rect;
use crate::theme::ChromeTheme;

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
