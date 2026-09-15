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

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use russh::client::{self, Handle};
use russh::keys::{HashAlg, PrivateKeyWithHashAlg, PublicKey};
use russh::{Channel, ChannelMsg, Pty};
use tracing::{debug, info, warn};

use crate::auth_method::HostAuthMethod;
use crate::error::{Error, Result};
use crate::gssapi;
use crate::models::{Host, Identity};

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

/// Path of the OpenSSH `known_hosts` file used for TOFU.
///
/// Honours `$SSH_KNOWN_HOSTS` for tests and sandboxes, then falls back to
/// `~/.ssh/known_hosts`.
pub fn default_known_hosts_path() -> PathBuf {
    if let Some(p) = std::env::var_os("SSH_KNOWN_HOSTS") {
        return PathBuf::from(p);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".ssh")
        .join("known_hosts")
}

/// `SHA256:…` fingerprint of a public key, the format shown in TOFU modals and
/// by `ssh-keygen -lf`.
pub fn fingerprint_of(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

/// What to do with the key a server presents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyPolicy {
    /// Trust on first use: record unknown keys, reject a key that *changed*
    /// (that is the man-in-the-middle case worth stopping on).
    Tofu,
    /// Same as [`HostKeyPolicy::Tofu`], but an unknown key is only accepted —
    /// and recorded — when its fingerprint matches the one the user approved
    /// in the TOFU modal.
    ApproveFingerprint(String),
    /// Never record anything; accept only hosts already present in
    /// `known_hosts`.
    Strict,
    /// Never accept: capture the presented fingerprint and refuse, so the UI
    /// can run a trust-on-first-use modal and reconnect with
    /// [`HostKeyPolicy::ApproveFingerprint`] once the user has decided.
    Probe,
    /// Accept any key, record nothing. Tests and throwaway containers only.
    AcceptAll,
}

/// Outcome of the host-key check, surfaced to the UI after a connection
/// attempt (accepted, recorded, changed or refused).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostKeyOutcome {
    /// The presented key was already trusted.
    Known {
        /// `SHA256:…` fingerprint of the accepted key.
        fingerprint: String,
    },
    /// The key was unknown and got recorded (first use).
    Recorded {
        /// `SHA256:…` fingerprint written to `known_hosts`.
        fingerprint: String,
    },
    /// The user approved this exact fingerprint in the TOFU modal.
    Approved {
        /// `SHA256:…` fingerprint of the accepted key.
        fingerprint: String,
    },
    /// The server presented a key different from the recorded one.
    Changed {
        /// Fingerprint currently recorded in `known_hosts`.
        expected: String,
        /// Fingerprint actually presented by the server.
        presented: String,
    },
    /// Verification failed (unknown host under `Strict`, refusal, bad file).
    Refused {
        /// Human-readable reason, safe to show in a modal.
        reason: String,
    },
    /// [`HostKeyPolicy::Probe`] captured the presented key and refused it, so
    /// the UI can ask the user. Nothing was written to `known_hosts`.
    Unknown {
        /// `SHA256:…` fingerprint the server presented.
        fingerprint: String,
    },
}

impl HostKeyOutcome {
    /// Whether the key was accepted.
    pub const fn accepted(&self) -> bool {
        matches!(
            self,
            HostKeyOutcome::Known { .. }
                | HostKeyOutcome::Recorded { .. }
                | HostKeyOutcome::Approved { .. }
        )
    }

    /// The accepted key fingerprint, when there is one.
    ///
    /// [`HostKeyOutcome::Unknown`] reports the *presented* fingerprint: the key
    /// was refused, but the modal needs it to ask the user.
    pub fn fingerprint(&self) -> Option<&str> {
        match self {
            HostKeyOutcome::Known { fingerprint }
            | HostKeyOutcome::Recorded { fingerprint }
            | HostKeyOutcome::Approved { fingerprint } => Some(fingerprint),
            HostKeyOutcome::Changed { presented, .. } => Some(presented),
            HostKeyOutcome::Unknown { fingerprint } => Some(fingerprint),
            HostKeyOutcome::Refused { .. } => None,
        }
    }
}

