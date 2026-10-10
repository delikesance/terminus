//! SSH client transport built on [`russh`].
//!
//! Owns everything between "the store holds a `Host` row" and "terminal bytes
//! flow": TCP connection, host-key verification (TOFU against an OpenSSH
//! `known_hosts` file), authentication, PTY allocation and the channel
//! read/write loop.
//!
//! The module is deliberately UI-agnostic — [`SshSession`] yields
//! [`SshEvent`]s and accepts raw bytes, so the bridge can pump them through an
//! OS pipe exactly like a local PTY (`terminus-bridge::ssh_transport`).

use std::time::Duration;

mod auth;
mod handler;
mod known_hosts;
mod options;
mod probe;
mod session;
mod sftp_conn;

pub use handler::ClientHandler;
pub use known_hosts::{
    default_known_hosts_path, fingerprint_of, preferred_host_key_algorithms,
    HostKeyOutcome, HostKeyPolicy, KnownHostEntry, KnownHosts,
};
pub use options::{SshAuth, SshConnectOptions, SshEvent, SshPty};
pub use probe::{probe_options_from_host, probe_ssh_auth, ProbeError};
pub use session::SshSession;
pub use sftp_conn::{
    connect_sftp, connect_sftp_for_host, detect_remote_os, SftpConnection,
};

/// Default keepalive interval: matches `ServerAliveInterval 30`.
pub const DEFAULT_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(30);

/// Default inactivity timeout before russh garbage-collects the connection.
pub const DEFAULT_INACTIVITY_TIMEOUT: Duration = Duration::from_secs(300);

/// Default TCP + handshake budget.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Terminal type advertised to the remote host.
pub const DEFAULT_TERM: &str = "xterm-256color";

/// Byte size of a single read from the SSH channel.
pub const CHUNK_SIZE: usize = 8192;

#[cfg(test)]
mod connect_tests;
#[cfg(test)]
mod known_hosts_tests;
#[cfg(test)]
mod probe_tests;
