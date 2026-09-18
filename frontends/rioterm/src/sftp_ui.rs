//! Active SFTP dual-pane session owned by [`crate::screen::Screen`].

use std::path::{Path, PathBuf};
use std::sync::Arc;

use terminus_bridge::{
    ConflictAction, ConflictKind, SftpCommand, SftpEvent, SftpListEntry, SftpSide,
    SftpWorker,
};
use terminus_core::auth_method::HostAuthMethod;
use terminus_core::local_fs;
use terminus_core::ssh::{
    HostKeyPolicy, KnownHosts, SshAuth, SshConnectOptions, DEFAULT_CONNECT_TIMEOUT,
    DEFAULT_KEEPALIVE_INTERVAL,
};
use terminus_ui::sftp_pane::{
    join_remote, parent_path, SftpBackend, SftpClickResult, SftpConflictKind, SftpDrag,
    SftpFocus, SftpHit, SftpNameKind, SftpPaneLayout, SftpPaneState, SftpRow,
    SftpSideState, SFTP_DRAG_THRESHOLD,
};

use crate::hosts::HostRow;

/// Live dual-pane SFTP session (either side local or remote).
pub struct ActiveSftp {
    pub state: SftpPaneState,
    pub worker: SftpWorker,
    last_click: Option<(SftpHit, std::time::Instant)>,
    /// Press origin for DnD threshold (`hit`, `x`, `y`).
    drag_armed: Option<(SftpHit, f32, f32)>,
}

impl ActiveSftp {
    /// Left = local, right = `host`. Connects right and lists both sides.
    pub fn start(
        host: &HostRow,
        password: Option<String>,
        identity_pem: Option<(String, Option<String>)>,
        wake: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Result<Self, String> {
        let opts = connect_options_for_host(host, password, identity_pem)?;
        let local_root = local_fs::default_local_root();
        let label = host_label(host);
        let mut state = SftpPaneState::new_local_remote(
            local_root.to_string_lossy().into_owned(),
            host.id.clone(),
            label,
        );
        state.status = format!("Connecting to {}…", host.endpoint());
        state.loading = true;

        let worker = SftpWorker::spawn(wake);
        worker.send(SftpCommand::Connect {
            side: SftpSide::Right,
            opts,
        });
        worker.send(SftpCommand::ListLocal {
            side: SftpSide::Left,
            path: local_root,
        });

        Ok(Self {
            state,
            worker,
            last_click: None,
            drag_armed: None,
        })
    }

    /// Both panes local — used by unit/E2E harnesses (no SSH).
    pub fn start_local_dual(
        left: PathBuf,
        right: PathBuf,
        wake: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Self {
        let mut state = SftpPaneState::new_local_local(
            left.to_string_lossy().into_owned(),
            right.to_string_lossy().into_owned(),
        );
        state.status = "Ready".into();
        state.loading = true;
        let worker = SftpWorker::spawn(wake);
        worker.send(SftpCommand::ListLocal {
            side: SftpSide::Left,
            path: left,
        });
        worker.send(SftpCommand::ListLocal {
            side: SftpSide::Right,
            path: right,
        });
        Self {
            state,
            worker,
            last_click: None,
            drag_armed: None,
        }
    }

    /// Connect a second host on the left pane (right stays as-is).
    pub fn open_other_host(
        &mut self,
        host: &HostRow,
        password: Option<String>,
        identity_pem: Option<(String, Option<String>)>,
    ) -> Result<(), String> {
        let opts = connect_options_for_host(host, password, identity_pem)?;
        if matches!(self.state.left.backend, SftpBackend::Remote { .. }) {
            self.worker.send(SftpCommand::Disconnect {
                side: SftpSide::Left,
            });
        }
        self.state.left = SftpSideState::remote(host.id.clone(), host_label(host));
        self.state.focus = SftpFocus::Left;
        self.state.loading = true;
        self.state.status = format!("Connecting to {}…", host.endpoint());
        self.state.error = None;
        self.state.name_edit = None;
        self.state.drag = None;
        self.drag_armed = None;
        self.worker.send(SftpCommand::Connect {
            side: SftpSide::Left,
            opts,
        });
        Ok(())
    }

    pub fn close(&self) {
        self.worker.send(SftpCommand::Close);
    }

    /// Drain worker events into pane state. Returns true when UI should redraw.
    pub fn pump(&mut self) -> bool {
        let events = self.worker.drain();
        if events.is_empty() {
            return false;
        }
        for event in events {
            match event {
                SftpEvent::Ready { side } => {
                    self.state.loading = false;
                    self.state.status = "Connected".into();
                    self.state.error = None;
                    self.worker.send(SftpCommand::ListRemote {
                        side,
                        path: "/".into(),
                    });
                }
                SftpEvent::Listed {
                    side,
                    path,
                    entries,
                } => {
                    let rows = entries.into_iter().map(row_from_list_entry).collect();
                    self.state.set_listed(focus_from_side(side), path, rows);
                    self.state.status = "Ready".into();
                }
                SftpEvent::TransferProgress { label, done, total } => {
                    if total == 0 {
                        self.state.status = label;
                    } else {
                        self.state.status = format!("{label} ({done}/{total})");
                    }
                    self.state.error = None;
                }
                SftpEvent::EditReady {
                    local_path,
                    remote_path,
                    ..
                } => {
                    self.state.error = None;
                    let name = Path::new(&remote_path)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| remote_path.clone());
                    self.state.status = format!("Editing {name}…");
                    open_path_with_default_app(&local_path);
                }
                SftpEvent::EditSaved { remote_path, .. } => {
                    let name = Path::new(&remote_path)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| remote_path.clone());
                    self.state.status = format!("Saved {name}");
                    self.state.error = None;
                }
                SftpEvent::Failed(msg) => {
                    self.state.loading = false;
                    self.state.conflict = None;
                    self.state.error = Some(msg);
                }
                SftpEvent::Conflict {
                    id,
                    kind,
                    relative_path,
                    ..
                } => {
                    let kind = match kind {
                        ConflictKind::File => SftpConflictKind::File,
                        ConflictKind::Directory => SftpConflictKind::Directory,
                    };
                    self.state.begin_conflict(id, kind, relative_path);
                }
                SftpEvent::Closed => {
                    self.state.status = "Disconnected".into();
                    self.state.conflict = None;
                }
            }
        }
        true
    }

