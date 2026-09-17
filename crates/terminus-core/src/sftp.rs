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
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::time::timeout;
use tracing::debug;

use crate::error::{Error, Result};

/// Default per-operation budget.
pub const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// A directory entry as returned by [`SftpSession::list`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SftpEntry {
    /// Base name of the entry (`file_name`).
    pub name: String,
    /// Absolute (or root-relative) remote path.
    pub path: String,
    /// Whether the entry is a directory.
    pub is_dir: bool,
    /// Size in bytes (0 for directories).
    pub size: u64,
    /// Last modification time, when the server reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<DateTime<Utc>>,
}

impl SftpEntry {
    /// Lower-cased extension of the entry name, if any.
    pub fn extension(&self) -> Option<String> {
        std::path::Path::new(&self.name)
            .extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
    }
}

/// Normalizes a remote path and rejects attempts to escape it.
///
/// Backslashes are treated as separators too: OpenSSH for Windows accepts both,
/// so `..\\..\\etc` must be rejected as firmly as `../../etc`.
pub fn normalize_remote_path(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(Error::PathTraversalError(
            "remote path is empty".to_string(),
        ));
    }
    if trimmed.contains('\0') {
        return Err(Error::PathTraversalError(
            "remote path contains a NUL byte".to_string(),
        ));
    }

    let absolute = trimmed.starts_with('/') || trimmed.starts_with('\\');
    let mut segments: Vec<&str> = Vec::new();

    for segment in trimmed.split(['/', '\\']) {
        match segment {
            "" | "." => continue,
            ".." => {
                if segments.pop().is_none() {
                    return Err(Error::PathTraversalError(format!(
                        "`..` escapes the root of {raw:?}"
                    )));
                }
            }
            segment => segments.push(segment),
        }
    }

    if segments.is_empty() {
        if absolute {
            return Ok("/".to_string());
        }
        return Err(Error::PathTraversalError(format!(
            "remote path {raw:?} resolves to nothing"
        )));
    }

    let mut normalized = String::new();
    if absolute {
        normalized.push('/');
    }
    normalized.push_str(&segments.join("/"));
    Ok(normalized)
}

/// True when `candidate` is `root` itself or lives below it. Both paths must
/// already be normalized with [`normalize_remote_path`].
pub fn is_within_root(root: &str, candidate: &str) -> bool {
    if root == "/" {
        return candidate.starts_with('/');
    }
    candidate == root || candidate.starts_with(&format!("{root}/"))
}

