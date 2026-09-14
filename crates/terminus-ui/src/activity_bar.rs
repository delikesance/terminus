//! Activity bar: the fixed icon rail on the window's left edge.

use crate::geom::Rect;
use crate::icons::Icon;

/// Rail width, in logical pixels.
pub const WIDTH: f32 = 48.0;
/// Side of one section button.
pub const ITEM_SIZE: f32 = 40.0;
/// Vertical gap between buttons.
pub const ITEM_GAP: f32 = 4.0;
/// Space above the first button.
pub const TOP_PAD: f32 = 10.0;
/// Side of the icon drawn inside a button.
pub const ICON_SIZE: f32 = 18.0;
/// Width of the accent bar marking the selected section.
pub const MARKER_WIDTH: f32 = 2.0;

/// The sections the rail can select, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Hosts,
    Forwards,
    Sftp,
    Settings,
}

/// Every section, in the order it is painted.
pub const SECTIONS: [Section; 4] = [
    Section::Hosts,
    Section::Forwards,
    Section::Sftp,
    Section::Settings,
];

impl Section {
    pub const fn icon(self) -> Icon {
        match self {
            Section::Hosts => Icon::Server,
            Section::Forwards => Icon::ArrowRightLeft,
            Section::Sftp => Icon::Folder,
            Section::Settings => Icon::SlidersHorizontal,
        }
    }

    /// Index in [`SECTIONS`].
    pub const fn index(self) -> usize {
        match self {
            Section::Hosts => 0,
            Section::Forwards => 1,
            Section::Sftp => 2,
            Section::Settings => 3,
        }
    }
}

/// Rail state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityBarState {
    pub selected: Section,
    pub collapsed: bool,
}

impl Default for ActivityBarState {
    fn default() -> Self {
        Self {
            selected: Section::Hosts,
            collapsed: false,
        }
    }
}

impl ActivityBarState {
    pub fn toggle_collapse(&mut self) {
        self.collapsed = !self.collapsed;
    }
}

/// The rail's own box, given the chrome origin.
pub fn rect(origin_y: f32, height: f32) -> Rect {
    Rect::new(0.0, origin_y, WIDTH, height)
}

/// The box of the `index`th button.
pub fn item_rect(origin_y: f32, index: usize) -> Rect {
    Rect::new(
        0.0,
        origin_y + TOP_PAD + index as f32 * (ITEM_SIZE + ITEM_GAP),
        WIDTH,
        ITEM_SIZE,
    )
}

/// The icon box inside a button.
pub fn icon_rect(origin_y: f32, index: usize) -> Rect {
    let item = item_rect(origin_y, index);
    Rect::new(
        (WIDTH - ICON_SIZE) / 2.0,
        item.y + (ITEM_SIZE - ICON_SIZE) / 2.0,
        ICON_SIZE,
        ICON_SIZE,
    )
}

/// The accent marker of the `index`th button.
pub fn marker_rect(origin_y: f32, index: usize) -> Rect {
    Rect::new(0.0, item_rect(origin_y, index).y, MARKER_WIDTH, ITEM_SIZE)
}

/// Which section is under `(x, y)`, in logical pixels.
///
/// `None` below the last button: the rail's background is not
/// clickable, so a press there reaches the terminal instead of being
/// swallowed.
pub fn hit_test(origin_y: f32, x: f32, y: f32) -> Option<Section> {
    if x < 0.0 || x >= WIDTH {
        return None;
    }
    SECTIONS
        .iter()
        .copied()
        .find(|section| item_rect(origin_y, section.index()).contains(x, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_stack_without_overlapping() {
        let a = item_rect(0.0, 0);
        let b = item_rect(0.0, 1);
        assert_eq!(a.y, TOP_PAD);
        assert_eq!(b.y, TOP_PAD + ITEM_SIZE + ITEM_GAP);
        assert!(b.y >= a.bottom());
    }

    #[test]
    fn hit_test_finds_each_section_and_nothing_below_them() {
        for section in SECTIONS {
            let rect = item_rect(0.0, section.index());
            let (x, y) = (WIDTH / 2.0, rect.y + ITEM_SIZE / 2.0);
            assert_eq!(hit_test(0.0, x, y), Some(section));
        }

        let below = TOP_PAD + 4.0 * (ITEM_SIZE + ITEM_GAP) + 20.0;
        assert_eq!(hit_test(0.0, WIDTH / 2.0, below), None);
        // The rail never claims the space above its first button, and
        // never anything to the right of itself.
        assert_eq!(hit_test(0.0, WIDTH / 2.0, 1.0), None);
        assert_eq!(hit_test(0.0, WIDTH, item_rect(0.0, 0).y + 4.0), None);
    }

    #[test]
    fn hit_test_follows_the_chrome_origin() {
        // The same pixel names different sections depending on where the
        // chrome starts: the rail slides down under the tab strip. The
        // row pitch is 44 and the inset is less than that, so only a
        // pixel near a row's end changes section.
        let y = item_rect(38.0, 0).bottom() - 1.0;
        let x = WIDTH / 2.0;
        assert_eq!(hit_test(38.0, x, y), Some(Section::Hosts));
        assert_eq!(hit_test(0.0, x, y), Some(Section::Forwards));
    }

    #[test]
    fn the_icon_is_centered_in_its_button() {
        let item = item_rect(0.0, 1);
        let icon = icon_rect(0.0, 1);
        assert!((icon.y - item.y) - (item.height - icon.height) / 2.0 < f32::EPSILON);
        assert!((icon.x - (WIDTH - ICON_SIZE) / 2.0).abs() < f32::EPSILON);
    }
}