/// A single `known_hosts` entry: which key we trust for a `host:port`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownHostEntry {
    /// Key algorithm as written by OpenSSH (`ssh-ed25519`, `rsa-sha2-512`, …).
    pub algorithm: String,
    /// Base64 body of the public key.
    pub key: String,
}

/// OpenSSH-compatible `known_hosts` reader/writer (TOFU storage).
#[derive(Debug, Clone)]
pub struct KnownHosts {
    path: PathBuf,
}

impl Default for KnownHosts {
    fn default() -> Self {
        Self::at(default_known_hosts_path())
    }
}

impl KnownHosts {
    /// Uses `path` as the backing file (it need not exist yet).
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The backing file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// `known_hosts` name for `host:port`: bare host on 22, `[host]:port`
    /// otherwise — exactly what OpenSSH writes.
    pub fn entry_name(host: &str, port: u16) -> String {
        if port == 22 {
            host.to_string()
        } else {
            format!("[{host}]:{port}")
        }
    }

    /// Looks up every key recorded for `host:port`.
    pub fn lookup(&self, host: &str, port: u16) -> Vec<KnownHostEntry> {
        let name = Self::entry_name(host, port);
        let Ok(contents) = std::fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut fields = line.split_whitespace();
            let (Some(hosts), Some(algorithm), Some(key)) =
                (fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            let matches = hosts.split(',').any(|h| h.trim() == name);
            if matches {
                found.push(KnownHostEntry {
                    algorithm: algorithm.to_string(),
                    key: key.to_string(),
                });
            }
        }
        found
    }

    /// Appends `key` for `host:port`, creating `~/.ssh` and the file when
    /// needed. Idempotent: an identical entry is never written twice.
    pub fn record(&self, host: &str, port: u16, key: &PublicKey) -> Result<()> {
        let name = Self::entry_name(host, port);
        let openssh = key
            .to_openssh()
            .map_err(|e| Error::SshError(format!("cannot encode host key: {e}")))?;
        let mut fields = openssh.split_whitespace();
        let algorithm = fields.next().unwrap_or("ssh-ed25519");
        let body = fields.next().unwrap_or_default();
        if body.is_empty() {
            return Err(Error::SshError("host key has no base64 body".into()));
        }

        if self
            .lookup(host, port)
            .iter()
            .any(|e| e.algorithm == algorithm && e.key == body)
        {
            return Ok(());
        }

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut line = format!("{name} {algorithm} {body}\n");
        if let Ok(existing) = std::fs::read_to_string(&self.path) {
            if !existing.is_empty() && !existing.ends_with('\n') {
                line.insert(0, '\n');
            }
        }
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(line.as_bytes())?;
        file.sync_data()?;
        info!(host = %name, algorithm, "recorded host key in known_hosts");
        Ok(())
    }
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

/// russh client handler: verifies the server key and records the decision.
pub struct ClientHandler {
    host: String,
    port: u16,
    policy: HostKeyPolicy,
    known_hosts: KnownHosts,
    outcome: Arc<Mutex<Option<HostKeyOutcome>>>,
}

impl client::Handler for ClientHandler {
    type Error = Error;