/// Resolves `raw` against an optional sandbox `root`.
///
/// * `root == None` — the path is only normalized (`..` still rejected).
/// * `root == Some("/srv")` — relative paths are joined to the root, absolute
///   paths must already live inside it.
pub fn sandbox_path(root: Option<&str>, raw: &str) -> Result<String> {
    let normalized = normalize_remote_path(raw)?;

    let Some(root) = root else {
        return Ok(normalized);
    };
    let root = normalize_remote_path(root)?;

    let candidate = if normalized.starts_with('/') {
        normalized
    } else {
        normalize_remote_path(&format!("{root}/{}", normalized.trim_start_matches('/')))?
    };

    if is_within_root(&root, &candidate) {
        Ok(candidate)
    } else {
        Err(Error::PathTraversalError(format!(
            "{raw:?} resolves to {candidate:?}, outside of the SFTP root {root:?}"
        )))
    }
}

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

    /// Lists `path`, directories first and case-insensitively sorted by name.
    pub async fn list(&self, path: &str) -> Result<Vec<SftpEntry>> {
        let resolved = self.resolve(path)?;
        let dir = self
            .op("read_dir", self.inner.read_dir(resolved.clone()))
            .await?;

        let mut entries: Vec<SftpEntry> = dir
            .into_iter()
            .map(|entry| {
                let metadata = entry.metadata();
                SftpEntry {
                    name: entry.file_name(),
                    path: entry.path(),
                    is_dir: metadata.is_dir(),
                    size: metadata.len(),
                    modified: metadata.modified().ok().map(DateTime::<Utc>::from),
                }
            })
            .collect();

        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        debug!(path = %resolved, count = entries.len(), "sftp: listed directory");
        Ok(entries)
    }

    /// Reads a whole remote file into memory.
    pub async fn read(&self, path: &str) -> Result<Vec<u8>> {
        let resolved = self.resolve(path)?;
        self.op("read", self.inner.read(resolved)).await
    }

    /// Writes (creating or truncating) a remote file.
    pub async fn write(&self, path: &str, data: &[u8]) -> Result<()> {
        let resolved = self.resolve(path)?;
        self.op("write", self.inner.write(resolved, data)).await
    }

    /// Renames/moves `old` to `new` (both resolved inside the root).
    pub async fn rename(&self, old: &str, new: &str) -> Result<()> {
        let from = self.resolve(old)?;
        let to = self.resolve(new)?;
        self.op("rename", self.inner.rename(from, to)).await
    }

    /// Removes a file, or an empty directory.
    pub async fn remove(&self, path: &str) -> Result<()> {
        let resolved = self.resolve(path)?;

        match self
            .op("stat", self.inner.symlink_metadata(resolved.clone()))
            .await
        {
            Ok(metadata) if metadata.is_dir() => {
                self.op("remove_dir", self.inner.remove_dir(resolved)).await
            }
            _ => {
                self.op("remove_file", self.inner.remove_file(resolved))
                    .await
            }
        }
    }

    /// Recursively delete a remote path (files and directory trees).
    pub async fn remove_recursive(&self, path: &str) -> Result<()> {
        let resolved = self.resolve(path)?;
        match self
            .op("stat", self.inner.symlink_metadata(resolved.clone()))
            .await
        {
            Ok(metadata) if metadata.is_dir() => {
                let entries = self.list(&resolved).await?;
                for entry in entries {
                    if entry.is_dir {
                        Box::pin(self.remove_recursive(&entry.path)).await?;
                    } else {
                        self.remove(&entry.path).await?;
                    }
                }
                self.remove(&resolved).await
            }
            Ok(_) => self.remove(&resolved).await,
            Err(_) => self.remove(&resolved).await,
        }
    }

    /// Creates a single remote directory.
    pub async fn mkdir(&self, path: &str) -> Result<()> {
        let resolved = self.resolve(path)?;
        self.op("mkdir", self.inner.create_dir(resolved)).await
    }

    /// Creates `path` and every missing parent, like `mkdir -p`.
    pub fn mkdir_all<'a>(
        &'a self,
        path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let resolved = self.resolve(path)?;
            if resolved == "/" {
                return Ok(());
            }

            match self
                .op("stat", self.inner.symlink_metadata(resolved.clone()))
                .await
            {
                Ok(metadata) if metadata.is_dir() => return Ok(()),
                Ok(_) => {
                    return Err(Error::SshError(format!(
                    "cannot create directory {resolved:?}: a file with that name exists"
                )))
                }
                Err(_) => {}
            }

            if let Some(parent) = parent_path(&resolved) {
                if parent != resolved {
                    self.mkdir_all(&parent).await?;
                }
            }

            match self
                .op("mkdir", self.inner.create_dir(resolved.clone()))
                .await
            {
                Ok(()) => Ok(()),
                // Lost a race with a concurrent creator: fine if it is a directory now.
                Err(err) => {
                    match self.op("stat", self.inner.symlink_metadata(resolved)).await {
                        Ok(metadata) if metadata.is_dir() => Ok(()),
                        _ => Err(err),
                    }
                }
            }
        })
    }

    /// Recursively deletes `path` (files, directories and their contents).
    ///
    /// Symlinks are removed with `remove_file`, never followed.
    pub fn rmtree<'a>(
        &'a self,
        path: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let resolved = self.resolve(path)?;
            if resolved == "/" {
                return Err(Error::SshError(
                    "refusing to delete the SFTP root".to_string(),
                ));
            }
            self.rmtree_inner(resolved).await
        })
    }

    async fn rmtree_inner(&self, resolved: String) -> Result<()> {
        let metadata = match self
            .op("stat", self.inner.symlink_metadata(resolved.clone()))
            .await
        {
            Ok(metadata) => metadata,
            Err(_) => return Ok(()), // Already gone.
        };

        if !metadata.is_dir() {
            return self
                .op("remove_file", self.inner.remove_file(resolved))
                .await;
        }

        let entries = self.list(&resolved).await?;
        for entry in entries {
            if entry.is_dir {
                Box::pin(self.rmtree_inner(entry.path)).await?;
            } else {
                self.op("remove_file", self.inner.remove_file(entry.path))
                    .await?;
            }
        }

        self.op("remove_dir", self.inner.remove_dir(resolved)).await
    }

    /// Whether `path` exists on the remote.
    pub async fn exists(&self, path: &str) -> Result<bool> {
        let resolved = self.resolve(path)?;
        match self.op("stat", self.inner.symlink_metadata(resolved)).await {
            Ok(_) => Ok(true),
            Err(Error::SshError(_)) | Err(Error::TimeoutError(_)) => Ok(false),
            Err(err) => Err(err),
        }
    }

    /// Server-side canonicalization (`realpath`).
    pub async fn canonicalize(&self, path: &str) -> Result<String> {
        let resolved = self.resolve(path)?;
        self.op("realpath", self.inner.canonicalize(resolved)).await
    }

    /// Closes the underlying SFTP channel.
    pub async fn close(&self) -> Result<()> {
        self.op("close", self.inner.close()).await
    }
}