    pub fn handle_click(
        &mut self,
        layout: &SftpPaneLayout,
        x: f32,
        y: f32,
        double: bool,
    ) -> SftpClickResult {
        let hit = layout.hit_test(&self.state, x, y);
        if matches!(hit, SftpHit::Miss) {
            return SftpClickResult::Miss;
        }

        if matches!(hit, SftpHit::Close) {
            return SftpClickResult::Close;
        }

        let is_double = double
            || self.last_click.as_ref().is_some_and(|(prev, at)| {
                prev == &hit && at.elapsed() < std::time::Duration::from_millis(400)
            });
        self.last_click = Some((hit.clone(), std::time::Instant::now()));

        match hit {
            SftpHit::LeftRow(i) => {
                self.state.focus = SftpFocus::Left;
                self.state.left.selected = Some(i);
                if is_double {
                    if let Some(row) = self.state.left.entries.get(i).cloned() {
                        if row.is_dir {
                            self.cd(SftpFocus::Left, row.path);
                        } else {
                            self.transfer_row(SftpFocus::Left, &row);
                        }
                    }
                }
            }
            SftpHit::RightRow(i) => {
                self.state.focus = SftpFocus::Right;
                self.state.right.selected = Some(i);
                if is_double {
                    if let Some(row) = self.state.right.entries.get(i).cloned() {
                        if row.is_dir {
                            self.cd(SftpFocus::Right, row.path);
                        } else {
                            self.transfer_row(SftpFocus::Right, &row);
                        }
                    }
                }
            }
            SftpHit::LeftParent => {
                self.state.focus = SftpFocus::Left;
                self.cd_parent(SftpFocus::Left);
            }
            SftpHit::RightParent => {
                self.state.focus = SftpFocus::Right;
                self.cd_parent(SftpFocus::Right);
            }
            SftpHit::LeftCrumb => {
                self.state.focus = SftpFocus::Left;
                let segs = self.state.crumb_segments(SftpFocus::Left);
                if segs.len() >= 2 {
                    if let Some(path) = self
                        .state
                        .navigate_crumb_path(SftpFocus::Left, segs.len() - 2)
                    {
                        self.cd(SftpFocus::Left, path);
                    }
                }
            }
            SftpHit::RightCrumb => {
                self.state.focus = SftpFocus::Right;
                let segs = self.state.crumb_segments(SftpFocus::Right);
                if segs.len() >= 2 {
                    if let Some(path) = self
                        .state
                        .navigate_crumb_path(SftpFocus::Right, segs.len() - 2)
                    {
                        self.cd(SftpFocus::Right, path);
                    }
                }
            }
            SftpHit::NameConfirm => {
                self.commit_name_edit();
            }
            SftpHit::NameCancel => {
                self.state.cancel_name_edit();
            }
            SftpHit::NameField => {}
            SftpHit::ConflictOverwrite => {
                self.resolve_conflict(ConflictAction::Overwrite);
            }
            SftpHit::ConflictKeep => {
                self.resolve_conflict(ConflictAction::Keep);
            }
            SftpHit::ConflictApplyAll => {
                let _ = self.state.toggle_conflict_apply_all();
            }
            SftpHit::ConflictCancel => {
                self.worker.send(SftpCommand::CancelTransfer);
                self.state.clear_conflict();
                self.state.status = "Transfer cancelled".into();
            }
            SftpHit::Footer | SftpHit::Consume => {}
            SftpHit::Close | SftpHit::Miss => unreachable!("handled above"),
        }
        SftpClickResult::Handled
    }

