use tokio::time::timeout;

use crate::error::{Error, Result};

use super::*;

/// Chunk size for streamed transfers (each chunk has its own timeout).
pub(super) const TRANSFER_CHUNK: usize = 256 * 1024;

/// Hidden temp sibling of a resolved remote path: `/dir/.name.terminus-<tag>-<id>`.
pub(super) fn temp_sibling(target: &str, tag: &str) -> String {
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

impl SftpSession {
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
}