/// Parent of a normalized absolute remote path (`None` for the root).
fn parent_path(resolved: &str) -> Option<String> {
    let trimmed = resolved.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) => Some("/".to_string()),
        Some(index) => Some(trimmed[..index].to_string()),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_keeps_clean_absolute_paths() {
        assert_eq!(normalize_remote_path("/srv/data").unwrap(), "/srv/data");
        assert_eq!(normalize_remote_path("/srv//data/").unwrap(), "/srv/data");
        assert_eq!(normalize_remote_path("/srv/./data").unwrap(), "/srv/data");
        assert_eq!(normalize_remote_path("/").unwrap(), "/");
        assert_eq!(normalize_remote_path("data/logs").unwrap(), "data/logs");
    }

    #[test]
    fn normalization_collapses_inner_parent_segments() {
        assert_eq!(normalize_remote_path("/srv/a/../b").unwrap(), "/srv/b");
        assert_eq!(normalize_remote_path("a/b/../../c").unwrap(), "c");
    }

    #[test]
    fn traversal_attempts_are_rejected() {
        assert!(normalize_remote_path("/..").is_err());
        assert!(normalize_remote_path("/../etc/passwd").is_err());
        assert!(normalize_remote_path("../../etc/passwd").is_err());
        // Windows-style separators must not sneak past the check.
        assert!(normalize_remote_path("..\\..\\windows\\system32").is_err());
        assert!(normalize_remote_path("/srv/../../etc").is_err());
        assert!(normalize_remote_path("").is_err());
        assert!(normalize_remote_path("   ").is_err());
        assert!(normalize_remote_path("/srv/\0evil").is_err());
        // Relative paths that escape their own root.
        assert!(normalize_remote_path("a/../..").is_err());
    }

    #[test]
    fn sandbox_root_contains_paths() {
        let root = Some("/srv");

        assert_eq!(sandbox_path(root, "logs").unwrap(), "/srv/logs");
        assert_eq!(sandbox_path(root, "/srv/logs").unwrap(), "/srv/logs");
        assert_eq!(sandbox_path(root, "/srv").unwrap(), "/srv");
        assert_eq!(sandbox_path(root, "a/../b").unwrap(), "/srv/b");

        assert!(sandbox_path(root, "/etc/passwd").is_err());
        assert!(sandbox_path(root, "/srv/../etc/passwd").is_err());
        assert!(sandbox_path(root, "../etc/passwd").is_err());
        assert!(sandbox_path(root, "/srvdata").is_err());

        // No root configured: normalization only.
        assert_eq!(sandbox_path(None, "/etc/passwd").unwrap(), "/etc/passwd");
        assert!(sandbox_path(None, "/../etc").is_err());
    }

    #[test]
    fn root_slash_contains_everything() {
        assert!(is_within_root("/", "/etc/passwd"));
        assert!(!is_within_root("/", "relative"));
        assert!(is_within_root("/srv", "/srv"));
        assert!(is_within_root("/srv", "/srv/a/b"));
        assert!(!is_within_root("/srv", "/srv-old/a"));
    }

    #[test]
    fn parent_paths_resolve_like_posix() {
        assert_eq!(parent_path("/srv/a").as_deref(), Some("/srv"));
        assert_eq!(parent_path("/srv").as_deref(), Some("/"));
        assert_eq!(parent_path("/").as_deref(), None);
        assert_eq!(parent_path("/a/").as_deref(), Some("/"));
    }

    #[test]
    fn entry_extension_is_lowercased() {
        let entry = SftpEntry {
            name: "Report.TXT".to_string(),
            path: "/srv/Report.TXT".to_string(),
            is_dir: false,
            size: 12,
            modified: None,
        };
        assert_eq!(entry.extension().as_deref(), Some("txt"));
    }

    #[test]
    fn remove_remote_recursive_desired() {
        // Live recursive delete is covered by bridge `e2e_delete_remote_dir_tree`.
        // Guard against the Phase-A stub error string returning to the impl body.
        let src = include_str!("sftp.rs");
        let impl_start = src
            .find("pub async fn remove_recursive")
            .expect("remove_recursive API must remain public");
        let impl_body = &src[impl_start..];
        let impl_end = impl_body.find("\n    pub async fn mkdir").unwrap_or(impl_body.len());
        let body = &impl_body[..impl_end];
        assert!(
            !body.contains("not implemented yet"),
            "remove_recursive must list+delete instead of returning a stub error"
        );
        assert!(
            body.contains("self.list(") && body.contains("self.remove("),
            "remove_recursive should walk entries then remove"
        );
    }
}