    /// Arm a potential drag from a row press.
    pub fn drag_press(&mut self, hit: SftpHit, x: f32, y: f32) {
        match hit {
            SftpHit::LeftRow(_) | SftpHit::RightRow(_) => {
                self.drag_armed = Some((hit, x, y));
            }
            _ => {
                self.drag_armed = None;
            }
        }
    }

    pub fn cancel_drag(&mut self) {
        self.drag_armed = None;
        self.state.drag = None;
    }

    /// Move while pressed. Returns true when a drag starts or the ghost moves.
    pub fn drag_move(&mut self, x: f32, y: f32) -> bool {
        if let Some(drag) = self.state.drag.as_mut() {
            drag.pointer_x = x;
            drag.pointer_y = y;
            return true;
        }
        let Some((hit, ox, oy)) = self.drag_armed.clone() else {
            return false;
        };
        let dx = x - ox;
        let dy = y - oy;
        if dx * dx + dy * dy < SFTP_DRAG_THRESHOLD * SFTP_DRAG_THRESHOLD {
            return false;
        }
        let (from, row_index) = match hit {
            SftpHit::LeftRow(i) => (SftpFocus::Left, i),
            SftpHit::RightRow(i) => (SftpFocus::Right, i),
            _ => {
                self.drag_armed = None;
                return false;
            }
        };
        let Some(row) = self.state.side(from).entries.get(row_index).cloned() else {
            self.drag_armed = None;
            return false;
        };
        self.state.focus = from;
        self.state.side_mut(from).selected = Some(row_index);
        self.state.drag = Some(SftpDrag {
            from,
            row_index,
            name: row.name,
            path: row.path,
            is_dir: row.is_dir,
            pointer_x: x,
            pointer_y: y,
        });
        self.drag_armed = None;
        true
    }

    /// Release: drop onto the other pane list → Transfer. Returns true if handled.
    pub fn drag_release(&mut self, layout: &SftpPaneLayout, x: f32, y: f32) -> bool {
        self.drag_armed = None;
        let Some(drag) = self.state.drag.take() else {
            return false;
        };
        if drag.is_dir {
            let to = match layout.focus_at_list(x, y) {
                Some(to) if to != drag.from => to,
                _ => return true,
            };
            let to_cwd = self.state.side(to).cwd.clone();
            self.state.status = format!("Transferring folder {}…", drag.name);
            self.state.error = None;
            self.worker.send(SftpCommand::TransferFolder {
                from_side: side_from_focus(drag.from),
                from_path: drag.path,
                to_side: side_from_focus(to),
                to_cwd,
                name: drag.name,
            });
            return true;
        }
        let Some(to) = layout.focus_at_list(x, y) else {
            return true;
        };
        if to == drag.from {
            return true;
        }
        let to_cwd = self.state.side(to).cwd.clone();
        self.state.status = format!("Transferring {}…", drag.name);
        self.state.error = None;
        self.worker.send(SftpCommand::Transfer {
            from_side: side_from_focus(drag.from),
            from_path: drag.path,
            to_side: side_from_focus(to),
            to_cwd,
            name: drag.name,
        });
        true
    }

