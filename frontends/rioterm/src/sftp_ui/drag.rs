use super::*;

impl ActiveSftp {
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

    /// Release a drag over pane `target` (`None` = outside both panes).
    /// `into` is a folder entry of `target` to drop into instead of its cwd.
    pub fn drag_release_to(
        &mut self,
        target: Option<SftpFocus>,
        into: Option<usize>,
    ) -> bool {
        self.drag_armed = None;
        let Some(drag) = self.state.drag.take() else {
            return false;
        };
        if drag.is_dir {
            let to = match target {
                Some(to) if to != drag.from => to,
                _ => return true,
            };
            let to_cwd = self.drop_cwd(to, into);
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
        let Some(to) = target else {
            return true;
        };
        if to == drag.from {
            return true;
        }
        let to_cwd = self.drop_cwd(to, into);
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

    /// Destination directory of a drop on `to` (a folder row, else its cwd).
    pub(super) fn drop_cwd(&self, to: SftpFocus, into: Option<usize>) -> String {
        let side = self.state.side(to);
        into.and_then(|i| side.entries.get(i))
            .filter(|r| r.is_dir)
            .map(|r| r.path.clone())
            .unwrap_or_else(|| side.cwd.clone())
    }
}
