//! Reopen last launch's tabs, and keep the list on disk for the next one.
//!
//! The list (`terminus_core::saved_tabs`) is written whenever the tabs or
//! their names change, checked at most every [`CHECK_EVERY`], and once more
//! when the window goes away. It is read back once the host worker has
//! answered its first refresh: until then a stored host or a WSL distro
//! cannot be turned back into a shell.
//!
//! Only one window owns the file: the first one to restore. A second
//! window neither reopens the tabs again nor overwrites the list with its
//! own.

use super::Screen;
use crate::hosts;
use rio_backend::clipboard::Clipboard;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use terminus_core::saved_tabs::{self, SavedTab, SavedTabs};

/// How often the open tabs are compared with what was last written.
const CHECK_EVERY: Duration = Duration::from_secs(2);

/// Label the pinned home tab starts with (`ContextManager::start`).
const HOME_LABEL: &str = "This computer";

/// Set by the window that restored the tabs, the one that saves them.
static CLAIMED: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
pub(super) struct SavedTabsState {
    /// The host worker's first refresh has landed; restore on next frame.
    restore_due: bool,
    /// This window restored the tabs and keeps the file up to date.
    owner: bool,
    checked_at: Option<Instant>,
    written: Option<SavedTabs>,
}

impl SavedTabsState {
    pub(super) fn hosts_known(&mut self) {
        if !self.owner {
            self.restore_due = true;
        }
    }
}

fn file_path() -> std::path::PathBuf {
    hosts::data_dir().join(saved_tabs::FILE_NAME)
}

impl Screen<'_> {
    /// The open tabs, as they would be saved.
    fn tabs_snapshot(&self) -> SavedTabs {
        let manager = &self.context_manager;
        let tabs = (0..manager.len())
            .map(|i| {
                let home = manager.is_pinned(i);
                let host_id = manager
                    .tab_host_id(i)
                    .unwrap_or(hosts::LOCAL_ID)
                    .to_string();
                let default_label = if home {
                    Some(HOME_LABEL.to_string())
                } else {
                    self.chrome
                        .panel
                        .rows
                        .iter()
                        .filter_map(terminus_ui::sidebar::Row::host)
                        .find(|item| item.id == host_id)
                        .map(|item| item.name.clone())
                };
                let title = saved_tabs::title_to_save(
                    manager.custom_title(i),
                    default_label.as_deref(),
                );
                let cwd = if !home && host_id == hosts::LOCAL_ID {
                    manager.tab_working_dir(i)
                } else {
                    None
                };
                SavedTab {
                    host_id,
                    title,
                    cwd,
                    home,
                }
            })
            .collect();
        SavedTabs::new(tabs, manager.current_index())
    }

    /// Write the tabs if they changed since the last write. Called from
    /// every chrome pump; does the work at most every [`CHECK_EVERY`].
    pub(super) fn autosave_tabs(&mut self) {
        if !self.saved_tabs.owner {
            return;
        }
        let now = Instant::now();
        if self
            .saved_tabs
            .checked_at
            .is_some_and(|at| now.duration_since(at) < CHECK_EVERY)
        {
            return;
        }
        self.saved_tabs.checked_at = Some(now);
        self.save_tabs_now();
    }

    /// Write the tabs now if they changed (the window is closing).
    pub fn save_tabs_now(&mut self) {
        if !self.saved_tabs.owner || self.context_manager.len() == 0 {
            return;
        }
        let snapshot = self.tabs_snapshot();
        if self.saved_tabs.written.as_ref() == Some(&snapshot) {
            return;
        }
        match snapshot.save(&file_path()) {
            Ok(()) => self.saved_tabs.written = Some(snapshot),
            Err(err) => tracing::warn!("could not save open tabs: {err}"),
        }
    }

    /// Once the hosts are known, reopen the tabs saved by the last launch.
    /// Returns whether any tab changed.
    pub fn restore_saved_tabs_if_due(&mut self, clipboard: &mut Clipboard) -> bool {
        if !std::mem::take(&mut self.saved_tabs.restore_due) {
            return false;
        }
        if CLAIMED.swap(true, Ordering::SeqCst) {
            return false;
        }
        self.saved_tabs.owner = true;
        let Some(saved) = SavedTabs::load(&file_path()) else {
            return false;
        };

        if let (Some(home), Some(name)) =
            (self.context_manager.find_home_tab(), saved.home_title())
        {
            self.context_manager
                .set_custom_title(home, Some(name.to_string()));
        }
        let to_open: Vec<(usize, SavedTab)> = saved
            .to_open()
            .map(|(index, tab)| (index, tab.clone()))
            .collect();
        let front = to_open.iter().position(|(index, _)| *index == saved.active);
        let opened = self.reopen_saved_tabs(
            to_open.into_iter().map(|(_, tab)| tab).collect(),
            clipboard,
        );

        // Bring back the tab that was in front, or the home tab when that
        // was it (or it could not reopen).
        let target = front
            .and_then(|position| opened.get(position).copied().flatten())
            .or_else(|| self.context_manager.find_home_tab());
        if let Some(index) = target {
            self.focus_session(index, clipboard);
        }
        // Don't write back what was just read.
        self.saved_tabs.written = Some(self.tabs_snapshot());
        true
    }

    /// Open a tab for each of `tabs`. A host that needs the vault while it
    /// is locked is held back and the unlock prompt asks for it once; a
    /// host that no longer exists is dropped. Returns, for each of `tabs`,
    /// the index of the tab it opened.
    pub fn reopen_saved_tabs(
        &mut self,
        tabs: Vec<SavedTab>,
        clipboard: &mut Clipboard,
    ) -> Vec<Option<usize>> {
        let mut opened = Vec::new();
        let mut locked = Vec::new();
        for tab in tabs {
            let id = tab.host_id.as_str();
            let known = id == hosts::LOCAL_ID
                || self.host_store.platform().distro_named(id).is_some()
                || self.host_store.hosts().iter().any(|host| host.id == id);
            if !known {
                tracing::info!("not reopening a tab for {id}: the host is gone");
                opened.push(None);
                continue;
            }
            let (shell, env) = match self.shell_for_row(id) {
                Ok(launch) => launch,
                Err(err) if err.contains("Unlock the vault") => {
                    locked.push(tab);
                    opened.push(None);
                    continue;
                }
                Err(err) => {
                    tracing::warn!("could not reopen a tab for {id}: {err}");
                    opened.push(None);
                    continue;
                }
            };
            let start_dir = tab.cwd.clone().filter(|dir| {
                std::path::Path::new(dir).is_dir() && id == hosts::LOCAL_ID
            });
            if let Err(err) =
                self.create_tab_in(clipboard, shell, env, Some(id.to_string()), start_dir)
            {
                tracing::warn!("could not reopen a tab for {id}: {err}");
                opened.push(None);
                continue;
            }
            let index = self.context_manager.current_index();
            if id != hosts::LOCAL_ID {
                let label = self.host_row_label(id);
                self.dress_host_tab(index, id, label);
                self.host_store.detect_os(id);
            }
            if let Some(name) = &tab.title {
                self.context_manager
                    .set_custom_title(index, Some(name.clone()));
            }
            opened.push(Some(index));
        }
        if !locked.is_empty() {
            self.open_vault_unlock_for(terminus_ui::PendingVaultAction::RestoreTabs(
                locked,
            ));
        }
        opened
    }
}

impl Drop for Screen<'_> {
    /// Closing the window is the last chance to record its tabs.
    fn drop(&mut self) {
        self.save_tabs_now();
    }
}
