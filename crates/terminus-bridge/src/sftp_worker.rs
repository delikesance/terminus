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
    /// Copy a directory tree between panes (roadmap 2.3 — stub until implemented).
    TransferFolder {
        from_side: SftpSide,
        from_path: String,
        to_side: SftpSide,
        to_cwd: String,
        name: String,
    },
    /// Recursively delete a remote path (roadmap gap — stub until implemented).
    RemoveRemoteRecursive {
        side: SftpSide,
        path: String,
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
        SftpCommand::TransferFolder {
            from_side,
            from_path,
            to_side,
            to_cwd,
            name,
        } => {
            match transfer_folder(
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
        SftpCommand::RemoveRemoteRecursive { side, path } => {
            let Some(conn) = conn_ref(left_conn, right_conn, side) else {
                emit(events, wake, SftpEvent::Failed(not_connected(side)));
                return false;
            };
            let parent = parent_remote(&path);
            match conn.remove_recursive(&path).await {
                Ok(()) => emit_listed_remote(events, wake, side, conn, &parent).await,
                Err(err) => emit(events, wake, SftpEvent::Failed(err.to_string())),
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
        return Box::pin(transfer_folder(
            left, right, from_side, from_path, to_side, to_cwd, name, events, wake,
        ))
        .await;
    }

    transfer_file(
        left, right, from_side, from_path, to_side, to_cwd, name, from_remote, to_remote, events, wake,
    )
    .await
}

async fn transfer_file(
    left: &Option<SftpConnection>,
    right: &Option<SftpConnection>,
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
        // Remote → Remote: chunked SFTP relay (no temp file).
        (true, true) => {
            let from_conn = conn_ref(left, right, from_side)
                .ok_or_else(|| not_connected(from_side))?;
            let to_conn = conn_ref(left, right, to_side)
                .ok_or_else(|| not_connected(to_side))?;
            emit(
                events,
                wake,
                SftpEvent::TransferProgress {
                    label: format!("Copy {name}"),
                    done: 0,
                    total: 0,
                },
            );
            let total = from_conn
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
                .map_err(|e| e.to_string())?;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArchiveKind {
    TarGz,
    Zip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoteFamily {
    Unix,
    Windows,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackEngine {
    NativeTar,
    NativeZip,
    /// Windows built-in `Compress-Archive` / `Expand-Archive`.
    PowerShell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RemoteArchiveTools {
    tar: bool,
    zip: bool,
    unzip: bool,
    powershell: bool,
}

impl RemoteArchiveTools {
    fn none() -> Self {
        Self {
            tar: false,
            zip: false,
            unzip: false,
            powershell: false,
        }
    }
}

#[derive(Debug, Clone)]
struct RemoteEnv {
    tools: RemoteArchiveTools,
    family: RemoteFamily,
    #[allow(dead_code)]
    arch: String,
    tmp: String,
}

const NO_REMOTE_ARCHIVE_TOOLS: &str =
    "Remote has no tar, zip/unzip, or PowerShell Compress-Archive; cannot transfer folders.";

/// Active pack/extract capability for one remote (native tools only).
struct RemotePackSession {
    env: RemoteEnv,
    engine: PackEngine,
    kind: ArchiveKind,
}

fn parse_probe_stdout(stdout: &str) -> RemoteEnv {
    let mut tools = RemoteArchiveTools::none();
    let mut family = RemoteFamily::Unix;
    let mut arch = "x86_64".to_string();
    let mut tmp = "/tmp".to_string();
    for part in stdout.split_whitespace() {
        if let Some(v) = part.strip_prefix("tar=") {
            tools.tar = v == "1" || v.eq_ignore_ascii_case("true");
        } else if let Some(v) = part.strip_prefix("zip=") {
            tools.zip = v == "1" || v.eq_ignore_ascii_case("true");
        } else if let Some(v) = part.strip_prefix("unzip=") {
            tools.unzip = v == "1" || v.eq_ignore_ascii_case("true");
        } else if let Some(v) = part.strip_prefix("ps=") {
            tools.powershell = v == "1" || v.eq_ignore_ascii_case("true");
        } else if let Some(v) = part.strip_prefix("family=") {
            family = if v.eq_ignore_ascii_case("windows") {
                RemoteFamily::Windows
            } else {
                RemoteFamily::Unix
            };
        } else if let Some(v) = part.strip_prefix("arch=") {
            arch = normalize_arch(v);
        } else if let Some(v) = part.strip_prefix("tmp=") {
            if !v.is_empty() {
                tmp = v.to_string();
            }
        }
    }
    RemoteEnv {
        tools,
        family,
        arch,
        tmp,
    }
}

fn normalize_arch(raw: &str) -> String {
    match raw.to_ascii_lowercase().as_str() {
        "x86_64" | "amd64" | "x64" => "x86_64".into(),
        "aarch64" | "arm64" => "aarch64".into(),
        "i386" | "i686" | "x86" => "x86".into(),
        other => other.to_string(),
    }
}

fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

fn remote_parent_base(path: &str) -> (String, String) {
    let path = path.trim_end_matches('/');
    match path.rsplit_once('/') {
        Some(("", base)) => ("/".into(), base.to_string()),
        Some((parent, base)) => (parent.to_string(), base.to_string()),
        None => (".".into(), path.to_string()),
    }
}

fn archive_ext(kind: ArchiveKind) -> &'static str {
    match kind {
        ArchiveKind::TarGz => "tar.gz",
        ArchiveKind::Zip => "zip",
    }
}

fn join_tmp(tmp: &str, name: &str) -> String {
    let tmp = tmp.trim_end_matches(['/', '\\']);
    if tmp.contains('\\') || tmp.chars().nth(1) == Some(':') {
        format!("{tmp}\\{name}")
    } else if tmp == "/" {
        format!("/{name}")
    } else {
        format!("{tmp}/{name}")
    }
}

/// Probe remote OS family, arch, temp dir, and archive tools.
async fn probe_remote_env(conn: &SftpConnection) -> RemoteEnv {
    let script = concat!(
        "# terminus-sftp-probe-v2\n",
        "TAR=0; ZIP=0; UNZIP=0; PS=0\n",
        "command -v tar >/dev/null 2>&1 && TAR=1\n",
        "command -v zip >/dev/null 2>&1 && ZIP=1\n",
        "command -v unzip >/dev/null 2>&1 && UNZIP=1\n",
        "(command -v powershell.exe >/dev/null 2>&1 || command -v pwsh >/dev/null 2>&1 || command -v powershell >/dev/null 2>&1) && PS=1\n",
        "UNAME=$(uname -s 2>/dev/null || printf unknown)\n",
        "ARCH=$(uname -m 2>/dev/null || printf x86_64)\n",
        "FAMILY=unix\n",
        "case \"$UNAME\" in\n",
        "  MINGW*|MSYS*|CYGWIN*|Windows_NT|windows*) FAMILY=windows ;;\n",
        "esac\n",
        "if [ -n \"${WINDIR:-}${SYSTEMROOT:-}\" ]; then FAMILY=windows; fi\n",
        "TMP=${TMPDIR:-/tmp}\n",
        "if [ \"$FAMILY\" = windows ]; then\n",
        "  TMP=${TEMP:-${TMPDIR:-/tmp}}\n",
        "elif [ -d /tmp ] && [ -w /tmp ]; then\n",
        "  TMP=/tmp\n",
        "fi\n",
        "printf 'tar=%s zip=%s unzip=%s ps=%s family=%s arch=%s tmp=%s\\n' \\\n",
        "  \"$TAR\" \"$ZIP\" \"$UNZIP\" \"$PS\" \"$FAMILY\" \"$ARCH\" \"$TMP\"\n",
    );
    match conn.exec(script).await {
        Ok((0, stdout, _)) => parse_probe_stdout(&String::from_utf8_lossy(&stdout)),
        Ok((code, _, err)) => {
            debug!(
                code,
                stderr = %String::from_utf8_lossy(&err),
                "archive env probe non-zero"
            );
            RemoteEnv {
                tools: RemoteArchiveTools::none(),
                family: RemoteFamily::Unix,
                arch: "x86_64".into(),
                tmp: "/tmp".into(),
            }
        }
        Err(err) => {
            debug!(error = %err, "archive env probe failed");
            RemoteEnv {
                tools: RemoteArchiveTools::none(),
                family: RemoteFamily::Unix,
                arch: "x86_64".into(),
                tmp: "/tmp".into(),
            }
        }
    }
}

/// Pick native tar/zip/PowerShell; error if the remote has none.
async fn acquire_pack_session(conn: &SftpConnection) -> Result<RemotePackSession, String> {
    let env = probe_remote_env(conn).await;

    if env.tools.tar {
        return Ok(RemotePackSession {
            env,
            engine: PackEngine::NativeTar,
            kind: ArchiveKind::TarGz,
        });
    }
    if env.tools.zip {
        return Ok(RemotePackSession {
            env,
            engine: PackEngine::NativeZip,
            kind: ArchiveKind::Zip,
        });
    }
    if env.family == RemoteFamily::Windows && env.tools.powershell {
        return Ok(RemotePackSession {
            env,
            engine: PackEngine::PowerShell,
            kind: ArchiveKind::Zip,
        });
    }

    Err(NO_REMOTE_ARCHIVE_TOOLS.into())
}

/// Acquire a session that can **extract** `kind` with native tools only.
async fn acquire_extract_session(
    conn: &SftpConnection,
    kind: ArchiveKind,
) -> Result<RemotePackSession, String> {
    let env = probe_remote_env(conn).await;
    match kind {
        ArchiveKind::TarGz => {
            if env.tools.tar {
                Ok(RemotePackSession {
                    env,
                    engine: PackEngine::NativeTar,
                    kind,
                })
            } else {
                Err(NO_REMOTE_ARCHIVE_TOOLS.into())
            }
        }
        ArchiveKind::Zip => {
            if env.tools.unzip {
                return Ok(RemotePackSession {
                    env,
                    engine: PackEngine::NativeZip,
                    kind,
                });
            }
            if env.family == RemoteFamily::Windows && env.tools.powershell {
                return Ok(RemotePackSession {
                    env,
                    engine: PackEngine::PowerShell,
                    kind,
                });
            }
            Err(NO_REMOTE_ARCHIVE_TOOLS.into())
        }
    }
}

fn archive_create_script(
    parent: &str,
    base: &str,
    name: &str,
    out: &str,
    session: &RemotePackSession,
) -> String {
    match session.engine {
        PackEngine::NativeTar => format!(
            concat!(
                "# terminus-sftp-archive-v1\n",
                "TERMINUS_ARCHIVE_MODE=create\n",
                "TERMINUS_ARCHIVE_PARENT={parent}\n",
                "TERMINUS_ARCHIVE_BASE={base}\n",
                "TERMINUS_ARCHIVE_NAME={name}\n",
                "TERMINUS_ARCHIVE_OUT={out}\n",
                "set -e\n",
                "cd {parent}\n",
                "ROOT={base}\n",
                "CLEANUP=\n",
                "if [ {base} != {name} ]; then\n",
                "  ln -snf {base} {name}\n",
                "  ROOT={name}\n",
                "  CLEANUP=1\n",
                "fi\n",
                "tar -czhf {out} -h \"$ROOT\"\n",
                "if [ -n \"$CLEANUP\" ]; then rm -f {name}; fi\n",
                "printf '%s\\n' {out}\n",
            ),
            parent = sh_quote(parent),
            base = sh_quote(base),
            name = sh_quote(name),
            out = sh_quote(out),
        ),
        PackEngine::NativeZip => format!(
            concat!(
                "# terminus-sftp-archive-v1\n",
                "TERMINUS_ARCHIVE_MODE=create\n",
                "TERMINUS_ARCHIVE_PARENT={parent}\n",
                "TERMINUS_ARCHIVE_BASE={base}\n",
                "TERMINUS_ARCHIVE_NAME={name}\n",
                "TERMINUS_ARCHIVE_OUT={out}\n",
                "set -e\n",
                "cd {parent}\n",
                "ROOT={base}\n",
                "CLEANUP=\n",
                "if [ {base} != {name} ]; then\n",
                "  ln -snf {base} {name}\n",
                "  ROOT={name}\n",
                "  CLEANUP=1\n",
                "fi\n",
                "zip -rq {out} \"$ROOT\"\n",
                "if [ -n \"$CLEANUP\" ]; then rm -f {name}; fi\n",
                "printf '%s\\n' {out}\n",
            ),
            parent = sh_quote(parent),
            base = sh_quote(base),
            name = sh_quote(name),
            out = sh_quote(out),
        ),
        PackEngine::PowerShell => {
            let src = if parent == "/" {
                format!("/{base}")
            } else if parent.contains('\\') {
                format!("{parent}\\{base}")
            } else {
                format!("{parent}/{base}")
            };
            // Compress-Archive uses the leaf folder name as zip root.
            format!(
                "powershell -NoProfile -Command \"Compress-Archive -Path {} -DestinationPath {} -Force\"",
                ps_quote(&src),
                ps_quote(out),
            )
        }
    }
}

fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn archive_extract_script(dest: &str, archive: &str, session: &RemotePackSession) -> String {
    match session.engine {
        PackEngine::NativeTar => format!(
            concat!(
                "# terminus-sftp-archive-v1\n",
                "TERMINUS_ARCHIVE_MODE=extract\n",
                "TERMINUS_ARCHIVE_PARENT={dest}\n",
                "TERMINUS_ARCHIVE_BASE=\n",
                "TERMINUS_ARCHIVE_OUT={archive}\n",
                "set -e\n",
                "mkdir -p {dest}\n",
                "tar -C {dest} -xzf {archive}\n",
            ),
            dest = sh_quote(dest),
            archive = sh_quote(archive),
        ),
        PackEngine::NativeZip => format!(
            concat!(
                "# terminus-sftp-archive-v1\n",
                "TERMINUS_ARCHIVE_MODE=extract\n",
                "TERMINUS_ARCHIVE_PARENT={dest}\n",
                "TERMINUS_ARCHIVE_BASE=\n",
                "TERMINUS_ARCHIVE_OUT={archive}\n",
                "set -e\n",
                "mkdir -p {dest}\n",
                "unzip -qo {archive} -d {dest}\n",
            ),
            dest = sh_quote(dest),
            archive = sh_quote(archive),
        ),
        PackEngine::PowerShell => format!(
            "powershell -NoProfile -Command \"Expand-Archive -Path {} -DestinationPath {} -Force\"",
            ps_quote(archive),
            ps_quote(dest),
        ),
    }
}

async fn remote_create_archive(
    conn: &SftpConnection,
    from_path: &str,
    name: &str,
    session: &RemotePackSession,
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
) -> Result<String, String> {
    let (parent, base) = remote_parent_base(from_path);
    let out = join_tmp(
        &session.env.tmp,
        &format!(
            "terminus-sftp-{}-{}.{}",
            std::process::id(),
            uuid::Uuid::new_v4(),
            archive_ext(session.kind)
        ),
    );

    emit(
        events,
        wake,
        SftpEvent::TransferProgress {
            label: format!("Creating remote {}…", archive_ext(session.kind)),
            done: 0,
            total: 0,
        },
    );

    let script = archive_create_script(&parent, &base, name, &out, session);
    let (code, stdout, stderr) = conn.exec(&script).await.map_err(|e| e.to_string())?;
    if code != 0 {
        return Err(format!(
            "remote archive create failed (exit {code}): {}",
            String::from_utf8_lossy(&stderr)
        ));
    }
    let reported = String::from_utf8_lossy(&stdout);
    let remote = reported
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.contains('='))
        .unwrap_or(&out)
        .to_string();
    Ok(remote)
}

async fn try_remote_pack_to_local(
    conn: &SftpConnection,
    from_path: &str,
    name: &str,
    staging: &Path,
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
) -> Result<ArchiveKind, String> {
    let session = acquire_pack_session(conn).await?;
    let local = staging_with_ext(staging, session.kind);
    let remote = remote_create_archive(conn, from_path, name, &session, events, wake).await?;
    let download = transfer_download(conn, &remote, &local, events, wake).await;
    let _ = conn.remove(&remote).await;
    download?;
    Ok(session.kind)
}

async fn remote_extract_uploaded(
    conn: &SftpConnection,
    local_archive: &Path,
    to_cwd: &str,
    _name: &str,
    kind: ArchiveKind,
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
) -> Result<(), String> {
    let session = acquire_extract_session(conn, kind).await?;
    let remote_name = format!(
        "terminus-sftp-in-{}-{}.{}",
        std::process::id(),
        uuid::Uuid::new_v4(),
        archive_ext(kind)
    );
    let remote = join_tmp(&session.env.tmp, &remote_name);
    transfer_upload(conn, local_archive, &remote, events, wake).await?;
    let script = archive_extract_script(to_cwd, &remote, &session);
    let (code, _stdout, stderr) = conn.exec(&script).await.map_err(|e| e.to_string())?;
    let _ = conn.remove(&remote).await;
    if code != 0 {
        return Err(format!(
            "remote archive extract failed (exit {code}): {}",
            String::from_utf8_lossy(&stderr)
        ));
    }
    Ok(())
}

fn extract_local_archive(
    archive: &Path,
    dest_cwd: &Path,
    kind: ArchiveKind,
) -> Result<(), String> {
    match kind {
        ArchiveKind::Zip => unzip_local(archive, dest_cwd),
        ArchiveKind::TarGz => {
            std::fs::create_dir_all(dest_cwd).map_err(|e| e.to_string())?;
            let status = std::process::Command::new("tar")
                .arg("-C")
                .arg(dest_cwd)
                .arg("-xzf")
                .arg(archive)
                .status()
                .map_err(|e| format!("local tar extract: {e}"))?;
            if status.success() {
                Ok(())
            } else {
                Err(format!("local tar extract failed ({status})"))
            }
        }
    }
}

/// Copy a directory tree with as few SFTP round-trips as possible.
///
/// Remote side archives in one shot via native `tar`/`zip` or PowerShell
/// `Compress-Archive`. If the remote has none of these tools, the transfer
/// fails with a clear error (no per-file SFTP fallback).
async fn transfer_folder(
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

    if from_remote && to_remote {
        let from_conn = conn_ref(left, right, from_side).ok_or_else(|| not_connected(from_side))?;
        let to_conn = conn_ref(left, right, to_side).ok_or_else(|| not_connected(to_side))?;
        return transfer_folder_remote_to_remote(
            from_conn, to_conn, from_path, to_cwd, name, events, wake,
        )
        .await;
    }

    // --- paths with a local side ---
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
        let conn = conn_ref(left, right, from_side).ok_or_else(|| not_connected(from_side))?;
        let kind =
            try_remote_pack_to_local(conn, from_path, name, &staging, events, wake).await?;
        (staging_with_ext(&staging, kind), kind)
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
        let conn = conn_ref(left, right, to_side).ok_or_else(|| not_connected(to_side))?;
        emit(
            events,
            wake,
            SftpEvent::TransferProgress {
                label: format!("Unpacking {name} on server…"),
                done: 0,
                total: zip_len,
            },
        );
        remote_extract_uploaded(conn, &archive_path, to_cwd, name, kind, events, wake).await
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

fn staging_with_ext(base: &Path, kind: ArchiveKind) -> PathBuf {
    match kind {
        ArchiveKind::TarGz => base.with_extension("tar.gz"),
        ArchiveKind::Zip => base.with_extension("zip"),
    }
}

/// Host A → Host B: archive via native tools on both sides (error if missing).
async fn transfer_folder_remote_to_remote(
    from: &SftpConnection,
    to: &SftpConnection,
    from_path: &str,
    to_cwd: &str,
    name: &str,
    events: &Sender<SftpEvent>,
    wake: &Option<Arc<dyn Fn() + Send + Sync>>,
) -> Result<(), String> {
    transfer_folder_remote_to_remote_via_archive(
        from, to, from_path, to_cwd, name, events, wake,
    )
    .await
}

async fn transfer_folder_remote_to_remote_via_archive(
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

fn sanitize_temp_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn zip_options() -> zip::write::SimpleFileOptions {
    zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .unix_permissions(0o644)
}

fn zip_local_tree(src: &Path, root_name: &str, zip_path: &Path) -> Result<(), String> {
    let file = std::fs::File::create(zip_path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip_options();
    zip.add_directory(format!("{root_name}/"), options)
        .map_err(|e| e.to_string())?;
    zip_local_dir_recursive(&mut zip, src, root_name, options)?;
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

fn zip_local_dir_recursive(
    zip: &mut zip::ZipWriter<std::fs::File>,
    dir: &Path,
    prefix: &str,
    options: zip::write::SimpleFileOptions,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let rel = format!("{prefix}/{name}");
        let path = entry.path();
        let ft = entry.file_type().map_err(|e| e.to_string())?;
        if ft.is_dir() {
            zip.add_directory(format!("{rel}/"), options)
                .map_err(|e| e.to_string())?;
            zip_local_dir_recursive(zip, &path, &rel, options)?;
        } else if ft.is_file() || ft.is_symlink() {
            // Follow symlinks (npm .bin) so the zip stores real file bytes —
            // Windows extract cannot recreate Unix symlinks.
            let mut input = std::fs::File::open(&path).map_err(|e| e.to_string())?;
            zip.start_file(&rel, options).map_err(|e| e.to_string())?;
            std::io::copy(&mut input, zip).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Local zip extract (client-side dest).
fn unzip_local(zip_path: &Path, dest_cwd: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
            continue;
        };
        let out = dest_cwd.join(&rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut outfile = std::fs::File::create(&out).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut outfile).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
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
    fn parse_probe_stdout_reads_flags() {
        let env = parse_probe_stdout(
            "tar=1 zip=0 unzip=1 ps=0 family=unix arch=x86_64 tmp=/tmp\n",
        );
        assert!(env.tools.tar && !env.tools.zip && env.tools.unzip);
        assert_eq!(env.family, RemoteFamily::Unix);
        assert_eq!(env.tmp, "/tmp");
        assert_eq!(env.arch, "x86_64");

        let none = parse_probe_stdout(
            "tar=0 zip=0 unzip=0 ps=0 family=unix arch=aarch64 tmp=/var/tmp",
        );
        assert!(!none.tools.tar && !none.tools.zip);
        assert_eq!(none.arch, "aarch64");

        let win = parse_probe_stdout(
            "tar=0 zip=0 unzip=0 ps=1 family=windows arch=AMD64 tmp=C:\\Users\\a\\AppData\\Local\\Temp",
        );
        assert!(win.tools.powershell);
        assert_eq!(win.family, RemoteFamily::Windows);
        assert_eq!(win.arch, "x86_64");
    }

    #[test]
    fn remote_parent_base_splits_path() {
        assert_eq!(remote_parent_base("/src"), ("/".into(), "src".into()));
        assert_eq!(
            remote_parent_base("/home/alice/proj"),
            ("/home/alice".into(), "proj".into())
        );
        assert_eq!(remote_parent_base("relative"), (".".into(), "relative".into()));
    }

    #[test]
    fn join_tmp_respects_windows_style() {
        assert_eq!(join_tmp("/tmp", "a.bin"), "/tmp/a.bin");
        assert_eq!(
            join_tmp(r"C:\Users\x\AppData\Local\Temp", "a.exe"),
            r"C:\Users\x\AppData\Local\Temp\a.exe"
        );
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

    #[test]
    fn transfer_folder_command_exists() {
        let worker = SftpWorker::spawn(None);
        let dir = std::env::temp_dir().join(format!(
            "terminus-sftp-xfer-folder-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let left = dir.join("L");
        let right = dir.join("R");
        std::fs::create_dir_all(left.join("tree")).unwrap();
        std::fs::write(left.join("tree").join("a.txt"), b"a").unwrap();
        std::fs::create_dir_all(&right).unwrap();
        worker.send(SftpCommand::ListLocal {
            side: SftpSide::Left,
            path: left.clone(),
        });
        worker.send(SftpCommand::ListLocal {
            side: SftpSide::Right,
            path: right.clone(),
        });
        for _ in 0..50 {
            let _ = worker.drain();
            std::thread::sleep(Duration::from_millis(10));
        }
        worker.send(SftpCommand::TransferFolder {
            from_side: SftpSide::Left,
            from_path: left.join("tree").to_string_lossy().into_owned(),
            to_side: SftpSide::Right,
            to_cwd: right.to_string_lossy().into_owned(),
            name: "tree".into(),
        });
        let mut listed = false;
        for _ in 0..80 {
            for event in worker.drain() {
                if let SftpEvent::Listed {
                    side: SftpSide::Right,
                    entries,
                    ..
                } = event
                {
                    if entries.iter().any(|e| e.name == "tree") {
                        listed = true;
                    }
                }
            }
            if right.join("tree").join("a.txt").is_file() {
                listed = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            listed && right.join("tree").join("a.txt").is_file(),
            "TransferFolder should copy the tree"
        );
        worker.send(SftpCommand::Close);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn transfer_queue_events_desired() {
        let worker = SftpWorker::spawn(None);
        let dir = std::env::temp_dir().join(format!(
            "terminus-sftp-queue-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let left = dir.join("L");
        let right = dir.join("R");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::write(left.join("payload.bin"), b"xyz").unwrap();
        std::fs::create_dir_all(&right).unwrap();
        worker.send(SftpCommand::Transfer {
            from_side: SftpSide::Left,
            from_path: left.join("payload.bin").to_string_lossy().into_owned(),
            to_side: SftpSide::Right,
            to_cwd: right.to_string_lossy().into_owned(),
            name: "payload.bin".into(),
        });
        let mut saw_progress = false;
        for _ in 0..80 {
            for event in worker.drain() {
                if let SftpEvent::TransferProgress { .. } = event {
                    saw_progress = true;
                }
            }
            if saw_progress && right.join("payload.bin").is_file() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            saw_progress,
            "transfer should emit TransferProgress queue events"
        );
        assert_eq!(std::fs::read(right.join("payload.bin")).unwrap(), b"xyz");
        worker.send(SftpCommand::Close);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn remove_local_file() {
        let worker = SftpWorker::spawn(None);
        let dir = std::env::temp_dir().join(format!(
            "terminus-sftp-rm-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("x.txt");
        std::fs::write(&file, b"x").unwrap();
        worker.send(SftpCommand::RemoveLocal {
            side: SftpSide::Left,
            path: file.clone(),
            recursive: false,
        });
        let mut listed = false;
        for _ in 0..50 {
            for event in worker.drain() {
                if let SftpEvent::Listed { entries, .. } = event {
                    assert!(!entries.iter().any(|e| e.name == "x.txt"));
                    listed = true;
                }
            }
            if listed {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(listed);
        assert!(!file.exists());
        worker.send(SftpCommand::Close);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn transfer_local_to_local() {
        let worker = SftpWorker::spawn(None);
        let root = std::env::temp_dir().join(format!(
            "terminus-sftp-xfer-ll-{}",
            std::process::id()
        ));
        let left = root.join("L");
        let right = root.join("R");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&left).unwrap();
        std::fs::create_dir_all(&right).unwrap();
        std::fs::write(left.join("p.txt"), b"payload").unwrap();
        worker.send(SftpCommand::Transfer {
            from_side: SftpSide::Left,
            from_path: left.join("p.txt").to_string_lossy().into_owned(),
            to_side: SftpSide::Right,
            to_cwd: right.to_string_lossy().into_owned(),
            name: "p.txt".into(),
        });
        let mut ok = false;
        for _ in 0..80 {
            for event in worker.drain() {
                if let SftpEvent::Listed {
                    side: SftpSide::Right,
                    entries,
                    ..
                } = event
                {
                    if entries.iter().any(|e| e.name == "p.txt") {
                        ok = true;
                    }
                }
            }
            if ok {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(ok);
        assert_eq!(std::fs::read(right.join("p.txt")).unwrap(), b"payload");
        worker.send(SftpCommand::Close);
        let _ = std::fs::remove_dir_all(&root);
    }
}
