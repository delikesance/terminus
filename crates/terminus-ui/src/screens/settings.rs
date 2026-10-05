//! Settings page (stub): SSH keys · Sync · Appearance · Updates.
//!
//! Bridge for now: entering SSH keys or Sync also opens the existing
//! settings dialog on the matching tab (see `Chrome::show_view`); this
//! page paints behind it, and offers a button to reopen it once closed.
//! Updates checks for a new release; Appearance points at the config.

use super::{
    empty_state, estimate_label, primary_button, ViewAction, ViewInput, ViewOutcome,
};
use crate::geom::Rect;
use crate::shell::SettingsPage;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsAction {
    /// Reopen the legacy settings dialog on this page's tab.
    OpenDialog(SettingsPage),
    /// Look for a new Terminus release now.
    CheckForUpdates,
    /// Open the config file in the editor.
    OpenConfig,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SettingsState {
    /// Last update status line (from the updater), shown on Updates.
    pub update_status: String,
    pub cta_hover: bool,
    /// Measured width of the current page's button label (0 = estimate).
    pub cta_label_w: f32,
}

/// Title shown at the top of a page.
pub fn title(page: SettingsPage) -> &'static str {
    page.label()
}

/// Button label of a page.
pub fn cta(page: SettingsPage) -> &'static str {
    match page {
        SettingsPage::Keys => "Manage SSH keys",
        SettingsPage::Sync => "Set up sync",
        SettingsPage::Appearance => "Open config file",
        SettingsPage::Updates => "Check for updates",
    }
}

impl SettingsState {
    pub fn body(&self, page: SettingsPage) -> String {
        match page {
            SettingsPage::Keys => {
                "Keys Terminus can use to sign in to your servers.".into()
            }
            SettingsPage::Sync => {
                "Keep servers, keys and snippets in step across computers.".into()
            }
            SettingsPage::Appearance => {
                "Colours, fonts and the terminal itself are set in the config file."
                    .into()
            }
            SettingsPage::Updates if !self.update_status.is_empty() => {
                self.update_status.clone()
            }
            SettingsPage::Updates => {
                "Terminus checks for a new version when it starts.".into()
            }
        }
    }

    pub fn cta_rect(&self, page: SettingsPage, content: Rect) -> Rect {
        let w = if self.cta_label_w > 0.0 {
            self.cta_label_w
        } else {
            estimate_label(cta(page))
        };
        primary_button(empty_state(content).button, w)
    }

    pub fn is_clickable(
        &self,
        page: SettingsPage,
        content: Rect,
        x: f32,
        y: f32,
    ) -> bool {
        self.cta_rect(page, content).contains(x, y)
    }

    pub fn handle(
        &mut self,
        page: SettingsPage,
        content: Rect,
        input: &ViewInput,
    ) -> ViewOutcome {
        match *input {
            ViewInput::Press { x, y, .. } if self.is_clickable(page, content, x, y) => {
                let action = match page {
                    SettingsPage::Keys | SettingsPage::Sync => {
                        SettingsAction::OpenDialog(page)
                    }
                    SettingsPage::Appearance => SettingsAction::OpenConfig,
                    SettingsPage::Updates => SettingsAction::CheckForUpdates,
                };
                ViewOutcome::Action(ViewAction::Settings(action))
            }
            ViewInput::Move { x, y, .. } => {
                let hover = self.is_clickable(page, content, x, y);
                if hover != self.cta_hover {
                    self.cta_hover = hover;
                    ViewOutcome::Redraw
                } else {
                    ViewOutcome::Consumed
                }
            }
            _ => ViewOutcome::Ignored,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content() -> Rect {
        Rect::new(260.0, 104.0, 1172.0, 788.0)
    }

    #[test]
    fn each_page_button_maps_to_its_action() {
        let cases = [
            (
                SettingsPage::Keys,
                SettingsAction::OpenDialog(SettingsPage::Keys),
            ),
            (
                SettingsPage::Sync,
                SettingsAction::OpenDialog(SettingsPage::Sync),
            ),
            (SettingsPage::Appearance, SettingsAction::OpenConfig),
            (SettingsPage::Updates, SettingsAction::CheckForUpdates),
        ];
        for (page, action) in cases {
            let mut s = SettingsState::default();
            let r = s.cta_rect(page, content());
            let out = s.handle(
                page,
                content(),
                &ViewInput::Press {
                    x: r.x + 1.0,
                    y: r.y + 1.0,
                    double: false,
                },
            );
            assert_eq!(out, ViewOutcome::Action(ViewAction::Settings(action)));
        }
    }

    #[test]
    fn updates_shows_the_updater_status_when_known() {
        let mut s = SettingsState::default();
        assert!(s.body(SettingsPage::Updates).contains("checks"));
        s.update_status = "Terminus 0.6.5 is available".into();
        assert_eq!(s.body(SettingsPage::Updates), "Terminus 0.6.5 is available");
    }
}
