use super::*;

pub(super) fn emit(
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
    event: SftpEvent,
) {
    if events.send(event).is_ok() {
        if let Some(wake) = wake {
            wake();
        }
    }
}

pub(super) fn conn_mut<'a>(
    left: &'a mut Option<Arc<SftpConnection>>,
    right: &'a mut Option<Arc<SftpConnection>>,
    side: SftpSide,
) -> &'a mut Option<Arc<SftpConnection>> {
    match side {
        SftpSide::Left => left,
        SftpSide::Right => right,
    }
}

pub(super) fn conn_ref<'a>(
    left: &'a Option<Arc<SftpConnection>>,
    right: &'a Option<Arc<SftpConnection>>,
    side: SftpSide,
) -> Option<&'a SftpConnection> {
    match side {
        SftpSide::Left => left.as_deref(),
        SftpSide::Right => right.as_deref(),
    }
}

pub(super) fn side_is_remote(
    left: &Option<Arc<SftpConnection>>,
    right: &Option<Arc<SftpConnection>>,
    side: SftpSide,
) -> bool {
    conn_ref(left, right, side).is_some()
}

pub(super) fn join_remote(cwd: &str, name: &str) -> String {
    let cwd = cwd.trim_end_matches('/');
    if cwd.is_empty() || cwd == "/" {
        format!("/{name}")
    } else {
        format!("{cwd}/{name}")
    }
}

pub(super) fn not_connected(side: SftpSide) -> String {
    format!("SFTP is not connected on {side:?}")
}

pub(super) async fn emit_listed_local(
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
    side: SftpSide,
    path: &Path,
) {
    match list_local(path).await {
        Ok((cwd, entries)) => emit(
            events,
            wake,
            SftpEvent::Listed {
                side,
                path: cwd,
                entries,
            },
        ),
        Err(err) => emit(events, wake, SftpEvent::Failed(err)),
    }
}

/// `ListRemote` path that lists the remote login directory.
pub const REMOTE_HOME: &str = "~";

pub(super) async fn emit_listed_remote(
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
    side: SftpSide,
    conn: &SftpConnection,
    path: &str,
) {
    // `~` (the UI's first listing) means the login directory.
    let home;
    let path = if path == REMOTE_HOME {
        home = conn.home_dir().await.unwrap_or_else(|_| "/".to_string());
        home.as_str()
    } else {
        path
    };
    match list_remote(conn, path).await {
        Ok((cwd, entries)) => emit(
            events,
            wake,
            SftpEvent::Listed {
                side,
                path: cwd,
                entries,
            },
        ),
        Err(err) => emit(events, wake, SftpEvent::Failed(err)),
    }
}

pub(super) fn worker(
    commands: Receiver<SftpCommand>,
    events: Sender<SftpEvent>,
    conflicts: Receiver<ConflictReply>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
) {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .thread_name("terminus-sftp-rt")
        .build()
    {
        Ok(rt) => rt,
        Err(err) => {
            let _ = events.send(SftpEvent::Failed(format!(
                "cannot start SFTP runtime: {err}"
            )));
            return;
        }
    };

    runtime.block_on(async move {
        let (transfers, jobs) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(run_transfers(
            jobs,
            events.clone(),
            wake.clone(),
            Arc::new(ConflictInbox(Mutex::new(conflicts))),
        ));
        let mut left_conn: Option<Arc<SftpConnection>> = None;
        let mut right_conn: Option<Arc<SftpConnection>> = None;
        let mut edit_sessions: Vec<EditSession> = Vec::new();
        let mut next_edit_id: u64 = 1;

        loop {
            match commands.recv_timeout(Duration::from_millis(500)) {
                Ok(cmd) => {
                    let should_break = handle_command(
                        cmd,
                        &mut left_conn,
                        &mut right_conn,
                        &mut edit_sessions,
                        &mut next_edit_id,
                        &events,
                        &wake,
                        &transfers,
                    )
                    .await;
                    if should_break {
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }

            poll_edit_sessions(
                &mut edit_sessions,
                &left_conn,
                &right_conn,
                &events,
                &wake,
            )
            .await;
        }
    });
}

/// Best-effort directory check: local via metadata; remote via `list` succeeding.
pub(super) async fn is_directory(
    left: &Option<Arc<SftpConnection>>,
    right: &Option<Arc<SftpConnection>>,
    side: SftpSide,
    path: &str,
    is_remote: bool,
) -> Result<bool, String> {
    if is_remote {
        let conn = conn_ref(left, right, side).ok_or_else(|| not_connected(side))?;
        Ok(conn.list(path).await.is_ok())
    } else {
        match tokio::fs::metadata(path).await {
            Ok(meta) => Ok(meta.is_dir()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                Err(format!("not found: {path}"))
            }
            Err(err) => Err(err.to_string()),
        }
    }
}

pub(super) async fn list_local(
    path: &Path,
) -> Result<(String, Vec<SftpListEntry>), String> {
    let entries = local_fs::list_local_dir(path)
        .await
        .map_err(|e| e.to_string())?;
    Ok((
        path.to_string_lossy().into_owned(),
        entries.into_iter().map(SftpListEntry::from).collect(),
    ))
}

pub(super) async fn list_remote(
    conn: &SftpConnection,
    path: &str,
) -> Result<(String, Vec<SftpListEntry>), String> {
    let entries = run_with_retry(Operation::ListRemote, RetryPolicy::DEFAULT, || async {
        conn.list(path).await.map_err(|e| e.to_string())
    })
    .await?;
    Ok((
        path.to_string(),
        entries.into_iter().map(SftpListEntry::from).collect(),
    ))
}

pub(super) fn parent_remote(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rsplit_once('/') {
        Some(("", _)) => "/".into(),
        Some((parent, _)) if !parent.is_empty() => parent.to_string(),
        _ => "/".into(),
    }
}
