//! The window's top-level boxes: sidebar, main card, header, pills row and
//! the content rect a view paints into (`App.dc.html`).
//!
//! ```text
//! ┌────────┬────────────────────────────────────────────┐
//! │sidebar │ ╭ main (8px gutter top/right/bottom) ──────╮ │
//! │ 260    │ │ header 96                                │ │
//! │        │ │ pills 52 (Terminal only)                 │ │
//! │        │ │ content                                  │ │
//! │        │ ╰──────────────────────────────────────────╯ │
//! └────────┴────────────────────────────────────────────┘
//! ```

use super::workspace::WorkspaceView;
use crate::geom::Rect;

/// Sidebar width (`aside` 260px).
pub const SIDEBAR_WIDTH: f32 = 260.0;
/// Gutter between the main card and the window edges (not the sidebar).
pub const MAIN_MARGIN: f32 = 8.0;
/// Main card corner radius.
pub const MAIN_RADIUS: f32 = 16.0;
/// Machine / settings / home header height.
pub const HEADER_HEIGHT: f32 = 96.0;
/// Session pills row height.
pub const PILLS_HEIGHT: f32 = 52.0;
/// Window control buttons (min / max / close), top-right of the header.
pub const CONTROL_WIDTH: f32 = 40.0;
pub const CONTROL_HEIGHT: f32 = 32.0;
pub const CONTROLS_PAD_TOP: f32 = 8.0;
pub const CONTROLS_PAD_RIGHT: f32 = 8.0;
/// Horizontal padding of the terminal text inside the main card, before
/// the config margin (rio's default margin of 2 brings it to the mock's 28).
pub const TERMINAL_PAD_X: f32 = 26.0;
/// Gap between the pills row and the first terminal line, and below the
/// last one, before the config margin.
pub const TERMINAL_PAD_Y: f32 = 8.0;

/// Space the shell takes from each window edge before the terminal grid,
/// in logical pixels. Added to the user's `margin` config.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Insets {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

/// The grid's inset. Constant across views on purpose: switching to Files
/// and back never resizes the PTY (no SIGWINCH, no reflow) because the
/// grid keeps its box; other views only cover it.
pub const fn grid_insets() -> Insets {
    Insets {
        top: MAIN_MARGIN + HEADER_HEIGHT + PILLS_HEIGHT + TERMINAL_PAD_Y,
        right: MAIN_MARGIN + TERMINAL_PAD_X,
        bottom: MAIN_MARGIN + TERMINAL_PAD_Y,
        left: SIDEBAR_WIDTH + TERMINAL_PAD_X,
    }
}

/// The shell's boxes for one window size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShellLayout {
    pub window: Rect,
    pub sidebar: Rect,
    /// The rounded main card.
    pub main: Rect,
    pub header: Rect,
    /// Pills row (only painted / hit on Terminal).
    pub pills: Rect,
}

impl ShellLayout {
    pub fn new(window_width: f32, window_height: f32) -> Self {
        let w = window_width.max(0.0);
        let h = window_height.max(0.0);
        let main = Rect::new(
            SIDEBAR_WIDTH,
            MAIN_MARGIN,
            (w - SIDEBAR_WIDTH - MAIN_MARGIN).max(0.0),
            (h - 2.0 * MAIN_MARGIN).max(0.0),
        );
        let header = Rect::new(main.x, main.y, main.width, HEADER_HEIGHT);
        let pills = Rect::new(main.x, header.bottom(), main.width, PILLS_HEIGHT);
        Self {
            window: Rect::new(0.0, 0.0, w, h),
            sidebar: Rect::new(0.0, 0.0, SIDEBAR_WIDTH.min(w), h),
            main,
            header,
            pills,
        }
    }

    /// Where `view` paints: everything under the header (and under the
    /// pills row on Terminal) inside the main card.
    pub fn content(&self, view: WorkspaceView) -> Rect {
        let top = if view.shows_pills() {
            self.pills.bottom()
        } else {
            self.header.bottom()
        };
        Rect::new(
            self.main.x,
            top,
            self.main.width,
            (self.main.bottom() - top).max(0.0),
        )
    }

    /// Minimize, maximize, close — left to right.
    pub fn window_controls(&self) -> [Rect; 3] {
        let x0 = self.main.right() - CONTROLS_PAD_RIGHT - 3.0 * CONTROL_WIDTH;
        let y = self.main.y + CONTROLS_PAD_TOP;
        [0.0, 1.0, 2.0]
            .map(|i| Rect::new(x0 + i * CONTROL_WIDTH, y, CONTROL_WIDTH, CONTROL_HEIGHT))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::workspace::SettingsPage;

    #[test]
    fn boxes_match_the_mock_at_1440x900() {
        let l = ShellLayout::new(1440.0, 900.0);
        assert_eq!(l.sidebar, Rect::new(0.0, 0.0, 260.0, 900.0));
        assert_eq!(l.main, Rect::new(260.0, 8.0, 1172.0, 884.0));
        assert_eq!(l.header, Rect::new(260.0, 8.0, 1172.0, 96.0));
        assert_eq!(l.pills, Rect::new(260.0, 104.0, 1172.0, 52.0));
    }

    #[test]
    fn content_sits_under_the_pills_on_terminal_and_under_the_header_elsewhere() {
        let l = ShellLayout::new(1440.0, 900.0);
        let t = l.content(WorkspaceView::Terminal);
        assert_eq!(t.y, 156.0);
        assert_eq!(t.bottom(), 892.0);
        for v in [
            WorkspaceView::Files,
            WorkspaceView::Home,
            WorkspaceView::Settings(SettingsPage::Updates),
        ] {
            let c = l.content(v);
            assert_eq!((c.x, c.y, c.width), (260.0, 104.0, 1172.0), "{v:?}");
            assert_eq!(c.bottom(), l.main.bottom());
        }
    }

    #[test]
    fn the_grid_inset_keeps_the_terminal_inside_the_terminal_content() {
        let l = ShellLayout::new(1440.0, 900.0);
        let i = grid_insets();
        let c = l.content(WorkspaceView::Terminal);
        assert!(i.top >= c.y, "grid starts under the pills");
        assert!(i.left >= c.x);
        assert!(1440.0 - i.right <= c.right());
        assert!(900.0 - i.bottom <= c.bottom());
        // With rio's default margin (2) the text starts 28px into the card.
        assert_eq!(i.left + 2.0 - l.main.x, 28.0);
    }

    #[test]
    fn window_controls_hug_the_header_top_right() {
        let l = ShellLayout::new(1440.0, 900.0);
        let [min, max, close] = l.window_controls();
        assert_eq!(close.right(), l.main.right() - 8.0);
        assert_eq!(min.y, l.main.y + 8.0);
        assert_eq!((min.width, min.height), (40.0, 32.0));
        assert_eq!(max.x, min.right());
        assert_eq!(close.x, max.right());
        assert!(close.bottom() <= l.header.bottom());
    }

    #[test]
    fn a_tiny_window_never_yields_negative_boxes() {
        let l = ShellLayout::new(100.0, 10.0);
        assert!(l.main.width >= 0.0 && l.main.height >= 0.0);
        let c = l.content(WorkspaceView::Terminal);
        assert!(c.height >= 0.0);
    }
}
