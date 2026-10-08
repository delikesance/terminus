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

/// How an `ssh` session's process ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEnd {
    /// Exit status 0: the user logged out. The tab closes as before.
    Clean,
    /// Exit status 255: ssh gave up on the link itself.
    ConnectionLost,
    /// Any other non-zero status: the remote shell's last command status
    /// after the user left (`exit`, Ctrl-D, Ctrl-C). Closes the tab.
    Exited(i32),
    /// Killed by a signal.
    Signaled(i32),
    /// The platform reported nothing; the tab closes as before.
    Unknown,
}

/// Classify an exit from its decoded `code` and `signal` (at most one is
/// set: a process either exits or is killed).
pub fn classify_exit(code: Option<i32>, signal: Option<i32>) -> SessionEnd {
    match (code, signal) {
        (Some(0), _) => SessionEnd::Clean,
        (Some(SSH_CONNECTION_ERROR), _) => SessionEnd::ConnectionLost,
        (Some(code), _) => SessionEnd::Exited(code),
        (None, Some(signal)) => SessionEnd::Signaled(signal),
        (None, None) => SessionEnd::Unknown,
    }
}

impl SessionEnd {
    /// Whether the tab stays open behind a card: only a dropped link
    /// (exit 255) or a local ssh killed by a signal. Any other exit status
    /// is the remote shell's last command status, i.e. the user leaving
    /// (`exit`, Ctrl-D, Ctrl-C), and closes the tab as before.
    pub fn keeps_tab(self) -> bool {
        matches!(self, SessionEnd::ConnectionLost | SessionEnd::Signaled(_))
    }

    /// The status as the card words it, e.g. `exit status 1` or
    /// `signal 9 (SIGKILL)`.
    pub fn status_text(self) -> String {
        match self {
            SessionEnd::Clean => "exit status 0".to_string(),
            SessionEnd::ConnectionLost => format!("exit status {SSH_CONNECTION_ERROR}"),
            SessionEnd::Exited(code) => format!("exit status {code}"),
            SessionEnd::Signaled(signal) => match signal_name(signal) {
                Some(name) => format!("signal {signal} ({name})"),
                None => format!("signal {signal}"),
            },
            SessionEnd::Unknown => "unknown status".to_string(),
        }
    }
}

/// Name of the common POSIX signals a session can die from.
fn signal_name(signal: i32) -> Option<&'static str> {
    Some(match signal {
        1 => "SIGHUP",
        2 => "SIGINT",
        3 => "SIGQUIT",
        6 => "SIGABRT",
        9 => "SIGKILL",
        11 => "SIGSEGV",
        13 => "SIGPIPE",
        14 => "SIGALRM",
        15 => "SIGTERM",
        _ => return None,
    })
}

