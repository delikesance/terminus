use super::*;

/// Copy a directory tree with as few SFTP round-trips as possible.
///
/// Remote side archives in one shot via native `tar`/`zip` or PowerShell
/// `Compress-Archive`. If the remote has none of these tools, the transfer
/// fails with a clear error (no per-file SFTP fallback).
#[allow(clippy::too_many_arguments)]
pub(super) async fn transfer_folder(
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

    // Differential download when the destination folder already exists locally.
    if from_remote && !to_remote {
        let dest = PathBuf::from(to_cwd).join(name);
        if dest.is_dir() {
            let conn = conn_ref(left, right, from_side)
                .ok_or_else(|| not_connected(from_side))?;
            return transfer_folder_differential_to_local(
                conn, from_path, &dest, name, events, wake, conflicts,
            )
            .await;
        }
    }

    if from_remote && to_remote {
        let from_conn =
            conn_ref(left, right, from_side).ok_or_else(|| not_connected(from_side))?;
        let to_conn =
            conn_ref(left, right, to_side).ok_or_else(|| not_connected(to_side))?;
        let dest = join_remote(to_cwd, name);
        if to_conn.exists(&dest).await.unwrap_or(false) {
            return transfer_folder_differential_remote_to_remote(
                from_conn, to_conn, from_path, to_cwd, name, events, wake, conflicts,
            )
            .await;
        }
        return transfer_folder_remote_to_remote(
            from_conn, to_conn, from_path, to_cwd, name, events, wake,
        )
        .await;
    }

    // --- paths with a local side ---
    // Local → remote: check write access first so Permission Denied is not
    // masked by a later "no unzip" failure (e.g. uploading into /etc/nixos).
    if !from_remote && to_remote {
        let conn =
            conn_ref(left, right, to_side).ok_or_else(|| not_connected(to_side))?;
        ensure_remote_cwd_writable(conn, to_cwd).await?;
    }

    let staging = std::env::temp_dir().join(format!(
        "terminus-sftp-folder-{}-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4(),
        sanitize_temp_name(name)
    ));

    emit(
        events,
        wake,
        SftpEvent::TransferProgress {
            label: format!("Packing {name}…"),
            done: 0,
            total: 0,
        },
    );

    let (archive_path, kind) = if from_remote {
        let conn =
            conn_ref(left, right, from_side).ok_or_else(|| not_connected(from_side))?;
        let kind =
            try_remote_pack_to_local(conn, from_path, name, &staging, events, wake)
                .await?;
        (staging_with_ext(&staging, kind), kind)
    } else if to_remote {
        let conn =
            conn_ref(left, right, to_side).ok_or_else(|| not_connected(to_side))?;
        let env = probe_remote_env(conn).await;
        let kind = choose_upload_archive_kind(&env.tools, env.family)?;
        let archive_path = staging_with_ext(&staging, kind);
        let src = PathBuf::from(from_path);
        let out = archive_path.clone();
        let root = name.to_string();
        tokio::task::spawn_blocking(move || match kind {
            ArchiveKind::Zip => zip_local_tree(&src, &root, &out),
            ArchiveKind::TarGz => tar_gz_local_tree(&src, &root, &out),
        })
        .await
        .map_err(|e| e.to_string())??;
        (archive_path, kind)
    } else {
        let zip_path = staging_with_ext(&staging, ArchiveKind::Zip);
        let src = PathBuf::from(from_path);
        let zip = zip_path.clone();
        let root = name.to_string();
        tokio::task::spawn_blocking(move || zip_local_tree(&src, &root, &zip))
            .await
            .map_err(|e| e.to_string())??;
        (zip_path, ArchiveKind::Zip)
    };

    let zip_len = tokio::fs::metadata(&archive_path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);
    emit(
        events,
        wake,
        SftpEvent::TransferProgress {
            label: format!("Archive ready ({zip_len} bytes)"),
            done: 0,
            total: zip_len,
        },
    );

    let unpack = if to_remote {
        let conn =
            conn_ref(left, right, to_side).ok_or_else(|| not_connected(to_side))?;
        emit(
            events,
            wake,
            SftpEvent::TransferProgress {
                label: format!("Unpacking {name} on server…"),
                done: 0,
                total: zip_len,
            },
        );
        remote_extract_uploaded(conn, &archive_path, to_cwd, name, kind, events, wake)
            .await
    } else {
        emit(
            events,
            wake,
            SftpEvent::TransferProgress {
                label: format!("Unpacking {name}…"),
                done: 0,
                total: zip_len,
            },
        );
        let dest = PathBuf::from(to_cwd);
        let archive = archive_path.clone();
        tokio::task::spawn_blocking(move || extract_local_archive(&archive, &dest, kind))
            .await
            .map_err(|e| e.to_string())?
    };

    let _ = tokio::fs::remove_file(&archive_path).await;
    unpack?;

    emit(
        events,
        wake,
        SftpEvent::TransferProgress {
            label: format!("Copied folder {name}"),
            done: zip_len,
            total: zip_len,
        },
    );
    Ok(())
}

