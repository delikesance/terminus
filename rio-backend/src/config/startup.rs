use serde::{Deserialize, Serialize};

/// `[startup]`: what happens when Terminus launches.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
pub struct Startup {
    /// Bring back the tabs that were open when Terminus last closed.
    #[serde(default, rename = "restore-tabs")]
    pub restore_tabs: bool,
}
