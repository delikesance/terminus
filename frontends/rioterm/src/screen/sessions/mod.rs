//! `Screen` sessions surface, split out of `screen/mod.rs`.

mod connecting;
mod groups;
mod lost;
mod open;
mod probe;
mod split;
mod tabs;
#[cfg(test)]
mod tests;

use crate::hosts;

/// What the pane a split opens should run.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum SplitTarget {
    /// The app's own shell, as a split always did.
    Local,
    /// The session of this sidebar row (an ssh host or a WSL distro): the
    /// same launch as opening that row again.
    Row(String),
}

/// Which session a split of a pane opened from `host_id` should start.
pub(super) fn split_target(host_id: Option<&str>) -> SplitTarget {
    match host_id {
        None => SplitTarget::Local,
        Some(id) if id == hosts::LOCAL_ID => SplitTarget::Local,
        Some(id) => SplitTarget::Row(id.to_string()),
    }
}
