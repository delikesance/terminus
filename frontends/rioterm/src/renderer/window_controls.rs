//! Custom Windows caption buttons (minimize / maximize / close).
//!
//! Painted in the island title-bar band when Tab navigation runs without
//! OS decorations. macOS keeps traffic lights on the left instead.

use crate::renderer::chrome;
use crate::renderer::island::CONTEXT_BAR_HEIGHT;
use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::icons::{Icon, IconPlacement};

/// Logical width reserved on the right for the three caption buttons.
pub const MARGIN_RIGHT: f32 = 138.0;

const BUTTON_WIDTH: f32 = 46.0;
const BUTTON_COUNT: usize = 3;
const ICON_SIZE: f32 = 12.0;

const HOVER_IDLE: [f32; 4] = [1.0, 1.0, 1.0, 0.08];
const HOVER_CLOSE: [f32; 4] = [0.91, 0.18, 0.22, 1.0];
const ICON_IDLE: [f32; 4] = [0.85, 0.85, 0.85, 0.92];
const ICON_ON_CLOSE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

/// Which caption button the pointer is over (or pressed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowControl {
    Minimize,
    Maximize,
    Close,
}

impl WindowControl {
    fn index(self) -> usize {
        match self {
            Self::Minimize => 0,
            Self::Maximize => 1,
            Self::Close => 2,
        }
    }

    fn from_index(i: usize) -> Option<Self> {
        match i {
            0 => Some(Self::Minimize),
            1 => Some(Self::Maximize),
            2 => Some(Self::Close),
            _ => None,
        }
    }
}

/// Left edge of the caption button strip in logical pixels.
#[inline]
pub fn strip_left(window_width_logical: f32) -> f32 {
    (window_width_logical - MARGIN_RIGHT).max(0.0)
}

#[inline]
fn button_rect(window_width_logical: f32, control: WindowControl) -> (f32, f32, f32, f32) {
    let x = strip_left(window_width_logical) + control.index() as f32 * BUTTON_WIDTH;
    (x, 0.0, BUTTON_WIDTH, CONTEXT_BAR_HEIGHT)
}

/// Hit-test in logical coordinates. `y` must already be inside the band.
#[inline]
pub fn hit_test(window_width_logical: f32, x: f32, y: f32) -> Option<WindowControl> {
    if y < 0.0 || y > CONTEXT_BAR_HEIGHT {
        return None;
    }
    let left = strip_left(window_width_logical);
    if x < left || x >= left + BUTTON_WIDTH * BUTTON_COUNT as f32 {
        return None;
    }
    let index = ((x - left) / BUTTON_WIDTH).floor() as usize;
    WindowControl::from_index(index.min(BUTTON_COUNT - 1))
}

/// Paint the three caption buttons. `maximized` selects Square vs Copy.
pub fn render(
    sugarloaf: &mut Sugarloaf,
    window_width_logical: f32,
    scale_factor: f32,
    maximized: bool,
    hover: Option<WindowControl>,
    icon_color: [f32; 4],
) {
    for i in 0..BUTTON_COUNT {
        let control = WindowControl::from_index(i).expect("button index");
        let (x, y, w, h) = button_rect(window_width_logical, control);
        let is_hover = hover == Some(control);

        if is_hover {
            let fill = if control == WindowControl::Close {
                HOVER_CLOSE
            } else {
                HOVER_IDLE
            };
            sugarloaf.rect(None, x, y, w, h, fill, 0.0, 20);
        }

        let icon = match control {
            WindowControl::Minimize => Icon::Minus,
            WindowControl::Maximize => {
                if maximized {
                    Icon::Copy
                } else {
                    Icon::Square
                }
            }
            WindowControl::Close => Icon::X,
        };

        let color = if is_hover && control == WindowControl::Close {
            ICON_ON_CLOSE
        } else {
            [
                icon_color[0],
                icon_color[1],
                icon_color[2],
                icon_color[3].max(ICON_IDLE[3]),
            ]
        };

        let ix = x + (w - ICON_SIZE) / 2.0;
        let iy = y + (h - ICON_SIZE) / 2.0;
        chrome::draw_icon(
            sugarloaf,
            icon,
            IconPlacement::new(ix, iy, ICON_SIZE),
            color,
            scale_factor,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_test_maps_three_slots() {
        let w = 800.0;
        let left = strip_left(w);
        assert_eq!(
            hit_test(w, left + 1.0, CONTEXT_BAR_HEIGHT / 2.0),
            Some(WindowControl::Minimize)
        );
        assert_eq!(
            hit_test(w, left + BUTTON_WIDTH + 1.0, CONTEXT_BAR_HEIGHT / 2.0),
            Some(WindowControl::Maximize)
        );
        assert_eq!(
            hit_test(w, left + BUTTON_WIDTH * 2.0 + 1.0, CONTEXT_BAR_HEIGHT / 2.0),
            Some(WindowControl::Close)
        );
        assert_eq!(hit_test(w, left - 1.0, CONTEXT_BAR_HEIGHT / 2.0), None);
        assert_eq!(hit_test(w, left + 1.0, CONTEXT_BAR_HEIGHT + 1.0), None);
    }

    #[test]
    fn margin_fits_three_buttons() {
        assert_eq!(MARGIN_RIGHT, BUTTON_WIDTH * BUTTON_COUNT as f32);
    }
}
