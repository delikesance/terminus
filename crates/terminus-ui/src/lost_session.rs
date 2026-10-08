//! A host session whose connection dropped (the laptop slept, the network
//! changed, the server restarted).
//!
//! Without this the tab either froze (ssh waiting forever on a dead TCP
//! socket) or vanished. Instead the tab stays, and a card over its terminal
//! says what happened and offers Reconnect, which reopens the same host in
//! place. Paint-free: the card is a confirmation dialog laid out in the
//! terminal's content rect ([`crate::confirm::ConfirmSpec::layout_in`]), so
//! the painter and the hit-test share one layout.

use crate::components::overlay::{
    dialog_key, DialogFocus, DialogHit, DialogKey, DialogKind, DialogOutcome,
};
use crate::confirm::{ConfirmLayout, ConfirmSpec};
use crate::geom::Rect;

/// What OpenSSH exits with when the connection itself failed, as opposed
/// to the remote shell exiting (whose status `ssh` passes through).
pub const SSH_CONNECTION_ERROR: i32 = 255;

/// Whether an `ssh` process that exited with `code` lost its connection.
///
/// A remote `exit` returns the shell's own status (0 or a command's code)
/// and closes the tab as before; only 255 means ssh gave up on the link.
/// No code (killed by a signal, or the platform did not say) is not a
/// connection loss: the tab closes as it always did.
pub fn ssh_exit_is_connection_loss(code: Option<i32>) -> bool {
    code == Some(SSH_CONNECTION_ERROR)
}

pub const TITLE: &str = "Connection lost";
pub const RECONNECT: &str = "Reconnect";
pub const CLOSE_TAB: &str = "Close tab";

/// One sentence on why the link dropped, from what ssh printed last.
///
/// `output` is the session's lines top to bottom; ssh's own message is
/// the last thing it writes, so only the last few non-empty lines count
/// (an older "Broken pipe" in the scrollback is not this disconnect).
pub fn lost_reason(output: &[String]) -> &'static str {
    let tail = output
        .iter()
        .rev()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .take(3);
    for line in tail {
        let lower = line.to_ascii_lowercase();
        if lower.contains("not responding") || lower.starts_with("timeout") {
            return "The server stopped answering. The computer may have slept or the network changed.";
        }
        if lower.contains("closed by remote host") {
            return "The server closed the connection. It may have restarted.";
        }
        if lower.contains("broken pipe")
            || lower.contains("connection reset")
            || lower.contains("network is unreachable")
            || lower.contains("connection abort")
        {
            return "The network connection was interrupted.";
        }
    }
    "The connection to the server ended unexpectedly."
}

/// Result of a key or click on the card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LostOutcome {
    /// Nothing visible changed.
    Idle,
    /// Focus moved: repaint.
    Changed,
    Reconnect,
    Close,
}

/// The card shown over a tab whose connection dropped.
#[derive(Debug, Clone, PartialEq)]
pub struct LostSession {
    /// The terminal (route) that lost its connection.
    pub route_id: usize,
    /// Sidebar row the session was opened from; Reconnect reopens it.
    pub host_id: String,
    pub spec: ConfirmSpec,
    pub focus: DialogFocus,
    pub hover: Option<DialogFocus>,
}

impl LostSession {
    /// `name` is the host as the sidebar shows it; `output` is what the
    /// session printed last (see [`lost_reason`]).
    pub fn new(route_id: usize, host_id: &str, name: &str, output: &[String]) -> Self {
        let body = format!("{name}: {}", lost_reason(output));
        let spec =
            ConfirmSpec::new(DialogKind::Confirm, TITLE, body, CLOSE_TAB, RECONNECT);
        Self {
            route_id,
            host_id: host_id.to_string(),
            focus: spec.kind.default_focus(),
            spec,
            hover: None,
        }
    }

    /// The card centred over `area` (the terminal's content rect), which
    /// it dims.
    pub fn layout_in(&self, area: Rect) -> ConfirmLayout {
        self.spec.layout_in(area)
    }

    /// Enter runs the focused button (Reconnect first), Tab swaps focus.
    /// Escape does nothing: closing the tab is a button, never a reflex.
    pub fn key(&mut self, key: DialogKey) -> LostOutcome {
        if key == DialogKey::Escape {
            return LostOutcome::Idle;
        }
        match dialog_key(key, self.focus) {
            DialogOutcome::Confirm => LostOutcome::Reconnect,
            DialogOutcome::Cancel => LostOutcome::Close,
            DialogOutcome::Focus(focus) => {
                self.focus = focus;
                LostOutcome::Changed
            }
        }
    }

    /// Press at `(x, y)`. The dimmed terminal around the card is inert:
    /// the session behind it is gone.
    pub fn press(&mut self, area: Rect, x: f32, y: f32) -> LostOutcome {
        match self.layout_in(area).hit_test(x, y) {
            DialogHit::Confirm => LostOutcome::Reconnect,
            DialogHit::Cancel => LostOutcome::Close,
            DialogHit::Option | DialogHit::Inside | DialogHit::Scrim => LostOutcome::Idle,
        }
    }

    /// Track the pointer; true when the hovered button changed.
    pub fn hover_at(&mut self, area: Rect, x: f32, y: f32) -> bool {
        let hover = self.button_at(area, x, y);
        let changed = hover != self.hover;
        self.hover = hover;
        changed
    }

