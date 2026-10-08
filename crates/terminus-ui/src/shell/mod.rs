//! The app shell: sidebar chrome, workspace header, session pills and the
//! view state that decides what the main area shows.
//!
//! Geometry and hit-testing only (no GPU). The frontend painter
//! (`rioterm::renderer::shell`) walks the same rects, and measures the
//! text it draws into [`Shell::record_width`] so that layout here uses the
//! real glyph widths.
//!
//! See `SHELL_CONTRACT.md` (redesign notes) for how views plug in.

pub mod header;
pub mod layout;
pub mod pills;
pub mod sidebar;
pub mod workspace;

use std::collections::HashMap;

use crate::components::navigation::TabSize;
use crate::geom::Rect;
use crate::TextDraft;
pub use header::{HeaderGeom, HeaderTab};
pub use layout::{grid_insets, min_window_size, Insets, ShellLayout};
pub use pills::{PillsGeom, PillsHit, SessionPill};
pub use workspace::{SettingsPage, Workspace, WorkspaceView};

/// Which text a width belongs to (each is drawn at its own size/weight).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextKind {
    /// Header title, Sora SemiBold 28.
    Title,
    /// Header address, Martian Mono 11.
    Address,
    /// View tab label, Sora 14 (Medium width, so activation never shifts).
    Tab,
    /// View tab badge, Sora 14.
    Badge,
    /// Session pill label, Sora 13.
    Pill,
}

impl TextKind {
    /// Rough per-character width, used until the painter has measured.
    fn estimate(self) -> f32 {
        match self {
            TextKind::Title => 15.0,
            TextKind::Address => 7.0,
            TextKind::Tab | TextKind::Badge => 8.0,
            TextKind::Pill => 7.5,
        }
    }
}

/// The machine whose workspace is shown.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MachineInfo {
    /// Sidebar row id: `local`, `wsl:<distro>` or a stored host id.
    pub id: String,
    pub name: String,
    /// `user@host`, `WSL · NixOS`, …
    pub address: String,
}

/// Inline rename of a session pill (double-click, or "Rename" in its
/// context menu).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PillRename {
    /// Tab whose name is being edited.
    pub tab_index: usize,
    pub text: TextDraft,
}

/// Header window controls (painted on Windows, where the app draws its
/// own caption buttons).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowButton {
    Minimize,
    Maximize,
    Close,
}

/// What a pointer position on the shell resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellHit {
    /// Brand word: go Home.
    Brand,
    /// "Search or run…": open the command palette.
    CommandBar,
    AddServer,
    Settings,
    /// A header view tab.
    Tab(WorkspaceView),
    Control(WindowButton),
    /// Session pill, by tab index.
    Pill(usize),
    /// Close × of the pill for this tab index.
    PillClose(usize),
    NewSession,
    SplitRight,
    SplitDown,
    /// Empty header: drags the window, double-click maximizes.
    Drag,
    /// Empty part of the pills row: swallowed, does nothing.
    Inert,
}

impl ShellHit {
    /// Clickable controls get the pointer cursor.
    pub fn is_control(self) -> bool {
        !matches!(self, ShellHit::Drag | ShellHit::Inert)
    }
}

/// Shell state for one window.
#[derive(Debug, Clone, PartialEq)]
pub struct Shell {
    pub workspace: Workspace,
    /// The selected machine (the current tab's host). `None` before the
    /// first sync.
    pub machine: Option<MachineInfo>,
    /// Sessions of the selected machine, in tab order.
    pub pills: Vec<SessionPill>,
    /// The pill being renamed, if any.
    pub rename: Option<PillRename>,
    /// Running tunnels of the selected machine (Tunnels tab badge).
    pub tunnel_badge: u32,
    /// Sync is configured and healthy: "Synced" on the Settings button.
    pub sync_ok: bool,
    /// Paint / hit the min-max-close buttons (Windows).
    pub window_controls: bool,
    pub hover: Option<ShellHit>,
    /// Logical window size, refreshed by the frontend.
    pub window: (f32, f32),
    widths: HashMap<(TextKind, String), f32>,
}

impl Default for Shell {
    fn default() -> Self {
        Self {
            workspace: Workspace::default(),
            machine: None,
            pills: Vec::new(),
            rename: None,
            tunnel_badge: 0,
            sync_ok: false,
            window_controls: cfg!(target_os = "windows"),
            hover: None,
            window: (1200.0, 800.0),
            widths: HashMap::new(),
        }
    }
}