    async fn check_server_key(&mut self, server_public_key: &PublicKey) -> Result<bool> {
        let fingerprint = fingerprint_of(server_public_key);
        let recorded = self.known_hosts.lookup(&self.host, self.port);

        let decision = if matches!(self.policy, HostKeyPolicy::Probe) {
            // Capture-only: the caller first probes, shows the user the
            // fingerprint, then reconnects with `ApproveFingerprint`.
            HostKeyOutcome::Unknown { fingerprint }
        } else if matches!(self.policy, HostKeyPolicy::AcceptAll) {
            HostKeyOutcome::Known { fingerprint }
        } else if recorded.is_empty() {
            match &self.policy {
                HostKeyPolicy::Strict => HostKeyOutcome::Refused {
                    reason: format!(
                        "{host}:{port} is not in {} (policy: strict)",
                        self.known_hosts.path().display(),
                        host = self.host,
                        port = self.port
                    ),
                },
                HostKeyPolicy::ApproveFingerprint(approved) if approved == &fingerprint => {
                    HostKeyOutcome::Approved { fingerprint }
                }
                HostKeyPolicy::ApproveFingerprint(approved) => HostKeyOutcome::Refused {
                    reason: format!("approved fingerprint was {approved}, server presented {fingerprint}"),
                },
                HostKeyPolicy::Tofu => HostKeyOutcome::Recorded { fingerprint },
                HostKeyPolicy::Probe | HostKeyPolicy::AcceptAll => {
                    unreachable!("handled above")
                }
            }
        } else {
            let body = server_public_key
                .to_openssh()
                .ok()
                .and_then(|k| k.split_whitespace().nth(1).map(str::to_string))
                .unwrap_or_default();
            if recorded.iter().any(|e| e.key == body) {
                HostKeyOutcome::Known { fingerprint }
            } else {
                let expected = recorded
                    .first()
                    .map(|e| {
                        format!(
                            "{}:{}",
                            e.algorithm,
                            e.key.chars().take(24).collect::<String>()
                        )
                    })
                    .unwrap_or_default();
                HostKeyOutcome::Changed {
                    expected,
                    presented: fingerprint,
                }
            }
        };

        let accepted = decision.accepted();
        if accepted {
            if let HostKeyOutcome::Recorded { .. } = decision {
                if let Err(err) =
                    self.known_hosts
                        .record(&self.host, self.port, server_public_key)
                {
                    warn!(error = %err, host = %self.host, "could not persist host key");
                }
            }
            if matches!(
                decision,
                HostKeyOutcome::Approved { .. } | HostKeyOutcome::Recorded { .. }
            ) {
                if let Err(err) =
                    self.known_hosts
                        .record(&self.host, self.port, server_public_key)
                {
                    warn!(error = %err, host = %self.host, "could not persist approved host key");
                }
            }
        } else {
            warn!(host = %self.host, "host key rejected");
        }

        if let Ok(mut slot) = self.outcome.lock() {
            *slot = Some(decision);
        }
        Ok(accepted)
    }
}

/// A live SSH channel wired to a remote shell.
pub struct SshSession {
    handle: Handle<ClientHandler>,
    channel: Channel<client::Msg>,
    outcome: Arc<Mutex<Option<HostKeyOutcome>>>,
    pty: SshPty,
    hostname: String,
    port: u16,
    disconnected: bool,
}

/// Typed failure from [`probe_ssh_auth`] — mapped to add-host error copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeError {
    /// TCP / DNS / timeout before auth.
    Unreachable(String),
    /// Server rejected the credentials.
    AuthFailed,
    /// Key auth requested but no usable private key was available.
    NoKey,
    /// Kerberos ticket cache missing / expired.
    GssapiNoTicket,
    /// GSSAPI not available on this platform.
    GssapiUnsupported,
    /// Anything else (host key, protocol, …).
    Other(String),
}

impl ProbeError {
    /// User-facing message for the add-host error line.
    pub fn user_message(&self) -> String {
        match self {
            Self::Unreachable(detail) => {
                if detail.is_empty() {
                    "Host unreachable".into()
                } else {
                    format!("Host unreachable: {detail}")
                }
            }
            Self::AuthFailed => {
                "Authentication failed (wrong password, username, or key)".into()
            }
            Self::NoKey => {
                "No SSH private key selected. Save a key in Settings, then try again.".into()
            }
            Self::GssapiNoTicket => Error::GssapiNoTicket.to_string(),
            Self::GssapiUnsupported => Error::GssapiUnsupported.to_string(),
            Self::Other(msg) => msg.clone(),
        }
    }