pub const TITLE: &str = "Connection lost";
pub const KILLED_TITLE: &str = "Session killed";
pub const RECONNECT: &str = "Reconnect";
pub const CLOSE_TAB: &str = "Close tab";
pub const CLOSE_PANE: &str = "Close pane";

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
        Self::ended(
            route_id,
            host_id,
            name,
            output,
            SessionEnd::ConnectionLost,
            true,
        )
        .expect("a lost connection always has a card")
    }

    /// The card for a session that ended as `end` (see [`classify_exit`]),
    /// or `None` when that end just closes the tab.
    /// `whole_tab` is false when the session is one pane of a split tab:
    /// the card then closes just that pane.
    pub fn ended(
        route_id: usize,
        host_id: &str,
        name: &str,
        output: &[String],
        end: SessionEnd,
        whole_tab: bool,
    ) -> Option<Self> {
        let (title, body) = match end {
            SessionEnd::ConnectionLost => (
                TITLE,
                format!("{name}: {} ({})", lost_reason(output), end.status_text()),
            ),
            SessionEnd::Signaled(_) => (
                KILLED_TITLE,
                format!("{name}: ssh was killed by {}.", end.status_text()),
            ),
            SessionEnd::Clean | SessionEnd::Exited(_) | SessionEnd::Unknown => {
                return None
            }
        };
        let close = if whole_tab { CLOSE_TAB } else { CLOSE_PANE };
        let spec = ConfirmSpec::new(DialogKind::Confirm, title, body, close, RECONNECT);
        Some(Self {
            route_id,
            host_id: host_id.to_string(),
            focus: spec.kind.default_focus(),
            spec,
            hover: None,
        })
    }

    /// Re-label the close button when the tab is split or unsplit after the
    /// card was made: "Close tab" for a lone session, "Close pane" in a split.
    pub fn set_whole_tab(&mut self, whole_tab: bool) {
        self.spec.cancel = if whole_tab { CLOSE_TAB } else { CLOSE_PANE }.to_string();
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
    fn classify_separates_clean_255_other_codes_and_signals() {
        assert_eq!(classify_exit(Some(0), None), SessionEnd::Clean);
        assert_eq!(classify_exit(Some(255), None), SessionEnd::ConnectionLost);
        // `exit` after a failing command, a ssh startup error, Ctrl-C.
        assert_eq!(classify_exit(Some(1), None), SessionEnd::Exited(1));
        assert_eq!(classify_exit(Some(130), None), SessionEnd::Exited(130));
        assert_eq!(classify_exit(None, Some(9)), SessionEnd::Signaled(9));
        assert_eq!(classify_exit(None, None), SessionEnd::Unknown);
    }

    #[test]
    fn only_255_and_a_signal_keep_the_tab() {
        assert!(!SessionEnd::Clean.keeps_tab());
        assert!(!SessionEnd::Unknown.keeps_tab());
        assert!(SessionEnd::ConnectionLost.keeps_tab());
        assert!(SessionEnd::Signaled(15).keeps_tab());
        // The remote shell's last status after `exit` / Ctrl-D / Ctrl-C:
        // the user leaving, not a drop.
        assert!(!SessionEnd::Exited(1).keeps_tab());
        assert!(!SessionEnd::Exited(130).keeps_tab());
    }

    #[test]
    fn status_text_names_the_code_or_signal() {
        assert_eq!(SessionEnd::ConnectionLost.status_text(), "exit status 255");
        assert_eq!(SessionEnd::Exited(1).status_text(), "exit status 1");
        assert_eq!(SessionEnd::Signaled(9).status_text(), "signal 9 (SIGKILL)");
        assert_eq!(
            SessionEnd::Signaled(15).status_text(),
            "signal 15 (SIGTERM)"
        );
        assert_eq!(SessionEnd::Signaled(64).status_text(), "signal 64");
    }

    #[test]
    fn ends_that_close_the_tab_get_no_card() {
        for end in [
            SessionEnd::Clean,
            SessionEnd::Unknown,
            SessionEnd::Exited(1),
        ] {
            assert_eq!(LostSession::ended(3, "h", "box", &[], end, true), None);
        }
    }

    #[test]
    fn signaled_card_names_the_signal() {
        let lost = LostSession::ended(3, "h", "box", &[], SessionEnd::Signaled(9), true)
            .unwrap();
        assert_eq!(lost.spec.title, "Session killed");
        assert_eq!(lost.spec.body, "box: ssh was killed by signal 9 (SIGKILL).");
    }

    #[test]
    fn lost_card_for_255_keeps_its_reason_and_adds_the_status() {
        let lost = LostSession::ended(
            3,
            "h",
            "box",
            &lines(&["Broken pipe"]),
            SessionEnd::ConnectionLost,
            true,
        )
        .unwrap();
        assert_eq!(lost.spec.title, "Connection lost");
        assert_eq!(
            lost.spec.body,
            "box: The network connection was interrupted. (exit status 255)"
        );
    }

    #[test]
    fn the_close_label_follows_the_tab_being_split_or_not() {
        let mut lost = LostSession::new(3, "h", "box", &[]);
        assert_eq!(lost.spec.cancel, "Close tab");
        lost.set_whole_tab(false);
        assert_eq!(lost.spec.cancel, "Close pane");
        lost.set_whole_tab(true);
        assert_eq!(lost.spec.cancel, "Close tab");
    }

    #[test]
    fn a_split_pane_card_closes_the_pane_not_the_tab() {
        let pane = LostSession::ended(3, "h", "box", &[], SessionEnd::Signaled(1), false)
            .unwrap();
        assert_eq!(pane.spec.cancel, "Close pane");
        let tab = LostSession::ended(3, "h", "box", &[], SessionEnd::Signaled(1), true)
            .unwrap();
        assert_eq!(tab.spec.cancel, "Close tab");
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
            "prod-db: The network connection was interrupted. (exit status 255)"
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
