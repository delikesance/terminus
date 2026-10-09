use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) async fn transfer(
    left: &Option<Arc<SftpConnection>>,
    right: &Option<Arc<SftpConnection>>,
    from_side: SftpSide,
    from_path: &str,
    to_side: SftpSide,
    to_cwd: &str,
    name: &str,
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
    conflicts: &ConflictInbox,
) -> Result<(), String> {
    let from_remote = side_is_remote(left, right, from_side);
    let to_remote = side_is_remote(left, right, to_side);

    if is_directory(left, right, from_side, from_path, from_remote).await? {
        return Box::pin(transfer_folder(
            left, right, from_side, from_path, to_side, to_cwd, name, events, wake,
            conflicts,
        ))
        .await;
    }

    // Never silently replace a file on the other side: ask first, like a
    // folder transfer does for each clashing file.
    if destination_exists(left, right, to_side, to_cwd, name, to_remote).await {
        let target = if to_remote {
            join_remote(to_cwd, name)
        } else {
            PathBuf::from(to_cwd)
                .join(name)
                .to_string_lossy()
                .into_owned()
        };
        if !from_remote && !to_remote && same_local_file(from_path, &target) {
            return Err(format!("{name} is already in this folder"));
        }
        let (remote_path, local_path) = if to_remote {
            (target.clone(), from_path.to_string())
        } else {
            (from_path.to_string(), target.clone())
        };
        let mut policy = ConflictPolicy::default();
        let mut next_id = SINGLE_FILE_CONFLICT_ID.fetch_add(1, Ordering::Relaxed);
        let action = ask_conflict(
            conflicts,
            events,
            wake,
            &mut policy,
            &mut next_id,
            ConflictKind::File,
            name,
            &remote_path,
            &local_path,
        )
        .await?;
        if action == ConflictAction::Keep {
            emit(
                events,
                wake,
                SftpEvent::TransferProgress {
                    label: format!("Kept existing {name}"),
                    done: 0,
                    total: 0,
                },
            );
            return Ok(());
        }
    }

    transfer_file(
        left,
        right,
        from_side,
        from_path,
        to_side,
        to_cwd,
        name,
        from_remote,
        to_remote,
        events,
        wake,
    )
    .await
}

/// Conflict ids for single-file transfers: disjoint from the per-folder
/// counters (which start at 1), so a late reply can never match the wrong
/// prompt.
pub(super) static SINGLE_FILE_CONFLICT_ID: AtomicU64 = AtomicU64::new(1 << 40);

pub(super) async fn destination_exists(
    left: &Option<Arc<SftpConnection>>,
    right: &Option<Arc<SftpConnection>>,
    to_side: SftpSide,
    to_cwd: &str,
    name: &str,
    to_remote: bool,
) -> bool {
    if to_remote {
        match conn_ref(left, right, to_side) {
            Some(conn) => conn
                .exists(&join_remote(to_cwd, name))
                .await
                .unwrap_or(false),
            None => false,
        }
    } else {
        tokio::fs::symlink_metadata(PathBuf::from(to_cwd).join(name))
            .await
            .is_ok()
    }
}