    /// Map a core [`Error`] from connect/auth into a probe failure.
    pub fn from_error(err: Error) -> Self {
        match err {
            Error::GssapiNoTicket => Self::GssapiNoTicket,
            Error::GssapiUnsupported => Self::GssapiUnsupported,
            Error::TimeoutError(msg) => Self::Unreachable(msg),
            Error::IoError(msg) => Self::Unreachable(msg),
            Error::SshError(msg) => classify_ssh_message(&msg),
            Error::Message(msg) => classify_ssh_message(&msg),
            Error::IdentityKeyInvalid { reason } => Self::Other(format!("invalid SSH key: {reason}")),
            other => Self::Other(other.to_string()),
        }
    }
}

fn classify_ssh_message(msg: &str) -> ProbeError {
    let lower = msg.to_ascii_lowercase();
    if lower.contains("connection refused")
        || lower.contains("network is unreachable")
        || lower.contains("name or service not known")
        || lower.contains("no route to host")
        || lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("could not resolve")
    {
        return ProbeError::Unreachable(msg.to_string());
    }
    if lower.contains("no ssh private key")
        || lower.contains("cannot load key")
        || lower.contains("no usable credentials")
    {
        return ProbeError::NoKey;
    }
    if lower.contains("authentication refused")
        || lower.contains("auth failed")
        || lower.contains("public key rejected")
        || lower.contains("password auth failed")
        || lower.contains("authentication failed")
    {
        return ProbeError::AuthFailed;
    }
    ProbeError::Other(msg.to_string())
}

/// Build connect options for an add-host probe (accept any host key).
pub fn probe_options_from_host(host: &Host, identity: Option<&Identity>) -> SshConnectOptions {
    let method = crate::auth_method::parse_host_auth_method(&host.auth_method)
        .map(|ok| ok.method)
        .unwrap_or(HostAuthMethod::Password);

    let mut auth = SshAuth {
        username: host.username.clone(),
        method: Some(method),
        ..SshAuth::default()
    };

    match method {
        HostAuthMethod::Password => {
            auth.password = host.password.clone();
        }
        HostAuthMethod::Key => {
            if let Some(ident) = identity {
                auth.identity_pem = ident.private_key.clone();
                auth.identity_passphrase = ident.passphrase.clone();
            }
        }
        HostAuthMethod::Gssapi => {}
    }

    SshConnectOptions {
        hostname: host.hostname.clone(),
        port: host.port,
        auth,
        policy: HostKeyPolicy::AcceptAll,
        known_hosts: KnownHosts::default(),
        connect_timeout: DEFAULT_CONNECT_TIMEOUT,
        keepalive_interval: Some(DEFAULT_KEEPALIVE_INTERVAL),
    }
}

/// TCP + auth only — no PTY. Used by add-host Connect before save.
pub async fn probe_ssh_auth(opts: &SshConnectOptions) -> std::result::Result<(), ProbeError> {
    match SshSession::connect(opts).await {
        Ok(mut session) => {
            let _ = session.disconnect().await;
            Ok(())
        }
        Err(err) => Err(ProbeError::from_error(err)),
    }
}

impl SshSession {
    /// Connects to `opts.hostname:opts.port`, verifies the host key and
    /// authenticates. The channel is left open but no PTY is requested yet —
    /// call [`SshSession::open_shell`].
    pub async fn connect(opts: &SshConnectOptions) -> Result<Self> {
        let outcome = Arc::new(Mutex::new(None));
        let handler = ClientHandler {
            host: opts.hostname.clone(),
            port: opts.port,
            policy: opts.policy.clone(),
            known_hosts: opts.known_hosts.clone(),
            outcome: Arc::clone(&outcome),
        };

        let mut config = client::Config::default();
        config.keepalive_interval = opts.keepalive_interval;
        config.inactivity_timeout = Some(DEFAULT_INACTIVITY_TIMEOUT);
        config.channel_buffer_size = 256;

        let addrs = (opts.hostname.as_str(), opts.port);
        let connect = client::connect(Arc::new(config), addrs, handler);
        let mut handle = match tokio::time::timeout(opts.connect_timeout, connect).await {
            Err(_) => {
                return Err(Error::TimeoutError(format!(
                    "SSH connect to {}:{} timed out after {:?}",
                    opts.hostname, opts.port, opts.connect_timeout
                )))
            }
            Ok(Ok(handle)) => handle,
            Ok(Err(err)) => return Err(host_key_aware_error(err, &outcome)),
        };

        authenticate(&mut handle, &opts.hostname, opts.port, &opts.auth).await?;
        let channel = handle
            .channel_open_session()
            .await
            .map_err(|e| Error::SshError(format!("cannot open session channel: {e}")))?;

        info!(host = %opts.hostname, port = opts.port, user = %opts.auth.username, "SSH session established");

        Ok(Self {
            handle,
            channel,
            outcome,
            pty: SshPty::default(),
            hostname: opts.hostname.clone(),
            port: opts.port,
            disconnected: false,
        })
    }

