//! Updates tab: version + status card with Check now, and the two toggles
//! bound to `[updates] check` / `auto-install`.

use super::{button_spec, column, Measure, SettingsAction, ROW_GAP};
use crate::components::button::{ButtonKind, ButtonSize};
use crate::components::selection::{toggle_rect, TOGGLE_HEIGHT, TOGGLE_WIDTH};
use crate::geom::Rect;

pub const CARD_PAD_X: f32 = 20.0;
pub const VERSION_CARD_HEIGHT: f32 = 78.0;
pub const TOGGLE_CARD_HEIGHT: f32 = 74.0;
pub const MAX_WIDTH: f32 = 760.0;

/// What the updater is doing, in the words the page shows.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum UpdateStatus {
    #[default]
    Idle,
    Checking,
    UpToDate,
    /// A newer release; `can_install` is false when this install type must
    /// update elsewhere (package manager, installer page).
    Available {
        version: String,
        can_install: bool,
    },
    Downloading {
        version: String,
    },
    ReadyToRestart {
        version: String,
    },
    Failed(String),
}

impl UpdateStatus {
    pub fn text(&self) -> String {
        match self {
            UpdateStatus::Idle => "Checks for updates in the background.".into(),
            UpdateStatus::Checking => "Looking for a new version\u{2026}".into(),
            UpdateStatus::UpToDate => "You're up to date.".into(),
            UpdateStatus::Available { version, .. } => {
                format!("Version {version} is available.")
            }
            UpdateStatus::Downloading { version } => {
                format!("Downloading {version}\u{2026}")
            }
            UpdateStatus::ReadyToRestart { version } => {
                format!("Restart to finish updating to {version}.")
            }
            UpdateStatus::Failed(e) => format!("Couldn't check for updates: {e}"),
        }
    }

    pub fn is_error(&self) -> bool {
        matches!(self, UpdateStatus::Failed(_))
    }

