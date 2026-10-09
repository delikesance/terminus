use super::*;

impl Chrome {
    pub(super) fn press_shell(&mut self, hit: crate::shell::ShellHit) -> ChromeAction {
        use crate::shell::{SettingsPage, ShellHit, WorkspaceView};
        let view = |chrome: &mut Self, v: WorkspaceView| {
            chrome.show_view(v);
            ChromeAction::ViewChanged(v)
        };
        match hit {
            ShellHit::Brand => view(self, WorkspaceView::Home),
            ShellHit::CommandBar => ChromeAction::OpenPalette,
            ShellHit::AddServer => ChromeAction::AddHost,
            ShellHit::Settings => view(self, WorkspaceView::Settings(SettingsPage::Keys)),
            ShellHit::Tab(v) => view(self, v),
            ShellHit::Control(b) => ChromeAction::WindowControl(b),
            // Clicks inside the field being edited stay with it.
            ShellHit::Pill(tab) | ShellHit::PillClose(tab)
                if self.shell.is_renaming(tab) =>
            {
                ChromeAction::Consumed
            }
            ShellHit::Pill(tab) => ChromeAction::OpenSession(tab),
            ShellHit::PillClose(tab) => ChromeAction::CloseSession(tab),
            ShellHit::NewSession => ChromeAction::AddHostSession(
                self.shell
                    .machine
                    .as_ref()
                    .map(|m| m.id.clone())
                    .unwrap_or_else(|| "local".to_string()),
            ),
            ShellHit::SplitRight => ChromeAction::Split { down: false },
            ShellHit::SplitDown => ChromeAction::Split { down: true },
            ShellHit::Drag => ChromeAction::WindowDrag,
            ShellHit::Inert => ChromeAction::Consumed,
        }
    }

    /// Route a mouse press, in logical pixels.
    ///
    /// Called with the form open too: a click on the scrim dismisses the
    /// editor rather than reaching the terminal behind it, which is what
    /// makes the dialog feel modal.
    pub fn handle_press(
        &mut self,
        window_width: f32,
        window_height: f32,
        x: f32,
        y: f32,
    ) -> ChromeAction {
        let action = self.route_press(window_width, window_height, x, y);
        // Touching the chrome retires the "Added …" line: it has been
        // read by then, and leaving it up would hide a later failure.
        if action != ChromeAction::Ignored {
            self.panel.notice = None;
        }
        action
    }

    /// Right-click: open a context menu over a host or group row.
    ///
    /// Kept separate from [`Self::handle_press`] so a right-click never
    /// arms a host drag or opens a session.
    ///
    /// `sftp_open` adds "Open in other pane" on host rows when an SFTP
    /// session is already active.
    pub fn handle_context_press(
        &mut self,
        window_width: f32,
        window_height: f32,
        x: f32,
        y: f32,
        sftp_open: bool,
    ) -> ChromeAction {
        // Modals / overlays own the pointer; don't open under them.
        if self.settings.open
            || self.connection_hit(x, y).is_some()
            || self.lost_hit(x, y).is_some()
            || self.form.is_open()
            || self.snippet_form.is_open()
            || self.vault_unlock.is_open()
            || self.confirm.is_some()
        {
            self.close_context_menu();
            return ChromeAction::Ignored;
        }
        if !self.hosts_visible() {
            self.close_context_menu();
            return ChromeAction::Ignored;
        }

        // Session pills: rename / close.
        use crate::shell::ShellHit;
        if let Some(ShellHit::Pill(tab) | ShellHit::PillClose(tab)) =
            self.shell.hit_test(x, y)
        {
            let closable = self
                .shell
                .pills
                .iter()
                .find(|p| p.tab_index == tab)
                .is_some_and(|p| p.closable);
            self.context_menu = ActionMenu::for_session(x, y, tab, closable)
                .map(|m| m.clamped(window_width, window_height));
            return ChromeAction::Consumed;
        }

        let origin_y = self.origin_y();
        let height = (window_height - origin_y).max(0.0);
        let menu = match self.panel.hit_test(origin_y, height, x, y) {
            Some(PanelHit::Item(index)) => self
                .panel
                .rows
                .get(index)
                .and_then(Row::host)
                .filter(|h| h.stored)
                .and_then(|h| {
                    ActionMenu::for_host_with_sftp(x, y, h.id.clone(), sftp_open)
                }),
            Some(PanelHit::Group(index)) => self
                .panel
                .rows
                .get(index)
                .and_then(|row| match row {
                    Row::Group { id, .. } => Some(id.clone()),
                    _ => None,
                })
                .and_then(|id| ActionMenu::for_group(x, y, id)),
            _ => None,
        };

        match menu {
            Some(m) => {
                self.context_menu = Some(m.clamped(window_width, window_height));
                ChromeAction::Consumed
            }
            None => {
                self.close_context_menu();
                ChromeAction::Ignored
            }
        }
    }

