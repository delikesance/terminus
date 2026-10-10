//! Async SFTP worker: UI thread never awaits the network.
//!
//! Mirrors [`crate::ssh_transport`] / the host repository pattern: a dedicated
//! tokio runtime on a background thread, commands in / events out via
//! `std::sync::mpsc`, optional `wake` so an idle corcovado loop repaints.
//!
//! Dual-pane model: each [`SftpSide`] (`Left` / `Right`) is either local FS
//! (no connection) or a remote SFTP session ([`SftpConnection`]).

mod archive;
mod commands;
mod conflict;
mod diff_plan;
mod differential_local;
mod differential_remote;
mod edit;
mod folder;
mod local_tree;
mod queue;
mod remote_archive;
mod retry;
mod transfer;
mod walk_remote;
mod worker;

#[cfg(test)]
mod retry_tests;
#[cfg(test)]
mod tests;

use self::archive::*;
use self::commands::*;
use self::conflict::*;
use self::diff_plan::*;
use self::differential_local::*;
use self::differential_remote::*;
use self::edit::*;
use self::folder::*;
use self::local_tree::*;
use self::queue::*;
use self::remote_archive::*;
use self::retry::*;
use self::transfer::*;
pub use self::worker::REMOTE_HOME;
use self::worker::*;

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

use terminus_core::local_fs::{self, LocalEntry};
use terminus_core::sftp::SftpEntry;
use terminus_core::ssh::{connect_sftp_for_host, SftpConnection, SshConnectOptions};
use tracing::{debug, warn};

use crate::folder_diff::{
    plan_differential, ConflictAction, ConflictPolicy, DiffAction, FileNode,
};
use crate::progress_throttle::ProgressThrottle;

pub use crate::folder_diff::ConflictAction as SftpConflictAction;

/// Which dual-pane side a command / event refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SftpSide {
    Left,
    Right,
}

/// File vs directory conflict prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictKind {
    File,
    Directory,
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
    /// Copy a directory tree between panes.
    TransferFolder {
        from_side: SftpSide,
        from_path: String,
        to_side: SftpSide,
        to_cwd: String,
        name: String,
    },
    /// Answer a [`SftpEvent::Conflict`] prompt (routed on a dedicated channel so
    /// it is delivered while a folder transfer is blocked waiting).
    ResolveConflict {
        id: u64,
        action: ConflictAction,
        apply_to_all: bool,
    },
    /// Abort the in-flight differential transfer waiting on a conflict.
    CancelTransfer,
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
    /// Last modification time, Unix seconds (UTC), when known.
    pub modified: Option<i64>,
}

impl From<LocalEntry> for SftpListEntry {
    fn from(entry: LocalEntry) -> Self {
        Self {
            name: entry.name,
            path: entry.path.to_string_lossy().into_owned(),
            is_dir: entry.is_dir,
            size: entry.size,
            modified: entry.modified.map(|t| t.timestamp()),
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
            modified: entry.modified.map(|t| t.timestamp()),
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
    /// The host on `side` could not be reached; `message` is for people.
    ConnectFailed {
        side: SftpSide,
        message: String,
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
    /// Destination already has this path; UI must reply with
    /// [`SftpCommand::ResolveConflict`] or [`SftpCommand::CancelTransfer`].
    Conflict {
        id: u64,
        kind: ConflictKind,
        relative_path: String,
        remote_path: String,
        local_path: String,
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

enum ConflictReply {
    Resolve {
        id: u64,
        action: ConflictAction,
        apply_to_all: bool,
    },
    Cancel,
}

/// Conflict replies shared with the transfer task, which runs off the command loop.
struct ConflictInbox(Mutex<Receiver<ConflictReply>>);

impl ConflictInbox {
    fn try_recv(&self) -> Result<ConflictReply, TryRecvError> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .try_recv()
    }

    fn recv_timeout(&self, wait: Duration) -> Result<ConflictReply, RecvTimeoutError> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .recv_timeout(wait)
    }
}

/// A copy queued for the transfer task, with the connections it was started on.
struct TransferJob {
    folder: bool,
    from_side: SftpSide,
    from_path: String,
    to_side: SftpSide,
    to_cwd: String,
    name: String,
    left: Option<Arc<SftpConnection>>,
    right: Option<Arc<SftpConnection>>,
}

/// UI-side handle to the SFTP worker thread.
pub struct SftpWorker {
    commands: Sender<SftpCommand>,
    events: Receiver<SftpEvent>,
    conflicts: Sender<ConflictReply>,
}

impl SftpWorker {
    /// Start the worker. `wake` is invoked after each event (repaint hook).
    pub fn spawn(wake: Option<Arc<dyn Fn() + Send + Sync>>) -> Self {
        let (command_tx, command_rx) = channel::<SftpCommand>();
        let (event_tx, event_rx) = channel::<SftpEvent>();
        let (conflict_tx, conflict_rx) = channel::<ConflictReply>();

        let _ = std::thread::Builder::new()
            .name("terminus-sftp".to_string())
            .spawn(move || worker(command_rx, event_tx, conflict_rx, wake));

        Self {
            commands: command_tx,
            events: event_rx,
            conflicts: conflict_tx,
        }
    }

    /// Queue a command (non-blocking for the UI).
    pub fn send(&self, cmd: SftpCommand) {
        // Conflict replies must not sit behind an in-flight TransferFolder on
        // the command queue — route them onto the dedicated channel.
        match cmd {
            SftpCommand::ResolveConflict {
                id,
                action,
                apply_to_all,
            } => {
                if let Err(err) = self.conflicts.send(ConflictReply::Resolve {
                    id,
                    action,
                    apply_to_all,
                }) {
                    warn!(error = %err, "sftp conflict reply channel closed");
                }
            }
            SftpCommand::CancelTransfer => {
                if let Err(err) = self.conflicts.send(ConflictReply::Cancel) {
                    warn!(error = %err, "sftp conflict reply channel closed");
                }
            }
            other => {
                if let Err(err) = self.commands.send(other) {
                    warn!(error = %err, "sftp worker command channel closed");
                }
            }
        }
    }

    /// Drain pending events without blocking.
    pub fn drain(&self) -> Vec<SftpEvent> {
        let mut out = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            out.push(event);
        }
        out
    }
}
