use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
