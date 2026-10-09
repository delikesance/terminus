//! Floating right-click context menu: paint-free geometry and hit-testing.
//!
//! [`ContextMenu`] is the reusable chrome component for any pointer-anchored
//! list of actions. Height is always derived from `items.len()` via
//! [`ContextMenu::height`] — never a fixed shell — so host (3 rows) and group
//! (2 rows) menus paint at different sizes. Callers build items then
//! [`ContextMenu::open`] / [`ContextMenu::clamped`]; the painter only consumes
//! [`ContextMenu::rect`] and [`ContextMenu::item_rect`].

use crate::geom::Rect;

pub const ITEM_HEIGHT: f32 = 36.0;
pub const MENU_PAD_Y: f32 = 6.0;
pub const MENU_PAD_X: f32 = 6.0;
pub const MENU_MIN_WIDTH: f32 = 236.0;
pub const MENU_RADIUS: f32 = 12.0;
/// Divider row (1px line with 5px margins) before a flagged item.
pub const SEPARATOR_HEIGHT: f32 = 11.0;
/// Approximate label advance used before the painter measures glyphs.
const LABEL_ESTIMATE: f32 = 7.0;

/// What selecting a context-menu row should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextAction {
    /// Open another session on a stored SSH host.
    NewSession(String),
    /// Put the host's `ssh …` command line on the clipboard.
    CopySshCommand(String),
    /// Soft-delete a stored SSH host.
    DeleteHost(String),
    /// Soft-delete a host group (hosts inside become ungrouped by the store).
    DeleteGroup(String),
    /// Open the host editor prefilled for a stored SSH host.
    EditHost(String),
    /// Open the dual-pane SFTP browser for a stored SSH host (default: Local | Host).
    OpenSftp(String),
    /// Open a stored host in the other SFTP pane (Host | Host, or swap Local).
    OpenSftpOtherPane(String),
    /// Begin renaming a stored SSH host.
    RenameHost(String),
    /// Begin renaming a host group.
    RenameGroup(String),
    /// Open a session on every host of a group.
    OpenGroup(String),
    /// Close every session opened from the hosts of a group.
    CloseGroup(String),
    /// Begin renaming the session pill of this tab index.
    RenameSession(usize),
    /// Close the session of this tab index.
    CloseSession(usize),
    /// Copy the active selection / clipboard write (terminal / fields).
    Copy,
    /// Paste clipboard into the focused sink.
    Paste,
    /// SFTP: create a folder on the focused side.
    SftpNewFolder,
    /// SFTP: rename the selected entry.
    SftpRename,
    /// Sftp: delete the selected entry.
    SftpDelete,
    /// SFTP: transfer the selected file to the other pane.
    SftpTransfer,
    /// SFTP: download a remote file to temp, open in the default app, reupload on change.
    SftpEdit,
    /// SFTP: enter the selected directory.
    SftpOpen,
    /// SFTP: refresh the focused pane listing.
    SftpRefresh,
}

/// One row in the menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextItem {
    pub label: String,
    pub action: ContextAction,
    /// Destructive styling (e.g. Delete).
    pub danger: bool,
    /// A divider is drawn above this row.
    pub separator_before: bool,
}

impl ContextItem {
    pub fn new(label: impl Into<String>, action: ContextAction) -> Self {
        Self {
            label: label.into(),
            action,
            danger: false,
            separator_before: false,
        }
    }