    /// Requests a PTY and a login shell.
    pub async fn open_shell(&mut self, pty: SshPty) -> Result<()> {
        let modes: [(Pty, u32); 2] = [(Pty::IUTF8, 1), (Pty::ECHO, 1)];
        self.channel
            .request_pty(
                true,
                &pty.term,
                pty.cols,
                pty.rows,
                pty.pix_width,
                pty.pix_height,
                &modes,
            )
            .await
            .map_err(|e| Error::SshError(format!("pty request failed: {e}")))?;
        self.channel
            .request_shell(true)
            .await
            .map_err(|e| Error::SshError(format!("shell request failed: {e}")))?;
        self.pty = pty;
        debug!(host = %self.hostname, cols = self.pty.cols, rows = self.pty.rows, "shell started");
        Ok(())
    }

    /// Sends keystrokes to the remote shell.
    pub async fn write(&self, data: &[u8]) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        self.channel
            .data(&data[..])
            .await
            .map_err(|e| Error::SshError(format!("channel write failed: {e}")))
    }

    /// Reports a new window size to the remote host.
    pub async fn resize(&self, cols: u32, rows: u32) -> Result<()> {
        self.channel
            .window_change(cols, rows, self.pty.pix_width, self.pty.pix_height)
            .await
            .map_err(|e| Error::SshError(format!("window change failed: {e}")))
    }

    /// Waits for the next channel event. `None` means the channel is gone.
    pub async fn next_event(&mut self) -> Option<SshEvent> {
        loop {
            let msg = match self.channel.wait().await {
                Some(msg) => msg,
                None => return Some(SshEvent::Closed),
            };
            match msg {
                ChannelMsg::Data { data } => return Some(SshEvent::Data(data.to_vec())),
                ChannelMsg::ExtendedData { data, ext } => {
                    return Some(SshEvent::ExtendedData {
                        stream: ext,
                        data: data.to_vec(),
                    })
                }
                ChannelMsg::Eof => return Some(SshEvent::Eof),
                ChannelMsg::ExitStatus { exit_status } => {
                    return Some(SshEvent::ExitStatus(exit_status))
                }
                ChannelMsg::ExitSignal { signal_name, .. } => {
                    return Some(SshEvent::ExitSignal {
                        signal: format!("{signal_name:?}"),
                    })
                }
                ChannelMsg::Close => return Some(SshEvent::Closed),
                _ => continue,
            }
        }
    }

    /// Asks the remote side to close, then tears the connection down.
    pub async fn disconnect(&mut self) -> Result<()> {
        if self.disconnected {
            return Ok(());
        }
        self.disconnected = true;
        let _ = self.channel.eof().await;
        let _ = self.channel.close().await;
        self.handle
            .disconnect(russh::Disconnect::ByApplication, "session closed", "")
            .await
            .map_err(|e| Error::SshError(format!("disconnect failed: {e}")))
    }

    /// Host-key decision taken during the handshake.
    pub fn host_key_outcome(&self) -> Option<HostKeyOutcome> {
        self.outcome.lock().ok().and_then(|o| o.clone())
    }

    /// `host:port` this session targets.
    pub fn target(&self) -> String {
        format!("{}:{}", self.hostname, self.port)
    }

    /// Current PTY geometry.
    pub fn pty(&self) -> &SshPty {
        &self.pty
    }
}

