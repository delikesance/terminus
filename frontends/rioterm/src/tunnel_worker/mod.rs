//! Tunnels backend: ssh command building, process registry, persistence.
//!
//! Tunnels run with the system OpenSSH (`ssh -N -L/-R/-D …`), exactly like the
//! interactive host tabs (decision "Option A", see `milestone.md`): the caller
//! builds the base `ssh` arguments with the same builder the shells use
//! (`screen::shell::ssh_shell`, so user/host/port/identity and
//! `StrictHostKeyChecking=accept-new` are identical) and this module only adds
//! the forwarding flags.
//!
//! * [`forward_args`] / [`tunnel_ssh_args`] / [`friendly_error`] are pure.
//! * [`TunnelRegistry`] owns the child processes: start, stop, exit
//!   detection, kill-on-drop.
//! * [`TunnelController`] ties the view state
//!   ([`terminus_ui::views::tunnels::TunnelsState`]), the persistence worker
//!   (SQLite on its own thread, like `hosts.rs`) and the registry.

use std::time::Duration;

/// `ExitOnForwardFailure=yes` a bad forward or a refused login ends the
/// process within the connect timeout, so "still alive after a moment" is the
/// only success signal `ssh -N` gives.
pub const DEFAULT_SETTLE: Duration = Duration::from_millis(2500);

mod args;
mod controller;
mod persistence;
mod registry;
mod stats;
mod ticker;

pub use args::tunnel_ssh_args;
pub use controller::{SpawnFn, TunnelController};

#[cfg(test)]
mod args_tests;
#[cfg(test)]
mod controller_tests;
#[cfg(test)]
mod registry_tests;
#[cfg(test)]
mod stats_tests;
#[cfg(test)]
mod test_support;
