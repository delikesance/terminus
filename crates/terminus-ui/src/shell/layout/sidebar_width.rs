//! Resizable sidebar width: clamping, drag state, handle rect and the
//! key/value form persisted in the settings store (`sidebar_width`).
//!
//! Geometry lives here (terminus-ui) so the painter and the hit-test share
//! it. The frontend still has to feed pointer events to [`SidebarWidth`]
//! and replay `reapply_chrome_inset` when [`SidebarWidth::get`] changes.

use super::{ShellLayout, SIDEBAR_WIDTH};
use crate::geom::Rect;

/// Settings-store key holding the width as a decimal string.
pub const SETTING_KEY: &str = "sidebar_width";
pub const MIN_SIDEBAR_WIDTH: f32 = 200.0;
pub const MAX_SIDEBAR_WIDTH: f32 = 480.0;
/// Half-width of the grab zone centred on the sidebar's right edge.
pub const HANDLE_HALF: f32 = 4.0;

pub fn clamp_width(w: f32) -> f32 {
    if w.is_nan() {
        return SIDEBAR_WIDTH;
    }
    w.clamp(MIN_SIDEBAR_WIDTH, MAX_SIDEBAR_WIDTH)
}

/// Parse a persisted value; anything unusable falls back to the default.
pub fn parse_setting(raw: Option<&str>) -> f32 {
    raw.and_then(|s| s.trim().parse::<f32>().ok())
        .filter(|w| w.is_finite())
        .map_or(SIDEBAR_WIDTH, clamp_width)
}

pub fn format_setting(width: f32) -> String {
    format!("{}", clamp_width(width).round())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SidebarWidth {
    width: f32,
    /// Pointer x minus width at press time.
    drag_offset: Option<f32>,
}

impl Default for SidebarWidth {
    fn default() -> Self {
        Self::new(SIDEBAR_WIDTH)
    }
}

impl SidebarWidth {
    pub fn new(width: f32) -> Self {
        Self {
            width: clamp_width(width),
            drag_offset: None,
        }
    }

    pub fn get(&self) -> f32 {
        self.width
    }

    pub fn is_dragging(&self) -> bool {
        self.drag_offset.is_some()
    }

    /// Grab zone straddling the sidebar's right edge.
    pub fn handle_rect(&self, window_height: f32) -> Rect {
        Rect::new(
            self.width - HANDLE_HALF,
            0.0,
            2.0 * HANDLE_HALF,
            window_height.max(0.0),
        )
    }

    pub fn hit_handle(&self, x: f32, y: f32, window_height: f32) -> bool {
        self.handle_rect(window_height).contains(x, y)
    }

    pub fn begin_drag(&mut self, x: f32) {
        self.drag_offset = Some(x - self.width);
    }

    /// Returns true when the width changed (caller replays the inset).
    pub fn drag_to(&mut self, x: f32) -> bool {
        let Some(off) = self.drag_offset else {
            return false;
        };
        let next = clamp_width(x - off);
        let changed = next != self.width;
        self.width = next;
        changed
    }

    /// Ends the drag; returns the width to persist, if a drag was active.
    pub fn end_drag(&mut self) -> Option<f32> {
        self.drag_offset.take().map(|_| self.width)
    }
}

impl ShellLayout {
    /// Same boxes as [`ShellLayout::new`] with a custom sidebar width.
    pub fn with_sidebar_width(
        window_width: f32,
        window_height: f32,
        sidebar: f32,
    ) -> Self {
        let sidebar = clamp_width(sidebar);
        let base = Self::new(window_width, window_height);
        let w = base.window.width;
        let dx = sidebar - SIDEBAR_WIDTH;
        let shift = |r: Rect| Rect::new(r.x + dx, r.y, (r.width - dx).max(0.0), r.height);
        Self {
            window: base.window,
            sidebar: Rect::new(0.0, 0.0, sidebar.min(w), base.window.height),
            main: shift(base.main),
            header: shift(base.header),
            pills: shift(base.pills),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::grid_insets_for;

    #[test]
    fn clamps_and_defaults() {
        assert_eq!(clamp_width(10.0), MIN_SIDEBAR_WIDTH);
        assert_eq!(clamp_width(9999.0), MAX_SIDEBAR_WIDTH);
        assert_eq!(clamp_width(f32::NAN), SIDEBAR_WIDTH);
        assert_eq!(SidebarWidth::default().get(), 260.0);
    }

    #[test]
    fn setting_round_trips_and_rejects_garbage() {
        assert_eq!(parse_setting(Some(&format_setting(300.4))), 300.0);
        assert_eq!(parse_setting(None), SIDEBAR_WIDTH);
        assert_eq!(parse_setting(Some("abc")), SIDEBAR_WIDTH);
        assert_eq!(parse_setting(Some("inf")), SIDEBAR_WIDTH);
        assert_eq!(parse_setting(Some("5")), MIN_SIDEBAR_WIDTH);
    }

    #[test]
    fn drag_follows_pointer_without_jump_and_reports_persist_value() {
        let mut s = SidebarWidth::default();
        assert!(s.hit_handle(262.0, 100.0, 900.0));
        assert!(!s.hit_handle(300.0, 100.0, 900.0));
        assert!(!s.drag_to(400.0), "no drag, no change");
        s.begin_drag(262.0);
        assert!(s.drag_to(302.0));
        assert_eq!(s.get(), 300.0);
        assert!(s.drag_to(5000.0));
        assert_eq!(s.get(), MAX_SIDEBAR_WIDTH);
        assert_eq!(s.end_drag(), Some(MAX_SIDEBAR_WIDTH));
        assert_eq!(s.end_drag(), None);
    }

    #[test]
    fn layout_and_grid_inset_follow_the_width() {
        let l = ShellLayout::with_sidebar_width(1440.0, 900.0, 320.0);
        assert_eq!(l.sidebar.width, 320.0);
        assert_eq!(l.main.x, 320.0);
        assert_eq!(l.main.right(), 1440.0 - 8.0);
        assert_eq!(l.header.x, 320.0);
        assert_eq!(l.pills.x, 320.0);
        assert_eq!(grid_insets_for(320.0).left, 320.0 + 26.0);
        assert_eq!(
            ShellLayout::with_sidebar_width(1440.0, 900.0, 260.0),
            ShellLayout::new(1440.0, 900.0)
        );
    }
}