pub(super) fn staging_with_ext(base: &Path, kind: ArchiveKind) -> PathBuf {
    match kind {
        ArchiveKind::TarGz => base.with_extension("tar.gz"),
        ArchiveKind::Zip => base.with_extension("zip"),
    }
}

/// Host A → Host B: archive via native tools on both sides (error if missing).
pub(super) async fn transfer_folder_remote_to_remote(
    from: &SftpConnection,
    to: &SftpConnection,
    from_path: &str,
    to_cwd: &str,
    name: &str,
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
) -> Result<(), String> {
    run_with_retry(Operation::RemoteArchiveCopy, RetryPolicy::DEFAULT, || {
        transfer_folder_remote_to_remote_via_archive(
            from, to, from_path, to_cwd, name, events, wake,
        )
    })
    .await
}

pub(super) async fn transfer_folder_remote_to_remote_via_archive(
    from: &SftpConnection,
    to: &SftpConnection,
    from_path: &str,
    to_cwd: &str,
    name: &str,
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
) -> Result<(), String> {
    let from_session = acquire_pack_session(from).await?;
    let to_session = acquire_extract_session(to, from_session.kind).await?;

    let local = std::env::temp_dir().join(format!(
        "terminus-sftp-ab-{}-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4(),
        sanitize_temp_name(name)
    ));
    let local = staging_with_ext(&local, from_session.kind);

    emit(
        events,
        wake,
        SftpEvent::TransferProgress {
            label: format!("Archiving {name} on source…"),
            done: 0,
            total: 0,
        },
    );
    let remote_archive =
        remote_create_archive(from, from_path, name, &from_session, events, wake).await?;

    emit(
        events,
        wake,
        SftpEvent::TransferProgress {
            label: format!("Downloading {name} archive…"),
            done: 0,
            total: 0,
        },
    );
    let download = transfer_download(from, &remote_archive, &local, events, wake).await;
    let _ = from.remove(&remote_archive).await;
    download?;

    emit(
        events,
        wake,
        SftpEvent::TransferProgress {
            label: format!("Uploading {name} archive…"),
            done: 0,
            total: 0,
        },
    );
    let remote_name = format!(
        "terminus-sftp-in-{}-{}.{}",
        std::process::id(),
        uuid::Uuid::new_v4(),
        archive_ext(from_session.kind)
    );
    let remote_in = join_tmp(&to_session.env.tmp, &remote_name);
    transfer_upload(to, &local, &remote_in, events, wake).await?;
    let script = archive_extract_script(to_cwd, &remote_in, &to_session);
    let (code, _stdout, stderr) = to.exec(&script).await.map_err(|e| e.to_string())?;
    let _ = to.remove(&remote_in).await;
    if code != 0 {
        return Err(format!(
            "remote archive extract failed (exit {code}): {}",
            String::from_utf8_lossy(&stderr)
        ));
    }
    let _ = tokio::fs::remove_file(&local).await;
    emit(
        events,
        wake,
        SftpEvent::TransferProgress {
            label: format!("Copied folder {name}"),
            done: 1,
            total: 1,
        },
    );
    Ok(())
}
