use super::*;

pub(super) async fn transfer_folder_differential_to_local(
    conn: &SftpConnection,
    remote_root: &str,
    local_root: &Path,
    name: &str,
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
    conflicts: &ConflictInbox,
) -> Result<(), String> {
    drain_stale_conflicts(conflicts);
    emit(
        events,
        wake,
        SftpEvent::TransferProgress {
            label: format!("Comparing {name}…"),
            done: 0,
            total: 0,
        },
    );

    let plan =
        plan_via_exec_manifest(conn, remote_root, local_root, name, events, wake).await?;

    let mut policy = ConflictPolicy::default();
    let mut next_id: u64 = 1;
    let total = plan.len() as u64;

    for (i, action) in plan.into_iter().enumerate() {
        emit(
            events,
            wake,
            SftpEvent::TransferProgress {
                label: format!("Syncing {name}…"),
                done: i as u64,
                total,
            },
        );
        match action {
            DiffAction::Skip { .. } => {}
            DiffAction::DownloadFile { relative } => {
                let remote = join_rel_remote(remote_root, &relative);
                let local = join_rel_path(local_root, &relative);
                if let Some(parent) = local.parent() {
                    tokio::fs::create_dir_all(parent)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                transfer_download(conn, &remote, &local, events, wake).await?;
            }
            DiffAction::ZipSubtree { relative } => {
                let remote = join_rel_remote(remote_root, &relative);
                let folder_name = relative.rsplit('/').next().unwrap_or(&relative);
                let extract_cwd = if let Some(parent) = Path::new(&relative).parent() {
                    if parent.as_os_str().is_empty() {
                        local_root.to_path_buf()
                    } else {
                        join_rel_path(local_root, &parent.to_string_lossy())
                    }
                } else {
                    local_root.to_path_buf()
                };
                tokio::fs::create_dir_all(&extract_cwd)
                    .await
                    .map_err(|e| e.to_string())?;
                // If overwriting, remove existing local subtree first.
                let dest_dir = extract_cwd.join(folder_name);
                if dest_dir.exists() {
                    tokio::fs::remove_dir_all(&dest_dir)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                zip_remote_subtree_to_local(
                    conn,
                    &remote,
                    folder_name,
                    &extract_cwd,
                    events,
                    wake,
                )
                .await?;
            }
            DiffAction::AskFile { relative } => {
                let remote = join_rel_remote(remote_root, &relative);
                let local = join_rel_path(local_root, &relative);
                let decision = ask_conflict(
                    conflicts,
                    events,
                    wake,
                    &mut policy,
                    &mut next_id,
                    ConflictKind::File,
                    &relative,
                    &remote,
                    &local.to_string_lossy(),
                )
                .await?;
                if decision == ConflictAction::Overwrite {
                    if let Some(parent) = local.parent() {
                        tokio::fs::create_dir_all(parent)
                            .await
                            .map_err(|e| e.to_string())?;
                    }
                    transfer_download(conn, &remote, &local, events, wake).await?;
                }
            }
            DiffAction::AskDir { relative } => {
                let remote = join_rel_remote(remote_root, &relative);
                let local = join_rel_path(local_root, &relative);
                let decision = ask_conflict(
                    conflicts,
                    events,
                    wake,
                    &mut policy,
                    &mut next_id,
                    ConflictKind::Directory,
                    &relative,
                    &remote,
                    &local.to_string_lossy(),
                )
                .await?;
                if decision == ConflictAction::Overwrite {
                    let folder_name = relative.rsplit('/').next().unwrap_or(&relative);
                    let extract_cwd = if let Some(parent) = Path::new(&relative).parent()
                    {
                        if parent.as_os_str().is_empty() {
                            local_root.to_path_buf()
                        } else {
                            join_rel_path(local_root, &parent.to_string_lossy())
                        }
                    } else {
                        local_root.to_path_buf()
                    };
                    if local.exists() {
                        tokio::fs::remove_dir_all(&local)
                            .await
                            .map_err(|e| e.to_string())?;
                    }
                    tokio::fs::create_dir_all(&extract_cwd)
                        .await
                        .map_err(|e| e.to_string())?;
                    zip_remote_subtree_to_local(
                        conn,
                        &remote,
                        folder_name,
                        &extract_cwd,
                        events,
                        wake,
                    )
                    .await?;
                }
            }
        }
    }

    emit(
        events,
        wake,
        SftpEvent::TransferProgress {
            label: format!("Synced folder {name}"),
            done: total,
            total,
        },
    );
    Ok(())
}