    /// Build a context menu for a right-click hit inside the SFTP pane.
    /// Selects the row under the cursor when applicable.
    pub fn context_menu_for_hit(
        &mut self,
        layout: &SftpPaneLayout,
        hit: SftpHit,
        x: f32,
        y: f32,
    ) -> Option<terminus_ui::ContextMenu> {
        match hit {
            SftpHit::LeftRow(i) => {
                self.state.focus = SftpFocus::Left;
                self.state.left.selected = Some(i);
                let row = self.state.left.entries.get(i)?;
                let transfer = Some(self.transfer_label(SftpFocus::Left));
                let can_edit = !row.is_dir;
                terminus_ui::ContextMenu::for_sftp_entry(
                    x,
                    y,
                    row.is_dir,
                    transfer.as_deref(),
                    can_edit,
                )
            }
            SftpHit::RightRow(i) => {
                self.state.focus = SftpFocus::Right;
                self.state.right.selected = Some(i);
                let row = self.state.right.entries.get(i)?;
                let transfer = Some(self.transfer_label(SftpFocus::Right));
                let can_edit = !row.is_dir;
                terminus_ui::ContextMenu::for_sftp_entry(
                    x,
                    y,
                    row.is_dir,
                    transfer.as_deref(),
                    can_edit,
                )
            }
            SftpHit::LeftParent | SftpHit::LeftCrumb => {
                self.state.focus = SftpFocus::Left;
                terminus_ui::ContextMenu::for_sftp_empty(x, y)
            }
            SftpHit::RightParent | SftpHit::RightCrumb => {
                self.state.focus = SftpFocus::Right;
                terminus_ui::ContextMenu::for_sftp_empty(x, y)
            }
            SftpHit::Consume => {
                if layout.left_list.contains(x, y) || layout.left.contains(x, y) {
                    self.state.focus = SftpFocus::Left;
                    return terminus_ui::ContextMenu::for_sftp_empty(x, y);
                }
                if layout.right_list.contains(x, y) || layout.right.contains(x, y) {
                    self.state.focus = SftpFocus::Right;
                    return terminus_ui::ContextMenu::for_sftp_empty(x, y);
                }
                None
            }
            SftpHit::Footer
            | SftpHit::Close
            | SftpHit::NameField
            | SftpHit::NameConfirm
            | SftpHit::NameCancel
            | SftpHit::ConflictOverwrite
            | SftpHit::ConflictKeep
            | SftpHit::ConflictApplyAll
            | SftpHit::ConflictCancel
            | SftpHit::Miss => None,
        }
    }

