//! Files view (stub): dual-pane SFTP browser for the selected machine.
//!
//! Bridge for now: when an SFTP session is open, the frontend paints the
//! existing SFTP pane (`renderer::sftp_pane`) in the content rect and
//! routes input to it; this state only covers the "nothing open yet"
//! empty state with its "Browse files" button.

use super::{
    empty_state, estimate_label, primary_button, ViewAction, ViewInput, ViewOutcome,
};
use crate::geom::Rect;

pub const TITLE: &str = "Files";
pub const CTA: &str = "Browse files";

/// What the Files view asks the frontend to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilesAction {
    /// Open the SFTP browser for the selected machine.
    OpenBrowser,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FilesState {
    /// Display name of the selected machine.
    pub machine_name: String,
    /// The selected machine is a stored SSH host (SFTP works).
    pub can_browse: bool,
    /// An SFTP session is open (the legacy pane is shown instead).
    pub session_open: bool,
    pub cta_hover: bool,
    /// Measured width of [`CTA`] (0 = estimate).
    pub cta_label_w: f32,
}

impl FilesState {
    pub fn body(&self) -> String {
        if self.can_browse {
            format!("Browse and transfer files on {}.", self.machine_name)
        } else {
            "File browsing works with SSH servers. Pick one in the sidebar.".into()
        }
    }

    /// The call-to-action button, when it is shown.
    pub fn cta_rect(&self, content: Rect) -> Option<Rect> {
        if !self.can_browse || self.session_open {
            return None;
        }
        let w = if self.cta_label_w > 0.0 {
            self.cta_label_w
        } else {
            estimate_label(CTA)
        };
        Some(primary_button(empty_state(content).button, w))
    }

    pub fn is_clickable(&self, content: Rect, x: f32, y: f32) -> bool {
        self.cta_rect(content).is_some_and(|r| r.contains(x, y))
    }

    pub fn handle(&mut self, content: Rect, input: &ViewInput) -> ViewOutcome {
        match *input {
            ViewInput::Press { x, y, .. } if self.is_clickable(content, x, y) => {
                ViewOutcome::Action(ViewAction::Files(FilesAction::OpenBrowser))
            }
            ViewInput::Move { x, y, .. } => {
                let hover = self.is_clickable(content, x, y);
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
    fn the_button_opens_the_browser_for_ssh_hosts_only() {
        let mut s = FilesState {
            machine_name: "jerem prod".into(),
            can_browse: true,
            ..Default::default()
        };
        let r = s.cta_rect(content()).expect("button");
        let out = s.handle(
            content(),
            &ViewInput::Press {
                x: r.x + 2.0,
                y: r.y + 2.0,
                double: false,
            },
        );
        assert_eq!(
            out,
            ViewOutcome::Action(ViewAction::Files(FilesAction::OpenBrowser))
        );
        s.can_browse = false;
        assert!(s.cta_rect(content()).is_none());
        assert!(s.body().contains("SSH"));
    }

    #[test]
    fn hovering_the_button_repaints_once() {
        let mut s = FilesState {
            can_browse: true,
            ..Default::default()
        };
        let r = s.cta_rect(content()).unwrap();
        let mv = ViewInput::Move {
            x: r.x + 1.0,
            y: r.y + 1.0,
            dragging: false,
        };
        assert_eq!(s.handle(content(), &mv), ViewOutcome::Redraw);
        assert_eq!(s.handle(content(), &mv), ViewOutcome::Consumed);
    }
}
