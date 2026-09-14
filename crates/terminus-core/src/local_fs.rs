//! Local filesystem helpers for the SFTP dual-pane browser (left pane).
//!
//! Everything is async (`tokio::fs`) so a slow network mount cannot block the
//! terminal event loop.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::warn;

use crate::error::{Error, Result};

/// A local directory entry, shaped to line up with
/// [`SftpEntry`](crate::sftp::SftpEntry) so both panes render from the same data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalEntry {
    /// Base name of the entry (`file_name`).
    pub name: String,
    /// Full path on this machine.
    pub path: PathBuf,
    /// Whether the entry is a directory (symlinks are resolved).
    pub is_dir: bool,
    /// Size in bytes (0 for directories).
    pub size: u64,
    /// Last modification time, when the platform reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<DateTime<Utc>>,
}

impl LocalEntry {
    /// Lower-cased extension of the entry name, if any.
    pub fn extension(&self) -> Option<String> {
        Path::new(&self.name)
            .extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
    }

    /// The entry's parent directory.
    pub fn parent(&self) -> Option<&Path> {
        self.path.parent()
    }
}

/// Lists `path` (directories first, then case-insensitive by name).
///
/// Entries that disappear mid-listing (common on `/proc`, or when a build is
/// running) are skipped with a warning instead of failing the whole listing.
pub async fn list_local_dir(path: impl AsRef<Path>) -> Result<Vec<LocalEntry>> {
    let dir = path.as_ref();
    let mut reader = fs::read_dir(dir).await.map_err(|err| {
        Error::FileSystemError(format!("cannot read directory {}: {err}", dir.display()))
    })?;

    let mut entries: Vec<LocalEntry> = Vec::new();

    loop {
        match reader.next_entry().await {
            Ok(Some(entry)) => {
                let entry_path = entry.path();
                // `metadata()` follows symlinks; fall back to the dirent type
                // when the target is gone.
                let (is_dir, size, modified) = match entry.metadata().await {
                    Ok(metadata) => (
                        metadata.is_dir(),
                        metadata.len(),
                        metadata.modified().ok().map(DateTime::<Utc>::from),
                    ),
                    Err(_) => (
                        entry
                            .file_type()
                            .await
                            .map(|file_type| file_type.is_dir())
                            .unwrap_or(false),
                        0,
                        None,
                    ),
                };

                entries.push(LocalEntry {
                    name: entry.file_name().to_string_lossy().into_owned(),
                    path: entry_path,
                    is_dir,
                    size,
                    modified,
                });
            }
            Ok(None) => break,
            Err(err) => {
                warn!(directory = %dir.display(), error = %err, "local_fs: skipping unreadable entry");
                break;
            }
        }
    }

    sort_entries(&mut entries);
    Ok(entries)
}

/// Sorts entries the way both panes display them.
pub fn sort_entries(entries: &mut [LocalEntry]) {
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

/// Stats a single path without following the listing shortcut.
pub async fn local_entry(path: impl AsRef<Path>) -> Result<LocalEntry> {
    let path = path.as_ref();
    let metadata = fs::metadata(path).await.map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            Error::NotFoundError(format!("{}", path.display()))
        } else {
            Error::FileSystemError(format!("cannot stat {}: {err}", path.display()))
        }
    })?;

    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());

    Ok(LocalEntry {
        name,
        path: path.to_path_buf(),
        is_dir: metadata.is_dir(),
        size: metadata.len(),
        modified: metadata.modified().ok().map(DateTime::<Utc>::from),
    })
}

/// Reads a local file (used for uploads to the remote pane).
pub async fn read_local_file(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    let path = path.as_ref();
    fs::read(path).await.map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            Error::NotFoundError(format!("{}", path.display()))
        } else {
            Error::FileSystemError(format!("cannot read {}: {err}", path.display()))
        }
    })
}

/// Writes a local file (used for downloads from the remote pane), creating
/// missing parent directories.
pub async fn write_local_file(path: impl AsRef<Path>, data: &[u8]) -> Result<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).await.map_err(|err| {
                Error::FileSystemError(format!(
                    "cannot create directory {}: {err}",
                    parent.display()
                ))
            })?;
        }
    }

    fs::write(path, data).await.map_err(|err| {
        Error::FileSystemError(format!("cannot write {}: {err}", path.display()))
    })
}

