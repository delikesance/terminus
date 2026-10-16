//! Which view the main area shows: a machine's Terminal / Files / Tunnels /
//! Snippets / History, the Settings page, or Home ("Where to?").
//!
//! The window shows one workspace at a time, but each machine remembers the
//! machine view it was last on ([`Workspace::leave_host`] /
//! [`Workspace::enter_host`]); a machine never visited opens on Terminal.
//! Settings and Home stay global to the window and return to the current
//! machine's view. The memory lasts for the process only. Per-view data that
//! must survive a machine switch (a Files path, a filter) belongs to that
//! view's own state in [`crate::screens`], keyed by machine id when it needs
//! to be.

use std::collections::HashMap;

/// A section of the Settings page (header tabs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingsPage {
    General,
    Keys,
    Sync,
    Appearance,
    Updates,
    Uploads,
}

impl SettingsPage {
    pub const ALL: [SettingsPage; 6] = [
        SettingsPage::General,
        SettingsPage::Keys,
        SettingsPage::Sync,
        SettingsPage::Appearance,
        SettingsPage::Updates,
        SettingsPage::Uploads,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SettingsPage::General => "General",
            SettingsPage::Keys => "SSH keys",
            SettingsPage::Sync => "Sync",
            SettingsPage::Appearance => "Appearance",
            SettingsPage::Updates => "Updates",
            SettingsPage::Uploads => "Uploads",
        }
    }
}

impl From<SettingsPage> for crate::views::settings::Page {
    fn from(page: SettingsPage) -> Self {
        use crate::views::settings::Page;
        match page {
            SettingsPage::General => Page::General,
            SettingsPage::Keys => Page::Keys,
            SettingsPage::Sync => Page::Sync,
            SettingsPage::Appearance => Page::Appearance,
            SettingsPage::Updates => Page::Updates,
            SettingsPage::Uploads => Page::Uploads,
        }
    }
}

/// What the main area shows below the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum WorkspaceView {
    /// "Where to?": no machine selected.
    Home,
    #[default]
    Terminal,
    Files,
    Tunnels,
    Snippets,
    History,
    Settings(SettingsPage),
}

impl WorkspaceView {
    /// The machine header's view tabs, left to right.
    pub const MACHINE_TABS: [WorkspaceView; 5] = [
        WorkspaceView::Terminal,
        WorkspaceView::Files,
        WorkspaceView::Tunnels,
        WorkspaceView::Snippets,
        WorkspaceView::History,
    ];

    pub fn label(self) -> &'static str {
        match self {
            WorkspaceView::Home => "Home",
            WorkspaceView::Terminal => "Terminal",
            WorkspaceView::Files => "Files",
            WorkspaceView::Tunnels => "Tunnels",
            WorkspaceView::Snippets => "Snippets",
            WorkspaceView::History => "History",
            WorkspaceView::Settings(page) => page.label(),
        }
    }

    /// One of the five views of a machine (header with name + address).
    pub fn is_machine_view(self) -> bool {
        Self::MACHINE_TABS.contains(&self)
    }

    pub fn is_settings(self) -> bool {
        matches!(self, WorkspaceView::Settings(_))
    }

    /// Whether the terminal grid is visible and receives input.
    pub fn shows_terminal(self) -> bool {
        self == WorkspaceView::Terminal
    }

    /// The session pills row sits under the header only on Terminal.
    pub fn shows_pills(self) -> bool {
        self == WorkspaceView::Terminal
    }

    /// The next (or previous) machine view, wrapping. Non-machine views
    /// cycle from Terminal.
    pub fn cycled(self, forward: bool) -> WorkspaceView {
        let tabs = Self::MACHINE_TABS;
        let Some(i) = tabs.iter().position(|v| *v == self) else {
            return WorkspaceView::Terminal;
        };
        let n = tabs.len();
        tabs[if forward {
            (i + 1) % n
        } else {
            (i + n - 1) % n
        }]
    }
}

/// The window's current view plus where Settings / Home return to.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Workspace {
    view: WorkspaceView,
    /// The machine view to come back to when Settings or Home is left.
    machine_view: WorkspaceView,
    /// The machine view each host (sidebar row id) was last left on.
    per_host: HashMap<String, WorkspaceView>,
    /// The host the window is on, as last told to [`Workspace::follow_host`].
    current_host: Option<String>,
}

impl Workspace {
    pub fn view(&self) -> WorkspaceView {
        self.view
    }

