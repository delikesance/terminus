//! Files view: dual-pane SFTP browser for the selected machine.
//!
//! Showing Files on an SSH host opens the browser directly
//! ([`FilesState::take_auto_open`]); while a session is open the frontend
//! paints the SFTP pane (`renderer::sftp_pane`) in the content rect and
//! routes input to it. This state covers what is shown otherwise: the
//! empty state for machines without SFTP, the "Browse files" button when
//! the browser was not opened (vault left locked), and a failed
//! connection with its error and "Retry".

use super::{
    empty_state, estimate_label, primary_button, ViewAction, ViewInput, ViewOutcome,
};
use crate::geom::Rect;

pub const TITLE: &str = "Files";
pub const CTA: &str = "Browse files";
pub const RETRY: &str = "Retry";

/// What the Files view asks the frontend to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilesAction {
    /// Open the SFTP browser for the selected machine.
    OpenBrowser,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct FilesState {
    /// Sidebar id of the selected machine.
    pub machine_id: String,
    /// Display name of the selected machine.
    pub machine_name: String,
    /// The selected machine is a stored SSH host (SFTP works).
    pub can_browse: bool,
    /// An SFTP session is open (the legacy pane is shown instead).
    pub session_open: bool,
    pub cta_hover: bool,
    /// Measured width of [`CTA`] (0 = estimate).
    pub cta_label_w: f32,
    /// Measured width of [`RETRY`] (0 = estimate).
    pub retry_label_w: f32,
    /// Why the browser could not be opened on this machine.
    pub error: Option<String>,
    /// Machine the browser was already opened for while Files stays shown.
    auto_opened: Option<String>,
}

impl FilesState {
    /// Track the selected machine; a new machine forgets the last error.
    pub fn set_machine(&mut self, id: &str, name: &str, can_browse: bool) {
        if self.machine_id != id {
            self.machine_id = id.to_string();
            self.error = None;
            self.auto_opened = None;
        }
        if self.machine_name != name {
            self.machine_name = name.to_string();
        }
        self.can_browse = can_browse;
    }

    /// Whether to open the SFTP browser now: Files is `shown` for an SSH
    /// host with no session and no error, once per visit (a locked vault
    /// that the user dismissed is not asked again every frame).
    pub fn take_auto_open(&mut self, shown: bool) -> bool {
        if !shown {
            self.auto_opened = None;
            return false;
        }
        if !self.can_browse
            || self.session_open
            || self.error.is_some()
            || self.auto_opened.as_deref() == Some(self.machine_id.as_str())
        {
            return false;
        }
        self.auto_opened = Some(self.machine_id.clone());
        true
    }

    /// The browser could not be opened (or its connection failed).
    pub fn fail(&mut self, message: String) {
        self.error = Some(message);
    }

    /// Label of the call-to-action button.
    pub fn cta_label(&self) -> &'static str {
        if self.error.is_some() {
            RETRY
        } else {
            CTA
        }
    }

    pub fn body(&self) -> String {
        if let Some(err) = &self.error {
            let err = crate::components::list::elide_end(err.trim(), 96);
            format!("Couldn't open files on {}: {err}", self.machine_name)
        } else if self.can_browse {
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
        let measured = if self.error.is_some() {
            self.retry_label_w
        } else {
            self.cta_label_w
        };
        let w = if measured > 0.0 {
            measured
        } else {
            estimate_label(self.cta_label())
        };
        Some(primary_button(empty_state(content).button, w))
    }

    pub fn is_clickable(&self, content: Rect, x: f32, y: f32) -> bool {
        self.cta_rect(content).is_some_and(|r| r.contains(x, y))
    }

    pub fn handle(&mut self, content: Rect, input: &ViewInput) -> ViewOutcome {
        match *input {
            ViewInput::Press { x, y, .. } if self.is_clickable(content, x, y) => {
                // Retry: the frontend reopens; a new failure sets it again.
                self.error = None;
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

    #[test]
    fn opening_files_on_an_ssh_host_opens_the_browser_once() {
        let mut s = FilesState::default();
        s.set_machine("h1", "e2e local", true);
        assert!(s.take_auto_open(true), "first show opens SFTP");
        assert!(!s.take_auto_open(true), "not again every frame");
        // Leaving Files and coming back opens it again.
        assert!(!s.take_auto_open(false));
        assert!(s.take_auto_open(true));
        // Another machine while Files stays on screen.
        s.set_machine("h2", "Host-002", true);
        assert!(s.take_auto_open(true));
    }

    #[test]
    fn no_auto_open_without_sftp_or_with_a_session_open() {
        let mut s = FilesState::default();
        s.set_machine("local", "This computer", false);
        assert!(!s.take_auto_open(true));
        s.set_machine("h1", "e2e local", true);
        s.session_open = true;
        assert!(!s.take_auto_open(true));
    }

    #[test]
    fn a_failed_connection_shows_the_error_and_retry() {
        let mut s = FilesState::default();
        s.set_machine("h1", "e2e local", true);
        assert!(s.take_auto_open(true));
        s.fail("Connection refused".into());
        assert!(!s.take_auto_open(false));
        assert!(!s.take_auto_open(true), "an error never retries by itself");
        assert!(s.body().contains("Connection refused"), "{}", s.body());
        assert_eq!(s.cta_label(), RETRY);
        let r = s.cta_rect(content()).expect("retry button");
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
        assert!(s.error.is_none(), "retry clears the error");
        assert_eq!(s.cta_label(), CTA);
        // Picking another machine forgets the old error.
        s.fail("x".into());
        s.set_machine("h2", "Host-002", true);
        assert!(s.error.is_none());
    }
}
