use super::*;

impl ActiveSftp {
    /// Context menu for `hit`; `blank_pane` is the pane under the pointer when
    /// the hit is empty space (Files view passes its own geometry's answer).
    pub fn context_menu_for(
        &mut self,
        hit: SftpHit,
        blank_pane: Option<SftpFocus>,
        x: f32,
        y: f32,
    ) -> Option<terminus_ui::ActionMenu> {
        match hit {
            SftpHit::LeftRow(i) => {
                self.state.focus = SftpFocus::Left;
                self.state.left.selected = Some(i);
                let row = self.state.left.entries.get(i)?;
                let transfer = Some(self.transfer_label(SftpFocus::Left));
                let can_edit = !row.is_dir;
                terminus_ui::ActionMenu::for_sftp_entry(
                    x, y, row.is_dir, transfer, can_edit,
                )
            }
            SftpHit::RightRow(i) => {
                self.state.focus = SftpFocus::Right;
                self.state.right.selected = Some(i);
                let row = self.state.right.entries.get(i)?;
                let transfer = Some(self.transfer_label(SftpFocus::Right));
                let can_edit = !row.is_dir;
                terminus_ui::ActionMenu::for_sftp_entry(
                    x, y, row.is_dir, transfer, can_edit,
                )
            }
            SftpHit::LeftParent | SftpHit::LeftCrumb => {
                self.state.focus = SftpFocus::Left;
                terminus_ui::ActionMenu::for_sftp_empty(x, y)
            }
            SftpHit::RightParent | SftpHit::RightCrumb => {
                self.state.focus = SftpFocus::Right;
                terminus_ui::ActionMenu::for_sftp_empty(x, y)
            }
            SftpHit::Consume => {
                let pane = blank_pane?;
                self.state.focus = pane;
                terminus_ui::ActionMenu::for_sftp_empty(x, y)
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

    pub(super) fn transfer_label(&self, from: SftpFocus) -> &'static str {
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
}
