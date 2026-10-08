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

/// Chunk size for streamed transfers (each chunk has its own timeout).
const TRANSFER_CHUNK: usize = 256 * 1024;

/// Hidden temp sibling of a resolved remote path: `/dir/.name.terminus-<tag>-<id>`.
fn temp_sibling(target: &str, tag: &str) -> String {
    let (dir, name) = match target.rsplit_once('/') {
        Some((dir, name)) => (dir, name),
        None => ("", target),
    };
    let id = &uuid::Uuid::new_v4().simple().to_string()[..8];
    let leaf = format!(".{name}.terminus-{tag}-{id}");
    if dir.is_empty() && target.starts_with('/') {
        format!("/{leaf}")
    } else if dir.is_empty() {
        leaf
    } else {
        format!("{dir}/{leaf}")
    }
}

/// Normalizes a remote path and rejects attempts to escape it.
///
/// Names are kept byte-for-byte: POSIX file names may start or end with
/// spaces and may contain `\\`, so neither is trimmed nor split in a `/` path.
/// A path without any `/` that uses `\\` is a Windows-style path (OpenSSH for
/// Windows accepts both separators) and is split on `\\`. Either way `..`
/// hidden behind a backslash (`..\\..\\etc`) is rejected as traversal.
pub fn normalize_remote_path(raw: &str) -> Result<String> {
    if raw.trim().is_empty() {
        return Err(Error::PathTraversalError(
            "remote path is empty".to_string(),
        ));
    }
    if raw.contains('\0') {
        return Err(Error::PathTraversalError(
            "remote path contains a NUL byte".to_string(),
        ));
    }

    let windows_style = !raw.contains('/') && raw.contains('\\');
    let path = if windows_style {
        std::borrow::Cow::Owned(raw.replace('\\', "/"))
    } else {
        std::borrow::Cow::Borrowed(raw)
    };
    let absolute = path.starts_with('/');
    let mut segments: Vec<&str> = Vec::new();

    for segment in path.split('/') {
        match segment {
            "" | "." => continue,
            ".." => {
                if segments.pop().is_none() {
                    return Err(Error::PathTraversalError(format!(
                        "`..` escapes the root of {raw:?}"
                    )));
                }
            }
            segment if segment.split('\\').any(|part| part == "..") => {
                return Err(Error::PathTraversalError(format!(
                    "backslash `..` traversal in {raw:?}"
                )));
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

        let mut entries: Vec<SftpEntry> = Vec::new();
        for entry in dir {
            let mut metadata = entry.metadata();
            // readdir reports lstat attributes: resolve symlinks so a link to
            // a directory browses like one (a dangling link stays a file).
            if metadata.is_symlink() {
                if let Ok(Ok(target)) =
                    timeout(self.timeout, self.inner.metadata(entry.path())).await
                {
                    metadata = target;
                }
            }
            entries.push(SftpEntry {
                name: entry.file_name(),
                path: entry.path(),
                is_dir: metadata.is_dir(),
                size: metadata.len(),
                modified: metadata.modified().ok().map(DateTime::<Utc>::from),
            });
        }

        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        debug!(path = %resolved, count = entries.len(), "sftp: listed directory");
        Ok(entries)
    }

    /// Reads a whole remote file into memory (small files: edit, probes).
    ///
    /// Each chunk read has its own [`Self::timeout`] budget, so a slow link
    /// only fails when it actually stalls. Always awaits an explicit handle
    /// close: relying on [`Drop`] alone uses `close_nowait`, which races under
    /// rapid sequential opens and trips the server's SFTP handle limit.
    pub async fn read(&self, path: &str) -> Result<Vec<u8>> {
        let mut buffer = Vec::new();
        self.read_into(path, &mut buffer, |_| {}).await?;
        Ok(buffer)
    }

    /// Stream `path` into `sink`, one timed chunk at a time.
    async fn read_into<W, F>(
        &self,
        path: &str,
        sink: &mut W,
        mut on_chunk: F,
    ) -> Result<u64>
    where
        W: tokio::io::AsyncWrite + Unpin,
        F: FnMut(u64),
    {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut file = self.open_read(path).await?;
        let mut buf = vec![0u8; TRANSFER_CHUNK];
        let mut total = 0u64;
        loop {
            let n = match timeout(self.timeout, file.read(&mut buf)).await {
                Ok(Ok(0)) => break,
                Ok(Ok(n)) => n,
                Ok(Err(err)) => return Err(Error::SshError(format!("sftp read: {err}"))),
                Err(_) => {
                    return Err(Error::TimeoutError(format!(
                        "sftp read stalled for {:?}",
                        self.timeout
                    )))
                }
            };
            sink.write_all(&buf[..n])
                .await
                .map_err(|e| Error::IoError(e.to_string()))?;
            total = total.saturating_add(n as u64);
            on_chunk(total);
        }
        self.close_file(file, "read").await?;
        Ok(total)
    }

    /// Writes (creating or replacing) a remote file.
    ///
    /// The data lands in a temporary sibling first and replaces `path` only
    /// once complete, so a failed or stalled write never leaves a truncated
    /// file behind. Each chunk has its own timeout budget.
    pub async fn write(&self, path: &str, data: &[u8]) -> Result<()> {
        let mut reader = data;
        self.write_from(path, &mut reader, |_| {}).await.map(|_| ())
    }

    /// Stream `source` into `path` via a temp sibling, then swap it in.
    ///
    /// The temp file inherits the target's permission bits. A symlinked
    /// target, or a directory where no new file can be created, is written in
    /// place instead (still chunked with per-chunk timeouts), so the link or
    /// the file's identity survives.
    async fn write_from<R, F>(
        &self,
        path: &str,
        source: &mut R,
        on_chunk: F,
    ) -> Result<u64>
    where
        R: tokio::io::AsyncRead + Unpin,
        F: FnMut(u64),
    {
        let target = self.resolve(path)?;
        let existing = timeout(self.timeout, self.inner.symlink_metadata(target.clone()))
            .await
            .ok()
            .and_then(|r| r.ok());
        if existing.as_ref().is_some_and(|m| m.is_symlink()) {
            return self.stream_into(&target, source, on_chunk).await;
        }
        let temp = temp_sibling(&target, "part");
        let file = match self.op("create", self.inner.create(temp.clone())).await {
            Ok(file) => file,
            // Writable file in a directory we cannot add to: in place.
            Err(_) => return self.stream_into(&target, source, on_chunk).await,
        };
        let copied = self.stream_to_file(file, source, on_chunk).await;
        let copied = match copied {
            Ok(n) => n,
            Err(err) => {
                let _ = timeout(self.timeout, self.inner.remove_file(temp)).await;
                return Err(err);
            }
        };
        if let Some(mode) = existing.as_ref().and_then(|m| m.permissions) {
            let mut attrs = russh_sftp::protocol::FileAttributes::empty();
            attrs.permissions = Some(mode & 0o7777);
            let _ =
                timeout(self.timeout, self.inner.set_metadata(temp.clone(), attrs)).await;
        }
        if let Err(err) = self.replace(&temp, &target).await {
            let _ = timeout(self.timeout, self.inner.remove_file(temp)).await;
            return Err(err);
        }
        Ok(copied)
    }

    /// Truncate `target` and stream into it directly (no temp sibling).
    async fn stream_into<R, F>(
        &self,
        target: &str,
        source: &mut R,
        on_chunk: F,
    ) -> Result<u64>
    where
        R: tokio::io::AsyncRead + Unpin,
        F: FnMut(u64),
    {
        let file = self
            .op("create", self.inner.create(target.to_string()))
            .await?;
        self.stream_to_file(file, source, on_chunk).await
    }

    /// Copy `source` into an open remote `file` in timed chunks, then close it.
    async fn stream_to_file<R, F>(
        &self,
        mut file: russh_sftp::client::fs::File,
        source: &mut R,
        mut on_chunk: F,
    ) -> Result<u64>
    where
        R: tokio::io::AsyncRead + Unpin,
        F: FnMut(u64),
    {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let mut buf = vec![0u8; TRANSFER_CHUNK];
        let mut total = 0u64;
        let written: Result<()> = async {
            loop {
                let n = source
                    .read(&mut buf)
                    .await
                    .map_err(|e| Error::IoError(e.to_string()))?;
                if n == 0 {
                    break;
                }
                match timeout(self.timeout, file.write_all(&buf[..n])).await {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) => {
                        return Err(Error::SshError(format!("sftp write: {err}")))
                    }
                    Err(_) => {
                        return Err(Error::TimeoutError(format!(
                            "sftp write stalled for {:?}",
                            self.timeout
                        )))
                    }
                }
                total = total.saturating_add(n as u64);
                on_chunk(total);
            }
            match timeout(self.timeout, file.flush()).await {
                Ok(Ok(())) => Ok(()),
                Ok(Err(err)) => Err(Error::SshError(format!("sftp write: {err}"))),
                Err(_) => Err(Error::TimeoutError(format!(
                    "sftp write stalled for {:?}",
                    self.timeout
                ))),
            }
        }
        .await;
        let closed = self.close_file(file, "write").await;
        written.and(closed).map(|()| total)
    }

    /// Move the finished `temp` over `target` (already resolved paths).
    ///
    /// SFTP v3 `rename` refuses an existing target, so the old file is moved
    /// aside first and restored if the swap fails; it is only deleted once
    /// the new file is in place.
    async fn replace(&self, temp: &str, target: &str) -> Result<()> {
        let exists = matches!(
            timeout(self.timeout, self.inner.try_exists(target.to_string())).await,
            Ok(Ok(true))
        );
        if !exists {
            return self
                .op(
                    "rename",
                    self.inner.rename(temp.to_string(), target.to_string()),
                )
                .await;
        }
        let backup = temp_sibling(target, "old");
        self.op(
            "rename",
            self.inner.rename(target.to_string(), backup.clone()),
        )
        .await?;
        if let Err(err) = self
            .op(
                "rename",
                self.inner.rename(temp.to_string(), target.to_string()),
            )
            .await
        {
            let _ = self
                .op("rename", self.inner.rename(backup, target.to_string()))
                .await;
            return Err(err);
        }
        let _ = self.op("remove", self.inner.remove_file(backup)).await;
        Ok(())
    }

    /// Close a handle explicitly (see [`Self::read`] on why not `Drop`).
    async fn close_file(
        &self,
        file: russh_sftp::client::fs::File,
        label: &str,
    ) -> Result<()> {
        match timeout(self.timeout, file.close()).await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(err)) => Err(Error::SshError(format!("sftp {label} close: {err}"))),
            Err(_) => Err(Error::TimeoutError(format!(
                "sftp {label} close timed out after {:?}",
                self.timeout
            ))),
        }
    }

    /// Upload local `from` to remote `to` in timed chunks via a temp sibling.
    /// `on_progress(done, total)` fires after every chunk.
    pub async fn upload_file<F>(
        &self,
        from: &std::path::Path,
        to: &str,
        mut on_progress: F,
    ) -> Result<u64>
    where
        F: FnMut(u64, u64),
    {
        let mut file = tokio::fs::File::open(from)
            .await
            .map_err(|e| Error::IoError(format!("{}: {e}", from.display())))?;
        let total = file.metadata().await.map(|m| m.len()).unwrap_or(0);
        on_progress(0, total);
        self.write_from(to, &mut file, |done| on_progress(done, total))
            .await
    }

    /// Download remote `from` to local `to` in timed chunks. The data lands in
    /// a temp sibling and replaces `to` only when complete.
    pub async fn download_file<F>(
        &self,
        from: &str,
        to: &std::path::Path,
        mut on_progress: F,
    ) -> Result<u64>
    where
        F: FnMut(u64, u64),
    {
        use tokio::io::AsyncWriteExt;

        let resolved = self.resolve(from)?;
        let total = match timeout(self.timeout, self.inner.metadata(resolved)).await {
            Ok(Ok(meta)) => meta.len(),
            _ => 0,
        };
        on_progress(0, total);
        if let Some(parent) = to.parent().filter(|p| !p.as_os_str().is_empty()) {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| Error::IoError(format!("{}: {e}", parent.display())))?;
        }
        let name = to
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "download".into());
        let temp = to.with_file_name(format!(
            ".{name}.terminus-part-{}",
            &uuid::Uuid::new_v4().simple().to_string()[..8]
        ));
        let result: Result<u64> = async {
            let mut file = tokio::fs::File::create(&temp)
                .await
                .map_err(|e| Error::IoError(format!("{}: {e}", temp.display())))?;
            let copied = self
                .read_into(from, &mut file, |done| on_progress(done, total))
                .await?;
            file.flush()
                .await
                .map_err(|e| Error::IoError(e.to_string()))?;
            file.sync_all()
                .await
                .map_err(|e| Error::IoError(e.to_string()))?;
            drop(file);
            match tokio::fs::symlink_metadata(to).await {
                // A symlinked destination keeps its link: copy through it.
                Ok(meta) if meta.file_type().is_symlink() => {
                    tokio::fs::copy(&temp, to)
                        .await
                        .map_err(|e| Error::IoError(format!("{}: {e}", to.display())))?;
                    let _ = tokio::fs::remove_file(&temp).await;
                }
                other => {
                    if let Ok(meta) = other {
                        let _ =
                            tokio::fs::set_permissions(&temp, meta.permissions()).await;
                    }
                    tokio::fs::rename(&temp, to)
                        .await
                        .map_err(|e| Error::IoError(format!("{}: {e}", to.display())))?;
                }
            }
            Ok(copied)
        }
        .await;
        if result.is_err() {
            let _ = tokio::fs::remove_file(&temp).await;
        }
        result
    }

    /// Opens `path` for streaming reads (`AsyncRead`).
    ///
    /// Prefer this over [`Self::read`] for large files or Host→Host copies.
    /// OpenSSH follows symlinks on open/read, so npm `.bin` links yield file bytes.
    pub async fn open_read(&self, path: &str) -> Result<russh_sftp::client::fs::File> {
        let resolved = self.resolve(path)?;
        self.op("open", self.inner.open(resolved)).await
    }

    /// Creates/truncates `path` for streaming writes (`AsyncWrite`).
    pub async fn create_write(&self, path: &str) -> Result<russh_sftp::client::fs::File> {
        let resolved = self.resolve(path)?;
        self.op("create", self.inner.create(resolved)).await
    }

    /// Stream-copy `from_path` on `self` to `to_path` on `dest` in chunks.
    ///
    /// Each read/write is bounded by [`Self::timeout`]; the overall copy may
    /// run longer than one budget (progress via `on_progress(bytes_copied)`).
    /// The destination is replaced only once the copy completed.
    pub async fn copy_to<F>(
        &self,
        from_path: &str,
        dest: &Self,
        to_path: &str,
        on_progress: F,
    ) -> Result<u64>
    where
        F: FnMut(u64),
    {
        // Relay through a bounded in-memory pipe: the reader side fills it
        // chunk by chunk while `write_from` drains it into the temp file.
        let (mut tx, mut rx) = tokio::io::duplex(TRANSFER_CHUNK * 4);
        let read = async {
            let copied = self.read_into(from_path, &mut tx, |_| {}).await;
            drop(tx);
            copied
        };
        let write = dest.write_from(to_path, &mut rx, on_progress);
        let (read, written) = tokio::join!(read, write);
        read?;
        written
    }

    /// The login directory (`realpath(".")` on the server), where a freshly
    /// opened pane should start.
    pub async fn home_dir(&self) -> Result<String> {
        let home = self.op("realpath", self.inner.canonicalize(".")).await?;
        match &self.root {
            Some(root) if !is_within_root(root, &home) => Ok(root.clone()),
            _ => Ok(home),
        }
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
    fn normalization_keeps_edge_whitespace_and_literal_backslashes() {
        // POSIX file names may end in spaces or contain `\`.
        assert_eq!(
            normalize_remote_path("/home/u/trail ").unwrap(),
            "/home/u/trail "
        );
        assert_eq!(
            normalize_remote_path("/home/u/ lead").unwrap(),
            "/home/u/ lead"
        );
        assert_eq!(
            normalize_remote_path("/home/u/a\\b.txt").unwrap(),
            "/home/u/a\\b.txt"
        );
        // Windows-style paths (no `/`) still use `\` as the separator.
        assert_eq!(normalize_remote_path("\\Users\\me").unwrap(), "/Users/me");
        // …and `..` hidden behind backslashes is still traversal.
        assert!(normalize_remote_path("/srv/..\\etc").is_err());
        assert!(normalize_remote_path("/srv/x\\..\\..\\etc").is_err());
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
        let impl_end = impl_body
            .find("\n    pub async fn mkdir")
            .unwrap_or(impl_body.len());
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