impl Shell {
    pub fn view(&self) -> WorkspaceView {
        self.workspace.view()
    }

    pub fn layout(&self) -> ShellLayout {
        ShellLayout::new(self.window.0, self.window.1)
    }

    /// Where the current view paints and takes input.
    pub fn content_rect(&self) -> Rect {
        self.layout().content(self.view())
    }

    // ---- text widths -------------------------------------------------

    /// Width of `text` drawn as `kind`: measured if the painter recorded
    /// it, estimated otherwise.
    pub fn width(&self, kind: TextKind, text: &str) -> f32 {
        self.widths
            .get(&(kind, text.to_string()))
            .copied()
            .unwrap_or_else(|| text.chars().count() as f32 * kind.estimate())
    }

    pub fn has_width(&self, kind: TextKind, text: &str) -> bool {
        self.widths.contains_key(&(kind, text.to_string()))
    }

    /// Store a measured width. Returns whether it changed the layout.
    pub fn record_width(&mut self, kind: TextKind, text: &str, width: f32) -> bool {
        if self.widths.len() > 512 {
            self.widths.clear();
        }
        let prev = self.widths.insert((kind, text.to_string()), width);
        prev != Some(width)
    }

    /// Every text the current frame lays out, for the painter to measure.
    pub fn texts(&self) -> Vec<(TextKind, String)> {
        let (title, address) = self.title();
        let mut out = vec![(TextKind::Title, title)];
        if let Some(a) = address {
            out.push((TextKind::Address, a));
        }
        for tab in self.header_tabs() {
            out.push((TextKind::Tab, tab.label.to_string()));
            if !tab.badge.is_empty() {
                out.push((TextKind::Badge, tab.badge));
            }
        }
        for p in &self.pills {
            out.push((TextKind::Pill, p.label.clone()));
        }
        if let Some(r) = &self.rename {
            out.push((TextKind::Pill, r.text.value.clone()));
        }
        out
    }

    // ---- header ------------------------------------------------------

    /// Header title and (machine views only) its mono address.
    pub fn title(&self) -> (String, Option<String>) {
        let view = self.view();
        if view.is_settings() {
            return (header::SETTINGS_TITLE.to_string(), None);
        }
        if view == WorkspaceView::Home {
            return (header::HOME_TITLE.to_string(), None);
        }
        match &self.machine {
            Some(m) => (
                m.name.clone(),
                (!m.address.is_empty()).then(|| m.address.clone()),
            ),
            None => ("This computer".to_string(), None),
        }
    }

    pub fn header_tabs(&self) -> Vec<HeaderTab> {
        header::tabs_for(self.view(), self.tunnel_badge)
    }

    pub fn header_geom(&self) -> HeaderGeom {
        let (title, address) = self.title();
        let mut block_w = self.width(TextKind::Title, &title);
        if let Some(a) = &address {
            block_w = block_w.max(self.width(TextKind::Address, a));
        }
        let sizes: Vec<TabSize> = self
            .header_tabs()
            .iter()
            .map(|t| TabSize {
                label_w: self.width(TextKind::Tab, t.label),
                badge_w: if t.badge.is_empty() {
                    0.0
                } else {
                    self.width(TextKind::Badge, &t.badge)
                },
            })
            .collect();
        header::layout(&self.layout().header, address.is_some(), block_w, &sizes)
    }

    // ---- pills -------------------------------------------------------

    /// Position (in [`Self::pills`]) of the hovered pill.
    pub fn hovered_pill(&self) -> Option<usize> {
        let tab = match self.hover? {
            ShellHit::Pill(t) | ShellHit::PillClose(t) => t,
            _ => return None,
        };
        self.pills.iter().position(|p| p.tab_index == tab)
    }

    /// Pills row boxes, when the current view shows it.
    pub fn pills_geom(&self) -> Option<PillsGeom> {
        if !self.view().shows_pills() {
            return None;
        }
        let widths: Vec<f32> = self
            .pills
            .iter()
            .map(|p| match &self.rename {
                Some(r) if r.tab_index == p.tab_index => self
                    .width(TextKind::Pill, &r.text.value)
                    .max(pills::RENAME_MIN_LABEL),
                _ => self.width(TextKind::Pill, &p.label),
            })
            .collect();
        Some(pills::layout(
            &self.layout().pills,
            &self.pills,
            &widths,
            self.hovered_pill(),
        ))
    }

