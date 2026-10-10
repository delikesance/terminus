use tokio::time::timeout;

use crate::error::{Error, Result};

use super::transfer::{temp_sibling, TRANSFER_CHUNK};
use super::*;

impl SftpSession {
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
    pub(super) async fn read_into<W, F>(
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
    pub(super) async fn write_from<R, F>(
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
    pub(super) async fn stream_into<R, F>(
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
    pub(super) async fn stream_to_file<R, F>(
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
    pub(super) async fn replace(&self, temp: &str, target: &str) -> Result<()> {
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
    pub(super) async fn close_file(
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
}
