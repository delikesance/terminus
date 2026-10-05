//! Fixed sidebar chrome around the machine list: brand, the "Search or
//! run…" command bar, and the Add server / Settings buttons at the bottom.
//!
//! The machine list itself (sections, rows, drag & drop, rename, filter)
//! is [`crate::sidebar::HostPanel`]; it scrolls between [`list_top`] and
//! [`list_bottom`].

use super::layout::SIDEBAR_WIDTH;
use crate::geom::Rect;

/// `aside` padding: 18 top, 12 sides, 10 bottom; 18 between blocks.
pub const PAD_TOP: f32 = 18.0;
pub const PAD_X: f32 = 12.0;
pub const PAD_BOTTOM: f32 = 10.0;
pub const GAP: f32 = 18.0;
/// Brand "Terminus": Sora SemiBold 20 on a 24px line, inset 10.
pub const BRAND_SIZE: f32 = 20.0;
pub const BRAND_LINE: f32 = 24.0;
pub const BRAND_INSET: f32 = 10.0;
/// Command bar ("Search or run…  Ctrl K").
pub const COMMAND_HEIGHT: f32 = 38.0;
pub const COMMAND_RADIUS: f32 = 10.0;
pub const COMMAND_PAD_X: f32 = 10.0;
pub const COMMAND_ICON: f32 = 15.0;
pub const COMMAND_GAP: f32 = 10.0;
pub const COMMAND_SIZE: f32 = 13.0;
pub const COMMAND_HINT_SIZE: f32 = 10.0;
/// Bottom buttons (Add server, Settings): 40 high, 2 apart.
pub const FOOT_HEIGHT: f32 = 40.0;
pub const FOOT_GAP: f32 = 2.0;
pub const FOOT_RADIUS: f32 = 10.0;
pub const FOOT_PAD_X: f32 = 10.0;
pub const FOOT_ICON: f32 = 15.0;
pub const FOOT_ICON_GAP: f32 = 10.0;
pub const FOOT_SIZE: f32 = 14.0;
/// "Synced" word on the Settings button.
pub const SYNCED_SIZE: f32 = 12.0;

fn inner_width() -> f32 {
    SIDEBAR_WIDTH - 2.0 * PAD_X
}

/// Line box of the brand word (the text starts at `x + BRAND_INSET`).
pub fn brand_rect() -> Rect {
    Rect::new(PAD_X, PAD_TOP, inner_width(), BRAND_LINE)
}

pub fn command_bar_rect() -> Rect {
    Rect::new(
        PAD_X,
        brand_rect().bottom() + GAP,
        inner_width(),
        COMMAND_HEIGHT,
    )
}

/// Top of the scrolling machine list.
pub fn list_top() -> f32 {
    command_bar_rect().bottom() + GAP
}

pub fn settings_rect(window_height: f32) -> Rect {
    Rect::new(
        PAD_X,
        window_height - PAD_BOTTOM - FOOT_HEIGHT,
        inner_width(),
        FOOT_HEIGHT,
    )
}

pub fn add_server_rect(window_height: f32) -> Rect {
    let settings = settings_rect(window_height);
    Rect::new(
        PAD_X,
        settings.y - FOOT_GAP - FOOT_HEIGHT,
        inner_width(),
        FOOT_HEIGHT,
    )
}

/// Bottom of the scrolling machine list (never above its top).
pub fn list_bottom(window_height: f32) -> f32 {
    (add_server_rect(window_height).y - GAP).max(list_top())
}

/// Icon box inside a command bar / footer button, left-aligned.
pub fn lead_icon_rect(button: &Rect, pad_x: f32, size: f32) -> Rect {
    Rect::new(
        button.x + pad_x,
        button.y + (button.height - size) * 0.5,
        size,
        size,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_blocks_follow_the_mock_rhythm() {
        assert_eq!(brand_rect(), Rect::new(12.0, 18.0, 236.0, 24.0));
        assert_eq!(command_bar_rect(), Rect::new(12.0, 60.0, 236.0, 38.0));
        assert_eq!(list_top(), 116.0);
    }

    #[test]
    fn bottom_buttons_stack_from_the_window_bottom() {
        let s = settings_rect(900.0);
        let a = add_server_rect(900.0);
        assert_eq!(s, Rect::new(12.0, 850.0, 236.0, 40.0));
        assert_eq!(a.bottom() + FOOT_GAP, s.y);
        assert_eq!(list_bottom(900.0), a.y - 18.0);
    }

    #[test]
    fn the_list_never_inverts_in_a_short_window() {
        assert!(list_bottom(100.0) >= list_top());
    }

    #[test]
    fn lead_icons_are_vertically_centred() {
        let b = command_bar_rect();
        let i = lead_icon_rect(&b, COMMAND_PAD_X, COMMAND_ICON);
        assert_eq!(i.x, b.x + 10.0);
        assert_eq!(i.y + i.height / 2.0, b.y + b.height / 2.0);
    }
}