    /// Replace the pills (each frame). A rename whose tab is no longer
    /// shown is dropped.
    pub fn set_pills(&mut self, pills: Vec<SessionPill>) {
        self.pills = pills;
        if let Some(r) = &self.rename {
            if !self.pills.iter().any(|p| p.tab_index == r.tab_index) {
                self.rename = None;
            }
        }
    }

    /// Start renaming the pill of `tab_index`, its current label selected
    /// so typing replaces it. Returns whether that pill exists.
    pub fn begin_rename(&mut self, tab_index: usize) -> bool {
        let Some(pill) = self.pills.iter().find(|p| p.tab_index == tab_index) else {
            return false;
        };
        let mut text = TextDraft::new(pill.label.clone());
        text.select_all();
        self.rename = Some(PillRename { tab_index, text });
        true
    }

    pub fn is_renaming(&self, tab_index: usize) -> bool {
        self.rename
            .as_ref()
            .is_some_and(|r| r.tab_index == tab_index)
    }

    /// End the rename and return `(tab, name)` to apply. A blank name is
    /// `None`: the tab goes back to its default label.
    pub fn take_rename(&mut self) -> Option<(usize, Option<String>)> {
        let r = self.rename.take()?;
        let name = r.text.value.trim();
        Some((r.tab_index, (!name.is_empty()).then(|| name.to_string())))
    }

    /// Drop the rename without applying it. Returns whether one was open.
    pub fn cancel_rename(&mut self) -> bool {
        self.rename.take().is_some()
    }

    // ---- hit-testing -------------------------------------------------

    /// What `(x, y)` lands on. `None` outside the shell's own controls:
    /// the sidebar list, the content area and the terminal handle those.
    pub fn hit_test(&self, x: f32, y: f32) -> Option<ShellHit> {
        let l = self.layout();
        let h = self.window.1;
        if l.sidebar.contains(x, y) {
            return if sidebar::brand_rect().contains(x, y) {
                Some(ShellHit::Brand)
            } else if sidebar::command_bar_rect().contains(x, y) {
                Some(ShellHit::CommandBar)
            } else if sidebar::add_server_rect(h).contains(x, y) {
                Some(ShellHit::AddServer)
            } else if sidebar::settings_rect(h).contains(x, y) {
                Some(ShellHit::Settings)
            } else {
                None
            };
        }
        if l.header.contains(x, y) {
            if self.window_controls {
                let buttons = [
                    WindowButton::Minimize,
                    WindowButton::Maximize,
                    WindowButton::Close,
                ];
                for (rect, b) in l.window_controls().iter().zip(buttons) {
                    if rect.contains(x, y) {
                        return Some(ShellHit::Control(b));
                    }
                }
            }
            let geom = self.header_geom();
            for (rect, tab) in geom.tabs.iter().zip(self.header_tabs()) {
                if rect.contains(x, y) {
                    return Some(ShellHit::Tab(tab.view));
                }
            }
            return Some(ShellHit::Drag);
        }
        if let Some(geom) = self.pills_geom() {
            if l.pills.contains(x, y) {
                let hit = geom.hit(&self.pills, self.hovered_pill(), x, y);
                return Some(match hit {
                    Some(PillsHit::Pill(i)) => ShellHit::Pill(self.pills[i].tab_index),
                    Some(PillsHit::Close(i)) => {
                        ShellHit::PillClose(self.pills[i].tab_index)
                    }
                    Some(PillsHit::NewSession) => ShellHit::NewSession,
                    Some(PillsHit::SplitRight) => ShellHit::SplitRight,
                    Some(PillsHit::SplitDown) => ShellHit::SplitDown,
                    None => ShellHit::Inert,
                });
            }
        }
        None
    }

    /// Track the hovered control. Returns whether a repaint is needed.
    pub fn set_hover(&mut self, hit: Option<ShellHit>) -> bool {
        let hit = hit.filter(|h| h.is_control());
        if self.hover == hit {
            return false;
        }
        self.hover = hit;
        true
    }