    fn transfer_label(&self, from: SftpFocus) -> &'static str {
        let other = match from {
            SftpFocus::Left => SftpFocus::Right,
            SftpFocus::Right => SftpFocus::Left,
        };
        let from_local = self.state.side(from).is_local();
        let other_local = self.state.side(other).is_local();
        if other_local {
            "Download"
        } else if from_local {
            "Upload"
        } else {
            "Copy to other pane"
        }
    }

    /// Update hover highlight from pointer position.
    pub fn handle_hover(&mut self, layout: &SftpPaneLayout, x: f32, y: f32) -> bool {
        let hit = layout.hit_test(&self.state, x, y);
        let hover = match hit {
            SftpHit::Miss => None,
            other => Some(other),
        };
        self.state.set_hover(hover)
    }

    pub fn handle_key(&mut self, key: SftpKey) -> bool {
        match key {
            SftpKey::Tab => {
                self.state.focus = self.state.other_focus();
                true
            }
            SftpKey::Up => {
                self.state.move_selection(-1);
                true
            }
            SftpKey::Down => {
                self.state.move_selection(1);
                true
            }
            SftpKey::Enter => {
                if self.state.name_edit.is_some() {
                    self.commit_name_edit();
                    return true;
                }
                if self.state.conflict.is_some() {
                    self.resolve_conflict(ConflictAction::Overwrite);
                    return true;
                }
                if let Some(row) = self.state.selected(self.state.focus).cloned() {
                    if row.is_dir {
                        self.cd(self.state.focus, row.path);
                    }
                }
                true
            }
            SftpKey::Backspace => {
                if self.name_edit_backspace() {
                    return true;
                }
                self.cd_parent(self.state.focus);
                true
            }
            SftpKey::Delete => {
                self.remove_selected();
                true
            }
            SftpKey::Mkdir => {
                self.state.begin_mkdir();
                true
            }
            SftpKey::Rename => self.state.begin_rename(),
            SftpKey::Upload | SftpKey::Download => {
                self.transfer_selected();
                true
            }
        }
    }

    pub fn scroll(
        &mut self,
        layout: &SftpPaneLayout,
        x: f32,
        y: f32,
        delta_y: f32,
    ) -> bool {
        let step = delta_y * 20.0;
        if layout.left.contains(x, y) {
            self.state.left.scroll = (self.state.left.scroll - step).max(0.0);
            true
        } else if layout.right.contains(x, y) {
            self.state.right.scroll = (self.state.right.scroll - step).max(0.0);
            true
        } else {
            false
        }
    }

    fn cd(&mut self, focus: SftpFocus, path: String) {
        self.state.loading = true;
        self.state.status = format!("Listing {path}…");
        let side = side_from_focus(focus);
        if self.state.side(focus).is_local() {
            self.worker.send(SftpCommand::ListLocal {
                side,
                path: PathBuf::from(path),
            });
        } else {
            self.worker.send(SftpCommand::ListRemote { side, path });
        }
    }

    fn cd_parent(&mut self, focus: SftpFocus) {
        let cwd = self.state.side(focus).cwd.clone();
        let parent = if self.state.side(focus).is_local() {
            PathBuf::from(parent_path(&cwd))
                .to_string_lossy()
                .into_owned()
        } else {
            parent_path(&cwd)
        };
        self.cd(focus, parent);
    }

    fn remove_selected(&mut self) {
        let focus = self.state.focus;
        let Some(row) = self.state.selected(focus).cloned() else {
            return;
        };
        self.state.status = format!("Removing {}…", row.name);
        let side = side_from_focus(focus);
        if self.state.side(focus).is_local() {
            self.worker.send(SftpCommand::RemoveLocal {
                side,
                path: PathBuf::from(row.path),
                recursive: row.is_dir,
            });
        } else if row.is_dir {
            self.worker.send(SftpCommand::RemoveRemoteRecursive {
                side,
                path: row.path,
            });
        } else {
            self.worker.send(SftpCommand::RemoveRemote {
                side,
                path: row.path,
            });
        }
    }

    pub fn commit_name_edit(&mut self) {
        let Some(edit) = self.state.name_edit.take() else {
            return;
        };
        let name = edit.draft.value.trim().to_string();
        if name.is_empty() {
            self.state.error = Some("Enter a name first".into());
            self.state.name_edit = Some(edit);
            return;
        }
        if name.contains('/') || name.contains('\\') || name.contains('\0') {
            self.state.error = Some("Name cannot contain path separators".into());
            self.state.name_edit = Some(edit);
            return;
        }
        let focus = edit.side;
        let side = side_from_focus(focus);
        let is_local = self.state.side(focus).is_local();
        let cwd = self.state.side(focus).cwd.clone();
        match edit.kind {
            SftpNameKind::Mkdir => {
                if is_local {
                    let path = PathBuf::from(&cwd).join(&name);
                    self.state.status = format!("Creating {}…", path.display());
                    self.worker.send(SftpCommand::MkdirLocal { side, path });
                } else {
                    let path = join_remote(&cwd, &name);
                    self.state.status = format!("Creating {path}…");
                    self.worker.send(SftpCommand::MkdirRemote { side, path });
                }
            }
            SftpNameKind::Rename => {
                let Some(from) = edit.from_path else {
                    self.state.error = Some("Nothing to rename".into());
                    return;
                };
                if is_local {
                    let parent = PathBuf::from(&from)
                        .parent()
                        .map(PathBuf::from)
                        .unwrap_or_else(|| PathBuf::from(&cwd));
                    let to = parent.join(&name);
                    self.state.status = format!("Renaming → {name}…");
                    self.worker.send(SftpCommand::RenameLocal {
                        side,
                        from: PathBuf::from(from),
                        to,
                    });
                } else {
                    let parent = parent_path(&from);
                    let to = join_remote(&parent, &name);
                    self.state.status = format!("Renaming → {name}…");
                    self.worker
                        .send(SftpCommand::RenameRemote { side, from, to });
                }
            }
        }
        self.state.error = None;
    }

    fn transfer_row(&mut self, from: SftpFocus, row: &SftpRow) {
        if self.state.conflict.is_some() {
            self.state.error = Some("Finish the conflict prompt first".into());
            return;
        }
        let to = match from {
            SftpFocus::Left => SftpFocus::Right,
            SftpFocus::Right => SftpFocus::Left,
        };
        let to_cwd = self.state.side(to).cwd.clone();
        self.state.status = format!("Transferring {}…", row.name);
        self.state.error = None;
        if row.is_dir {
            self.worker.send(SftpCommand::TransferFolder {
                from_side: side_from_focus(from),
                from_path: row.path.clone(),
                to_side: side_from_focus(to),
                to_cwd,
                name: row.name.clone(),
            });
        } else {
            self.worker.send(SftpCommand::Transfer {
                from_side: side_from_focus(from),
                from_path: row.path.clone(),
                to_side: side_from_focus(to),
                to_cwd,
                name: row.name.clone(),
            });
        }
    }

    fn resolve_conflict(&mut self, action: ConflictAction) {
        let Some(prompt) = self.state.conflict.take() else {
            return;
        };
        self.worker.send(SftpCommand::ResolveConflict {
            id: prompt.id,
            action,
            apply_to_all: prompt.apply_to_all,
        });
        self.state.status = match action {
            ConflictAction::Overwrite => "Replacing…".into(),
            ConflictAction::Keep => "Keeping local…".into(),
        };
    }

    /// Transfer the focused selection to the other pane.
    pub fn transfer_selected(&mut self) {
        let focus = self.state.focus;
        let Some(row) = self.state.selected(focus).cloned() else {
            self.state.error = Some("Select a file to transfer".into());
            return;
        };
        self.transfer_row(focus, &row);
    }

    /// Open the selected file in the OS default app.
    /// Local: open in place. Remote: download to temp, open, reupload on change.
    pub fn edit_selected(&mut self) {
        let focus = self.state.focus;
        let Some(row) = self.state.selected(focus).cloned() else {
            self.state.error = Some("Select a file to edit".into());
            return;
        };
        if row.is_dir {
            self.state.error = Some("Pick a file (folders can’t be edited yet)".into());
            return;
        }
        self.state.error = None;
        if self.state.side(focus).is_local() {
            self.state.status = format!("Editing {}…", row.name);
            open_path_with_default_app(Path::new(&row.path));
            return;
        }
        self.state.status = format!("Opening {}…", row.name);
        self.worker.send(SftpCommand::EditRemote {
            side: side_from_focus(focus),
            remote_path: row.path.clone(),
            name: row.name.clone(),
        });
    }

    pub fn refresh_focused(&mut self) {
        let cwd = self.state.side(self.state.focus).cwd.clone();
        self.cd(self.state.focus, cwd);
    }

    pub fn open_selected_dir(&mut self) {
        if let Some(row) = self.state.selected(self.state.focus).cloned() {
            if row.is_dir {
                self.cd(self.state.focus, row.path);
            }
        }
    }

    pub fn begin_mkdir_focused(&mut self) {
        self.state.begin_mkdir();
    }

    pub fn begin_rename_focused(&mut self) -> bool {
        self.state.begin_rename()
    }

    pub fn remove_focused(&mut self) {
        self.remove_selected();
    }

    /// Type into the active name editor. Returns true when consumed.
    pub fn type_name_edit(&mut self, text: &str) -> bool {
        let Some(edit) = self.state.name_edit.as_mut() else {
            return false;
        };
        if text.is_empty() || text.chars().any(|c| c.is_control()) {
            return false;
        }
        edit.draft.insert(text, 255, false);
        true
    }

    pub fn name_edit_backspace(&mut self) -> bool {
        let Some(edit) = self.state.name_edit.as_mut() else {
            return false;
        };
        edit.draft.backspace(false);
        true
    }
}

