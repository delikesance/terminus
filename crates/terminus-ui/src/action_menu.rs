//! Context menu whose rows carry a [`ContextAction`]: builders for the host,
//! group, session, edit and SFTP menus on top of the generic
//! [`Menu`], which owns geometry, hit-testing and painting.

use std::ops::{Deref, DerefMut};

use crate::components::overlay::{Menu, MenuEntry};

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

/// A [`Menu`] plus the action of each of its entries (`None` for separators).
#[derive(Debug, Clone, PartialEq)]
pub struct ActionMenu {
    menu: Menu,
    actions: Vec<Option<ContextAction>>,
}

impl Deref for ActionMenu {
    type Target = Menu;

    fn deref(&self) -> &Menu {
        &self.menu
    }
}

impl DerefMut for ActionMenu {
    fn deref_mut(&mut self) -> &mut Menu {
        &mut self.menu
    }
}

impl ActionMenu {
    pub fn open(x: f32, y: f32, items: Vec<ContextItem>) -> Option<Self> {
        let mut entries = Vec::new();
        let mut actions = Vec::new();
        for item in items {
            if item.separator_before {
                entries.push(MenuEntry::separator());
                actions.push(None);
            }
            let entry = MenuEntry::item(item.label);
            entries.push(if item.danger { entry.danger() } else { entry });
            actions.push(Some(item.action));
        }
        Some(Self {
            menu: Menu::open(x, y, entries)?,
            actions,
        })
    }

    /// Clamp the menu into the window so it never hangs off-screen.
    pub fn clamped(mut self, window_width: f32, window_height: f32) -> Self {
        self.menu = self.menu.clamped(window_width, window_height);
        self
    }

    /// Action of entry `index`; `None` for separators and out-of-range indices.
    pub fn take_action(&self, index: usize) -> Option<ContextAction> {
        self.actions.get(index)?.clone()
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
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::components::overlay::{
        EntryState, MenuHit, MENU_PAD, MENU_WIDTH, SEPARATOR_HEIGHT,
    };

    fn labels(menu: &ActionMenu) -> Vec<&str> {
        menu.entries.iter().map(|e| e.label.as_str()).collect()
    }

    #[test]
    fn a_host_menu_leads_with_connecting() {
        let menu = ActionMenu::for_host(10.0, 10.0, "h1").unwrap();
        assert_eq!(menu.entries[0].label, "New session");
        assert!(labels(&menu).contains(&"Copy SSH command"));
    }

    #[test]
    fn separators_become_actionless_entries_and_danger_is_kept() {
        let group = ActionMenu::for_group(10.0, 10.0, "g1").unwrap();
        let host = ActionMenu::for_host(10.0, 10.0, "h1").unwrap();
        assert_eq!(group.entries.len(), 6);
        assert_eq!(host.entries.len(), 7);
        let seps = |m: &ActionMenu| m.entries.iter().filter(|e| e.separator).count();
        assert!(seps(&host) >= 1 && seps(&group) == 2);
        assert!(group.height() < host.height());
        assert_eq!(group.rect().width, MENU_WIDTH);
        let last = host.item_rect(6).unwrap();
        assert!((last.bottom() + MENU_PAD - host.rect().bottom()).abs() < 0.01);
        assert_eq!(host.entries[6].state, EntryState::Danger);
    }

    #[test]
    fn group_menu_offers_open_close_rename_delete_in_order() {
        let menu = ActionMenu::for_group(0.0, 0.0, "g").unwrap();
        let actions: Vec<_> = (0..menu.entries.len())
            .filter_map(|i| menu.take_action(i))
            .collect();
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
        let menu = ActionMenu::for_group(0.0, 0.0, "g").unwrap();
        let rename = menu.item_rect(3).unwrap();
        let delete = menu.item_rect(5).unwrap();
        assert_eq!(delete.y - rename.bottom(), SEPARATOR_HEIGHT);
        let mid = rename.bottom() + SEPARATOR_HEIGHT / 2.0;
        assert_eq!(menu.hit_test(rename.x + 4.0, mid), MenuHit::Consume);
        assert_eq!(menu.take_action(4), None);
        assert_eq!(menu.separator_rect(1), None);
    }

    #[test]
    fn host_menu_hits_delete_and_dismisses_outside() {
        let menu = ActionMenu::for_host(100.0, 100.0, "h1").unwrap();
        let item = menu.item_rect(6).unwrap();
        assert_eq!(menu.hit_test(item.x + 2.0, item.y + 2.0), MenuHit::Item(6));
        assert_eq!(menu.hit_test(0.0, 0.0), MenuHit::Dismiss);
        let id = || "h1".to_string();
        let expected = [
            Some(ContextAction::NewSession(id())),
            Some(ContextAction::OpenSftp(id())),
            Some(ContextAction::CopySshCommand(id())),
            None,
            Some(ContextAction::EditHost(id())),
            Some(ContextAction::RenameHost(id())),
            Some(ContextAction::DeleteHost(id())),
        ];
        for (i, action) in expected.into_iter().enumerate() {
            assert_eq!(menu.take_action(i), action, "entry {i}");
        }
    }

    #[test]
    fn clamps_into_the_window() {
        let menu = ActionMenu::for_host(2000.0, 2000.0, "h1")
            .unwrap()
            .clamped(800.0, 600.0);
        assert!(menu.rect().right() <= 800.0);
        assert!(menu.rect().bottom() <= 600.0);
    }

    #[test]
    fn sftp_empty_menu() {
        let menu = ActionMenu::for_sftp_empty(10.0, 10.0).unwrap();
        assert_eq!(menu.take_action(0), Some(ContextAction::SftpNewFolder));
        assert_eq!(menu.take_action(1), Some(ContextAction::SftpRefresh));
    }

    #[test]
    fn host_menu_includes_open_sftp_other_pane() {
        let menu = ActionMenu::for_host_with_sftp(10.0, 10.0, "h1", true).unwrap();
        let has = |f: fn(&ContextAction) -> bool| {
            (0..menu.entries.len()).any(|i| menu.take_action(i).is_some_and(|a| f(&a)))
        };
        assert!(has(|a| matches!(a, ContextAction::OpenSftpOtherPane(_))));
        assert!(has(|a| matches!(a, ContextAction::OpenSftp(_))));
    }

    #[test]
    fn sftp_dir_menu_offers_open() {
        let menu =
            ActionMenu::for_sftp_entry(10.0, 10.0, true, Some("Download"), true).unwrap();
        assert_eq!(menu.take_action(0), Some(ContextAction::SftpOpen));
    }

    #[test]
    fn sftp_file_menu_offers_edit_when_allowed() {
        let remote =
            ActionMenu::for_sftp_entry(10.0, 10.0, false, Some("Download"), true)
                .unwrap();
        assert_eq!(remote.take_action(0), Some(ContextAction::SftpEdit));
        assert_eq!(remote.take_action(1), Some(ContextAction::SftpTransfer));
        let local =
            ActionMenu::for_sftp_entry(10.0, 10.0, false, Some("Upload"), true).unwrap();
        assert_eq!(local.take_action(0), Some(ContextAction::SftpEdit));
        let dirs =
            ActionMenu::for_sftp_entry(10.0, 10.0, false, Some("Upload"), false).unwrap();
        assert_ne!(dirs.take_action(0), Some(ContextAction::SftpEdit));
    }
}