    /// The button under `(x, y)`, if any.
    pub fn button_at(&self, area: Rect, x: f32, y: f32) -> Option<DialogFocus> {
        match self.layout_in(area).hit_test(x, y) {
            DialogHit::Confirm => Some(DialogFocus::Confirm),
            DialogHit::Cancel => Some(DialogFocus::Cancel),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect {
        x: 300.0,
        y: 80.0,
        width: 900.0,
        height: 700.0,
    };

    fn lines(text: &[&str]) -> Vec<String> {
        text.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn only_ssh_255_counts_as_a_lost_connection() {
        assert!(ssh_exit_is_connection_loss(Some(255)));
        // `exit` on the remote, or a failing last command.
        assert!(!ssh_exit_is_connection_loss(Some(0)));
        assert!(!ssh_exit_is_connection_loss(Some(1)));
        assert!(!ssh_exit_is_connection_loss(Some(130)));
        assert!(!ssh_exit_is_connection_loss(None));
    }

    #[test]
    fn reason_names_a_server_that_stopped_answering() {
        let out = lines(&["user@box:~$ ", "Timeout, server box.lan not responding."]);
        assert!(lost_reason(&out).starts_with("The server stopped answering"));
    }

    #[test]
    fn reason_names_a_server_restart() {
        let out = lines(&["Connection to 10.0.0.5 closed by remote host.", "", ""]);
        assert!(lost_reason(&out).contains("may have restarted"));
    }

    #[test]
    fn reason_names_a_network_drop() {
        for line in [
            "client_loop: send disconnect: Broken pipe",
            "Read from remote host box: Connection reset by peer",
            "packet_write_wait: Connection to 1.2.3.4 port 22: Broken pipe",
        ] {
            assert_eq!(
                lost_reason(&lines(&[line])),
                "The network connection was interrupted.",
                "{line}"
            );
        }
    }

    #[test]
    fn reason_ignores_old_scrollback_and_falls_back() {
        let out = lines(&["Broken pipe", "a", "b", "c", "Connection to x closed."]);
        assert_eq!(
            lost_reason(&out),
            "The connection to the server ended unexpectedly."
        );
        assert_eq!(
            lost_reason(&[]),
            "The connection to the server ended unexpectedly."
        );
    }

    #[test]
    fn card_names_the_host_and_offers_reconnect_first() {
        let lost = LostSession::new(7, "h1", "prod-db", &lines(&["Broken pipe"]));
        assert_eq!(lost.spec.title, "Connection lost");
        assert_eq!(
            lost.spec.body,
            "prod-db: The network connection was interrupted."
        );
        assert_eq!(
            (lost.spec.cancel.as_str(), lost.spec.confirm.as_str()),
            ("Close tab", "Reconnect")
        );
        assert_eq!(lost.focus, DialogFocus::Confirm);
        assert_eq!((lost.route_id, lost.host_id.as_str()), (7, "h1"));
    }

    #[test]
    fn enter_reconnects_tab_then_enter_closes_and_escape_is_inert() {
        let mut lost = LostSession::new(1, "h", "h", &[]);
        assert_eq!(lost.key(DialogKey::Escape), LostOutcome::Idle);
        assert_eq!(lost.key(DialogKey::Enter), LostOutcome::Reconnect);
        assert_eq!(lost.key(DialogKey::Tab), LostOutcome::Changed);
        assert_eq!(lost.focus, DialogFocus::Cancel);
        assert_eq!(lost.key(DialogKey::Enter), LostOutcome::Close);
    }

    #[test]
    fn card_sits_in_the_terminal_area_and_its_buttons_are_clickable() {
        let mut lost = LostSession::new(1, "h", "h", &[]);
        let l = lost.layout_in(AREA).dialog;
        assert_eq!(l.scrim, AREA);
        assert!(l.dialog.x >= AREA.x && l.dialog.right() <= AREA.right());
        assert!(l.dialog.y >= AREA.y && l.dialog.bottom() <= AREA.bottom());

        let r = l.confirm;
        assert_eq!(
            lost.press(AREA, r.x + 4.0, r.y + 4.0),
            LostOutcome::Reconnect
        );
        let c = l.cancel;
        assert_eq!(lost.press(AREA, c.x + 4.0, c.y + 4.0), LostOutcome::Close);
        // The dimmed terminal and the card body do nothing.
        assert_eq!(
            lost.press(AREA, AREA.x + 2.0, AREA.y + 2.0),
            LostOutcome::Idle
        );
        let t = l.title;
        assert_eq!(lost.press(AREA, t.x + 2.0, t.y + 2.0), LostOutcome::Idle);
    }

    #[test]
    fn hover_tracks_the_button_under_the_pointer() {
        let mut lost = LostSession::new(1, "h", "h", &[]);
        let r = lost.layout_in(AREA).dialog.confirm;
        assert!(lost.hover_at(AREA, r.x + 2.0, r.y + 2.0));
        assert_eq!(lost.hover, Some(DialogFocus::Confirm));
        assert!(!lost.hover_at(AREA, r.x + 3.0, r.y + 3.0));
        assert!(lost.hover_at(AREA, AREA.x + 1.0, AREA.y + 1.0));
        assert_eq!(lost.hover, None);
    }
}
