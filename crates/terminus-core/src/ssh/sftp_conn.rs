use std::sync::{Arc, Mutex};

use russh::client::{self, Handle};
use tracing::info;

use super::auth::authenticate;
use super::handler::{host_key_aware_error, ClientHandler};
use super::options::client_config;
use super::SshConnectOptions;
use crate::error::{Error, Result};

/// Dedicated SFTP connection: owns the russh [`Handle`] so the channel stays alive.
///
/// Terminal tabs use the OpenSSH CLI; SFTP opens an independent russh session
/// (same auth as the host) and requests the `sftp` subsystem.
pub struct SftpConnection {
    handle: Handle<ClientHandler>,
    session: crate::sftp::SftpSession,
}

impl SftpConnection {
    /// Borrow the wrapped [`crate::sftp::SftpSession`].
    pub fn session(&self) -> &crate::sftp::SftpSession {
        &self.session
    }

    /// Mutable borrow of the wrapped session.
    pub fn session_mut(&mut self) -> &mut crate::sftp::SftpSession {
        &mut self.session
    }

    /// Run a remote command on a new session channel (not the SFTP subsystem).
    ///
    /// Returns `(exit_status, stdout, stderr)`.
    pub async fn exec(&self, command: &str) -> Result<(u32, Vec<u8>, Vec<u8>)> {
        use russh::ChannelMsg;

        let mut channel = self
            .handle
            .channel_open_session()
            .await
            .map_err(|e| Error::SshError(format!("cannot open exec channel: {e}")))?;
        channel
            .exec(true, command)
            .await
            .map_err(|e| Error::SshError(format!("exec request failed: {e}")))?;

        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut code = None;
        loop {
            let Some(msg) = channel.wait().await else {
                break;
            };
            match msg {
                ChannelMsg::Data { ref data } => {
                    stdout.extend_from_slice(data);
                }
                ChannelMsg::ExtendedData { ref data, .. } => {
                    stderr.extend_from_slice(data);
                }
                ChannelMsg::ExitStatus { exit_status } => {
                    code = Some(exit_status);
                }
                _ => {}
            }
        }
        Ok((code.unwrap_or(255), stdout, stderr))
    }
}

impl std::ops::Deref for SftpConnection {
    type Target = crate::sftp::SftpSession;

    fn deref(&self) -> &Self::Target {
        &self.session
    }
}

impl std::ops::DerefMut for SftpConnection {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.session
    }
}

impl std::fmt::Debug for SftpConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SftpConnection")
            .field("session", &self.session)
            .finish_non_exhaustive()
    }
}

pub(super) async fn open_sftp_on_handle(
    handle: &Handle<ClientHandler>,
) -> Result<crate::sftp::SftpSession> {
    let channel = handle
        .channel_open_session()
        .await
        .map_err(|e| Error::SshError(format!("cannot open SFTP session channel: {e}")))?;
    channel
        .request_subsystem(true, "sftp")
        .await
        .map_err(|e| Error::SshError(format!("sftp subsystem request failed: {e}")))?;
    crate::sftp::SftpSession::connect(channel.into_stream()).await
}

/// Connect, authenticate, and open an SFTP subsystem (no shell / PTY).
pub async fn connect_sftp(opts: &SshConnectOptions) -> Result<SftpConnection> {
    let outcome = Arc::new(Mutex::new(None));
    let handler = ClientHandler {
        host: opts.hostname.clone(),
        port: opts.port,
        policy: opts.policy.clone(),
        known_hosts: opts.known_hosts.clone(),
        outcome: Arc::clone(&outcome),
    };

    let config = client_config(opts);

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
    let session = open_sftp_on_handle(&handle).await?;
    info!(
        host = %opts.hostname,
        port = opts.port,
        user = %opts.auth.username,
        "SFTP session established"
    );
    Ok(SftpConnection { handle, session })
}

/// Same as [`connect_sftp`], named for the host-sidebar / dual-pane entry path.
///
/// Callers typically build `opts` via [`probe_options_from_host`] (or a TOFU
/// variant) after resolving password / identity from the store.
pub async fn connect_sftp_for_host(opts: &SshConnectOptions) -> Result<SftpConnection> {
    connect_sftp(opts).await
}

/// Connect (SFTP session), run [`crate::os_detect::REMOTE_OS_PROBE_SCRIPT`], return canonical os_id.
pub async fn detect_remote_os(opts: &SshConnectOptions) -> Result<String> {
    use crate::os_detect::{parse_remote_os_probe, REMOTE_OS_PROBE_SCRIPT};

    let conn = connect_sftp(opts).await?;
    let (_code, stdout, _stderr) = conn.exec(REMOTE_OS_PROBE_SCRIPT).await?;
    Ok(parse_remote_os_probe(&String::from_utf8_lossy(&stdout)))
}
