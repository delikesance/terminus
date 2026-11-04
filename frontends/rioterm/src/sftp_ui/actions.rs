use super::*;

impl ActiveSftp {
    /// Abort the transfer in flight, the ones queued behind it and any
    /// conflict prompt waiting on it. The worker confirms with
    /// [`SftpEvent::Cancelled`] once the transfer really stopped.
    pub fn cancel_transfer(&mut self) {
        if !self.busy() {
            return;
        }
        self.worker.send(SftpCommand::CancelTransfer);
        self.state.clear_conflict();
        self.state.status = "Cancelling…".into();
    }

    /// Resolve the pending conflict: replace (`true`) or keep the existing one.
    pub fn answer_conflict(&mut self, replace: bool) {
        self.resolve_conflict(if replace {
            ConflictAction::Overwrite
        } else {
            ConflictAction::Keep
        });
    }

    pub(super) fn cd(&mut self, focus: SftpFocus, path: String) {
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

    pub(super) fn cd_parent(&mut self, focus: SftpFocus) {
        let cwd = self.state.side(focus).cwd.clone();
        // Land on the folder we came from.
        let left = cwd.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next();
        self.state.side_mut(focus).pending_select =
            left.filter(|n| !n.is_empty()).map(str::to_string);
        let parent = if self.state.side(focus).is_local() {
            PathBuf::from(parent_path(&cwd))
                .to_string_lossy()
                .into_owned()
        } else {
            parent_path(&cwd)
        };
        self.cd(focus, parent);
    }

    /// Remove every selected row of the focused pane.
    pub(super) fn remove_selected(&mut self) {
        let focus = self.state.focus;
        let rows: Vec<SftpRow> = self
            .state
            .side(focus)
            .selection()
            .into_iter()
            .cloned()
            .collect();
        if rows.is_empty() {
            return;
        }
        self.state.status = match rows.as_slice() {
            [row] => format!("Removing {}…", row.name),
            rows => format!("Removing {} items…", rows.len()),
        };
        let side = side_from_focus(focus);
        let local = self.state.side(focus).is_local();
        for row in rows {
            if local {
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
        // The new folder / renamed entry is what the user looks for next.
        self.state.side_mut(focus).pending_select = Some(name.clone());
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

    pub(super) fn transfer_row(&mut self, from: SftpFocus, row: &SftpRow) {
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
        self.queue_transfer(from, row, to, to_cwd);
    }

    /// Queue `row` of `from` for the worker, into `to_cwd` on `to`.
    pub(super) fn queue_transfer(
        &self,
        from: SftpFocus,
        row: &SftpRow,
        to: SftpFocus,
        to_cwd: String,
    ) {
        let (from_side, to_side) = (side_from_focus(from), side_from_focus(to));
        if row.is_dir {
            self.worker.send(SftpCommand::TransferFolder {
                from_side,
                from_path: row.path.clone(),
                to_side,
                to_cwd,
                name: row.name.clone(),
            });
        } else {
            self.worker.send(SftpCommand::Transfer {
                from_side,
                from_path: row.path.clone(),
                to_side,
                to_cwd,
                name: row.name.clone(),
            });
        }
    }

    pub(super) fn resolve_conflict(&mut self, action: ConflictAction) {
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
            ConflictAction::Keep => "Keeping existing…".into(),
        };
    }

    /// Transfer the focused selection (one row or several) to the other pane.
    pub fn transfer_selected(&mut self) {
        let focus = self.state.focus;
        let rows: Vec<SftpRow> = self
            .state
            .side(focus)
            .selection()
            .into_iter()
            .cloned()
            .collect();
        match rows.as_slice() {
            [] => self.state.error = Some("Select a file to transfer".into()),
            [row] => self.transfer_row(focus, row),
            rows => {
                if self.state.conflict.is_some() {
                    self.state.error = Some("Finish the conflict prompt first".into());
                    return;
                }
                let to = match focus {
                    SftpFocus::Left => SftpFocus::Right,
                    SftpFocus::Right => SftpFocus::Left,
                };
                let to_cwd = self.state.side(to).cwd.clone();
                self.state.status = format!("Transferring {} items…", rows.len());
                self.state.error = None;
                for row in rows {
                    self.queue_transfer(focus, row, to, to_cwd.clone());
                }
            }
        }
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

    /// The confirmation to show before removing the selection.
    pub fn delete_prompt(&self) -> Option<terminus_ui::confirm::ConfirmPrompt> {
        use terminus_ui::confirm::ConfirmPrompt;
        let rows = self.state.side(self.state.focus).selection();
        match rows.as_slice() {
            [] => None,
            [row] => Some(ConfirmPrompt::sftp_delete(&row.name, row.is_dir)),
            rows => Some(ConfirmPrompt::sftp_delete_many(
                rows.len(),
                rows.iter().filter(|r| r.is_dir).count(),
            )),
        }
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