/// Creates `path` and its parents (`mkdir -p`).
pub async fn create_dir_all(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    fs::create_dir_all(path).await.map_err(|err| {
        Error::FileSystemError(format!("cannot create {}: {err}", path.display()))
    })
}

/// Removes a file, a symlink, or a directory (`recursive` controls whether a
/// non-empty directory is deleted too).
pub async fn remove_local_path(path: impl AsRef<Path>, recursive: bool) -> Result<()> {
    let path = path.as_ref();
    // `symlink_metadata` avoids following a symlink out of the intended path.
    let metadata = fs::symlink_metadata(path).await.map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            Error::NotFoundError(format!("{}", path.display()))
        } else {
            Error::FileSystemError(format!("cannot stat {}: {err}", path.display()))
        }
    })?;

    let result = if metadata.is_dir() {
        if recursive {
            fs::remove_dir_all(path).await
        } else {
            fs::remove_dir(path).await
        }
    } else {
        fs::remove_file(path).await
    };

    result.map_err(|err| {
        Error::FileSystemError(format!("cannot remove {}: {err}", path.display()))
    })
}

/// The current user's home directory.
pub fn home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

/// The platform's default directory shown when the pane opens.
pub fn default_local_root() -> PathBuf {
    home_dir().unwrap_or_else(|| PathBuf::from("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unique scratch directory for a test.
    async fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("terminus-core-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).await.expect("create scratch dir");
        dir
    }

    #[tokio::test]
    async fn lists_directories_first_then_files() {
        let dir = scratch_dir("list").await;

        fs::create_dir(dir.join("zeta-dir")).await.unwrap();
        fs::write(dir.join("alpha.txt"), b"hello").await.unwrap();
        fs::create_dir(dir.join("Alpha-dir")).await.unwrap();
        fs::write(dir.join("Zebra.txt"), b"world!").await.unwrap();

        let entries = list_local_dir(&dir).await.expect("list");
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["Alpha-dir", "zeta-dir", "alpha.txt", "Zebra.txt"]
        );

        let file = entries.iter().find(|e| e.name == "alpha.txt").unwrap();
        assert!(!file.is_dir);
        assert_eq!(file.size, 5);
        assert!(file.modified.is_some());
        assert_eq!(file.extension().as_deref(), Some("txt"));

        let sub = entries.iter().find(|e| e.name == "zeta-dir").unwrap();
        assert!(sub.is_dir);
        assert_eq!(sub.parent(), Some(dir.as_path()));

        remove_local_path(&dir, true).await.expect("cleanup");
    }

    #[tokio::test]
    async fn missing_directory_reports_a_filesystem_error() {
        let missing = std::env::temp_dir().join("terminus-core-does-not-exist");
        let err = list_local_dir(&missing).await.expect_err("must fail");
        assert!(matches!(err, Error::FileSystemError(_)));
    }

    #[tokio::test]
    async fn stat_read_write_roundtrip() {
        let dir = scratch_dir("io").await;
        let nested = dir.join("nested/deeply/file.bin");

        // Parent directories are created on write.
        write_local_file(&nested, b"payload").await.expect("write");
        assert_eq!(read_local_file(&nested).await.expect("read"), b"payload");

        let entry = local_entry(&nested).await.expect("stat");
        assert_eq!(entry.name, "file.bin");
        assert!(!entry.is_dir);
        assert_eq!(entry.size, 7);

        remove_local_path(&nested, false)
            .await
            .expect("remove file");
        let err = local_entry(&nested).await.expect_err("gone");
        assert!(matches!(err, Error::NotFoundError(_)));

        remove_local_path(&dir, true).await.expect("cleanup");
    }

    #[tokio::test]
    async fn non_recursive_removal_refuses_non_empty_directories() {
        let dir = scratch_dir("rmdir").await;
        let inner = dir.join("inner");
        create_dir_all(&inner).await.expect("mkdir");
        write_local_file(inner.join("keep.txt"), b"keep")
            .await
            .unwrap();

        let err = remove_local_path(&dir, false).await.expect_err("must fail");
        assert!(matches!(err, Error::FileSystemError(_)));

        remove_local_path(&dir, true)
            .await
            .expect("recursive removal");
        assert!(!dir.exists());
    }

    #[test]
    fn default_root_is_a_directory() {
        let root = default_local_root();
        assert!(root.is_absolute());
    }
}