    /// Switch to `view`. Returns whether anything changed.
    pub fn show(&mut self, view: WorkspaceView) -> bool {
        if view == self.view {
            return false;
        }
        if self.view.is_machine_view() {
            self.machine_view = self.view;
        }
        self.view = view;
        true
    }

    /// The window is leaving host `id`: remember the machine view it was on
    /// (the last one, when Settings or Home is showing).
    pub fn leave_host(&mut self, id: &str) {
        let view = if self.view.is_machine_view() {
            self.view
        } else {
            self.machine_view
        };
        self.per_host.insert(id.to_string(), view);
    }

    /// The window is now on host `id`: show the view it was last on
    /// (Terminal when never visited). Returns whether the view changed.
    pub fn enter_host(&mut self, id: &str) -> bool {
        let view = self
            .per_host
            .get(id)
            .copied()
            .unwrap_or(WorkspaceView::Terminal);
        let changed = self.show(view);
        self.machine_view = view;
        changed
    }

    /// The front tab belongs to host `id`. Swaps the remembered views only
    /// when the host changed (switching between one host's tabs keeps the
    /// view). Returns whether the view changed.
    pub fn follow_host(&mut self, id: &str) -> bool {
        if self.current_host.as_deref() == Some(id) {
            return false;
        }
        if let Some(previous) = self.current_host.take() {
            self.leave_host(&previous);
        }
        self.current_host = Some(id.to_string());
        self.enter_host(id)
    }

    /// The machine view Settings / Home return to.
    pub fn machine_view(&self) -> WorkspaceView {
        self.machine_view
    }

    /// Drop what is remembered for host `id` (its last tab closed, or the
    /// host deleted).
    pub fn forget_host(&mut self, id: &str) {
        self.per_host.remove(id);
    }

