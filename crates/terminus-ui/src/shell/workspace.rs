//! Which view the main area shows: a machine's Terminal / Files / Tunnels /
//! Snippets / History, the Settings page, or Home ("Where to?").
//!
//! The view is **global to the window**, not stored per machine: the window
//! shows one workspace at a time, and the design resets to Terminal whenever
//! a machine is picked in the sidebar (`App.dc.html`: `go({machine, view:
//! 'terminal'})`). Per-view data that must survive a machine switch (a Files
//! path, a filter) belongs to that view's own state in
//! [`crate::screens`], keyed by machine id when it needs to be.

/// A section of the Settings page (header tabs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingsPage {
    Keys,
    Sync,
    Appearance,
    Updates,
    Uploads,
}

impl SettingsPage {
    pub const ALL: [SettingsPage; 5] = [
        SettingsPage::Keys,
        SettingsPage::Sync,
        SettingsPage::Appearance,
        SettingsPage::Updates,
        SettingsPage::Uploads,
    ];

    pub fn label(self) -> &'static str {
        match self {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Workspace {
    view: WorkspaceView,
    /// The machine view to come back to when Settings or Home is left.
    machine_view: WorkspaceView,
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

    /// A machine was picked (sidebar row, pill, palette host): its
    /// terminal comes to the front, like the design does.
    pub fn machine_selected(&mut self) -> bool {
        self.show(WorkspaceView::Terminal)
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
            ["SSH keys", "Sync", "Appearance", "Updates", "Uploads"]
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
    fn picking_a_machine_brings_its_terminal_forward() {
        let mut ws = Workspace::default();
        ws.show(WorkspaceView::History);
        assert!(ws.machine_selected());
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
