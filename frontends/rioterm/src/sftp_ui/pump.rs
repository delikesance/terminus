use super::*;

impl ActiveSftp {
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
                    // Start in the login directory, like `sftp` / `ssh` do.
                    self.worker.send(SftpCommand::ListRemote {
                        side,
                        path: terminus_bridge::REMOTE_HOME.into(),
                    });
                }
                SftpEvent::ConnectFailed { side, message } => {
                    self.state.set_connect_error(focus_from_side(side), message);
                    self.state.status = "Not connected".into();
                }
                SftpEvent::Listed {
                    side,
                    path,
                    entries,
                } => {
                    let rows = entries.into_iter().map(row_from_list_entry).collect();
                    self.state.set_listed(focus_from_side(side), path, rows);
                    self.state.clear_transfer();
                    self.state.status = "Ready".into();
                }
                SftpEvent::TransferProgress { label, done, total } => {
                    self.state.set_transfer(label.clone(), done, total);
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
                    self.state.clear_transfer();
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
                    self.state.clear_transfer();
                }
            }
        }
        true
    }
}
