//! The open tabs, kept on disk so the next launch can reopen them.
//!
//! Only what it takes to start each session again is kept: where it runs
//! (`host_id`, the same row id the sidebar uses: `local`, `wsl:<distro>` or
//! a stored host's id), the name the user gave the tab, and for local shells
//! the directory they were in. Scrollback and running programs are not.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// File name under the Terminus data dir.
pub const FILE_NAME: &str = "tabs.json";

/// Bumped when the format changes in a way an older build can't read.
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedTab {
    pub host_id: String,
    /// The name the user gave the tab; `None` keeps the default label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Working directory of a local shell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// The pinned "This computer" tab, which every launch opens anyway: only
    /// its name is restored.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub home: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedTabs {
    pub version: u32,
    /// Index into `tabs` of the tab that was in front.
    #[serde(default)]
    pub active: usize,
    #[serde(default)]
    pub tabs: Vec<SavedTab>,
}

impl SavedTabs {
    pub fn new(tabs: Vec<SavedTab>, active: usize) -> Self {
        Self {
            version: VERSION,
            active,
            tabs,
        }
    }

    /// Keep tabs that could not be reopened yet (vault locked) in the list,
    /// after the open ones so `active` still points at the same tab.
    pub fn with_unrestored(mut self, unrestored: &[SavedTab]) -> Self {
        self.tabs.extend_from_slice(unrestored);
        self
    }

    /// Read the saved tabs. A missing, unreadable or newer file is treated
    /// as "nothing to restore": a launch never fails over it.
    pub fn load(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        Self::decode(&text)
    }

    pub fn decode(text: &str) -> Option<Self> {
        let saved: Self = serde_json::from_str(text).ok()?;
        if saved.version == 0 || saved.version > VERSION {
            return None;
        }
        Some(saved)
    }

    pub fn encode(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Write through a temporary file and a rename, so a crash mid-write
    /// leaves the previous file rather than half of a new one.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, self.encode())?;
        std::fs::rename(&tmp, path)
    }

    /// Name given to the home tab, if any.
    pub fn home_title(&self) -> Option<&str> {
        self.tabs
            .iter()
            .find(|tab| tab.home)
            .and_then(|tab| tab.title.as_deref())
    }

    /// Tabs to open again, in order, each with its index in `tabs`.
    pub fn to_open(&self) -> impl Iterator<Item = (usize, &SavedTab)> {
        self.tabs
            .iter()
            .enumerate()
            .filter(|(_, tab)| !tab.home && !tab.host_id.is_empty())
    }
}

/// The name worth saving for a tab: only one the user chose. A tab still
/// showing its host's own label saves nothing, so renaming the host later
/// renames the restored tab too.
pub fn title_to_save(
    custom: Option<&str>,
    default_label: Option<&str>,
) -> Option<String> {
    let custom = custom.map(str::trim).filter(|t| !t.is_empty())?;
    if Some(custom) == default_label {
        return None;
    }
    Some(custom.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(host_id: &str) -> SavedTab {
        SavedTab {
            host_id: host_id.to_string(),
            ..SavedTab::default()
        }
    }

    #[test]
    fn unrestored_tabs_survive_an_autosave() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        let held = [tab("locked-host")];
        SavedTabs::new(vec![tab("local")], 0)
            .with_unrestored(&held)
            .save(&path)
            .unwrap();
        let loaded = SavedTabs::load(&path).unwrap();
        assert_eq!(loaded.tabs, vec![tab("local"), tab("locked-host")]);
        assert_eq!(loaded.active, 0);
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join(FILE_NAME);
        let saved = SavedTabs::new(
            vec![
                SavedTab {
                    title: Some("Main".into()),
                    home: true,
                    ..tab("local")
                },
                SavedTab {
                    cwd: Some("/home/me/src".into()),
                    ..tab("local")
                },
                SavedTab {
                    title: Some("prod".into()),
                    ..tab("4f1c")
                },
            ],
            2,
        );
        saved.save(&path).unwrap();
        assert_eq!(SavedTabs::load(&path), Some(saved));
        assert!(!path.with_extension("json.tmp").exists());
    }

    #[test]
    fn missing_or_corrupt_file_restores_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE_NAME);
        assert_eq!(SavedTabs::load(&path), None);
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(SavedTabs::load(&path), None);
    }

    #[test]
    fn file_from_a_newer_build_is_ignored() {
        let text = format!(r#"{{"version": {}, "tabs": []}}"#, VERSION + 1);
        assert_eq!(SavedTabs::decode(&text), None);
        assert_eq!(SavedTabs::decode(r#"{"version": 0}"#), None);
    }

    #[test]
    fn missing_fields_take_defaults() {
        let saved =
            SavedTabs::decode(r#"{"version": 1, "tabs": [{"host_id": "local"}]}"#)
                .unwrap();
        assert_eq!(saved.active, 0);
        assert_eq!(saved.tabs, vec![tab("local")]);
    }

    #[test]
    fn home_tab_gives_its_name_and_is_not_reopened() {
        let saved = SavedTabs::new(
            vec![
                SavedTab {
                    title: Some("Main".into()),
                    home: true,
                    ..tab("local")
                },
                tab("wsl:Ubuntu"),
                tab(""),
                tab("h1"),
            ],
            0,
        );
        assert_eq!(saved.home_title(), Some("Main"));
        let open: Vec<(usize, &str)> = saved
            .to_open()
            .map(|(i, t)| (i, t.host_id.as_str()))
            .collect();
        assert_eq!(open, vec![(1, "wsl:Ubuntu"), (3, "h1")]);
    }

    #[test]
    fn only_a_chosen_name_is_saved() {
        assert_eq!(title_to_save(None, Some("prod")), None);
        assert_eq!(title_to_save(Some("prod"), Some("prod")), None);
        assert_eq!(title_to_save(Some("  "), None), None);
        assert_eq!(
            title_to_save(Some("deploy box"), Some("prod")),
            Some("deploy box".into())
        );
        assert_eq!(title_to_save(Some("work"), None), Some("work".into()));
    }
}
