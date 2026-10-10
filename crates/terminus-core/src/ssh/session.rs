use std::sync::{Arc, Mutex};

use russh::client::{self, Handle};
use russh::{Channel, ChannelMsg, Pty};
use tracing::{debug, info};

use super::auth::authenticate;
use super::handler::{host_key_aware_error, ClientHandler};
use super::options::client_config;
use super::sftp_conn::open_sftp_on_handle;
use super::{HostKeyOutcome, SshConnectOptions, SshEvent, SshPty};
use crate::error::{Error, Result};

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

    /// Opens an additional channel and requests the SFTP subsystem on this
    /// connection (shell channel remains open).
    pub async fn open_sftp(&self) -> Result<crate::sftp::SftpSession> {
        open_sftp_on_handle(&self.handle).await
    }

    /// Sends keystrokes to the remote shell.
    pub async fn write(&self, data: &[u8]) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        self.channel
            .data(data)
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
