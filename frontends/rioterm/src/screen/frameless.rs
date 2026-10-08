//! Frameless-window behaviour on Linux (Wayland and X11).
//!
//! With OS decorations off (Tab navigation, see
//! `router::window::uses_custom_titlebar`) the compositor draws no
//! resize handles and no window menu, so the app asks for them: edge
//! hit-testing from `terminus_ui::window_edge`, then
//! `Window::drag_resize_window` / `Window::show_window_menu`.
//!
//! macOS keeps its native frame and Windows resizes through
//! `WM_NCHITTEST` in rio-window, so both get inert stubs.

use super::Screen;
use rio_window::window::{ResizeDirection, Window};
use terminus_ui::window_edge::ResizeEdge;

/// The rio-window resize direction for a hit-tested edge.
#[cfg_attr(any(target_os = "macos", windows), allow(dead_code))]
pub(crate) fn direction_for(edge: ResizeEdge) -> ResizeDirection {
    match edge {
        ResizeEdge::North => ResizeDirection::North,
        ResizeEdge::South => ResizeDirection::South,
        ResizeEdge::East => ResizeDirection::East,
        ResizeEdge::West => ResizeDirection::West,
        ResizeEdge::NorthEast => ResizeDirection::NorthEast,
        ResizeEdge::NorthWest => ResizeDirection::NorthWest,
        ResizeEdge::SouthEast => ResizeDirection::SouthEast,
        ResizeEdge::SouthWest => ResizeDirection::SouthWest,
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
impl Screen<'_> {
    /// The resize direction under the pointer, if edge resizing applies:
    /// the custom title bar is active, this is not the quake dropdown,
    /// and the window is neither maximised nor fullscreen.
    fn frameless_resize_dir(
        &self,
        window: &Window,
        quake: bool,
    ) -> Option<ResizeDirection> {
        if quake || !self.custom_titlebar {
            return None;
        }
        let scale = self.sugarloaf.scale_factor();
        let size = self.sugarloaf.window_size();
        let locked = window.is_maximized() || window.fullscreen().is_some();
        terminus_ui::window_edge::resize_edge_at(
            (size.width / scale, size.height / scale),
            self.mouse.x as f32 / scale,
            self.mouse.y as f32 / scale,
            terminus_ui::window_edge::RESIZE_BORDER,
            terminus_ui::window_edge::RESIZE_CORNER,
            locked,
        )
        .map(direction_for)
    }

    /// Pointer-move hook: show the resize cursor on an edge and report
    /// that the edge owns the pointer (`Some(repaint)`; `repaint` when a
    /// caption-button hover had to be cleared). Leaving the edge restores the
    /// default cursor and forces the grid path to re-evaluate its own
    /// (its "same cell" shortcut would otherwise keep the old one).
    /// Inactive while a button is held, so selections and drags that
    /// brush the border are left alone.
    pub fn frameless_edge_hover(&mut self, window: &Window, quake: bool) -> Option<bool> {
        let held =
            self.mouse.left_button_state == rio_window::event::ElementState::Pressed;
        let dir = if held {
            None
        } else {
            self.frameless_resize_dir(window, quake)
        };
        match dir {
            Some(dir) => {
                window.set_cursor(rio_window::window::CursorIcon::from(dir));
                self.on_resize_edge = true;
                // Force the grid hover path to re-run once we leave.
                self.mouse.on_border = true;
                // Coming straight from a header button: drop its hover.
                Some(self.chrome.shell.set_hover(None))
            }
            None => {
                if self.on_resize_edge {
                    self.on_resize_edge = false;
                    window.set_cursor(rio_window::window::CursorIcon::Default);
                    self.mouse.last_cell = None;
                }
                None
            }
        }
    }

    /// Left-press hook: start a compositor-driven resize when the press
    /// lands on an edge. Returns whether the press was consumed.
    pub fn frameless_edge_press(&mut self, window: &Window, quake: bool) -> bool {
        let Some(dir) = self.frameless_resize_dir(window, quake) else {
            return false;
        };
        // The compositor grab can swallow the release; do not leave a
        // phantom held button behind (it would extend a selection).
        self.mouse.left_button_state = rio_window::event::ElementState::Released;
        window.drag_resize_window(dir).is_ok()
    }

    /// Right-click on the empty header: the compositor's window menu
    /// (move / resize / maximise / close). Wayland only: rio-window's
    /// X11 backend leaves `show_window_menu` empty, so there the click
    /// is swallowed without a menu (as header right-clicks were before).
    pub fn frameless_header_menu(&mut self, window: &Window, quake: bool) -> bool {
        if quake || !self.custom_titlebar {
            return false;
        }
        let scale = self.sugarloaf.scale_factor();
        let (x, y) = (self.mouse.x as f32 / scale, self.mouse.y as f32 / scale);
        if self.chrome.shell.hit_test(x, y) != Some(terminus_ui::shell::ShellHit::Drag) {
            return false;
        }
        window
            .show_window_menu(rio_window::dpi::LogicalPosition::new(x as f64, y as f64));
        true
    }
}

#[cfg(any(target_os = "macos", windows))]
impl Screen<'_> {
    pub fn frameless_edge_hover(
        &mut self,
        _window: &Window,
        _quake: bool,
    ) -> Option<bool> {
        None
    }

    pub fn frameless_edge_press(&mut self, _window: &Window, _quake: bool) -> bool {
        false
    }

    pub fn frameless_header_menu(&mut self, _window: &Window, _quake: bool) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_edge_maps_to_its_own_direction() {
        use ResizeDirection as D;
        use ResizeEdge as E;
        let pairs = [
            (E::North, D::North),
            (E::South, D::South),
            (E::East, D::East),
            (E::West, D::West),
            (E::NorthEast, D::NorthEast),
            (E::NorthWest, D::NorthWest),
            (E::SouthEast, D::SouthEast),
            (E::SouthWest, D::SouthWest),
        ];
        for (edge, dir) in pairs {
            assert_eq!(direction_for(edge), dir);
        }
    }
}
