//! Async SFTP worker: UI thread never awaits the network.
//!
//! Mirrors [`crate::ssh_transport`] / the host repository pattern: a dedicated
//! tokio runtime on a background thread, commands in / events out via
//! `std::sync::mpsc`, optional `wake` so an idle corcovado loop repaints.
//!
//! Dual-pane model: each [`SftpSide`] (`Left` / `Right`) is either local FS
//! (no connection) or a remote SFTP session ([`SftpConnection`]).

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use terminus_core::local_fs::{self, LocalEntry};
use terminus_core::sftp::SftpEntry;
use terminus_core::ssh::{connect_sftp_for_host, SftpConnection, SshConnectOptions};
use tracing::{debug, warn};

/// Which dual-pane side a command / event refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SftpSide {
    Left,
    Right,
}

/// Commands the UI pushes to the worker.
#[derive(Debug)]
pub enum SftpCommand {
    /// Open a dedicated russh SFTP connection on `side`.
    Connect {
        side: SftpSide,
        opts: SshConnectOptions,
    },
    /// Drop the remote session on `side` (local FS ops still work).
    Disconnect {
        side: SftpSide,
    },
    ListLocal {
        side: SftpSide,
        path: PathBuf,
    },
    ListRemote {
        side: SftpSide,
        path: String,
    },
    MkdirLocal {
        side: SftpSide,
        path: PathBuf,
    },
    MkdirRemote {
        side: SftpSide,
        path: String,
    },
    RemoveLocal {
        side: SftpSide,
        path: PathBuf,
        recursive: bool,
    },
    RemoveRemote {
        side: SftpSide,
        path: String,
    },
    RenameLocal {
        side: SftpSide,
        from: PathBuf,
        to: PathBuf,
    },
    RenameRemote {
        side: SftpSide,
        from: String,
        to: String,
    },
    /// Copy a single file between panes (local↔remote, remote↔remote, local↔local).
    Transfer {
        from_side: SftpSide,
        from_path: String,
        to_side: SftpSide,
        to_cwd: String,
        name: String,
    },
    /// Download a remote file to OS temp, then watch for local edits and reupload.
    EditRemote {
        side: SftpSide,
        remote_path: String,
        name: String,
    },
    /// Tear down both sessions and stop the worker.
    Close,
}

/// Normalized directory entry for either pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SftpListEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
}

impl From<LocalEntry> for SftpListEntry {
    fn from(entry: LocalEntry) -> Self {
        Self {
            name: entry.name,
            path: entry.path.to_string_lossy().into_owned(),
            is_dir: entry.is_dir,
            size: entry.size,
        }
    }
}

impl From<SftpEntry> for SftpListEntry {
    fn from(entry: SftpEntry) -> Self {
        Self {
            name: entry.name,
            path: entry.path,
            is_dir: entry.is_dir,
            size: entry.size,
        }
    }
}

/// Events the worker pushes back to the UI.
#[derive(Debug, Clone)]
pub enum SftpEvent {
    /// Remote SFTP channel on `side` is ready.
    Ready {
        side: SftpSide,
    },
    Listed {
        side: SftpSide,
        path: String,
        entries: Vec<SftpListEntry>,
    },
    TransferProgress {
        label: String,
        done: u64,
        total: u64,
    },
    /// Remote file downloaded to `local_path`; UI should open it with the default app.
    EditReady {
        id: u64,
        side: SftpSide,
        remote_path: String,
        local_path: PathBuf,
    },
    /// Local edit was reuploaded to the remote path.
    EditSaved {
        id: u64,
        remote_path: String,
    },
    Failed(String),
    Closed,
}

/// UI-side handle to the SFTP worker thread.
pub struct SftpWorker {
    commands: Sender<SftpCommand>,
    events: Receiver<SftpEvent>,
}

