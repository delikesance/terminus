//! Terminus bridge: the seam between [`terminus_core`] and Rio.
//!
//! `terminus-core` is a pure Tokio crate with no UI dependency; Rio drives its
//! event loop on corcovado (a mio fork) from a dedicated thread. This crate is
//! the only place where both meet:
//!
//! * [`ssh_transport`] exposes a remote shell as a [`teletypewriter::EventedPty`],
//!   so the renderer cannot tell an SSH pane from a local one.
//! * [`modal_event_handlers`] carries the two flows that need the user
//!   (host-key approval, vault unlock) between the async core and the UI.

pub mod folder_diff;
pub mod modal_event_handlers;
mod progress_throttle;
pub mod sftp_worker;
pub mod ssh_transport;

// Pending: these land together with their implementations (see `milestone.md`,
// steps 1.2 and 1.4). Re-exporting them now breaks `cargo check`.
// pub use modal_event_handlers::{
//     HostKeyRequest, ModalDecision, ModalRequest, VaultUnlockRequest,
// };
pub use folder_diff::ConflictAction;
pub use sftp_worker::{
    ConflictKind, SftpCommand, SftpEvent, SftpListEntry, SftpSide, SftpWorker,
    REMOTE_HOME,
};
pub use ssh_transport::{Command, SshPump, SshTransport, TransportState};