    pub fn sep(mut self) -> Self {
        self.separator_before = true;
        self
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

    /// Host row context: edit, SFTP, rename, or delete the stored host.
    ///
    /// When `sftp_open` is true, also offers opening the host in the other SFTP pane.
    pub fn for_host(x: f32, y: f32, host_id: impl Into<String>) -> Option<Self> {
        Self::for_host_with_sftp(x, y, host_id, false)
    }

    pub fn for_host_with_sftp(
        x: f32,
        y: f32,
        host_id: impl Into<String>,
        sftp_open: bool,
    ) -> Option<Self> {
        let id = host_id.into();
        let mut items = vec![
            ContextItem::new("New session", ContextAction::NewSession(id.clone())),
            ContextItem::new("Open SFTP", ContextAction::OpenSftp(id.clone())),
        ];
        if sftp_open {
            items.push(ContextItem::new(
                "Open in other pane",
                ContextAction::OpenSftpOtherPane(id.clone()),
            ));
        }
        items.push(ContextItem::new(
            "Copy SSH command",
            ContextAction::CopySshCommand(id.clone()),
        ));
        items.push(
            ContextItem::new("Edit host", ContextAction::EditHost(id.clone())).sep(),
        );
        items.push(ContextItem::new(
            "Rename",
            ContextAction::RenameHost(id.clone()),
        ));
        items.push(
            ContextItem::new("Delete host", ContextAction::DeleteHost(id)).danger(),
        );
        Self::open(x, y, items)
    }

    /// SFTP empty list / background: new folder + refresh.
    pub fn for_sftp_empty(x: f32, y: f32) -> Option<Self> {
        Self::open(
            x,
            y,
            vec![
                ContextItem::new("New folder", ContextAction::SftpNewFolder),
                ContextItem::new("Refresh", ContextAction::SftpRefresh),
            ],
        )
    }

    /// SFTP row context for a file or directory.
    ///
    /// `transfer_label` is typically "Upload", "Download", or "Copy to other pane".
    /// `can_edit` enables "Edit" for files (local: open in place; remote: temp + reupload).
    pub fn for_sftp_entry(
        x: f32,
        y: f32,
        is_dir: bool,
        transfer_label: Option<&str>,
        can_edit: bool,
    ) -> Option<Self> {
        let mut items = Vec::new();
        if is_dir {
            items.push(ContextItem::new("Open", ContextAction::SftpOpen));
        }
        if !is_dir && can_edit {
            items.push(ContextItem::new("Edit", ContextAction::SftpEdit));
        }
        if let Some(label) = transfer_label {
            items.push(ContextItem::new(label, ContextAction::SftpTransfer));
        }
        items.push(ContextItem::new("Rename", ContextAction::SftpRename));
        items.push(ContextItem::new("New folder", ContextAction::SftpNewFolder));
        items.push(
            ContextItem::new("Delete", ContextAction::SftpDelete)
                .sep()
                .danger(),
        );
        items.push(ContextItem::new("Refresh", ContextAction::SftpRefresh));
        Self::open(x, y, items)
    }

    /// Group header context: open or close its sessions, rename or delete it.
    pub fn for_group(x: f32, y: f32, group_id: impl Into<String>) -> Option<Self> {
        let id = group_id.into();
        Self::open(
            x,
            y,
            vec![
                ContextItem::new("Open all", ContextAction::OpenGroup(id.clone())),
                ContextItem::new("Close all", ContextAction::CloseGroup(id.clone())),
                ContextItem::new("Rename", ContextAction::RenameGroup(id.clone())).sep(),
                ContextItem::new("Delete group", ContextAction::DeleteGroup(id))
                    .sep()
                    .danger(),
            ],
        )
    }

    /// Session pill context: rename, and close unless the tab is pinned.
    pub fn for_session(x: f32, y: f32, tab_index: usize, closable: bool) -> Option<Self> {
        let mut items = vec![ContextItem::new(
            "Rename",
            ContextAction::RenameSession(tab_index),
        )];
        if closable {
            items.push(ContextItem::new(
                "Close",
                ContextAction::CloseSession(tab_index),
            ));
        }
        Self::open(x, y, items)
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

    /// Dynamic shell height: vertical pad + one row per item.
    pub fn height(&self) -> f32 {
        let seps = self.items.iter().filter(|i| i.separator_before).count();
        Self::height_for(self.items.len()) + seps as f32 * SEPARATOR_HEIGHT
    }

    /// Height for `n` items (same formula as [`Self::height`]).
    pub fn height_for(item_count: usize) -> f32 {
        MENU_PAD_Y * 2.0 + item_count as f32 * ITEM_HEIGHT
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

    /// Top of row `index` (after any dividers above it).
    fn row_top(&self, index: usize) -> f32 {
        let seps = self.items[..=index]
            .iter()
            .filter(|i| i.separator_before)
            .count();
        self.y + MENU_PAD_Y + index as f32 * ITEM_HEIGHT + seps as f32 * SEPARATOR_HEIGHT
    }

    pub fn item_rect(&self, index: usize) -> Option<Rect> {
        if index >= self.items.len() {
            return None;
        }
        Some(Rect::new(
            self.x + MENU_PAD_X,
            self.row_top(index),
            self.width - 2.0 * MENU_PAD_X,
            ITEM_HEIGHT,
        ))
    }

    /// The divider above row `index`, if it has one.
    pub fn separator_rect(&self, index: usize) -> Option<Rect> {
        let item = self.items.get(index)?;
        item.separator_before.then(|| {
            Rect::new(
                self.x + MENU_PAD_X,
                self.row_top(index) - SEPARATOR_HEIGHT,
                self.width - 2.0 * MENU_PAD_X,
                SEPARATOR_HEIGHT,
            )
        })
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

    /// Relabel item `index`, widening the menu so the new label fits.
    pub fn set_label(&mut self, index: usize, label: impl Into<String>) {
        if let Some(item) = self.items.get_mut(index) {
            item.label = label.into();
            let needed = item.label.chars().count() as f32 * LABEL_ESTIMATE + 24.0;
            self.width = self.width.max(needed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_host_menu_leads_with_connecting() {
        let menu = ContextMenu::for_host(10.0, 10.0, "h1").unwrap();
        assert_eq!(menu.items[0].label, "New session");
        assert!(menu.items.iter().any(|i| i.label == "Copy SSH command"));
    }

    #[test]
    fn matches_the_design_menu_metrics() {
        assert_eq!(MENU_PAD_Y, 6.0);
        assert_eq!(ITEM_HEIGHT, 36.0);
        assert_eq!(MENU_MIN_WIDTH, 236.0);
        assert_eq!(MENU_RADIUS, 12.0);
        assert_eq!(SEPARATOR_HEIGHT, 11.0);
    }

    #[test]
    fn height_scales_with_items_and_separators() {
        assert_eq!(ContextMenu::height_for(0), MENU_PAD_Y * 2.0);
        assert_eq!(
            ContextMenu::height_for(2),
            MENU_PAD_Y * 2.0 + 2.0 * ITEM_HEIGHT
        );
        let group = ContextMenu::for_group(10.0, 10.0, "g1").unwrap();
        let host = ContextMenu::for_host(10.0, 10.0, "h1").unwrap();
        assert_eq!(group.items.len(), 4);
        assert_eq!(host.items.len(), 6);
        let seps =
            |m: &ContextMenu| m.items.iter().filter(|i| i.separator_before).count();
        assert!(seps(&host) >= 1 && seps(&group) == 2);
        assert_eq!(
            host.height(),
            ContextMenu::height_for(6) + seps(&host) as f32 * SEPARATOR_HEIGHT
        );
        assert!(group.height() < host.height());
        assert_eq!(group.rect().height, group.height());
        // The last row ends exactly one pad above the bottom edge.
        let last = host.item_rect(5).unwrap();
        assert!((last.bottom() + MENU_PAD_Y - host.rect().bottom()).abs() < 0.01);
    }

    #[test]
    fn group_menu_offers_open_close_rename_delete_in_order() {
        let menu = ContextMenu::for_group(0.0, 0.0, "g").unwrap();
        let actions: Vec<_> = (0..4).filter_map(|i| menu.take_action(i)).collect();
        assert_eq!(
            actions,
            [
                ContextAction::OpenGroup("g".into()),
                ContextAction::CloseGroup("g".into()),
                ContextAction::RenameGroup("g".into()),
                ContextAction::DeleteGroup("g".into()),
            ]
        );
    }

    #[test]
    fn a_separator_pushes_later_rows_down_and_is_not_clickable() {
        let menu = ContextMenu::for_group(0.0, 0.0, "g").unwrap();
        let rename = menu.item_rect(2).unwrap();
        let delete = menu.item_rect(3).unwrap();
        assert_eq!(delete.y - rename.bottom(), SEPARATOR_HEIGHT);
        let mid = rename.bottom() + SEPARATOR_HEIGHT / 2.0;
        assert_eq!(menu.hit_test(rename.x + 4.0, mid), ContextMenuHit::Consume);
        assert_eq!(
            menu.separator_rect(3).map(|r| r.height),
            Some(SEPARATOR_HEIGHT)
        );
        assert_eq!(menu.separator_rect(1), None);
    }

    #[test]
    fn host_menu_hits_delete_and_dismisses_outside() {
        let menu = ContextMenu::for_host(100.0, 100.0, "h1").unwrap();
        assert_eq!(menu.items.len(), 6);
        let item = menu.item_rect(5).unwrap();
        assert_eq!(
            menu.hit_test(item.x + 2.0, item.y + 2.0),
            ContextMenuHit::Item(5)
        );
        assert_eq!(menu.hit_test(0.0, 0.0), ContextMenuHit::Dismiss);
        let id = || "h1".to_string();
        let expected = [
            ContextAction::NewSession(id()),
            ContextAction::OpenSftp(id()),
            ContextAction::CopySshCommand(id()),
            ContextAction::EditHost(id()),
            ContextAction::RenameHost(id()),
            ContextAction::DeleteHost(id()),
        ];
        for (i, action) in expected.into_iter().enumerate() {
            assert_eq!(menu.take_action(i), Some(action), "item {i}");
        }
    }

    #[test]
    fn clamps_into_the_window() {
        let menu = ContextMenu::for_host(2000.0, 2000.0, "h1")
            .unwrap()
            .clamped(800.0, 600.0);
        assert!(menu.x + menu.width <= 800.0);
        assert!(menu.y + menu.height() <= 600.0);
    }

    #[test]
    fn sftp_empty_menu() {
        let menu = ContextMenu::for_sftp_empty(10.0, 10.0).unwrap();
        assert_eq!(menu.take_action(0), Some(ContextAction::SftpNewFolder));
        assert_eq!(menu.take_action(1), Some(ContextAction::SftpRefresh));
    }

    #[test]
    fn host_menu_includes_open_sftp_other_pane() {
        let menu = ContextMenu::for_host_with_sftp(10.0, 10.0, "h1", true).unwrap();
        assert!(menu
            .items
            .iter()
            .any(|i| { matches!(i.action, ContextAction::OpenSftpOtherPane(_)) }));
        assert!(menu
            .items
            .iter()
            .any(|i| { matches!(i.action, ContextAction::OpenSftp(_)) }));
    }

    #[test]
    fn sftp_dir_menu_offers_open() {
        let menu = ContextMenu::for_sftp_entry(10.0, 10.0, true, Some("Download"), true)
            .unwrap();
        assert_eq!(menu.take_action(0), Some(ContextAction::SftpOpen));
    }

    #[test]
    fn sftp_file_menu_offers_edit_when_allowed() {
        let remote =
            ContextMenu::for_sftp_entry(10.0, 10.0, false, Some("Download"), true)
                .unwrap();
        assert_eq!(remote.take_action(0), Some(ContextAction::SftpEdit));
        assert_eq!(remote.take_action(1), Some(ContextAction::SftpTransfer));
        let local =
            ContextMenu::for_sftp_entry(10.0, 10.0, false, Some("Upload"), true).unwrap();
        assert_eq!(local.take_action(0), Some(ContextAction::SftpEdit));
        let dirs = ContextMenu::for_sftp_entry(10.0, 10.0, false, Some("Upload"), false)
            .unwrap();
        assert_ne!(dirs.take_action(0), Some(ContextAction::SftpEdit));
    }
}