    pub fn close_context_menu(&mut self) {
        self.context_menu = None;
    }

    /// The confirmation a context-menu row raises, if it is destructive.
    pub(super) fn delete_prompt_for(&self, index: usize) -> Option<ConfirmPrompt> {
        let action = self.context_menu.as_ref()?.take_action(index)?;
        match action {
            ContextAction::DeleteHost(id) => {
                let host = self
                    .panel
                    .rows
                    .iter()
                    .filter_map(Row::host)
                    .find(|h| h.id == id);
                let name = host.map_or("this server", |h| h.name.as_str());
                let sessions = host.map_or(0, |h| h.session_count);
                Some(ConfirmPrompt::delete_host(&id, name, sessions))
            }
            ContextAction::DeleteGroup(id) => {
                let group = self.panel.rows.iter().find_map(|r| match r {
                    Row::Group {
                        id: gid,
                        name,
                        host_count,
                        ..
                    } if *gid == id => Some((name.as_str(), *host_count)),
                    _ => None,
                });
                let (name, hosts) = group.unwrap_or(("this group", 0));
                Some(ConfirmPrompt::delete_group(&id, name, hosts))
            }
            _ => None,
        }
    }

    pub(super) fn route_context_menu_press(
        &mut self,
        x: f32,
        y: f32,
    ) -> Option<ChromeAction> {
        let menu = self.context_menu.as_mut()?;
        match menu.hit_test(x, y) {
            MenuHit::Dismiss => {
                self.close_context_menu();
                // Swallow the dismiss click so it does not open a host.
                Some(ChromeAction::Consumed)
            }
            MenuHit::Consume => Some(ChromeAction::Consumed),
            MenuHit::Item(index) => {
                // Deleting a host or group cannot be undone: ask first.
                if let Some(prompt) = self.delete_prompt_for(index) {
                    self.open_confirm(prompt);
                    return Some(ChromeAction::Consumed);
                }
                let Some(menu) = self.context_menu.as_mut() else {
                    return Some(ChromeAction::Consumed);
                };
                let action = menu.take_action(index);
                self.close_context_menu();
                Some(match action {
                    Some(ContextAction::NewSession(id)) => {
                        ChromeAction::AddHostSession(id)
                    }
                    Some(ContextAction::CopySshCommand(id)) => {
                        ChromeAction::CopySshCommand(id)
                    }
                    Some(ContextAction::DeleteHost(id)) => ChromeAction::DeleteHost(id),
                    Some(ContextAction::DeleteGroup(id)) => ChromeAction::DeleteGroup(id),
                    Some(ContextAction::EditHost(id)) => ChromeAction::EditHost(id),
                    Some(ContextAction::OpenSftp(id)) => ChromeAction::OpenSftp(id),
                    Some(ContextAction::OpenSftpOtherPane(id)) => {
                        ChromeAction::OpenSftpOtherPane(id)
                    }
                    Some(ContextAction::RenameHost(id)) => ChromeAction::RenameHost(id),
                    Some(ContextAction::RenameGroup(id)) => ChromeAction::RenameGroup(id),
                    Some(ContextAction::OpenGroup(id)) => ChromeAction::OpenGroup(id),
                    Some(ContextAction::CloseGroup(id)) => ChromeAction::CloseGroup(id),
                    Some(ContextAction::RenameSession(tab)) => {
                        self.shell.begin_rename(tab);
                        ChromeAction::Consumed
                    }
                    Some(ContextAction::CloseSession(tab)) => {
                        ChromeAction::CloseSession(tab)
                    }
                    Some(ContextAction::Copy) => ChromeAction::ContextCopy,
                    Some(ContextAction::Paste) => ChromeAction::ContextPaste,
                    Some(ContextAction::SftpNewFolder) => ChromeAction::SftpNewFolder,
                    Some(ContextAction::SftpRename) => ChromeAction::SftpRename,
                    Some(ContextAction::SftpDelete) => ChromeAction::SftpDelete,
                    Some(ContextAction::SftpTransfer) => ChromeAction::SftpTransfer,
                    Some(ContextAction::SftpEdit) => ChromeAction::SftpEdit,
                    Some(ContextAction::SftpOpen) => ChromeAction::SftpOpen,
                    Some(ContextAction::SftpRefresh) => ChromeAction::SftpRefresh,
                    None => ChromeAction::Consumed,
                })
            }
        }
    }
}
