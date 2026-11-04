use super::*;

/// Rows a PageUp / PageDown moves the cursor.
const PAGE_ROWS: isize = 10;

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
            SftpHit::LeftRow(i) => self.click_row(SftpFocus::Left, i, is_double),
            SftpHit::RightRow(i) => self.click_row(SftpFocus::Right, i, is_double),
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

    /// A click on row `i`: Ctrl toggles it, Shift selects up to it, a plain
    /// click selects it alone and a double click opens a folder. A file is
    /// only selected: copying it across is an explicit command (Ctrl+U/D,
    /// F5, the context menu or a drag), never a side effect of a double click.
    fn click_row(&mut self, focus: SftpFocus, i: usize, is_double: bool) {
        self.state.focus = focus;
        let mods = self.select_mods;
        let side = self.state.side_mut(focus);
        if mods.toggle {
            side.toggle_mark(i);
            return;
        }
        if mods.range {
            side.select_range_to(i);
            return;
        }
        side.select_only(i);
        if is_double {
            if let Some(row) = side.entries.get(i).cloned().filter(|r| r.is_dir) {
                self.cd(focus, row.path);
            }
        }
    }

    /// A click on empty space in `focus`'s list drops its selection.
    pub fn click_blank(&mut self, focus: SftpFocus) {
        self.state.focus = focus;
        if !self.select_mods.toggle && !self.select_mods.range {
            self.state.side_mut(focus).clear_selection();
        }
    }

    pub fn handle_key(&mut self, key: SftpKey) -> bool {
        match key {
            SftpKey::ExtendUp => {
                self.state.move_selection_by(-1, true);
                true
            }
            SftpKey::ExtendDown => {
                self.state.move_selection_by(1, true);
                true
            }
            SftpKey::PageUp => {
                self.state.move_selection_by(-PAGE_ROWS, false);
                true
            }
            SftpKey::PageDown => {
                self.state.move_selection_by(PAGE_ROWS, false);
                true
            }
            SftpKey::Home => {
                self.state.move_selection_by(isize::MIN / 2, false);
                true
            }
            SftpKey::End => {
                self.state.move_selection_by(isize::MAX / 2, false);
                true
            }
            SftpKey::SelectAll => {
                let focus = self.state.focus;
                self.state.side_mut(focus).select_all_rows()
            }
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