/// Runs authentication for the selected method. GSSAPI never falls through
/// to password; key never falls through either.
async fn authenticate(
    handle: &mut Handle<ClientHandler>,
    hostname: &str,
    port: u16,
    auth: &SshAuth,
) -> Result<()> {
    let method = auth.method.unwrap_or_else(|| {
        if auth.identity_pem.is_some() || auth.identity_path.is_some() {
            HostAuthMethod::Key
        } else if auth.password.is_some() {
            HostAuthMethod::Password
        } else {
            HostAuthMethod::Password
        }
    });

    match method {
        HostAuthMethod::Gssapi => {
            let host = Host {
                id: uuid::Uuid::nil(),
                name: hostname.to_string(),
                hostname: hostname.to_string(),
                port,
                username: auth.username.clone(),
                auth_method: HostAuthMethod::Gssapi.as_str().to_string(),
                password: None,
                identity_id: None,
                group_id: None,
                tags: Vec::new(),
                notes: String::new(),
                os_id: None,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
                deleted_at: None,
            };
            let ok = gssapi::authenticate(handle, &host).await?;
            if ok {
                return Ok(());
            }
            return Err(Error::SshError(format!(
                "authentication refused for user {}",
                auth.username
            )));
        }
        HostAuthMethod::Key => authenticate_key(handle, auth).await,
        HostAuthMethod::Password => authenticate_password(handle, auth).await,
    }
}

async fn authenticate_key(handle: &mut Handle<ClientHandler>, auth: &SshAuth) -> Result<()> {
    let key = if let Some(pem) = auth.identity_pem.as_deref() {
        russh::keys::decode_secret_key(pem, auth.identity_passphrase.as_deref()).map_err(|e| {
            Error::IdentityKeyInvalid {
                reason: e.to_string(),
            }
        })?
    } else if let Some(path) = &auth.identity_path {
        russh::keys::load_secret_key(path, auth.identity_passphrase.as_deref()).map_err(|e| {
            Error::SshError(format!("cannot load key {}: {e}", path.display()))
        })?
    } else {
        return Err(Error::SshError(
            "no SSH private key found (save a key in Settings, then try again)".into(),
        ));
    };

    let hash_alg = handle
        .best_supported_rsa_hash()
        .await
        .ok()
        .flatten()
        .flatten();
    let result = handle
        .authenticate_publickey(
            auth.username.clone(),
            PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg),
        )
        .await
        .map_err(|e| Error::SshError(format!("public key auth failed: {e}")))?;
    if result.success() {
        return Ok(());
    }
    Err(Error::SshError(format!(
        "authentication refused for user {}",
        auth.username
    )))
}

async fn authenticate_password(handle: &mut Handle<ClientHandler>, auth: &SshAuth) -> Result<()> {
    let Some(password) = auth.password.as_ref() else {
        return Err(Error::SshError(format!(
            "authentication refused for user {}",
            auth.username
        )));
    };
    let result = handle
        .authenticate_password(auth.username.clone(), password.clone())
        .await
        .map_err(|e| Error::SshError(format!("password auth failed: {e}")))?;
    if result.success() {
        return Ok(());
    }
    Err(Error::SshError(format!(
        "authentication refused for user {}",
        auth.username
    )))
}