impl SftpWorker {
    /// Start the worker. `wake` is invoked after each event (repaint hook).
    pub fn spawn(wake: Option<Arc<dyn Fn() + Send + Sync>>) -> Self {
        let (command_tx, command_rx) = channel::<SftpCommand>();
        let (event_tx, event_rx) = channel::<SftpEvent>();

        let _ = std::thread::Builder::new()
            .name("terminus-sftp".to_string())
            .spawn(move || worker(command_rx, event_tx, wake));

        Self {
            commands: command_tx,
            events: event_rx,
        }
    }

    /// Queue a command (non-blocking for the UI).
    pub fn send(&self, cmd: SftpCommand) {
        if let Err(err) = self.commands.send(cmd) {
            warn!(error = %err, "sftp worker command channel closed");
        }
    }

    /// Drain pending events without blocking.
    pub fn drain(&self) -> Vec<SftpEvent> {
        let mut out = Vec::new();
        loop {
            match self.events.try_recv() {
                Ok(event) => out.push(event),
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        }
        out
    }
}

fn emit(
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

fn conn_mut<'a>(
    left: &'a mut Option<SftpConnection>,
    right: &'a mut Option<SftpConnection>,
    side: SftpSide,
) -> &'a mut Option<SftpConnection> {
    match side {
        SftpSide::Left => left,
        SftpSide::Right => right,
    }
}

fn conn_ref<'a>(
    left: &'a Option<SftpConnection>,
    right: &'a Option<SftpConnection>,
    side: SftpSide,
) -> Option<&'a SftpConnection> {
    match side {
        SftpSide::Left => left.as_ref(),
        SftpSide::Right => right.as_ref(),
    }
}

fn side_is_remote(
    left: &Option<SftpConnection>,
    right: &Option<SftpConnection>,
    side: SftpSide,
) -> bool {
    conn_ref(left, right, side).is_some()
}

fn join_remote(cwd: &str, name: &str) -> String {
    let cwd = cwd.trim_end_matches('/');
    if cwd.is_empty() || cwd == "/" {
        format!("/{name}")
    } else {
        format!("{cwd}/{name}")
    }
}

fn not_connected(side: SftpSide) -> String {
    format!("SFTP is not connected on {side:?}")
}

