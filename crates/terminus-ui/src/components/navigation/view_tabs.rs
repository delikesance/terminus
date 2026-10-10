use super::*;
use crate::geom::Rect;
use crate::theme::ChromeTheme;

/// 20px line + 12px padding + 2px underline.
pub const HEIGHT: f32 = 34.0;
pub const GAP: f32 = 24.0;
pub const BADGE_GAP: f32 = 6.0;
pub const UNDERLINE: f32 = 2.0;
pub const SIZE: f32 = 14.0;

pub fn width(size: TabSize) -> f32 {
    if size.badge_w > 0.0 {
        size.label_w + BADGE_GAP + size.badge_w
    } else {
        size.label_w
    }
}

/// Tab rects laid out left to right from `(x, y)`.
pub fn layout(x: f32, y: f32, sizes: &[TabSize]) -> Vec<Rect> {
    let mut cx = x;
    sizes
        .iter()
        .map(|s| {
            let r = Rect::new(cx, y, width(*s), HEIGHT);
            cx += r.width + GAP;
            r
        })
        .collect()
}

pub fn hit(tabs: &[Rect], px: f32, py: f32) -> Option<usize> {
    tabs.iter().position(|r| r.contains(px, py))
}

pub fn underline_rect(tab: &Rect) -> Rect {
    Rect::new(tab.x, tab.bottom() - UNDERLINE, tab.width, UNDERLINE)
}

pub fn style(theme: &ChromeTheme, state: TabState) -> TabStyle {
    match state {
        TabState::Default => TabStyle {
            text: theme.text_muted,
            medium: false,
            underline: None,
        },
        TabState::Hover => TabStyle {
            text: theme.text,
            medium: false,
            underline: Some(theme.line),
        },
        TabState::Active => TabStyle {
            text: theme.text,
            medium: true,
            underline: Some(theme.accent),
        },
    }
}