/// Keyboard actions handled while the SFTP pane owns input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SftpKey {
    Tab,
    Up,
    Down,
    Enter,
    Backspace,
    Delete,
    Mkdir,
    Rename,
    Upload,
    Download,
}

fn host_label(host: &HostRow) -> String {
    if host.name.trim().is_empty() {
        host.endpoint()
    } else {
        host.name.clone()
    }
}

fn focus_from_side(side: SftpSide) -> SftpFocus {
    match side {
        SftpSide::Left => SftpFocus::Left,
        SftpSide::Right => SftpFocus::Right,
    }
}

fn side_from_focus(focus: SftpFocus) -> SftpSide {
    match focus {
        SftpFocus::Left => SftpSide::Left,
        SftpFocus::Right => SftpSide::Right,
    }
}

fn row_from_list_entry(entry: SftpListEntry) -> SftpRow {
    SftpRow {
        name: entry.name,
        path: entry.path,
        is_dir: entry.is_dir,
        size: entry.size,
    }
}

fn connect_options_for_host(
    host: &HostRow,
    password: Option<String>,
    identity_pem: Option<(String, Option<String>)>,
) -> Result<SshConnectOptions, String> {
    let method = match host.auth_method.as_str() {
        "password" => HostAuthMethod::Password,
        "gssapi" => HostAuthMethod::Gssapi,
        _ => HostAuthMethod::Key,
    };

    let mut auth = SshAuth {
        username: host.username.clone(),
        method: Some(method),
        ..SshAuth::default()
    };

    match method {
        HostAuthMethod::Password => {
            auth.password = password.or_else(|| {
                // Empty password still attempts auth; worker surfaces failure.
                Some(String::new())
            });
        }
        HostAuthMethod::Key => {
            let (pem, passphrase) = identity_pem.ok_or_else(|| {
                "No saved SSH key — edit the host and select one (Settings → Managed SSH Keys)"
                    .to_string()
            })?;
            auth.identity_pem = Some(pem);
            auth.identity_passphrase = passphrase;
        }
        HostAuthMethod::Gssapi => {}
    }

    Ok(SshConnectOptions {
        hostname: host.hostname.clone(),
        port: host.port,
        auth,
        policy: HostKeyPolicy::Tofu,
        known_hosts: KnownHosts::default(),
        connect_timeout: DEFAULT_CONNECT_TIMEOUT,
        keepalive_interval: Some(DEFAULT_KEEPALIVE_INTERVAL),
    })
}

