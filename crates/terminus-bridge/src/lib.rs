//! Terminus bridge: the seam between [`terminus_core`] and Rio.
//!
//! `terminus-core` is a pure Tokio crate with no UI dependency; Rio drives its
//! event loop on corcovado (a mio fork) from a dedicated thread. This crate is
//! the only place where both meet:
//!
//! * [`ssh_transport`] exposes a remote shell as a [`teletypewriter::EventedPty`],
//!   so the renderer cannot tell an SSH pane from a local one.
//! * [`terminus_bridge_impl`] owns the core runtime ([`TerminusCoreService`])
//!   and the [`TerminusBridge`] trait the frontend calls into.
//! * [`modal_event_handlers`] carries the two flows that need the user
//!   (host-key approval, vault unlock) between the async core and the UI.

pub mod modal_event_handlers;
pub mod ssh_transport;
pub mod terminus_bridge_impl;

// Pending: these land together with their implementations (see `milestone.md`,
// steps 1.2 and 1.4). Re-exporting them now breaks `cargo check`.
// pub use modal_event_handlers::{
//     HostKeyRequest, ModalDecision, ModalRequest, VaultUnlockRequest,
// };
pub use ssh_transport::{Command, SshPump, SshTransport, TransportState};
// pub use terminus_bridge_impl::{SessionHandle, TerminusBridge, TerminusCoreService};
