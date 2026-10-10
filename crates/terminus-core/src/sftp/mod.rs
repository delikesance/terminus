//! SFTP client used by the dual-pane file browser.
//!
//! [`SftpSession`] wraps [`russh_sftp::client::SftpSession`] with three things
//! the pane needs and the raw client does not provide:
//!
//! * a **30 second budget per operation** (`tokio::time::timeout`), so a stalled
//!   channel can never freeze the UI task that awaits a listing;
//! * **path-traversal protection**: every path is normalized and, when a root is
//!   configured, verified to stay inside it (`..` segments are rejected);
//! * entries translated into this crate's own [`SftpEntry`] shape, sorted
//!   directories-first, ready for the pane renderer.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::timeout;

use crate::error::{Error, Result};

mod entry;
mod io;
mod ops;
mod path;
mod transfer;

pub use entry::SftpEntry;
pub use path::{is_within_root, normalize_remote_path, sandbox_path};

/// Default per-operation budget.
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// An authenticated SFTP channel with a per-operation timeout budget.
#[derive(Clone)]
pub struct SftpSession {
    inner: Arc<russh_sftp::client::SftpSession>,
    root: Option<String>,
    timeout: Duration,
}

impl std::fmt::Debug for SftpSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SftpSession")
            .field("root", &self.root)
            .field("timeout", &self.timeout)
            .finish()
    }
}

impl SftpSession {
    /// Wraps an opened channel and performs the SFTP subsystem handshake.
    pub async fn connect<S>(stream: S) -> Result<Self>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        let inner = russh_sftp::client::SftpSession::new(stream)
            .await
            .map_err(|e| Error::SshError(format!("sftp handshake failed: {e}")))?;

        Ok(Self::from_session(inner))
    }

    /// Like [`SftpSession::connect`], with a sandbox root.
    pub async fn connect_with_root<S>(stream: S, root: impl AsRef<str>) -> Result<Self>
    where
        S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    {
        Self::connect(stream).await?.with_root(root)
    }

    /// Wraps an already opened session.
    pub fn from_session(inner: russh_sftp::client::SftpSession) -> Self {
        let timeout = Duration::from_secs(DEFAULT_TIMEOUT_SECS);
        // Keep the inner protocol timer just under our own budget so the client
        // surfaces a clean `Timeout` error before our outer timeout fires.
        inner.set_timeout(DEFAULT_TIMEOUT_SECS - 1);
        Self {
            inner: Arc::new(inner),
            root: None,
            timeout,
        }
    }

    /// Restricts every operation to `root` (and rejects anything outside).
    pub fn with_root(mut self, root: impl AsRef<str>) -> Result<Self> {
        let root = normalize_remote_path(root.as_ref())?;
        if !root.starts_with('/') {
            return Err(Error::PathTraversalError(format!(
                "SFTP root must be an absolute path, got {root:?}"
            )));
        }
        self.root = Some(root);
        Ok(self)
    }

    /// The sandbox root, when configured.
    pub fn root(&self) -> Option<&str> {
        self.root.as_deref()
    }

    /// The per-operation timeout.
    pub fn timeout(&self) -> Duration {
        self.timeout
    }

    /// Overrides the per-operation timeout (also updates the inner client timer).
    pub fn set_timeout(&mut self, budget: Duration) {
        self.timeout = budget;
        self.inner
            .set_timeout(budget.as_secs().saturating_sub(1).max(1));
    }

    /// Resolves a caller-supplied path against the sandbox root.
    pub fn resolve(&self, path: &str) -> Result<String> {
        sandbox_path(self.root.as_deref(), path)
    }

    /// Runs a single operation under the timeout budget.
    async fn op<T, F>(&self, label: &str, fut: F) -> Result<T>
    where
        F: Future<Output = std::result::Result<T, russh_sftp::client::error::Error>>,
    {
        match timeout(self.timeout, fut).await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(err)) => Err(Error::SshError(format!("sftp {label}: {err}"))),
            Err(_) => Err(Error::TimeoutError(format!(
                "sftp {label} timed out after {:?}",
                self.timeout
            ))),
        }
    }
}

#[cfg(test)]
mod tests;
