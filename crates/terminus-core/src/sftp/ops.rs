use std::future::Future;
use std::pin::Pin;

use chrono::{DateTime, Utc};
use tokio::time::timeout;
use tracing::debug;

use crate::error::{Error, Result};

use super::*;

impl SftpSession {
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
        let stat = timeout(self.timeout, self.inner.symlink_metadata(resolved.clone()))
            .await
            .map_err(|_| {
                Error::TimeoutError(format!(
                    "sftp stat timed out after {:?}",
                    self.timeout
                ))
            })?;
        match stat {
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
            Err(err) if is_no_such_file(&err) => Ok(()),
            Err(err) => Err(Error::SshError(format!("sftp stat: {err}"))),
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
pub(super) fn parent_path(resolved: &str) -> Option<String> {
    let trimmed = resolved.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(0) => Some("/".to_string()),
        Some(index) => Some(trimmed[..index].to_string()),
        None => None,
    }
}

pub(super) fn is_no_such_file(err: &russh_sftp::client::error::Error) -> bool {
    matches!(
        err,
        russh_sftp::client::error::Error::Status(status)
            if status.status_code == russh_sftp::protocol::StatusCode::NoSuchFile
    )
}
