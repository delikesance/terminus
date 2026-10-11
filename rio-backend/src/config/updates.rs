use crate::config::defaults::default_bool_true;
use serde::{Deserialize, Serialize};

/// `[updates]`: Terminus self-update.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub struct Updates {
    /// Look for a new release at startup and once a day.
    #[serde(default = "default_bool_true")]
    pub check: bool,
    /// Install automatically when that needs no prompt (portable installs);
    /// installer- and package-based installs always ask first.
    #[serde(default = "default_bool_true", rename = "auto-install")]
    pub auto_install: bool,
}

impl Default for Updates {
    fn default() -> Self {
        Self {
            check: true,
            auto_install: true,
        }
    }
}