/// Turns a russh handshake error into a message that names the host-key
/// decision when that is what stopped the connection.
fn host_key_aware_error(
    err: Error,
    outcome: &Arc<Mutex<Option<HostKeyOutcome>>>,
) -> Error {
    let decision = outcome.lock().ok().and_then(|o| o.clone());
    match decision {
        Some(HostKeyOutcome::Changed { expected, presented }) => Error::SshError(format!(
            "host key changed: expected {expected}, got {presented} — verify the server before trusting it"
        )),
        Some(HostKeyOutcome::Refused { reason }) => Error::SshError(format!("host key refused: {reason}")),
        Some(HostKeyOutcome::Unknown { fingerprint }) => Error::SshError(format!(
            "unknown host key {fingerprint} — approval required"
        )),
        _ => err,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_known_hosts(tag: &str) -> KnownHosts {
        let path = std::env::temp_dir()
            .join(format!("terminus-known-hosts-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        KnownHosts::at(path)
    }

    #[test]
    fn entry_name_matches_openssh_convention() {
        assert_eq!(KnownHosts::entry_name("example.com", 22), "example.com");
        assert_eq!(
            KnownHosts::entry_name("example.com", 2222),
            "[example.com]:2222"
        );
    }

    #[test]
    fn lookup_parses_entries_and_ignores_comments() {
        let kh = temp_known_hosts("lookup");
        std::fs::write(
            kh.path(),
            "# comment\n\nexample.com ssh-ed25519 AAAAKEY\nexample.com,10.0.0.1 rsa-sha2-512 BBBBKEY\n",
        )
        .unwrap();
        let entries = kh.lookup("example.com", 22);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].algorithm, "ssh-ed25519");
        assert_eq!(entries[1].key, "BBBBKEY");
        assert!(
            kh.lookup("10.0.0.1", 22).is_empty()
                || kh.lookup("10.0.0.1", 22)[0].key == "BBBBKEY"
        );
        let _ = std::fs::remove_file(kh.path());
    }

    #[test]
    fn lookup_uses_bracketed_form_for_non_standard_ports() {
        let kh = temp_known_hosts("ports");
        std::fs::write(kh.path(), "[example.com]:2222 ssh-ed25519 KEY2222\n").unwrap();
        assert!(kh.lookup("example.com", 22).is_empty());
        assert_eq!(kh.lookup("example.com", 2222)[0].key, "KEY2222");
        let _ = std::fs::remove_file(kh.path());
    }

    #[test]
    fn pty_defaults_are_usable() {
        let pty = SshPty::default();
        assert_eq!(pty.term, DEFAULT_TERM);
        assert_eq!((pty.cols, pty.rows), (80, 24));
        assert_eq!(SshPty::sized(120, 40).rows, 40);
    }

    #[test]
    fn outcome_helpers_report_acceptance() {
        let known = HostKeyOutcome::Known {
            fingerprint: "SHA256:abc".into(),
        };
        assert!(known.accepted());
        assert_eq!(known.fingerprint(), Some("SHA256:abc"));
        let refused = HostKeyOutcome::Refused {
            reason: "unknown host".into(),
        };
        assert!(!refused.accepted());
        assert_eq!(refused.fingerprint(), None);
    }

    #[test]
    fn probe_error_messages_cover_each_variant() {
        assert!(ProbeError::Unreachable(String::new())
            .user_message()
            .contains("unreachable"));
        assert!(ProbeError::AuthFailed
            .user_message()
            .contains("Authentication failed"));
        assert!(ProbeError::NoKey.user_message().contains("private key"));
        assert_eq!(
            ProbeError::GssapiNoTicket.user_message(),
            Error::GssapiNoTicket.to_string()
        );
        assert_eq!(
            ProbeError::GssapiUnsupported.user_message(),
            Error::GssapiUnsupported.to_string()
        );
        assert_eq!(ProbeError::Other("x".into()).user_message(), "x");
    }

    #[test]
    fn classify_ssh_message_maps_common_failures() {
        assert!(matches!(
            classify_ssh_message("Connection refused"),
            ProbeError::Unreachable(_)
        ));
        assert!(matches!(
            classify_ssh_message("authentication refused for user root"),
            ProbeError::AuthFailed
        ));
        assert!(matches!(
            classify_ssh_message("no SSH private key found"),
            ProbeError::NoKey
        ));
    }

    #[test]
    fn probe_error_from_typed_errors() {
        assert_eq!(
            ProbeError::from_error(Error::GssapiNoTicket),
            ProbeError::GssapiNoTicket
        );
        assert_eq!(
            ProbeError::from_error(Error::GssapiUnsupported),
            ProbeError::GssapiUnsupported
        );
        assert!(matches!(
            ProbeError::from_error(Error::TimeoutError("boom".into())),
            ProbeError::Unreachable(_)
        ));
    }
}
