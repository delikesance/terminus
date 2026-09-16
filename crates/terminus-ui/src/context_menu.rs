//! Floating right-click context menu: paint-free geometry and hit-testing.
//!
//! The menu is a small list anchored at the pointer. Items carry a
//! [`ContextAction`] so the chrome can route a selection without the
//! painter knowing about hosts or clipboard.

use crate::geom::Rect;

pub const ITEM_HEIGHT: f32 = 32.0;
pub const MENU_PAD_Y: f32 = 4.0;
pub const MENU_PAD_X: f32 = 4.0;
pub const MENU_MIN_WIDTH: f32 = 160.0;
pub const MENU_RADIUS: f32 = 10.0;
/// Approximate label advance used before the painter measures glyphs.
const LABEL_ESTIMATE: f32 = 7.0;

/// What selecting a context-menu row should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextAction {
    /// Soft-delete a stored SSH host.
    DeleteHost(String),
    /// Soft-delete a host group (hosts inside become ungrouped by the store).
    DeleteGroup(String),
    /// Begin renaming a stored SSH host.
    RenameHost(String),
    /// Begin renaming a host group.
    RenameGroup(String),
    /// Copy the active selection / clipboard write (terminal / fields).
    Copy,
    /// Paste clipboard into the focused sink.
    Paste,
}

/// One row in the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextItem {
    pub label: String,
    pub action: ContextAction,
    /// Destructive styling (e.g. Delete).
    pub danger: bool,
}

impl ContextItem {
    pub fn new(label: impl Into<String>, action: ContextAction) -> Self {
        Self {
            label: label.into(),
            action,
            danger: false,
        }
    }

    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }
}

/// What a press on an open context menu hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMenuHit {
    /// An item row.
    Item(usize),
    /// Inside the menu chrome but not on a row.
    Consume,
    /// Outside the menu — dismiss.
    Dismiss,
}

/// Live context menu.
#[derive(Debug, Clone, PartialEq)]
pub struct ContextMenu {
    pub x: f32,
    pub y: f32,
    pub items: Vec<ContextItem>,
    pub hover: Option<usize>,
    /// Measured (or estimated) width of the widest label.
    width: f32,
}

impl ContextMenu {
    pub fn open(x: f32, y: f32, items: Vec<ContextItem>) -> Option<Self> {
        if items.is_empty() {
            return None;
        }
        let width = items
            .iter()
            .map(|i| i.label.chars().count() as f32 * LABEL_ESTIMATE + 24.0)
            .fold(MENU_MIN_WIDTH, f32::max);
        Some(Self {
            x,
            y,
            items,
            hover: None,
            width,
        })
    }

    /// Host row context: rename or delete the stored host.
    pub fn for_host(x: f32, y: f32, host_id: impl Into<String>) -> Option<Self> {
        let id = host_id.into();
        Self::open(
            x,
            y,
            vec![
                ContextItem::new("Rename", ContextAction::RenameHost(id.clone())),
                ContextItem::new("Delete host", ContextAction::DeleteHost(id)).danger(),
            ],
        )
    }

    /// Group header context: rename or delete the group.
    pub fn for_group(x: f32, y: f32, group_id: impl Into<String>) -> Option<Self> {
        let id = group_id.into();
        Self::open(
            x,
            y,
            vec![
                ContextItem::new("Rename", ContextAction::RenameGroup(id.clone())),
                ContextItem::new("Delete group", ContextAction::DeleteGroup(id)).danger(),
            ],
        )
    }

    /// Terminal / text field context: copy + paste.
    pub fn for_edit(x: f32, y: f32) -> Option<Self> {
        Self::open(
            x,
            y,
            vec![
                ContextItem::new("Copy", ContextAction::Copy),
                ContextItem::new("Paste", ContextAction::Paste),
            ],
        )
    }

    pub fn set_measured_width(&mut self, width: f32) {
        self.width = width.max(MENU_MIN_WIDTH);
    }

    pub fn height(&self) -> f32 {
        MENU_PAD_Y * 2.0 + self.items.len() as f32 * ITEM_HEIGHT
    }

    /// Clamp the menu into the window so it never hangs off-screen.
    pub fn clamped(mut self, window_width: f32, window_height: f32) -> Self {
        let w = self.width;
        let h = self.height();
        if self.x + w > window_width - 8.0 {
            self.x = (window_width - w - 8.0).max(8.0);
        }
        if self.y + h > window_height - 8.0 {
            self.y = (window_height - h - 8.0).max(8.0);
        }
        self.x = self.x.max(8.0);
        self.y = self.y.max(8.0);
        self
    }

    pub fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.width, self.height())
    }

    pub fn item_rect(&self, index: usize) -> Option<Rect> {
        if index >= self.items.len() {
            return None;
        }
        Some(Rect::new(
            self.x + MENU_PAD_X,
            self.y + MENU_PAD_Y + index as f32 * ITEM_HEIGHT,
            self.width - 2.0 * MENU_PAD_X,
            ITEM_HEIGHT,
        ))
    }

    pub fn hit_test(&self, x: f32, y: f32) -> ContextMenuHit {
        if !self.rect().contains(x, y) {
            return ContextMenuHit::Dismiss;
        }
        for i in 0..self.items.len() {
            if let Some(r) = self.item_rect(i) {
                if r.contains(x, y) {
                    return ContextMenuHit::Item(i);
                }
            }
        }
        ContextMenuHit::Consume
    }

    pub fn set_hover(&mut self, index: Option<usize>) -> bool {
        if self.hover == index {
            return false;
        }
        self.hover = index;
        true
    }

    pub fn hover_at(&mut self, x: f32, y: f32) -> bool {
        let mut hover = None;
        for i in 0..self.items.len() {
            if let Some(r) = self.item_rect(i) {
                if r.contains(x, y) {
                    hover = Some(i);
                    break;
                }
            }
        }
        self.set_hover(hover)
    }

    pub fn take_action(&self, index: usize) -> Option<ContextAction> {
        self.items.get(index).map(|i| i.action.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_menu_hits_delete_and_dismisses_outside() {
        let menu = ContextMenu::for_host(100.0, 100.0, "h1").unwrap();
        assert_eq!(menu.items.len(), 2);
        let item = menu.item_rect(1).unwrap();
        assert_eq!(
            menu.hit_test(item.x + 2.0, item.y + 2.0),
            ContextMenuHit::Item(1)
        );
        assert_eq!(menu.hit_test(0.0, 0.0), ContextMenuHit::Dismiss);
        assert_eq!(
            menu.take_action(0),
            Some(ContextAction::RenameHost("h1".into()))
        );
        assert_eq!(
            menu.take_action(1),
            Some(ContextAction::DeleteHost("h1".into()))
        );
    }

    #[test]
    fn clamps_into_the_window() {
        let menu = ContextMenu::for_host(2000.0, 2000.0, "h1")
            .unwrap()
            .clamped(800.0, 600.0);
        assert!(menu.x + menu.width <= 800.0);
        assert!(menu.y + menu.height() <= 600.0);
    }
}
