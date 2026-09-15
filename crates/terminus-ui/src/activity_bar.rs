//! Activity bar: the fixed icon rail on the window's left edge.
//!
//! Top: Servers / Snippets (drawer views).
//! Bottom: Cloud Sync / Settings (open the settings modal).

use crate::geom::Rect;
use crate::icons::Icon;

/// Rail width, in logical pixels (`w-16` = 64).
pub const WIDTH: f32 = 64.0;
/// Side of one section button.
pub const ITEM_SIZE: f32 = 44.0;
/// Vertical gap between buttons.
pub const ITEM_GAP: f32 = 12.0;
/// Space above the first top button.
pub const TOP_PAD: f32 = 16.0;
/// Space below the last bottom button.
pub const BOTTOM_PAD: f32 = 16.0;
/// Side of the icon drawn inside a button.
pub const ICON_SIZE: f32 = 20.0;
/// Horizontal inset of the active pill inside the rail.
pub const PILL_INSET_X: f32 = 10.0;
/// Corner radius of the active pill (`rounded-xl`).
pub const PILL_RADIUS: f32 = 12.0;

/// Drawer views the rail can select.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    /// Servers & Hosts drawer (formerly Hosts).
    Servers,
    /// Command snippets drawer.
    Snippets,
}

/// Bottom-pinned actions that open settings (not drawer views).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RailAction {
    CloudSync,
    Settings,
}

/// Hit result for a press on the rail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RailHit {
    Section(Section),
    Action(RailAction),
}

/// Top sections, in paint order.
pub const TOP_SECTIONS: [Section; 2] = [Section::Servers, Section::Snippets];

/// Bottom actions, top-to-bottom within the bottom cluster.
pub const BOTTOM_ACTIONS: [RailAction; 2] = [RailAction::CloudSync, RailAction::Settings];

/// Compatibility alias used by older call sites.
pub const SECTIONS: [Section; 2] = TOP_SECTIONS;

impl Section {
    pub const fn icon(self) -> Icon {
        match self {
            Section::Servers => Icon::SquareTerminal,
            Section::Snippets => Icon::CodeXml,
        }
    }

    pub const fn index(self) -> usize {
        match self {
            Section::Servers => 0,
            Section::Snippets => 1,
        }
    }
}

impl RailAction {
    pub const fn icon(self) -> Icon {
        match self {
            RailAction::CloudSync => Icon::CloudUpload,
            RailAction::Settings => Icon::Settings,
        }
    }

    pub const fn index(self) -> usize {
        match self {
            RailAction::CloudSync => 0,
            RailAction::Settings => 1,
        }
    }
}

/// Rail state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityBarState {
    pub selected: Section,
    pub collapsed: bool,
    /// Cloud sync is active (emerald tint on the cloud icon).
    pub cloud_sync_active: bool,
}

impl Default for ActivityBarState {
    fn default() -> Self {
        Self {
            selected: Section::Servers,
            collapsed: false,
            cloud_sync_active: true,
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

/// Box of a top section button.
pub fn section_rect(origin_y: f32, section: Section) -> Rect {
    Rect::new(
        0.0,
        origin_y + TOP_PAD + section.index() as f32 * (ITEM_SIZE + ITEM_GAP),
        WIDTH,
        ITEM_SIZE,
    )
}

/// Box of a bottom action button (pinned to the rail bottom).
pub fn action_rect(origin_y: f32, height: f32, action: RailAction) -> Rect {
    let count = BOTTOM_ACTIONS.len() as f32;
    let cluster = count * ITEM_SIZE + (count - 1.0) * ITEM_GAP;
    let top = origin_y + height - BOTTOM_PAD - cluster;
    Rect::new(
        0.0,
        top + action.index() as f32 * (ITEM_SIZE + ITEM_GAP),
        WIDTH,
        ITEM_SIZE,
    )
}

/// Active pill inset inside a button box.
pub fn pill_rect(item: Rect) -> Rect {
    Rect::new(
        item.x + PILL_INSET_X,
        item.y,
        item.width - 2.0 * PILL_INSET_X,
        item.height,
    )
}

/// Icon box centered in a button.
pub fn icon_in(item: Rect) -> Rect {
    Rect::new(
        item.x + (WIDTH - ICON_SIZE) / 2.0,
        item.y + (ITEM_SIZE - ICON_SIZE) / 2.0,
        ICON_SIZE,
        ICON_SIZE,
    )
}

/// Legacy index-based helper used by older tests/painters.
pub fn item_rect(origin_y: f32, index: usize) -> Rect {
    section_rect(
        origin_y,
        TOP_SECTIONS
            .get(index)
            .copied()
            .unwrap_or(Section::Servers),
    )
}

pub fn icon_rect(origin_y: f32, index: usize) -> Rect {
    icon_in(item_rect(origin_y, index))
}

pub fn marker_rect(origin_y: f32, index: usize) -> Rect {
    // Kept for API compatibility; Apple HIG uses a filled pill instead.
    let item = item_rect(origin_y, index);
    Rect::new(item.x, item.y, 0.0, item.height)
}

/// Which rail target is under `(x, y)`.
pub fn hit_test(origin_y: f32, height: f32, x: f32, y: f32) -> Option<RailHit> {
    if x < 0.0 || x >= WIDTH {
        return None;
    }
    for section in TOP_SECTIONS {
        if section_rect(origin_y, section).contains(x, y) {
            return Some(RailHit::Section(section));
        }
    }
    for action in BOTTOM_ACTIONS {
        if action_rect(origin_y, height, action).contains(x, y) {
            return Some(RailHit::Action(action));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rail_width_matches_mock_w16() {
        assert_eq!(WIDTH, 64.0);
        assert_eq!(ITEM_SIZE, 44.0);
        assert_eq!(PILL_RADIUS, 12.0);
    }

    #[test]
    fn top_buttons_stack_without_overlapping() {
        let a = section_rect(0.0, Section::Servers);
        let b = section_rect(0.0, Section::Snippets);
        assert_eq!(a.y, TOP_PAD);
        assert!(b.y >= a.bottom());
    }

    #[test]
    fn bottom_actions_pin_to_the_rail_bottom() {
        let height = 600.0;
        let settings = action_rect(0.0, height, RailAction::Settings);
        assert!((settings.bottom() - (height - BOTTOM_PAD)).abs() < 0.01);
        let cloud = action_rect(0.0, height, RailAction::CloudSync);
        assert!(cloud.y < settings.y);
    }

    #[test]
    fn hit_test_finds_sections_and_actions() {
        let height = 600.0;
        let servers = section_rect(0.0, Section::Servers);
        assert_eq!(
            hit_test(0.0, height, WIDTH / 2.0, servers.y + 10.0),
            Some(RailHit::Section(Section::Servers))
        );
        let settings = action_rect(0.0, height, RailAction::Settings);
        assert_eq!(
            hit_test(0.0, height, WIDTH / 2.0, settings.y + 10.0),
            Some(RailHit::Action(RailAction::Settings))
        );
        assert_eq!(hit_test(0.0, height, WIDTH / 2.0, 300.0), None);
    }
}
