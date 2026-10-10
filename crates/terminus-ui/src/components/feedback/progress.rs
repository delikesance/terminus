use crate::geom::Rect;
use crate::theme::ChromeTheme;

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