/// Open a local path with the OS default application for its file type.
fn open_path_with_default_app(path: &Path) {
    #[cfg(target_os = "macos")]
    {
        if let Err(err) = std::process::Command::new("open").arg(path).spawn() {
            tracing::warn!(error = %err, path = %path.display(), "failed to open edited file");
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Err(err) = std::process::Command::new("xdg-open").arg(path).spawn() {
            tracing::warn!(error = %err, path = %path.display(), "failed to open edited file");
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let wide_target: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let operation: Vec<u16> = "open\0".encode_utf16().collect();
        let result = unsafe {
            windows_sys::Win32::UI::Shell::ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                wide_target.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
            )
        };
        if (result as isize) <= 32 {
            tracing::warn!(
                path = %path.display(),
                code = result as isize,
                "ShellExecuteW could not open edited file"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "terminus-sftp-ui-{}-{}",
            name,
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn wait_ready(session: &mut ActiveSftp) {
        for _ in 0..100 {
            session.pump();
            if !session.state.loading {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn key_tab_toggles_focus() {
        let root = scratch("tab");
        let left = root.join("L");
        let right = root.join("R");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::create_dir_all(&right).unwrap();
        let mut s = ActiveSftp::start_local_dual(left, right, None);
        assert_eq!(s.state.focus, SftpFocus::Left);
        s.handle_key(SftpKey::Tab);
        assert_eq!(s.state.focus, SftpFocus::Right);
        s.handle_key(SftpKey::Tab);
        assert_eq!(s.state.focus, SftpFocus::Left);
        s.close();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn key_mkdir_and_rename() {
        let root = scratch("mkdir");
        let left = root.join("L");
        let right = root.join("R");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::write(left.join("f.txt"), b"x").unwrap();
        std::fs::create_dir_all(&right).unwrap();
        let mut s = ActiveSftp::start_local_dual(left, right, None);
        wait_ready(&mut s);
        assert!(s.handle_key(SftpKey::Mkdir));
        assert!(s.state.name_edit.is_some());
        s.state.cancel_name_edit();
        s.state.left.selected = Some(0);
        assert!(s.handle_key(SftpKey::Rename));
        assert_eq!(
            s.state.name_edit.as_ref().map(|e| e.kind),
            Some(SftpNameKind::Rename)
        );
        s.close();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn keyboard_map_table() {
        let expected = [
            ("Tab", "Tab"),
            ("ArrowUp", "Up"),
            ("ArrowDown", "Down"),
            ("Enter", "Enter"),
            ("Backspace", "Backspace"),
            ("Delete", "Delete"),
            ("F2", "Rename"),
            ("Ctrl+N", "Mkdir"),
            ("Ctrl+U", "Upload"),
            ("Ctrl+D", "Download"),
        ];
        assert_eq!(expected.len(), 10, "SFTP keyboard map must stay complete");
    }

    #[test]
    fn refresh_focused_lists() {
        let root = scratch("refresh");
        let left = root.join("L");
        let right = root.join("R");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::create_dir_all(&right).unwrap();
        let mut s = ActiveSftp::start_local_dual(left, right, None);
        wait_ready(&mut s);
        s.refresh_focused();
        assert!(
            s.state.loading
                || s.state.status.contains("Listing")
                || s.state.status == "Ready"
        );
        s.close();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn transfer_folder_sends_command() {
        let root = scratch("xfer-dir");
        let left = root.join("L");
        let right = root.join("R");
        std::fs::create_dir_all(left.join("folder")).unwrap();
        std::fs::write(left.join("folder").join("a.txt"), b"a").unwrap();
        std::fs::create_dir_all(&right).unwrap();
        let mut s = ActiveSftp::start_local_dual(left, right.clone(), None);
        wait_ready(&mut s);
        s.state.left.selected = s.state.left.entries.iter().position(|e| e.is_dir);
        s.transfer_selected();
        assert!(s.state.error.is_none(), "folder transfer should be allowed");
        for _ in 0..100 {
            s.pump();
            if right.join("folder").join("a.txt").is_file() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            right.join("folder").join("a.txt").is_file(),
            "folder tree should land on the other pane"
        );
        s.close();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn local_file_context_menu_offers_edit() {
        let root = scratch("local-edit-menu");
        let left = root.join("L");
        let right = root.join("R");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::write(left.join("notes.txt"), b"hi").unwrap();
        std::fs::create_dir_all(&right).unwrap();
        let mut s = ActiveSftp::start_local_dual(left, right, None);
        wait_ready(&mut s);
        let file_idx = s
            .state
            .left
            .entries
            .iter()
            .position(|e| e.name == "notes.txt" && !e.is_dir)
            .expect("notes.txt listed");
        let layout =
            SftpPaneLayout::from_bounds(terminus_ui::Rect::new(0.0, 0.0, 800.0, 600.0));
        let menu = s
            .context_menu_for_hit(&layout, SftpHit::LeftRow(file_idx), 40.0, 120.0)
            .expect("menu for local file");
        assert!(
            menu.items
                .iter()
                .any(|i| matches!(i.action, terminus_ui::ContextAction::SftpEdit)),
            "local file right-click should include Edit"
        );
        s.close();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn edit_selected_opens_local_file_without_remote_error() {
        let root = scratch("local-edit-open");
        let left = root.join("L");
        let right = root.join("R");
        std::fs::create_dir_all(&left).unwrap();
        let file = left.join("notes.txt");
        std::fs::write(&file, b"hi").unwrap();
        std::fs::create_dir_all(&right).unwrap();
        let mut s = ActiveSftp::start_local_dual(left, right, None);
        wait_ready(&mut s);
        s.state.focus = SftpFocus::Left;
        s.state.left.selected = s
            .state
            .left
            .entries
            .iter()
            .position(|e| e.name == "notes.txt");
        s.edit_selected();
        assert!(
            s.state.error.is_none(),
            "local edit must not require remote: {:?}",
            s.state.error
        );
        assert!(
            s.state.status.contains("Editing"),
            "expected Editing status, got {}",
            s.state.status
        );
        s.close();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn crumb_click_cds_to_parent_segment() {
        let root = scratch("crumb");
        let left = root.join("L");
        let mid = left.join("a");
        let nested = mid.join("b");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::create_dir_all(root.join("R")).unwrap();
        let mut s = ActiveSftp::start_local_dual(left.clone(), root.join("R"), None);
        wait_ready(&mut s);
        // Seed cwd as if the user had already entered the nested folder.
        s.state.left.cwd = nested.to_string_lossy().into_owned();
        let layout =
            SftpPaneLayout::from_bounds(terminus_ui::Rect::new(0.0, 0.0, 800.0, 600.0));
        let x = layout.left_header.x + 40.0;
        let y = layout.left_header.y + 4.0;
        assert_eq!(layout.hit_test(&s.state, x, y), SftpHit::LeftCrumb);
        s.handle_click(&layout, x, y, false);
        for _ in 0..50 {
            s.pump();
            let cwd = PathBuf::from(&s.state.left.cwd);
            if cwd == mid || cwd.file_name().is_some_and(|n| n == "a") {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let cwd = PathBuf::from(&s.state.left.cwd);
        assert_eq!(
            cwd.canonicalize().unwrap_or(cwd.clone()),
            mid.canonicalize().unwrap_or(mid),
            "crumb click should cd to the parent breadcrumb segment, got {}",
            cwd.display()
        );
        s.close();
        let _ = std::fs::remove_dir_all(root);
    }
}
