//! Fixed sidebar chrome around the machine list: brand, the "Search or
//! run…" command bar, and the Add server / Settings buttons at the bottom.
//!
//! The machine list itself (sections, rows, drag & drop, rename, filter)
//! is [`crate::sidebar::HostPanel`]; it scrolls between [`list_top`] and
//! [`list_bottom`].

use super::layout::SIDEBAR_WIDTH;
use crate::geom::Rect;
use crate::icons::Icon;

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

/// Whether Ctrl K opens the palette: only while a non-terminal view owns
/// the keyboard (in the terminal it stays readline's kill-line) and not on
/// macOS, where Cmd K already clears the scrollback.
pub fn ctrl_k_opens_palette(shows_terminal: bool, macos: bool) -> bool {
    !shows_terminal && !macos
}

/// One piece of a shortcut hint: mono text, or a key symbol drawn as an
/// icon (Sora and Martian Mono have no ⇧ / ⌘ glyphs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HintToken {
    Text(&'static str),
    Glyph(Icon),
}

/// Key-symbol icon size and the space between hint tokens.
pub const HINT_ICON: f32 = 12.0;
pub const HINT_GAP: f32 = 3.0;

/// Shortcut hint on the command bar: the binding that opens the palette
/// from where the user is ("Ctrl K" off the terminal, else Ctrl ⇧ P).
pub fn palette_hint(shows_terminal: bool, macos: bool) -> &'static [HintToken] {
    use HintToken::{Glyph, Text};
    if macos {
        &[Glyph(Icon::Command), Glyph(Icon::ArrowBigUp), Text("P")]
    } else if ctrl_k_opens_palette(shows_terminal, macos) {
        &[Text("Ctrl K")]
    } else {
        &[Text("Ctrl"), Glyph(Icon::ArrowBigUp), Text("P")]
    }
}

/// Where the hint tokens go: `xs[i]` is token `i`'s left edge, `left` the
/// hint's left edge. Right-aligned on the bar's inner padding.
#[derive(Debug, Clone, PartialEq)]
pub struct HintLayout {
    pub xs: Vec<f32>,
    pub left: f32,
}

/// Lay the hint out from its measured token widths (icons: [`HINT_ICON`]).
pub fn hint_layout(bar: &Rect, widths: &[f32]) -> HintLayout {
    let total: f32 =
        widths.iter().sum::<f32>() + HINT_GAP * widths.len().saturating_sub(1) as f32;
    let left = bar.right() - COMMAND_PAD_X - total;
    let mut xs = Vec::with_capacity(widths.len());
    let mut x = left;
    for w in widths {
        xs.push(x);
        x += w + HINT_GAP;
    }
    HintLayout { xs, left }
}

/// Left edge of "Search or run…".
pub fn command_label_x(bar: &Rect) -> f32 {
    lead_icon_rect(bar, COMMAND_PAD_X, COMMAND_ICON).right() + COMMAND_GAP
}

/// Widest the label may be drawn (it is elided to this) so it always
/// stops [`COMMAND_GAP`] before the hint.
pub fn command_label_max_width(bar: &Rect, hint_left: f32) -> f32 {
    (hint_left - COMMAND_GAP - command_label_x(bar)).max(0.0)
}

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
    fn palette_hint_shows_ctrl_k_where_it_works() {
        use HintToken::{Glyph, Text};
        // Off the terminal, Ctrl K opens the palette (the view owns keys).
        assert_eq!(palette_hint(false, false), &[Text("Ctrl K")]);
        // In the terminal Ctrl K stays readline's kill-line: Ctrl ⇧ P,
        // with the shift arrow drawn as an icon (no UI face has U+21E7).
        assert_eq!(
            palette_hint(true, false),
            &[Text("Ctrl"), Glyph(Icon::ArrowBigUp), Text("P")]
        );
        // macOS: Cmd K already clears the scrollback.
        let mac = [Glyph(Icon::Command), Glyph(Icon::ArrowBigUp), Text("P")];
        assert_eq!(palette_hint(false, true), &mac);
        assert_eq!(palette_hint(true, true), &mac);
    }

    /// Width a painter would measure: Martian Mono 10 is ~6.6 px/char.
    fn est(tokens: &[HintToken]) -> Vec<f32> {
        tokens
            .iter()
            .map(|t| match t {
                HintToken::Text(s) => s.chars().count() as f32 * 6.6,
                HintToken::Glyph(_) => HINT_ICON,
            })
            .collect()
    }

    #[test]
    fn the_hint_is_right_aligned_and_compact() {
        let bar = command_bar_rect();
        for (term, mac) in [(true, false), (false, false), (true, true)] {
            let widths = est(palette_hint(term, mac));
            let l = hint_layout(&bar, &widths);
            assert_eq!(l.xs.len(), widths.len());
            let last = l.xs.len() - 1;
            assert!(
                (l.xs[last] + widths[last] - (bar.right() - COMMAND_PAD_X)).abs() < 0.01
            );
            assert_eq!(l.left, l.xs[0]);
            for i in 1..l.xs.len() {
                assert!(
                    (l.xs[i] - (l.xs[i - 1] + widths[i - 1] + HINT_GAP)).abs() < 0.01
                );
            }
            // Never wider than the old "Ctrl Shift P".
            assert!(
                bar.right() - COMMAND_PAD_X - l.left <= 12.0 * 6.6,
                "{term} {mac}"
            );
        }
    }

    #[test]
    fn the_label_stops_before_the_hint_in_every_case() {
        let bar = command_bar_rect();
        let label_x = command_label_x(&bar);
        assert_eq!(
            label_x,
            lead_icon_rect(&bar, COMMAND_PAD_X, COMMAND_ICON).right() + COMMAND_GAP
        );
        for (term, mac) in [(true, false), (false, false), (true, true), (false, true)] {
            let l = hint_layout(&bar, &est(palette_hint(term, mac)));
            let max = command_label_max_width(&bar, l.left);
            assert!(max > 60.0, "room for a readable label: {max}");
            assert!(label_x + max + COMMAND_GAP <= l.left + 0.01);
        }
        // A hint wider than the bar leaves no room rather than a negative one.
        assert_eq!(command_label_max_width(&bar, bar.x), 0.0);
    }

    #[test]
    fn ctrl_k_opens_the_palette_only_off_the_terminal() {
        assert!(ctrl_k_opens_palette(false, false));
        assert!(!ctrl_k_opens_palette(true, false));
        assert!(!ctrl_k_opens_palette(false, true));
    }

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
