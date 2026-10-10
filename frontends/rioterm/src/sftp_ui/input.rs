use super::*;

impl ActiveSftp {
    /// Activate an already hit-tested target (shared by the classic pane and
    /// the Files view, which hit-tests with its own geometry).
    pub fn handle_hit(&mut self, hit: SftpHit, double: bool) -> SftpClickResult {
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
                self.cancel_transfer();
            }
            SftpHit::Footer | SftpHit::Consume => {}
            SftpHit::Close | SftpHit::Miss => unreachable!("handled above"),
        }
        SftpClickResult::Handled
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
            SftpKey::Delete => false,
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

    /// Scroll `pane` by `delta_px` (positive = further down), clamped to
    /// `0..=max`.
    pub fn scroll_pane(&mut self, pane: SftpFocus, delta_px: f32, max: f32) -> bool {
        let side = self.state.side_mut(pane);
        let next = (side.scroll + delta_px).clamp(0.0, max.max(0.0));
        let changed = (next - side.scroll).abs() > f32::EPSILON;
        side.scroll = next;
        changed
    }
}
