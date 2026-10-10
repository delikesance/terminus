use super::*;
use crate::geom::Rect;
use crate::theme::ChromeTheme;
use crate::tokens::{height, radius};

pub const HEIGHT: f32 = height::CONTROL_SM;
pub const RADIUS: f32 = radius::PILL;
pub const PAD_LEFT: f32 = 12.0;
pub const PAD_RIGHT: f32 = 12.0;
pub const PAD_RIGHT_CLOSE: f32 = 8.0;
pub const GAP: f32 = 8.0;
pub const DOT: f32 = 6.0;
pub const CLOSE_ICON: f32 = 12.0;
/// Clickable square around the close icon.
pub const CLOSE_HIT: f32 = 20.0;
pub const SIZE: f32 = 13.0;

/// Pill width for a label of `label_w`; `close` reserves the ×,
/// `dot` the activity dot.
pub fn width(label_w: f32, close: bool, dot: bool) -> f32 {
    let dot_w = if dot { DOT + GAP } else { 0.0 };
    let tail = if close {
        GAP + CLOSE_ICON + PAD_RIGHT_CLOSE
    } else {
        PAD_RIGHT
    };
    PAD_LEFT + dot_w + label_w + tail
}

pub fn dot_rect(pill: &Rect) -> Rect {
    Rect::new(
        pill.x + PAD_LEFT,
        pill.y + (pill.height - DOT) * 0.5,
        DOT,
        DOT,
    )
}

pub fn text_x(pill: &Rect, dot: bool) -> f32 {
    pill.x + PAD_LEFT + if dot { DOT + GAP } else { 0.0 }
}

/// Rect of the 12px × glyph.
pub fn close_icon_rect(pill: &Rect) -> Rect {
    Rect::new(
        pill.right() - PAD_RIGHT_CLOSE - CLOSE_ICON,
        pill.y + (pill.height - CLOSE_ICON) * 0.5,
        CLOSE_ICON,
        CLOSE_ICON,
    )
}

/// Hit rect of the close ×, larger than the glyph.
pub fn close_rect(pill: &Rect) -> Rect {
    let i = close_icon_rect(pill);
    let pad = (CLOSE_HIT - CLOSE_ICON) * 0.5;
    Rect::new(
        i.x - pad,
        pill.y + (pill.height - CLOSE_HIT) * 0.5,
        CLOSE_HIT,
        CLOSE_HIT,
    )
}

/// Hit-test; the close part only counts when `close` is shown.
pub fn hit(pill: &Rect, close: bool, px: f32, py: f32) -> Option<PillHit> {
    if !pill.contains(px, py) {
        return None;
    }
    if close && close_rect(pill).contains(px, py) {
        Some(PillHit::Close)
    } else {
        Some(PillHit::Body)
    }
}

pub fn style(theme: &ChromeTheme, state: PillState) -> PillStyle {
    match state {
        PillState::Default => PillStyle {
            bg: None,
            text: theme.text_muted,
            close: false,
            dot: false,
        },
        PillState::Hover => PillStyle {
            bg: Some(theme.surface),
            text: theme.text,
            close: true,
            dot: false,
        },
        PillState::Active => PillStyle {
            bg: Some(theme.divider),
            text: theme.text,
            close: true,
            dot: false,
        },
        PillState::NewOutput => PillStyle {
            bg: None,
            text: theme.text_muted,
            close: false,
            dot: true,
        },
    }
}