    /// Leave Settings / Home for the machine view shown before them.
    pub fn back_to_machine(&mut self) -> bool {
        if self.view.is_machine_view() {
            return false;
        }
        let back = self.machine_view;
        self.show(if back.is_machine_view() {
            back
        } else {
            WorkspaceView::Terminal
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_on_the_terminal() {
        let ws = Workspace::default();
        assert_eq!(ws.view(), WorkspaceView::Terminal);
        assert!(ws.view().shows_terminal());
        assert!(ws.view().shows_pills());
    }

    #[test]
    fn only_the_terminal_shows_the_grid_and_pills() {
        for v in [
            WorkspaceView::Home,
            WorkspaceView::Files,
            WorkspaceView::Tunnels,
            WorkspaceView::Snippets,
            WorkspaceView::History,
            WorkspaceView::Settings(SettingsPage::Sync),
        ] {
            assert!(!v.shows_terminal(), "{v:?}");
            assert!(!v.shows_pills(), "{v:?}");
        }
    }

    #[test]
    fn machine_tabs_are_the_five_views_in_order() {
        let labels: Vec<_> = WorkspaceView::MACHINE_TABS
            .iter()
            .map(|v| v.label())
            .collect();
        assert_eq!(
            labels,
            ["Terminal", "Files", "Tunnels", "Snippets", "History"]
        );
        assert!(WorkspaceView::Files.is_machine_view());
        assert!(!WorkspaceView::Home.is_machine_view());
        assert!(!WorkspaceView::Settings(SettingsPage::Keys).is_machine_view());
    }

    #[test]
    fn settings_pages_map_to_the_settings_view_pages() {
        use crate::views::settings::Page;
        assert_eq!(Page::from(SettingsPage::General), Page::General);
        assert_eq!(Page::from(SettingsPage::Keys), Page::Keys);
        assert_eq!(Page::from(SettingsPage::Sync), Page::Sync);
        assert_eq!(Page::from(SettingsPage::Appearance), Page::Appearance);
        assert_eq!(Page::from(SettingsPage::Updates), Page::Updates);
        assert_eq!(Page::from(SettingsPage::Uploads), Page::Uploads);
    }

    #[test]
    fn settings_page_labels_follow_the_mock() {
        let labels: Vec<_> = SettingsPage::ALL.iter().map(|p| p.label()).collect();
        assert_eq!(
            labels,
            [
                "General",
                "SSH keys",
                "Sync",
                "Appearance",
                "Updates",
                "Uploads"
            ]
        );
    }

    #[test]
    fn show_reports_changes_and_settings_returns_to_the_last_machine_view() {
        let mut ws = Workspace::default();
        assert!(!ws.show(WorkspaceView::Terminal));
        assert!(ws.show(WorkspaceView::Files));
        assert!(ws.show(WorkspaceView::Settings(SettingsPage::Keys)));
        assert!(ws.show(WorkspaceView::Settings(SettingsPage::Sync)));
        assert!(ws.back_to_machine());
        assert_eq!(ws.view(), WorkspaceView::Files);
        assert!(!ws.back_to_machine(), "already on a machine view");
    }

    #[test]
    fn home_returns_to_the_terminal_when_no_machine_view_was_seen() {
        let mut ws = Workspace::default();
        ws.show(WorkspaceView::Home);
        assert!(ws.back_to_machine());
        assert_eq!(ws.view(), WorkspaceView::Terminal);
    }

    #[test]
    fn each_host_gets_its_own_view_back() {
        let mut ws = Workspace::default();
        ws.enter_host("a");
        ws.show(WorkspaceView::Files);
        ws.leave_host("a");
        assert!(
            ws.enter_host("b"),
            "b was never visited: Terminal from Files"
        );
        assert_eq!(ws.view(), WorkspaceView::Terminal);
        ws.show(WorkspaceView::History);
        ws.leave_host("b");
        assert!(ws.enter_host("a"));
        assert_eq!(ws.view(), WorkspaceView::Files);
        ws.leave_host("a");
        assert!(ws.enter_host("b"));
        assert_eq!(ws.view(), WorkspaceView::History);
    }

    #[test]
    fn entering_the_host_already_shown_changes_nothing() {
        let mut ws = Workspace::default();
        ws.show(WorkspaceView::Tunnels);
        ws.leave_host("a");
        assert!(!ws.enter_host("a"));
        assert_eq!(ws.view(), WorkspaceView::Tunnels);
    }

    #[test]
    fn settings_round_trip_returns_to_the_current_hosts_view() {
        let mut ws = Workspace::default();
        ws.enter_host("a");
        ws.show(WorkspaceView::Files);
        ws.leave_host("a");
        ws.enter_host("b");
        ws.show(WorkspaceView::Settings(SettingsPage::Sync));
        // Leaving a host from Settings remembers its last machine view.
        ws.leave_host("b");
        ws.enter_host("a");
        assert_eq!(ws.view(), WorkspaceView::Files);
        ws.show(WorkspaceView::Home);
        assert!(ws.back_to_machine());
        assert_eq!(ws.view(), WorkspaceView::Files);
    }

    #[test]
    fn following_the_tab_host_swaps_views_only_when_the_host_changes() {
        let mut ws = Workspace::default();
        assert!(!ws.follow_host("a"), "first host: already on Terminal");
        ws.show(WorkspaceView::Files);
        assert!(!ws.follow_host("a"), "same host: the view stays");
        assert_eq!(ws.view(), WorkspaceView::Files);
        assert!(ws.follow_host("b"));
        assert_eq!(ws.view(), WorkspaceView::Terminal);
        assert!(ws.follow_host("a"));
        assert_eq!(ws.view(), WorkspaceView::Files);
    }

    #[test]
    fn forgetting_the_followed_host_after_leaving_it_resets_it() {
        let mut ws = Workspace::default();
        ws.follow_host("a");
        ws.show(WorkspaceView::History);
        ws.follow_host("b");
        ws.forget_host("a");
        ws.follow_host("a");
        assert_eq!(ws.view(), WorkspaceView::Terminal);
    }

    #[test]
    fn the_machine_view_is_what_settings_returns_to() {
        let mut ws = Workspace::default();
        ws.show(WorkspaceView::Tunnels);
        ws.show(WorkspaceView::Settings(SettingsPage::Sync));
        assert_eq!(ws.machine_view(), WorkspaceView::Tunnels);
    }

    #[test]
    fn a_forgotten_host_starts_on_the_terminal_again() {
        let mut ws = Workspace::default();
        ws.show(WorkspaceView::Snippets);
        ws.leave_host("a");
        ws.forget_host("a");
        ws.enter_host("a");
        assert_eq!(ws.view(), WorkspaceView::Terminal);
    }

    #[test]
    fn cycling_wraps_over_the_machine_tabs() {
        assert_eq!(WorkspaceView::Terminal.cycled(true), WorkspaceView::Files);
        assert_eq!(WorkspaceView::History.cycled(true), WorkspaceView::Terminal);
        assert_eq!(
            WorkspaceView::Terminal.cycled(false),
            WorkspaceView::History
        );
        assert_eq!(WorkspaceView::Home.cycled(true), WorkspaceView::Terminal);
    }
}