    /// Whether the main card's content area (not header / pills) holds
    /// `(x, y)` and the current view, not the terminal, owns it.
    pub fn view_owns(&self, x: f32, y: f32) -> bool {
        !self.view().shows_terminal() && self.content_rect().contains(x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell() -> Shell {
        let mut s = Shell {
            window: (1440.0, 900.0),
            window_controls: false,
            ..Shell::default()
        };
        s.machine = Some(MachineInfo {
            id: "h1".into(),
            name: "jerem prod".into(),
            address: "ubuntu@137.74.42.224".into(),
        });
        s.pills = vec![
            SessionPill {
                tab_index: 2,
                label: "~/app".into(),
                active: true,
                new_output: false,
                closable: true,
            },
            SessionPill {
                tab_index: 4,
                label: "logs".into(),
                active: false,
                new_output: true,
                closable: true,
            },
        ];
        s
    }

    fn centre(r: &Rect) -> (f32, f32) {
        (r.x + r.width / 2.0, r.y + r.height / 2.0)
    }

    #[test]
    fn sidebar_chrome_controls_hit() {
        let s = shell();
        let (x, y) = centre(&sidebar::command_bar_rect());
        assert_eq!(s.hit_test(x, y), Some(ShellHit::CommandBar));
        let (x, y) = centre(&sidebar::add_server_rect(900.0));
        assert_eq!(s.hit_test(x, y), Some(ShellHit::AddServer));
        let (x, y) = centre(&sidebar::settings_rect(900.0));
        assert_eq!(s.hit_test(x, y), Some(ShellHit::Settings));
        let (x, y) = centre(&sidebar::brand_rect());
        assert_eq!(s.hit_test(x, y), Some(ShellHit::Brand));
        // The list between them belongs to the host panel.
        assert_eq!(s.hit_test(100.0, 300.0), None);
    }

    #[test]
    fn header_tabs_switch_views_and_the_rest_drags() {
        let s = shell();
        let g = s.header_geom();
        let tabs = s.header_tabs();
        assert_eq!(g.tabs.len(), 5);
        let (x, y) = centre(&g.tabs[1]);
        assert_eq!(s.hit_test(x, y), Some(ShellHit::Tab(tabs[1].view)));
        assert_eq!(tabs[1].view, WorkspaceView::Files);
        assert_eq!(s.hit_test(1000.0, 30.0), Some(ShellHit::Drag));
        let (x, y) = centre(&g.ident.title);
        assert_eq!(s.hit_test(x, y), Some(ShellHit::Drag));
    }

    #[test]
    fn window_controls_only_hit_when_enabled() {
        let mut s = shell();
        let [_, _, close] = s.layout().window_controls();
        let (x, y) = centre(&close);
        assert_eq!(s.hit_test(x, y), Some(ShellHit::Drag));
        s.window_controls = true;
        assert_eq!(
            s.hit_test(x, y),
            Some(ShellHit::Control(WindowButton::Close))
        );
    }

    #[test]
    fn pills_map_to_tab_indices() {
        let s = shell();
        let g = s.pills_geom().expect("terminal shows pills");
        let (x, y) = centre(&g.pills[1]);
        assert_eq!(s.hit_test(x, y), Some(ShellHit::Pill(4)));
        let c = crate::components::navigation::session_pill::close_rect(&g.pills[0]);
        assert_eq!(
            s.hit_test(c.x + 2.0, c.y + 2.0),
            Some(ShellHit::PillClose(2))
        );
        let (x, y) = centre(&g.plus);
        assert_eq!(s.hit_test(x, y), Some(ShellHit::NewSession));
        let (x, y) = centre(&g.split_down);
        assert_eq!(s.hit_test(x, y), Some(ShellHit::SplitDown));
        assert_eq!(s.hit_test(900.0, 130.0), Some(ShellHit::Inert));
    }

    #[test]
    fn other_views_have_no_pills_row_and_own_their_content() {
        let mut s = shell();
        assert!(!s.view_owns(800.0, 500.0), "terminal owns its content");
        s.workspace.show(WorkspaceView::Files);
        assert!(s.pills_geom().is_none());
        assert_eq!(s.hit_test(900.0, 130.0), None, "content, not pills");
        assert!(s.view_owns(800.0, 130.0));
        assert!(s.view_owns(800.0, 500.0));
        assert!(!s.view_owns(100.0, 500.0), "sidebar is not content");
    }

    #[test]
    fn titles_follow_the_view() {
        let mut s = shell();
        assert_eq!(
            s.title(),
            (
                "jerem prod".to_string(),
                Some("ubuntu@137.74.42.224".to_string())
            )
        );
        s.workspace
            .show(WorkspaceView::Settings(SettingsPage::Keys));
        assert_eq!(s.title(), ("Settings".to_string(), None));
        assert_eq!(s.header_tabs().len(), 4);
        s.workspace.show(WorkspaceView::Home);
        assert_eq!(s.title(), ("Where to?".to_string(), None));
        assert!(s.header_tabs().is_empty());
    }

    #[test]
    fn measured_widths_replace_estimates_and_move_the_tabs() {
        let mut s = shell();
        let before = s.header_geom().tabs[0].x;
        assert!(s.record_width(TextKind::Title, "jerem prod", 300.0));
        assert!(!s.record_width(TextKind::Title, "jerem prod", 300.0));
        assert!(s.has_width(TextKind::Title, "jerem prod"));
        let after = s.header_geom().tabs[0].x;
        assert_ne!(before, after);
        let texts = s.texts();
        assert!(texts.contains(&(TextKind::Pill, "logs".to_string())));
        assert!(texts.contains(&(TextKind::Address, "ubuntu@137.74.42.224".into())));
    }

    #[test]
    fn hover_only_tracks_controls() {
        let mut s = shell();
        assert!(!s.set_hover(Some(ShellHit::Drag)));
        assert!(s.set_hover(Some(ShellHit::Pill(4))));
        assert_eq!(s.hovered_pill(), Some(1));
        assert!(!s.set_hover(Some(ShellHit::Pill(4))));
        assert!(s.set_hover(None));
    }

    #[test]
    fn renaming_a_pill_starts_from_its_label_selected() {
        let mut s = shell();
        assert!(s.begin_rename(4));
        let r = s.rename.as_ref().expect("rename draft");
        assert_eq!(r.tab_index, 4);
        assert_eq!(r.text.value, "logs");
        assert_eq!(r.text.selection_range(), Some((0, 4)));
        assert!(s.is_renaming(4));
        assert!(!s.is_renaming(2));
        assert!(!s.begin_rename(9), "no pill for that tab");
        assert!(s.is_renaming(4), "a failed begin keeps the current draft");
    }

    #[test]
    fn the_renamed_pill_grows_with_its_draft() {
        let mut s = shell();
        let before = s.pills_geom().unwrap().pills[1].width;
        s.begin_rename(4);
        s.rename.as_mut().unwrap().text = crate::TextDraft::new("production logs");
        let g = s.pills_geom().unwrap();
        assert!(g.pills[1].width > before);
        assert!(s
            .texts()
            .contains(&(TextKind::Pill, "production logs".to_string())));
        // An empty draft still leaves room for the caret.
        s.rename.as_mut().unwrap().text = crate::TextDraft::new("");
        let g = s.pills_geom().unwrap();
        assert!(g.label_w[1] >= pills::RENAME_MIN_LABEL);
    }

    #[test]
    fn committing_a_rename_trims_and_blank_resets_to_the_default() {
        let mut s = shell();
        s.begin_rename(4);
        s.rename.as_mut().unwrap().text = crate::TextDraft::new("  db  ");
        assert_eq!(s.take_rename(), Some((4, Some("db".to_string()))));
        assert!(s.rename.is_none());
        s.begin_rename(2);
        s.rename.as_mut().unwrap().text = crate::TextDraft::new("   ");
        assert_eq!(s.take_rename(), Some((2, None)));
        assert_eq!(s.take_rename(), None);
        s.begin_rename(2);
        assert!(s.cancel_rename());
        assert!(!s.cancel_rename());
    }

    #[test]
    fn a_rename_is_dropped_when_its_pill_goes_away() {
        let mut s = shell();
        s.begin_rename(4);
        let pills = s.pills[..1].to_vec();
        s.set_pills(pills);
        assert!(s.rename.is_none());
        s.begin_rename(2);
        let pills = s.pills.clone();
        s.set_pills(pills);
        assert!(s.is_renaming(2));
    }
}
