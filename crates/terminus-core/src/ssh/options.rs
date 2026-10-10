use std::time::Duration;

use russh::client;

use std::path::PathBuf;

use super::{
    preferred_host_key_algorithms, HostKeyPolicy, KnownHosts, DEFAULT_CONNECT_TIMEOUT,
    DEFAULT_INACTIVITY_TIMEOUT, DEFAULT_KEEPALIVE_INTERVAL, DEFAULT_TERM,
};
use crate::auth_method::HostAuthMethod;

/// russh client config for `opts`, with host-key preference from known_hosts.
pub(super) fn client_config(opts: &SshConnectOptions) -> client::Config {
    let mut config = client::Config {
        keepalive_interval: opts.keepalive_interval,
        inactivity_timeout: Some(DEFAULT_INACTIVITY_TIMEOUT),
        channel_buffer_size: 256,
        ..Default::default()
    };
    let recorded = opts.known_hosts.lookup(&opts.hostname, opts.port);
    if !recorded.is_empty() {
        config.preferred.key =
            std::borrow::Cow::Owned(preferred_host_key_algorithms(&recorded));
    }
    config
}

/// Credentials used to authenticate a session.
#[derive(Debug, Clone, Default)]
pub struct SshAuth {
    /// Remote user name.
    pub username: String,
    /// Password, when the host uses password auth.
    pub password: Option<String>,
    /// Private key file, when the host uses key auth from disk.
    pub identity_path: Option<PathBuf>,
    /// Passphrase protecting `identity_path` / PEM, when encrypted.
    pub identity_passphrase: Option<String>,
    /// In-memory OpenSSH private key PEM (from a saved [`Identity`]).
    pub identity_pem: Option<String>,
    /// Explicit auth method. When unset, falls back to capability order.
    pub method: Option<HostAuthMethod>,
}

/// Everything needed to open an SSH session.
#[derive(Debug, Clone)]
pub struct SshConnectOptions {
    /// Target host name or IP.
    pub hostname: String,
    /// TCP port (normally 22).
    pub port: u16,
    /// Credentials.
    pub auth: SshAuth,
    /// Host-key verification policy.
    pub policy: HostKeyPolicy,
    /// `known_hosts` file used for TOFU.
    pub known_hosts: KnownHosts,
    /// Budget for TCP + key exchange + authentication.
    pub connect_timeout: Duration,
    /// Keepalive interval; `None` disables it.
    pub keepalive_interval: Option<Duration>,
}

impl SshConnectOptions {
    /// Options for `hostname:port` with password auth and TOFU policy.
    pub fn password(
        hostname: impl Into<String>,
        port: u16,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        Self {
            hostname: hostname.into(),
            port,
            auth: SshAuth {
                username: username.into(),
                password: Some(password.into()),
                ..SshAuth::default()
            },
            policy: HostKeyPolicy::Tofu,
            known_hosts: KnownHosts::default(),
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            keepalive_interval: Some(DEFAULT_KEEPALIVE_INTERVAL),
        }
    }
}

/// PTY geometry requested from the remote host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshPty {
    /// `TERM` value.
    pub term: String,
    /// Columns.
    pub cols: u32,
    /// Rows.
    pub rows: u32,
    /// Pixel width (0 when unknown).
    pub pix_width: u32,
    /// Pixel height (0 when unknown).
    pub pix_height: u32,
}

impl Default for SshPty {
    fn default() -> Self {
        Self {
            term: DEFAULT_TERM.to_string(),
            cols: 80,
            rows: 24,
            pix_width: 0,
            pix_height: 0,
        }
    }
}

impl SshPty {
    /// PTY with `cols` × `rows` cells.
    pub fn sized(cols: u32, rows: u32) -> Self {
        Self {
            cols,
            rows,
            ..Self::default()
        }
    }
}

/// An event read from the remote channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SshEvent {
    /// Standard output bytes.
    Data(Vec<u8>),
    /// Extended (stderr) bytes, with the SSH stream id.
    ExtendedData {
        /// SSH extended-data stream id (usually 1 for stderr).
        stream: u32,
        /// Payload.
        data: Vec<u8>,
    },
    /// Remote side will send no more data; the session is winding down.
    Eof,
    /// Remote command exit status.
    ExitStatus(u32),
    /// Remote command killed by a signal.
    ExitSignal {
        /// Signal name as reported by the server.
        signal: String,
    },
    /// Channel closed: the transport is gone.
    Closed,
}