/// Whether two local paths name the same file (copying onto itself would
/// truncate it).
pub(super) fn same_local_file(a: &str, b: &str) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn transfer_file(
    left: &Option<Arc<SftpConnection>>,
    right: &Option<Arc<SftpConnection>>,
    from_side: SftpSide,
    from_path: &str,
    to_side: SftpSide,
    to_cwd: &str,
    name: &str,
    from_remote: bool,
    to_remote: bool,
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
) -> Result<(), String> {
    let to_path_remote = join_remote(to_cwd, name);
    let to_path_local = PathBuf::from(to_cwd).join(name);
    let from_path_local = PathBuf::from(from_path);

    match (from_remote, to_remote) {
        // Local → Remote: upload
        (false, true) => {
            let conn =
                conn_ref(left, right, to_side).ok_or_else(|| not_connected(to_side))?;
            run_with_retry(Operation::Upload, RetryPolicy::DEFAULT, || {
                transfer_upload(conn, &from_path_local, &to_path_remote, events, wake)
            })
            .await
        }
        // Remote → Local: download
        (true, false) => {
            let conn = conn_ref(left, right, from_side)
                .ok_or_else(|| not_connected(from_side))?;
            run_with_retry(Operation::Download, RetryPolicy::DEFAULT, || {
                transfer_download(conn, from_path, &to_path_local, events, wake)
            })
            .await
        }
        // Remote → Remote: chunked SFTP relay (no temp file).
        (true, true) => {
            let from_conn = conn_ref(left, right, from_side)
                .ok_or_else(|| not_connected(from_side))?;
            let to_conn =
                conn_ref(left, right, to_side).ok_or_else(|| not_connected(to_side))?;
            emit(
                events,
                wake,
                SftpEvent::TransferProgress {
                    label: format!("Copy {name}"),
                    done: 0,
                    total: 0,
                },
            );
            let total = run_with_retry(
                Operation::RemoteFileCopy,
                RetryPolicy::DEFAULT,
                || async {
                    from_conn
                        .copy_to(from_path, to_conn, &to_path_remote, |n| {
                            emit(
                                events,
                                wake,
                                SftpEvent::TransferProgress {
                                    label: format!("Copy {name}"),
                                    done: n,
                                    total: 0,
                                },
                            );
                        })
                        .await
                        .map_err(|e| e.to_string())
                },
            )
            .await?;
            emit(
                events,
                wake,
                SftpEvent::TransferProgress {
                    label: format!("Copy {name}"),
                    done: total,
                    total,
                },
            );
            Ok(())
        }
        // Local → Local: copy
        (false, false) => {
            let label = format!("Copy {name}");
            emit(
                events,
                wake,
                SftpEvent::TransferProgress {
                    label: label.clone(),
                    done: 0,
                    total: 0,
                },
            );
            if let Some(parent) = to_path_local.parent() {
                if !parent.as_os_str().is_empty() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(|e| e.to_string())?;
                }
            }
            let copied =
                run_with_retry(Operation::LocalCopy, RetryPolicy::DEFAULT, || async {
                    tokio::fs::copy(&from_path_local, &to_path_local)
                        .await
                        .map_err(|e| e.to_string())
                })
                .await?;
            emit(
                events,
                wake,
                SftpEvent::TransferProgress {
                    label,
                    done: copied,
                    total: copied,
                },
            );
            Ok(())
        }
    }
}

pub(super) async fn transfer_upload(
    conn: &SftpConnection,
    local: &Path,
    remote: &str,
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
) -> Result<(), String> {
    let label = format!(
        "Upload {}",
        local
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| local.display().to_string())
    );
    // Streamed in timed chunks into a temp sibling: a slow link is fine as
    // long as it keeps moving, and a failure never truncates the target.
    let mut throttle = ProgressThrottle::default();
    conn.upload_file(local, remote, |done, total| {
        if !throttle.should_emit(Instant::now(), done, total) {
            return;
        }
        emit(
            events,
            wake,
            SftpEvent::TransferProgress {
                label: label.clone(),
                done,
                total,
            },
        );
    })
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}

pub(super) async fn transfer_download(
    conn: &SftpConnection,
    remote: &str,
    local: &Path,
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
) -> Result<(), String> {
    let label = format!(
        "Download {}",
        Path::new(remote)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| remote.to_string())
    );
    let mut throttle = ProgressThrottle::default();
    conn.download_file(remote, local, |done, total| {
        if !throttle.should_emit(Instant::now(), done, total) {
            return;
        }
        emit(
            events,
            wake,
            SftpEvent::TransferProgress {
                label: label.clone(),
                done,
                total,
            },
        );
    })
    .await
    .map(|_| ())
    .map_err(|e| e.to_string())
}