    fn busy(&self) -> bool {
        matches!(
            self,
            UpdateStatus::Checking | UpdateStatus::Downloading { .. }
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdatesTarget {
    Check,
    Install,
    CheckToggle,
    AutoToggle,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdatesState {
    pub version: String,
    pub status: UpdateStatus,
    pub check: bool,
    pub auto_install: bool,
    pub hover: Option<UpdatesTarget>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToggleRow {
    pub card: Rect,
    pub text: Rect,
    pub toggle: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpdatesLayout {
    pub card: Rect,
    pub text: Rect,
    pub button: Rect,
    pub check: ToggleRow,
    pub auto: ToggleRow,
}

impl UpdatesState {
    pub fn new(version: &str) -> Self {
        Self {
            version: version.to_string(),
            status: UpdateStatus::Idle,
            check: true,
            auto_install: true,
            hover: None,
        }
    }

    pub fn title(&self) -> String {
        format!("Terminus {}", self.version)
    }

    /// Check now, or Install when a release waits and can be installed here.
    pub fn primary_button(&self) -> (UpdatesTarget, &'static str, ButtonKind) {
        match &self.status {
            UpdateStatus::Available {
                can_install: true, ..
            } => (UpdatesTarget::Install, "Install", ButtonKind::Primary),
            UpdateStatus::ReadyToRestart { .. } => {
                (UpdatesTarget::Install, "Restart", ButtonKind::Primary)
            }
            _ => (UpdatesTarget::Check, "Check now", ButtonKind::Secondary),
        }
    }

    pub fn button_disabled(&self) -> bool {
        self.status.busy()
    }

    pub fn layout(&self, content: Rect, m: Measure) -> UpdatesLayout {
        let col = column(content);
        let w = col.width.min(MAX_WIDTH);
        let (_, label, kind) = self.primary_button();
        let spec = button_spec(m, (0.0, 0.0), kind, ButtonSize::Medium, label);
        let card = Rect::new(col.x, col.y, w, VERSION_CARD_HEIGHT);
        let bh = ButtonSize::Medium.height();
        let button = Rect::new(
            card.right() - CARD_PAD_X - spec.width(),
            card.y + (card.height - bh) / 2.0,
            spec.width(),
            bh,
        );
        let text = Rect::new(
            card.x + CARD_PAD_X,
            card.y,
            (button.x - 16.0 - card.x - CARD_PAD_X).max(0.0),
            card.height,
        );
        let mut y = card.bottom() + 14.0;
        let row = |y: f32| {
            let card = Rect::new(col.x, y, w, TOGGLE_CARD_HEIGHT);
            let tr = toggle_rect(0.0, 0.0);
            let toggle = Rect::new(
                card.right() - CARD_PAD_X - TOGGLE_WIDTH,
                card.y + (card.height - TOGGLE_HEIGHT) / 2.0,
                tr.width,
                tr.height,
            );
            let text = Rect::new(
                card.x + CARD_PAD_X,
                card.y,
                (toggle.x - 16.0 - card.x - CARD_PAD_X).max(0.0),
                card.height,
            );
            ToggleRow { card, text, toggle }
        };
        let check = row(y);
        y = check.card.bottom() + ROW_GAP + 2.0;
        let auto = row(y);
        UpdatesLayout {
            card,
            text,
            button,
            check,
            auto,
        }
    }

    pub fn hit(&self, l: &UpdatesLayout, x: f32, y: f32) -> Option<UpdatesTarget> {
        if !self.button_disabled() && l.button.contains(x, y) {
            return Some(self.primary_button().0);
        }
        // The whole row toggles, not only the 44 px switch.
        if l.check.card.contains(x, y) {
            return Some(UpdatesTarget::CheckToggle);
        }
        l.auto
            .card
            .contains(x, y)
            .then_some(UpdatesTarget::AutoToggle)
    }

    pub fn hover(&mut self, content: Rect, m: Measure, x: f32, y: f32) -> bool {
        let l = self.layout(content, m);
        let t = self.hit(&l, x, y);
        let changed = t != self.hover;
        self.hover = t;
        changed
    }

    pub fn press(
        &mut self,
        content: Rect,
        m: Measure,
        x: f32,
        y: f32,
    ) -> Option<SettingsAction> {
        let l = self.layout(content, m);
        match self.hit(&l, x, y)? {
            UpdatesTarget::Check => {
                self.status = UpdateStatus::Checking;
                Some(SettingsAction::CheckUpdates)
            }
            UpdatesTarget::Install => Some(SettingsAction::InstallUpdate),
            UpdatesTarget::CheckToggle => {
                self.check = !self.check;
                Some(SettingsAction::SetCheckUpdates(self.check))
            }
            UpdatesTarget::AutoToggle => {
                self.auto_install = !self.auto_install;
                Some(SettingsAction::SetAutoInstall(self.auto_install))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_measure;
    use super::*;

    const CONTENT: Rect = Rect::new(260.0, 96.0, 1180.0, 804.0);

    fn center(r: Rect) -> (f32, f32) {
        (r.x + r.width / 2.0, r.y + r.height / 2.0)
    }

    #[test]
    fn title_and_statuses_use_the_mock_copy() {
        let mut s = UpdatesState::new("0.6.4");
        assert_eq!(s.title(), "Terminus 0.6.4");
        s.status = UpdateStatus::UpToDate;
        assert_eq!(s.status.text(), "You're up to date.");
        s.status = UpdateStatus::Available {
            version: "0.7.0".into(),
            can_install: true,
        };
        assert_eq!(s.status.text(), "Version 0.7.0 is available.");
    }

    #[test]
    fn check_now_goes_to_checking_and_is_inert_while_busy() {
        let mut s = UpdatesState::new("0.6.4");
        let l = s.layout(CONTENT, &mut test_measure);
        let (x, y) = center(l.button);
        assert_eq!(
            s.press(CONTENT, &mut test_measure, x, y),
            Some(SettingsAction::CheckUpdates)
        );
        assert_eq!(s.status, UpdateStatus::Checking);
        assert_eq!(s.press(CONTENT, &mut test_measure, x, y), None);
    }

    #[test]
    fn available_release_turns_the_button_into_install() {
        let mut s = UpdatesState::new("0.6.4");
        s.status = UpdateStatus::Available {
            version: "0.7.0".into(),
            can_install: true,
        };
        let l = s.layout(CONTENT, &mut test_measure);
        let (x, y) = center(l.button);
        assert_eq!(
            s.press(CONTENT, &mut test_measure, x, y),
            Some(SettingsAction::InstallUpdate)
        );
        // Not installable from here: keeps Check now.
        s.status = UpdateStatus::Available {
            version: "0.7.0".into(),
            can_install: false,
        };
        assert_eq!(s.primary_button().1, "Check now");
    }

    #[test]
    fn toggles_flip_and_report_the_new_value() {
        let mut s = UpdatesState::new("0.6.4");
        let l = s.layout(CONTENT, &mut test_measure);
        let (x, y) = center(l.check.toggle);
        assert_eq!(
            s.press(CONTENT, &mut test_measure, x, y),
            Some(SettingsAction::SetCheckUpdates(false))
        );
        let (x, y) = center(l.auto.card);
        assert_eq!(
            s.press(CONTENT, &mut test_measure, x, y),
            Some(SettingsAction::SetAutoInstall(false))
        );
        assert!(!s.check && !s.auto_install);
        let (x, y) = center(l.check.card);
        assert_eq!(
            s.press(CONTENT, &mut test_measure, x, y),
            Some(SettingsAction::SetCheckUpdates(true))
        );
    }

    #[test]
    fn cards_stack_in_order() {
        let s = UpdatesState::new("0.6.4");
        let l = s.layout(CONTENT, &mut test_measure);
        assert!(l.check.card.y > l.card.bottom());
        assert!(l.auto.card.y > l.check.card.bottom());
        assert!(l.button.right() <= l.card.right() - CARD_PAD_X + 0.01);
        assert!(l.check.toggle.right() <= l.check.card.right());
    }
}
