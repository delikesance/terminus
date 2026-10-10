use super::*;

/// Remote→remote differential: stage through local temp for file copies; zip
/// subtrees via the existing archive pipeline.
#[allow(clippy::too_many_arguments)]
pub(super) async fn transfer_folder_differential_remote_to_remote(
    from: &SftpConnection,
    to: &SftpConnection,
    from_path: &str,
    to_cwd: &str,
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

    let dest_root = join_remote(to_cwd, name);

    emit(
        events,
        wake,
        SftpEvent::TransferProgress {
            label: "Preparing walkers…".into(),
            done: 0,
            total: 0,
        },
    );
    let (from_walk, to_walk) = tokio::join!(
        walk_remote::ensure_remote_walk(from),
        walk_remote::ensure_remote_walk(to),
    );
    let (walk_from, env_from) = from_walk?;
    let (walk_to, env_to) = to_walk?;

    emit(
        events,
        wake,
        SftpEvent::TransferProgress {
            label: format!("Indexing {name}…"),
            done: 0,
            total: 0,
        },
    );
    let (meta_from, meta_to) = tokio::join!(
        walk_remote::remote_walk_meta(from, &walk_from, from_path),
        walk_remote::remote_walk_meta(to, &walk_to, &dest_root),
    );
    let meta_from = meta_from?;
    let meta_to = meta_to?;

    let plan = if meta_from == meta_to {
        Vec::new()
    } else {
        let suspects = walk_remote::content_suspects(&meta_from, &meta_to);
        let mut digests_from = terminus_walk::DigestMap::new();
        let mut digests_to = terminus_walk::DigestMap::new();
        if !suspects.is_empty() {
            emit(
                events,
                wake,
                SftpEvent::TransferProgress {
                    label: format!("Hashing {} files…", suspects.len()),
                    done: 0,
                    total: suspects.len() as u64,
                },
            );
            let (hf, ht) = tokio::join!(
                walk_remote::remote_walk_hash(
                    from,
                    &walk_from,
                    from_path,
                    &suspects,
                    &env_from.tmp
                ),
                walk_remote::remote_walk_hash(
                    to,
                    &walk_to,
                    &dest_root,
                    &suspects,
                    &env_to.tmp
                ),
            );
            digests_from = hf?;
            digests_to = ht?;
        }
        let remote_tree = file_tree_from_walk_meta(name, &meta_from, &digests_from);
        let dest_tree = file_tree_from_walk_meta(name, &meta_to, &digests_to);
        plan_differential(&remote_tree, &dest_tree)
    };

    let mut policy = ConflictPolicy::default();
    let mut next_id: u64 = 1;
    let total = plan.len() as u64;
    let staging_dir = std::env::temp_dir().join(format!(
        "terminus-sftp-r2r-diff-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    tokio::fs::create_dir_all(&staging_dir)
        .await
        .map_err(|e| e.to_string())?;

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
                let remote = join_rel_remote(from_path, &relative);
                let dest = join_rel_remote(&dest_root, &relative);
                let tmp = staging_dir.join(format!("f-{}", uuid::Uuid::new_v4()));
                transfer_download(from, &remote, &tmp, events, wake).await?;
                // Ensure parent exists on dest.
                if let Some((parent, _)) = dest.rsplit_once('/') {
                    let _ = to.mkdir(parent).await;
                    // Best-effort nested mkdir
                    let mut acc = String::new();
                    for part in parent.split('/').filter(|p| !p.is_empty()) {
                        acc = if acc.is_empty() {
                            format!("/{part}")
                        } else {
                            format!("{acc}/{part}")
                        };
                        let _ = to.mkdir(&acc).await;
                    }
                }
                transfer_upload(to, &tmp, &dest, events, wake).await?;
                let _ = tokio::fs::remove_file(&tmp).await;
            }
            DiffAction::ZipSubtree { relative } => {
                let remote = join_rel_remote(from_path, &relative);
                let folder_name = relative.rsplit('/').next().unwrap_or(&relative);
                let dest_parent = if let Some((parent, _)) = relative.rsplit_once('/') {
                    join_rel_remote(&dest_root, parent)
                } else {
                    dest_root.clone()
                };
                let mut acc = String::new();
                for part in dest_parent.split('/').filter(|p| !p.is_empty()) {
                    acc = if acc.is_empty() {
                        format!("/{part}")
                    } else {
                        format!("{acc}/{part}")
                    };
                    let _ = to.mkdir(&acc).await;
                }
                // Pack from source, upload+extract into dest_parent.
                let staging = staging_dir.join(format!("z-{}", uuid::Uuid::new_v4()));
                let kind = try_remote_pack_to_local(
                    from,
                    &remote,
                    folder_name,
                    &staging,
                    events,
                    wake,
                )
                .await?;
                let archive = staging_with_ext(&staging, kind);
                remote_extract_uploaded(
                    to,
                    &archive,
                    &dest_parent,
                    folder_name,
                    kind,
                    events,
                    wake,
                )
                .await?;
                let _ = tokio::fs::remove_file(&archive).await;
            }
            DiffAction::AskFile { relative } => {
                let remote = join_rel_remote(from_path, &relative);
                let dest = join_rel_remote(&dest_root, &relative);
                let decision = ask_conflict(
                    conflicts,
                    events,
                    wake,
                    &mut policy,
                    &mut next_id,
                    ConflictKind::File,
                    &relative,
                    &remote,
                    &dest,
                )
                .await?;
                if decision == ConflictAction::Overwrite {
                    let tmp = staging_dir.join(format!("f-{}", uuid::Uuid::new_v4()));
                    transfer_download(from, &remote, &tmp, events, wake).await?;
                    transfer_upload(to, &tmp, &dest, events, wake).await?;
                    let _ = tokio::fs::remove_file(&tmp).await;
                }
            }
            DiffAction::AskDir { relative } => {
                let remote = join_rel_remote(from_path, &relative);
                let dest = join_rel_remote(&dest_root, &relative);
                let decision = ask_conflict(
                    conflicts,
                    events,
                    wake,
                    &mut policy,
                    &mut next_id,
                    ConflictKind::Directory,
                    &relative,
                    &remote,
                    &dest,
                )
                .await?;
                if decision == ConflictAction::Overwrite {
                    let folder_name = relative.rsplit('/').next().unwrap_or(&relative);
                    let dest_parent = if let Some((parent, _)) = relative.rsplit_once('/')
                    {
                        join_rel_remote(&dest_root, parent)
                    } else {
                        dest_root.clone()
                    };
                    let _ = to.remove_recursive(&dest).await;
                    let mut acc = String::new();
                    for part in dest_parent.split('/').filter(|p| !p.is_empty()) {
                        acc = if acc.is_empty() {
                            format!("/{part}")
                        } else {
                            format!("{acc}/{part}")
                        };
                        let _ = to.mkdir(&acc).await;
                    }
                    let staging = staging_dir.join(format!("z-{}", uuid::Uuid::new_v4()));
                    let kind = try_remote_pack_to_local(
                        from,
                        &remote,
                        folder_name,
                        &staging,
                        events,
                        wake,
                    )
                    .await?;
                    let archive = staging_with_ext(&staging, kind);
                    remote_extract_uploaded(
                        to,
                        &archive,
                        &dest_parent,
                        folder_name,
                        kind,
                        events,
                        wake,
                    )
                    .await?;
                    let _ = tokio::fs::remove_file(&archive).await;
                }
            }
        }
    }

    let _ = tokio::fs::remove_dir_all(&staging_dir).await;
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
