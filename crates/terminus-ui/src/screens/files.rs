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

/// One SFTP browser per machine. The selected machine's session is the
/// "active" one (shown in Files, takes input); the others stay parked,
/// still running, so their transfers go on and they come back as they
/// were left when their machine is selected again.
#[derive(Debug)]
pub struct MachineSessions<T> {
    parked: Vec<(String, T)>,
    /// Machines whose last tab closed while their browser was busy (or
    /// not yet reaped).
    closing: Vec<String>,
}

impl<T> Default for MachineSessions<T> {
    fn default() -> Self {
        Self {
            parked: Vec::new(),
            closing: Vec::new(),
        }
    }
}

impl<T> MachineSessions<T> {
    /// Make `active` the session of `machine`: park the current one under
    /// its own machine and bring `machine`'s back (or none). Returns
    /// whether `active` changed.
    pub fn follow(
        &mut self,
        active: &mut Option<T>,
        owner: impl Fn(&T) -> &str,
        machine: &str,
    ) -> bool {
        if active.as_ref().is_some_and(|s| owner(s) == machine) {
            return false;
        }
        let mut changed = false;
        if let Some(prev) = active.take() {
            let id = owner(&prev).to_string();
            self.parked.push((id, prev));
            changed = true;
        }
        if let Some(next) = self.take(machine) {
            *active = Some(next);
            changed = true;
        }
        changed
    }

    /// Park `session` under its machine; a session already parked there is
    /// handed back (for the caller to close).
    pub fn park(&mut self, session: T, owner: impl Fn(&T) -> &str) -> Option<T> {
        let id = owner(&session).to_string();
        let old = self.take(&id);
        self.parked.push((id, session));
        old
    }

    /// Remove and return `machine`'s parked session.
    pub fn take(&mut self, machine: &str) -> Option<T> {
        let i = self.parked.iter().position(|(id, _)| id == machine)?;
        Some(self.parked.remove(i).1)
    }

    pub fn contains(&self, machine: &str) -> bool {
        self.parked.iter().any(|(id, _)| id == machine)
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
        self.parked.iter_mut().map(|(_, s)| s)
    }

    pub fn len(&self) -> usize {
        self.parked.len()
    }

    pub fn is_empty(&self) -> bool {
        self.parked.is_empty()
    }

    /// Note which machines lost their last tab (`before` → `after` are
    /// the machine ids of the open tabs). Their browsers are closed by
    /// [`Self::reap`] once idle; a machine that gets a tab back first is
    /// spared.
    pub fn tabs_changed(&mut self, before: &[String], after: &[String]) {
        for id in before {
            if !after.contains(id) && !self.closing.contains(id) {
                self.closing.push(id.clone());
            }
        }
        self.closing.retain(|id| !after.contains(id));
    }

    /// Hand back (for the caller to close) the browsers of machines that
    /// lost their last tab and are not `busy` (a transfer or a conflict
    /// prompt in flight). A busy one is kept and retried on a later
    /// call, so it closes when its transfer ends.
    pub fn reap(
        &mut self,
        active: &mut Option<T>,
        owner: impl Fn(&T) -> &str,
        busy: impl Fn(&T) -> bool,
    ) -> Vec<T> {
        let mut out = Vec::new();
        let closing = std::mem::take(&mut self.closing);
        for id in closing {
            let in_active = active.as_ref().is_some_and(|s| owner(s) == id);
            let session = if in_active {
                active.as_ref()
            } else {
                self.parked.iter().find(|(m, _)| *m == id).map(|(_, s)| s)
            };
            match session {
                Some(s) if busy(s) => self.closing.push(id),
                Some(_) if in_active => out.extend(active.take()),
                Some(_) => out.extend(self.take(&id)),
                None => {}
            }
        }
        out
    }

