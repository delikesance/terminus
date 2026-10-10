use super::*;

/// Tracked remote-edit: local temp file watched for mtime/size changes.
pub(super) struct EditSession {
    pub(super) id: u64,
    pub(super) side: SftpSide,
    pub(super) remote_path: String,
    pub(super) local_path: PathBuf,
    pub(super) last_mtime: SystemTime,
    pub(super) last_len: u64,
    /// True after a local change is detected; upload after one stable poll.
    pub(super) dirty: bool,
    pub(super) stable_polls: u8,
}

pub(super) fn drop_edit_sessions_for_side(
    sessions: &mut Vec<EditSession>,
    side: SftpSide,
) {
    sessions.retain(|s| {
        if s.side == side {
            cleanup_edit_temp(&s.local_path);
            false
        } else {
            true
        }
    });
}

pub(super) fn drop_all_edit_sessions(sessions: &mut Vec<EditSession>) {
    for session in sessions.drain(..) {
        cleanup_edit_temp(&session.local_path);
    }
}

pub(super) fn cleanup_edit_temp(local_path: &Path) {
    let _ = std::fs::remove_file(local_path);
    if let Some(parent) = local_path.parent() {
        let _ = std::fs::remove_dir(parent);
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn start_edit_remote(
    left: &Option<Arc<SftpConnection>>,
    right: &Option<Arc<SftpConnection>>,
    side: SftpSide,
    remote_path: &str,
    name: &str,
    sessions: &mut Vec<EditSession>,
    next_id: &mut u64,
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
) -> Result<(), String> {
    if !side_is_remote(left, right, side) {
        return Err("Edit is only available for remote files".into());
    }
    let conn = conn_ref(left, right, side).ok_or_else(|| not_connected(side))?;

    if is_directory(left, right, side, remote_path, true).await? {
        return Err("folders aren't supported yet".into());
    }

    let dir = std::env::temp_dir()
        .join("terminus-sftp-edit")
        .join(uuid::Uuid::new_v4().to_string());
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| e.to_string())?;
    let local_path = dir.join(name);

    transfer_download(conn, remote_path, &local_path, events, wake).await?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ =
            std::fs::set_permissions(&local_path, std::fs::Permissions::from_mode(0o600));
    }

    let meta = std::fs::metadata(&local_path).map_err(|e| e.to_string())?;
    let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let len = meta.len();
    let id = *next_id;
    *next_id = next_id.saturating_add(1);

    sessions.push(EditSession {
        id,
        side,
        remote_path: remote_path.to_string(),
        local_path: local_path.clone(),
        last_mtime: mtime,
        last_len: len,
        dirty: false,
        stable_polls: 0,
    });

    emit(
        events,
        wake,
        SftpEvent::EditReady {
            id,
            side,
            remote_path: remote_path.to_string(),
            local_path,
        },
    );
    Ok(())
}

pub(super) async fn poll_edit_sessions(
    sessions: &mut [EditSession],
    left: &Option<Arc<SftpConnection>>,
    right: &Option<Arc<SftpConnection>>,
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
) {
    // Collect uploads first so we don't hold overlapping borrows across await.
    let mut to_upload: Vec<(usize, u64, SftpSide, String, PathBuf)> = Vec::new();

    for (idx, session) in sessions.iter_mut().enumerate() {
        let meta = match std::fs::metadata(&session.local_path) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let len = meta.len();

        if mtime != session.last_mtime || len != session.last_len {
            session.last_mtime = mtime;
            session.last_len = len;
            session.dirty = true;
            session.stable_polls = 0;
            continue;
        }

        if !session.dirty {
            continue;
        }

        session.stable_polls = session.stable_polls.saturating_add(1);
        // Two stable polls (~1s) avoids half-written saves from editors that
        // truncate-then-write or rewrite via temp+rename.
        if session.stable_polls < 2 {
            continue;
        }

        to_upload.push((
            idx,
            session.id,
            session.side,
            session.remote_path.clone(),
            session.local_path.clone(),
        ));
        session.dirty = false;
        session.stable_polls = 0;
    }

    for (_idx, id, side, remote_path, local_path) in to_upload {
        let Some(conn) = conn_ref(left, right, side) else {
            emit(events, wake, SftpEvent::Failed(not_connected(side)));
            continue;
        };
        match transfer_upload(conn, &local_path, &remote_path, events, wake).await {
            Ok(()) => {
                // Refresh baseline after upload in case the editor touched the file again.
                if let Ok(meta) = std::fs::metadata(&local_path) {
                    if let Some(session) = sessions.iter_mut().find(|s| s.id == id) {
                        session.last_mtime =
                            meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                        session.last_len = meta.len();
                    }
                }
                emit(
                    events,
                    wake,
                    SftpEvent::EditSaved {
                        id,
                        remote_path: remote_path.clone(),
                    },
                );
                let parent = parent_remote(&remote_path);
                emit_listed_remote(events, wake, side, conn, &parent).await;
            }
            Err(err) => emit(events, wake, SftpEvent::Failed(err)),
        }
    }
}