async fn emit_listed_local(
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

async fn emit_listed_remote(
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
    side: SftpSide,
    conn: &SftpConnection,
    path: &str,
) {
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

fn worker(
    commands: Receiver<SftpCommand>,
    events: Sender<SftpEvent>,
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
        let mut left_conn: Option<SftpConnection> = None;
        let mut right_conn: Option<SftpConnection> = None;
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

/// Tracked remote-edit: local temp file watched for mtime/size changes.
struct EditSession {
    id: u64,
    side: SftpSide,
    remote_path: String,
    local_path: PathBuf,
    last_mtime: SystemTime,
    last_len: u64,
    /// True after a local change is detected; upload after one stable poll.
    dirty: bool,
    stable_polls: u8,
}

async fn handle_command(
    cmd: SftpCommand,
    left_conn: &mut Option<SftpConnection>,
    right_conn: &mut Option<SftpConnection>,
    edit_sessions: &mut Vec<EditSession>,
    next_edit_id: &mut u64,
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
) -> bool {
    match cmd {
        SftpCommand::Connect { side, opts } => {
            drop_edit_sessions_for_side(edit_sessions, side);
            *conn_mut(left_conn, right_conn, side) = None;
            match connect_sftp_for_host(&opts).await {
                Ok(conn) => {
                    *conn_mut(left_conn, right_conn, side) = Some(conn);
                    emit(events, wake, SftpEvent::Ready { side });
                }
                Err(err) => {
                    emit(events, wake, SftpEvent::Failed(err.to_string()));
                }
            }
            false
        }
        SftpCommand::Disconnect { side } => {
            drop_edit_sessions_for_side(edit_sessions, side);
            drop(conn_mut(left_conn, right_conn, side).take());
            debug!(?side, "sftp worker disconnected side");
            false
        }
        SftpCommand::ListLocal { side, path } => {
            emit_listed_local(events, wake, side, &path).await;
            false
        }
        SftpCommand::ListRemote { side, path } => {
            let Some(conn) = conn_ref(left_conn, right_conn, side) else {
                emit(events, wake, SftpEvent::Failed(not_connected(side)));
                return false;
            };
            emit_listed_remote(events, wake, side, conn, &path).await;
            false
        }
        SftpCommand::MkdirLocal { side, path } => {
            match local_fs::create_dir_all(&path).await {
                Ok(()) => {
                    if let Some(parent) = path.parent() {
                        emit_listed_local(events, wake, side, parent).await;
                    }
                }
                Err(err) => emit(events, wake, SftpEvent::Failed(err.to_string())),
            }
            false
        }
        SftpCommand::MkdirRemote { side, path } => {
            let Some(conn) = conn_ref(left_conn, right_conn, side) else {
                emit(events, wake, SftpEvent::Failed(not_connected(side)));
                return false;
            };
            match conn.mkdir(&path).await {
                Ok(()) => {
                    let parent = parent_remote(&path);
                    emit_listed_remote(events, wake, side, conn, &parent).await;
                }
                Err(err) => emit(events, wake, SftpEvent::Failed(err.to_string())),
            }
            false
        }
        SftpCommand::RemoveLocal {
            side,
            path,
            recursive,
        } => {
            let parent = path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("/"));
            match local_fs::remove_local_path(&path, recursive).await {
                Ok(()) => emit_listed_local(events, wake, side, &parent).await,
                Err(err) => emit(events, wake, SftpEvent::Failed(err.to_string())),
            }
            false
        }
        SftpCommand::RemoveRemote { side, path } => {
            let Some(conn) = conn_ref(left_conn, right_conn, side) else {
                emit(events, wake, SftpEvent::Failed(not_connected(side)));
                return false;
            };
            let parent = parent_remote(&path);
            match conn.remove(&path).await {
                Ok(()) => emit_listed_remote(events, wake, side, conn, &parent).await,
                Err(err) => emit(events, wake, SftpEvent::Failed(err.to_string())),
            }
            false
        }
        SftpCommand::RenameLocal { side, from, to } => {
            let parent = to
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("/"));
            match local_fs::rename_local(&from, &to).await {
                Ok(()) => emit_listed_local(events, wake, side, &parent).await,
                Err(err) => emit(events, wake, SftpEvent::Failed(err.to_string())),
            }
            false
        }
        SftpCommand::RenameRemote { side, from, to } => {
            let Some(conn) = conn_ref(left_conn, right_conn, side) else {
                emit(events, wake, SftpEvent::Failed(not_connected(side)));
                return false;
            };
            let parent = parent_remote(&to);
            match conn.rename(&from, &to).await {
                Ok(()) => emit_listed_remote(events, wake, side, conn, &parent).await,
                Err(err) => emit(events, wake, SftpEvent::Failed(err.to_string())),
            }
            false
        }
        SftpCommand::Transfer {
            from_side,
            from_path,
            to_side,
            to_cwd,
            name,
        } => {
            match transfer(
                left_conn,
                right_conn,
                from_side,
                &from_path,
                to_side,
                &to_cwd,
                &name,
                events,
                wake,
            )
            .await
            {
                Ok(()) => {
                    let to_remote = side_is_remote(left_conn, right_conn, to_side);
                    if to_remote {
                        if let Some(conn) = conn_ref(left_conn, right_conn, to_side) {
                            emit_listed_remote(events, wake, to_side, conn, &to_cwd).await;
                        }
                    } else {
                        emit_listed_local(events, wake, to_side, Path::new(&to_cwd)).await;
                    }
                }
                Err(err) => emit(events, wake, SftpEvent::Failed(err)),
            }
            false
        }
        SftpCommand::EditRemote {
            side,
            remote_path,
            name,
        } => {
            match start_edit_remote(
                left_conn,
                right_conn,
                side,
                &remote_path,
                &name,
                edit_sessions,
                next_edit_id,
                events,
                wake,
            )
            .await
            {
                Ok(()) => {}
                Err(err) => emit(events, wake, SftpEvent::Failed(err)),
            }
            false
        }
        SftpCommand::Close => {
            drop_all_edit_sessions(edit_sessions);
            drop(left_conn.take());
            drop(right_conn.take());
            emit(events, wake, SftpEvent::Closed);
            debug!("sftp worker closed");
            true
        }
    }
}

fn drop_edit_sessions_for_side(sessions: &mut Vec<EditSession>, side: SftpSide) {
    sessions.retain(|s| {
        if s.side == side {
            cleanup_edit_temp(&s.local_path);
            false
        } else {
            true
        }
    });
}

fn drop_all_edit_sessions(sessions: &mut Vec<EditSession>) {
    for session in sessions.drain(..) {
        cleanup_edit_temp(&session.local_path);
    }
}

fn cleanup_edit_temp(local_path: &Path) {
    let _ = std::fs::remove_file(local_path);
    if let Some(parent) = local_path.parent() {
        let _ = std::fs::remove_dir(parent);
    }
}

async fn start_edit_remote(
    left: &Option<SftpConnection>,
    right: &Option<SftpConnection>,
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
        let _ = std::fs::set_permissions(&local_path, std::fs::Permissions::from_mode(0o600));
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

async fn poll_edit_sessions(
    sessions: &mut Vec<EditSession>,
    left: &Option<SftpConnection>,
    right: &Option<SftpConnection>,
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

async fn transfer(
    left: &Option<SftpConnection>,
    right: &Option<SftpConnection>,
    from_side: SftpSide,
    from_path: &str,
    to_side: SftpSide,
    to_cwd: &str,
    name: &str,
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
) -> Result<(), String> {
    let from_remote = side_is_remote(left, right, from_side);
    let to_remote = side_is_remote(left, right, to_side);

    if is_directory(left, right, from_side, from_path, from_remote).await? {
        return Err("folders aren't supported yet".into());
    }

    let to_path_remote = join_remote(to_cwd, name);
    let to_path_local = PathBuf::from(to_cwd).join(name);
    let from_path_local = PathBuf::from(from_path);

    match (from_remote, to_remote) {
        // Local → Remote: upload
        (false, true) => {
            let conn = conn_ref(left, right, to_side)
                .ok_or_else(|| not_connected(to_side))?;
            transfer_upload(conn, &from_path_local, &to_path_remote, events, wake).await
        }
        // Remote → Local: download
        (true, false) => {
            let conn = conn_ref(left, right, from_side)
                .ok_or_else(|| not_connected(from_side))?;
            transfer_download(conn, from_path, &to_path_local, events, wake).await
        }
        // Remote → Remote: download to temp, then upload
        (true, true) => {
            let from_conn = conn_ref(left, right, from_side)
                .ok_or_else(|| not_connected(from_side))?;
            let to_conn = conn_ref(left, right, to_side)
                .ok_or_else(|| not_connected(to_side))?;

            let temp = std::env::temp_dir().join(format!(
                "terminus-xfer-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0),
                name
            ));

            let result = async {
                transfer_download(from_conn, from_path, &temp, events, wake).await?;
                transfer_upload(to_conn, &temp, &to_path_remote, events, wake).await
            }
            .await;

            let _ = tokio::fs::remove_file(&temp).await;
            result
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
            let copied = tokio::fs::copy(&from_path_local, &to_path_local)
                .await
                .map_err(|e| e.to_string())?;
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

/// Best-effort directory check: local via metadata; remote via `list` succeeding.
async fn is_directory(
    left: &Option<SftpConnection>,
    right: &Option<SftpConnection>,
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

async fn list_local(path: &Path) -> Result<(String, Vec<SftpListEntry>), String> {
    let entries = local_fs::list_local_dir(path)
        .await
        .map_err(|e| e.to_string())?;
    Ok((
        path.to_string_lossy().into_owned(),
        entries.into_iter().map(SftpListEntry::from).collect(),
    ))
}

async fn list_remote(
    conn: &SftpConnection,
    path: &str,
) -> Result<(String, Vec<SftpListEntry>), String> {
    let entries = conn.list(path).await.map_err(|e| e.to_string())?;
    Ok((
        path.to_string(),
        entries.into_iter().map(SftpListEntry::from).collect(),
    ))
}

fn parent_remote(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rsplit_once('/') {
        Some(("", _)) => "/".into(),
        Some((parent, _)) if !parent.is_empty() => parent.to_string(),
        _ => "/".into(),
    }
}

async fn transfer_upload(
    conn: &SftpConnection,
    local: &Path,
    remote: &str,
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
) -> Result<(), String> {
    let data = local_fs::read_local_file(local)
        .await
        .map_err(|e| e.to_string())?;
    let total = data.len() as u64;
    let label = format!(
        "Upload {}",
        local
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| local.display().to_string())
    );
    emit(
        events,
        wake,
        SftpEvent::TransferProgress {
            label: label.clone(),
            done: 0,
            total,
        },
    );
    conn.write(remote, &data)
        .await
        .map_err(|e| e.to_string())?;
    emit(
        events,
        wake,
        SftpEvent::TransferProgress {
            label,
            done: total,
            total,
        },
    );
    Ok(())
}

async fn transfer_download(
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
    emit(
        events,
        wake,
        SftpEvent::TransferProgress {
            label: label.clone(),
            done: 0,
            total: 0,
        },
    );
    let data = conn.read(remote).await.map_err(|e| e.to_string())?;
    let total = data.len() as u64;
    emit(
        events,
        wake,
        SftpEvent::TransferProgress {
            label: label.clone(),
            done: total / 2,
            total,
        },
    );
    local_fs::write_local_file(local, &data)
        .await
        .map_err(|e| e.to_string())?;
    emit(
        events,
        wake,
        SftpEvent::TransferProgress {
            label,
            done: total,
            total,
        },
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn local_list_command_routes_without_remote() {
        let worker = SftpWorker::spawn(None);
        let dir = std::env::temp_dir().join(format!(
            "terminus-sftp-worker-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), b"hi").unwrap();

        worker.send(SftpCommand::ListLocal {
            side: SftpSide::Left,
            path: dir.clone(),
        });

        let mut listed = None;
        for _ in 0..50 {
            for event in worker.drain() {
                if let SftpEvent::Listed {
                    side: SftpSide::Left,
                    path,
                    entries,
                } = event
                {
                    listed = Some((path, entries));
                }
            }
            if listed.is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }

        let (path, entries) = listed.expect("local list event");
        assert_eq!(Path::new(&path), dir.as_path());
        assert!(entries.iter().any(|e| e.name == "a.txt" && !e.is_dir));

        worker.send(SftpCommand::Close);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parent_remote_strips_last_segment() {
        assert_eq!(parent_remote("/home/alice/file"), "/home/alice");
        assert_eq!(parent_remote("/home"), "/");
        assert_eq!(parent_remote("/"), "/");
    }

    #[test]
    fn join_remote_appends_name() {
        assert_eq!(join_remote("/home/alice", "file.txt"), "/home/alice/file.txt");
        assert_eq!(join_remote("/", "file.txt"), "/file.txt");
        assert_eq!(join_remote("/home/", "file.txt"), "/home/file.txt");
    }

    #[test]
    fn edit_temp_path_keeps_basename_under_terminus_sftp_edit() {
        let name = "config.yaml";
        let dir = std::env::temp_dir()
            .join("terminus-sftp-edit")
            .join("00000000-0000-0000-0000-000000000001");
        let local = dir.join(name);
        assert!(local
            .to_string_lossy()
            .contains("terminus-sftp-edit"));
        assert_eq!(local.file_name().unwrap(), name);
    }
}