    /// Hand back (for the caller to close) every browser, shown or
    /// parked, that `stale` matches — its host was deleted or its
    /// connection settings changed — busy or not.
    pub fn retire_where(
        &mut self,
        active: &mut Option<T>,
        owner: impl Fn(&T) -> &str,
        stale: impl Fn(&T) -> bool,
    ) -> Vec<T> {
        let mut out = Vec::new();
        if active.as_ref().is_some_and(&stale) {
            let s = active.take().expect("checked");
            self.closing.retain(|id| id != owner(&s));
            out.push(s);
        }
        let mut i = 0;
        while i < self.parked.len() {
            if stale(&self.parked[i].1) {
                let (id, s) = self.parked.remove(i);
                self.closing.retain(|c| *c != id);
                out.push(s);
            } else {
                i += 1;
            }
        }
        out
    }
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
    /// Machines whose browser waits for its credentials (key read).
    connecting: Vec<String>,
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
            || self.is_connecting()
            || self.error.is_some()
            || self.auto_opened.as_deref() == Some(self.machine_id.as_str())
        {
            return false;
        }
        self.auto_opened = Some(self.machine_id.clone());
        true
    }

    /// `machine`'s browser waits for its credentials: Files shows
    /// "Connecting to …" without a button until it opens or fails.
    pub fn begin_connecting(&mut self, machine: &str) {
        if !self.connecting.iter().any(|m| m == machine) {
            self.connecting.push(machine.to_string());
        }
        if self.machine_id == machine {
            self.error = None;
        }
    }

    /// `machine`'s wait is over (browser opened, or the read failed).
    pub fn end_connecting(&mut self, machine: &str) {
        self.connecting.retain(|m| m != machine);
    }

    /// Whether the selected machine's browser is waiting to open.
    pub fn is_connecting(&self) -> bool {
        self.connecting.contains(&self.machine_id)
    }

    /// The browser could not be opened (or its connection failed).
    pub fn fail(&mut self, message: String) {
        let machine = self.machine_id.clone();
        self.end_connecting(&machine);
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
        if self.is_connecting() {
            format!("Connecting to {}…", self.machine_name)
        } else if let Some(err) = &self.error {
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
        if !self.can_browse || self.session_open || self.is_connecting() {
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

    /// A stand-in session: (machine id, progress of a transfer).
    #[derive(Debug, PartialEq)]
    struct Sess(&'static str, u32);

    fn owner(s: &Sess) -> &str {
        s.0
    }

    #[test]
    fn each_machine_keeps_its_own_browser() {
        let mut shelf = MachineSessions::default();
        let mut active = Some(Sess("a", 0));
        // B selected: A's browser is parked, B has none yet.
        assert!(shelf.follow(&mut active, owner, "b"));
        assert!(active.is_none(), "B must not show A's browser");
        assert_eq!(shelf.len(), 1);
        active = Some(Sess("b", 0));
        // Back to A: A's browser comes back intact, B's is parked.
        shelf.iter_mut().for_each(|s| s.1 = 42);
        assert!(shelf.follow(&mut active, owner, "a"));
        assert_eq!(active, Some(Sess("a", 42)));
        assert!(shelf.follow(&mut active, owner, "b"));
        assert_eq!(active, Some(Sess("b", 0)));
        assert_eq!(shelf.len(), 1);
    }

    #[test]
    fn staying_on_a_machine_changes_nothing() {
        let mut shelf = MachineSessions::default();
        let mut active = Some(Sess("a", 1));
        assert!(!shelf.follow(&mut active, owner, "a"));
        assert_eq!(active, Some(Sess("a", 1)));
        let mut none: Option<Sess> = None;
        assert!(!shelf.follow(&mut none, owner, "local"));
        assert!(shelf.is_empty());
    }

    #[test]
    fn a_parked_session_is_taken_back_once() {
        let mut shelf = MachineSessions::default();
        assert!(shelf.park(Sess("a", 3), owner).is_none());
        assert!(shelf.contains("a"));
        // Parking a second one for the same machine hands the old one back.
        assert_eq!(shelf.park(Sess("a", 4), owner), Some(Sess("a", 3)));
        assert_eq!(shelf.take("a"), Some(Sess("a", 4)));
        assert_eq!(shelf.take("a"), None);
    }

    /// A transfer is running while progress is strictly between 0 and 100.
    fn busy(s: &Sess) -> bool {
        (1..100).contains(&s.1)
    }

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_last_tab_closing_closes_an_idle_parked_browser() {
        let mut shelf = MachineSessions::default();
        let mut active = Some(Sess("b", 0));
        shelf.park(Sess("a", 0), owner);
        // A's last tab closes; B still has one.
        shelf.tabs_changed(&ids(&["a", "b"]), &ids(&["b"]));
        let closed = shelf.reap(&mut active, owner, busy);
        assert_eq!(closed, vec![Sess("a", 0)]);
        assert!(!shelf.contains("a"));
        assert_eq!(active, Some(Sess("b", 0)), "B's browser is untouched");
        // Nothing left to do on the next pump.
        assert!(shelf.reap(&mut active, owner, busy).is_empty());
    }

    #[test]
    fn a_running_transfer_keeps_the_browser_until_it_ends() {
        let mut shelf = MachineSessions::default();
        let mut active: Option<Sess> = None;
        shelf.park(Sess("a", 40), owner);
        shelf.tabs_changed(&ids(&["a"]), &ids(&[]));
        assert!(shelf.reap(&mut active, owner, busy).is_empty());
        assert!(shelf.contains("a"), "never cancel a transfer silently");
        // The transfer finishes: the next pump closes the browser.
        shelf.iter_mut().for_each(|s| s.1 = 100);
        assert_eq!(shelf.reap(&mut active, owner, busy), vec![Sess("a", 100)]);
    }

    #[test]
    fn a_tab_reopened_before_the_transfer_ends_keeps_the_browser() {
        let mut shelf = MachineSessions::default();
        let mut active: Option<Sess> = None;
        shelf.park(Sess("a", 40), owner);
        shelf.tabs_changed(&ids(&["a"]), &ids(&[]));
        shelf.tabs_changed(&ids(&[]), &ids(&["a"]));
        shelf.iter_mut().for_each(|s| s.1 = 100);
        assert!(shelf.reap(&mut active, owner, busy).is_empty());
        assert!(shelf.contains("a"));
    }

    #[test]
    fn the_shown_browser_is_closed_too_when_its_machine_lost_its_tabs() {
        let mut shelf: MachineSessions<Sess> = MachineSessions::default();
        let mut active = Some(Sess("a", 0));
        shelf.tabs_changed(&ids(&["a"]), &ids(&[]));
        assert_eq!(shelf.reap(&mut active, owner, busy), vec![Sess("a", 0)]);
        assert!(active.is_none());
    }

    #[test]
    fn a_browser_parked_before_any_tab_is_not_reaped() {
        // "Files on X is ready: select it in the sidebar" parks a browser
        // for a machine that never had a tab: no tab closed, no reaping.
        let mut shelf = MachineSessions::default();
        let mut active: Option<Sess> = None;
        shelf.park(Sess("x", 0), owner);
        shelf.tabs_changed(&ids(&["b"]), &ids(&["b"]));
        assert!(shelf.reap(&mut active, owner, busy).is_empty());
        assert!(shelf.contains("x"));
    }

    #[test]
    fn retiring_closes_matching_browsers_wherever_they_are() {
        let mut shelf = MachineSessions::default();
        let mut active = Some(Sess("a", 40));
        shelf.park(Sess("b", 0), owner);
        shelf.park(Sess("c", 0), owner);
        // Host A edited (busy or not: its connection is stale), C deleted.
        let closed = shelf.retire_where(&mut active, owner, |s| s.0 != "b");
        assert_eq!(closed, vec![Sess("a", 40), Sess("c", 0)]);
        assert!(active.is_none());
        assert!(shelf.contains("b") && shelf.len() == 1);
    }

    fn content() -> Rect {
        Rect::new(260.0, 104.0, 1172.0, 788.0)
    }

    #[test]
    fn waiting_for_the_key_shows_connecting_without_a_button() {
        let mut s = FilesState::default();
        s.set_machine("a", "e2e local", true);
        s.begin_connecting("a");
        assert!(s.is_connecting());
        assert_eq!(s.body(), "Connecting to e2e local…");
        assert!(
            s.cta_rect(content()).is_none(),
            "no Browse button meanwhile"
        );
        assert!(!s.take_auto_open(true), "no second request while waiting");

        // Another machine selected: its own state, not A's wait.
        s.set_machine("b", "box", true);
        assert!(!s.is_connecting());
        s.set_machine("a", "e2e local", true);
        assert!(s.is_connecting(), "A is still waiting for its key");

        // The browser opened, or the key read failed: the wait is over.
        s.end_connecting("a");
        assert!(!s.is_connecting());
        s.begin_connecting("a");
        s.fail("No SSH key".into());
        assert!(!s.is_connecting());
        assert!(s.body().contains("No SSH key"));
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
