//! Local filesystem access for the **local** pane of the bidirectional file
//! transfer. Unlike the sandboxed SFTP module, these operate on real OS paths
//! the user has explicitly chosen (native folder picker / path bar). No `..`
//! sandbox here — this is the user's own machine.
//!
//! List/remove/rename are synchronous (fast); read/write/mkdir are async so
//! large file transfers never block the Tauri runtime.

use crate::error::{Error, Result};
use crate::models::LocalEntry;
use std::path::Path;
use std::time::SystemTime;

/// Resolve the user's home directory (default local pane root).
pub fn local_home() -> Result<String> {
    dirs::home_dir()
        .map(|p| p.to_string_lossy().into_owned())
        .ok_or_else(|| Error::msg("could not resolve home directory"))
}

fn modified_ms(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn to_entry(name: &str, path: &Path, meta: &std::fs::Metadata) -> LocalEntry {
    LocalEntry {
        name: name.to_string(),
        path: path.to_string_lossy().into_owned(),
        is_dir: meta.is_dir(),
        size: meta.len(),
        modified: modified_ms(meta),
    }
}

/// List a local directory, directories first then name (case-insensitive).
pub fn local_list(path: &str) -> Result<Vec<LocalEntry>> {
    let dir = Path::new(path);
    let mut out = Vec::new();
    for item in std::fs::read_dir(dir).map_err(|e| Error::msg(format!("list {path}: {e}")))? {
        let item = item.map_err(|e| Error::msg(format!("list {path}: {e}")))?;
        let name = item.file_name().to_string_lossy().into_owned();
        if name == "." || name == ".." {
            continue;
        }
        let meta = item
            .metadata()
            .map_err(|e| Error::msg(format!("stat {name}: {e}")))?;
        out.push(to_entry(&name, &item.path(), &meta));
    }
    out.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(out)
}

/// Read a local file into memory (used by download/transfer).
pub async fn local_read(path: &str) -> Result<Vec<u8>> {
    tokio::fs::read(path)
        .await
        .map_err(|e| Error::msg(format!("read {path}: {e}")))
}

/// Write bytes to a local file, creating parent directories as needed.
pub async fn local_write(path: &str, data: &[u8]) -> Result<()> {
    if let Some(parent) = Path::new(path).parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| Error::msg(format!("mkdir {}: {e}", parent.display())))?;
    }
    tokio::fs::write(path, data)
        .await
        .map_err(|e| Error::msg(format!("write {path}: {e}")))
}

/// Recursively create a local directory.
pub async fn local_mkdir(path: &str) -> Result<()> {
    tokio::fs::create_dir_all(path)
        .await
        .map_err(|e| Error::msg(format!("mkdir {path}: {e}")))
}

/// Remove a local file or directory. Directories are removed recursively.
pub fn local_remove(path: &str, is_dir: bool) -> Result<()> {
    if is_dir {
        std::fs::remove_dir_all(path).map_err(|e| Error::msg(format!("remove dir {path}: {e}")))
    } else {
        std::fs::remove_file(path).map_err(|e| Error::msg(format!("remove {path}: {e}")))
    }
}

/// Rename / move a local file or directory.
pub fn local_rename(from: &str, to: &str) -> Result<()> {
    std::fs::rename(from, to).map_err(|e| Error::msg(format!("rename {from} -> {to}: {e}")))
}
