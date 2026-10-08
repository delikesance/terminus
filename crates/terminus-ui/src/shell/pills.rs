//! Session pills row: the selected machine's open sessions (one pill per
//! terminal tab), the new-session "+", and split-right / split-down icon
//! buttons on the right (`App.dc.html`, 52px row).

use crate::components::navigation::{session_pill, PillState};
use crate::geom::Rect;

pub const PAD_LEFT: f32 = 20.0;
pub const PAD_RIGHT: f32 = 12.0;
pub const GAP: f32 = 4.0;
/// Round "+" (new session) button.
pub const PLUS_SIZE: f32 = 30.0;
pub const PLUS_ICON: f32 = 14.0;
/// Split icon buttons.
pub const SPLIT_SIZE: f32 = 34.0;
pub const SPLIT_RADIUS: f32 = 9.0;
pub const SPLIT_ICON: f32 = 15.0;
/// Longest label before it is elided.
pub const MAX_LABEL: f32 = 180.0;
/// Narrowest label slot while a pill is being renamed, so an empty draft
/// still shows a field with its caret.
pub const RENAME_MIN_LABEL: f32 = 48.0;

/// One open session of the selected machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionPill {
    /// Index into the window's tab list.
    pub tab_index: usize,
    pub label: String,
    /// The focused tab.
    pub active: bool,
    /// Output arrived while the tab was in the background.
    pub new_output: bool,
    /// Pinned home tabs cannot be closed.
    pub closable: bool,
}

impl SessionPill {
    /// Visual state, given whether the pointer is over it.
    pub fn state(&self, hovered: bool) -> PillState {
        if self.active {
            PillState::Active
        } else if hovered {
            PillState::Hover
        } else if self.new_output {
            PillState::NewOutput
        } else {
            PillState::Default
        }
    }

    /// Whether the × is shown (and hit-tested).
    pub fn shows_close(&self, hovered: bool) -> bool {
        // Same rule as `session_pill::style(..).close`: hover and active.
        self.closable
            && matches!(self.state(hovered), PillState::Hover | PillState::Active)
    }
}

/// What a press on the row hit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PillsHit {
    /// Pill by position in the pills slice.
    Pill(usize),
    Close(usize),
    NewSession,
    SplitRight,
    SplitDown,
}

/// Boxes of the pills row.
#[derive(Debug, Clone, PartialEq)]
pub struct PillsGeom {
    pub pills: Vec<Rect>,
    /// Label width each pill was laid out with (clamped to [`MAX_LABEL`]).
    pub label_w: Vec<f32>,
    pub plus: Rect,
    pub split_right: Rect,
    pub split_down: Rect,
}

/// Lay the row out. `label_w[i]` is the measured label width of
/// `pills[i]` (missing entries fall back to an estimate); `hover` is the
/// hovered pill's position.
pub fn layout(
    row: &Rect,
    pills: &[SessionPill],
    label_w: &[f32],
    hover: Option<usize>,
) -> PillsGeom {
    let cy = row.y + row.height * 0.5;
    let split_down = Rect::new(
        row.right() - PAD_RIGHT - SPLIT_SIZE,
        cy - SPLIT_SIZE * 0.5,
        SPLIT_SIZE,
        SPLIT_SIZE,
    );
    let split_right = Rect::new(
        split_down.x - GAP - SPLIT_SIZE,
        split_down.y,
        SPLIT_SIZE,
        SPLIT_SIZE,
    );
    let mut x = row.x + PAD_LEFT;
    let mut rects = Vec::with_capacity(pills.len());
    let mut widths = Vec::with_capacity(pills.len());
    for (i, pill) in pills.iter().enumerate() {
        let hovered = hover == Some(i);
        let st = pill.state(hovered);
        let lw = label_w
            .get(i)
            .copied()
            .unwrap_or_else(|| estimate_label(&pill.label))
            .min(MAX_LABEL);
        let dot = st == PillState::NewOutput;
        let w = session_pill::width(lw, pill.shows_close(hovered), dot);
        rects.push(Rect::new(
            x,
            cy - session_pill::HEIGHT * 0.5,
            w,
            session_pill::HEIGHT,
        ));
        widths.push(lw);
        x += w + GAP;
    }
    let plus = Rect::new(x, cy - PLUS_SIZE * 0.5, PLUS_SIZE, PLUS_SIZE);
    PillsGeom {
        pills: rects,
        label_w: widths,
        plus,
        split_right,
        split_down,
    }
}

/// What a pill says for a session.
///
/// A name the user gave the tab wins. Otherwise the shell's own title is
/// used, minus the `user@host:` prefix most prompts put in it (the header
/// already names the machine), so pills read `~/app`, `htop`, …. The
/// machine name that new tabs are titled with says nothing in a row of
/// that machine's sessions, so it falls through too.
pub fn session_label(
    custom: Option<&str>,
    title: Option<&str>,
    machine_name: &str,
    position: usize,
) -> String {
    let clean = |s: &str| s.trim().to_string();
    if let Some(c) = custom.map(str::trim).filter(|c| !c.is_empty()) {
        if c != machine_name {
            return clean(c);
        }
    }
    if let Some(t) = title.map(str::trim).filter(|t| !t.is_empty()) {
        let after_prompt = match t.split_once(':') {
            Some((who, rest)) if who.contains('@') && !who.contains(' ') => rest.trim(),
            _ => t,
        };
        if !after_prompt.is_empty() {
            return clean(after_prompt);
        }
    }
    format!("Session {}", position + 1)
}

/// Label of a pill whose session runs on another machine (SSH, WSL).
/// `osc_title` is only what the remote shell set (OSC 0/2): until it
/// does, the tab's template title describes the local `ssh` process
/// (its cwd, its program), so the pill names the machine instead.
pub fn remote_session_label(
    custom: Option<&str>,
    osc_title: Option<&str>,
    machine_name: &str,
    position: usize,
) -> String {
    let has_custom = custom
        .map(str::trim)
        .is_some_and(|c| !c.is_empty() && c != machine_name);
    let has_title = osc_title.is_some_and(|t| !t.trim().is_empty());
    if has_custom || has_title {
        session_label(custom, osc_title, machine_name, position)
    } else {
        machine_name.to_string()
    }
}

/// Rough label width when the painter has not measured it yet.
pub fn estimate_label(label: &str) -> f32 {
    label.chars().count() as f32 * 7.5
}

impl PillsGeom {
    pub fn hit(
        &self,
        pills: &[SessionPill],
        hover: Option<usize>,
        x: f32,
        y: f32,
    ) -> Option<PillsHit> {
        if self.split_right.contains(x, y) {
            return Some(PillsHit::SplitRight);
        }
        if self.split_down.contains(x, y) {
            return Some(PillsHit::SplitDown);
        }
        if self.plus.contains(x, y) {
            return Some(PillsHit::NewSession);
        }
        for (i, rect) in self.pills.iter().enumerate() {
            let close = pills
                .get(i)
                .is_some_and(|p| p.shows_close(hover == Some(i)));
            match session_pill::hit(rect, close, x, y) {
                Some(crate::components::navigation::PillHit::Close) => {
                    return Some(PillsHit::Close(i))
                }
                Some(crate::components::navigation::PillHit::Body) => {
                    return Some(PillsHit::Pill(i))
                }
                None => {}
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pill(i: usize, active: bool) -> SessionPill {
        SessionPill {
            tab_index: i,
            label: format!("s{i}"),
            active,
            new_output: false,
            closable: true,
        }
    }

    fn row() -> Rect {
        Rect::new(260.0, 104.0, 1172.0, 52.0)
    }

    #[test]
    fn pills_start_20px_in_centred_and_the_plus_follows() {
        let pills = vec![pill(0, true), pill(1, false)];
        let g = layout(&row(), &pills, &[40.0, 30.0], None);
        assert_eq!(g.pills[0].x, 280.0);
        assert_eq!(g.pills[0].y + 15.0, 130.0);
        // Active pill shows its ×, the idle one does not.
        assert_eq!(g.pills[0].width, session_pill::width(40.0, true, false));
        assert_eq!(g.pills[1].width, session_pill::width(30.0, false, false));
        assert_eq!(g.pills[1].x, g.pills[0].right() + GAP);
        assert_eq!(g.plus.x, g.pills[1].right() + GAP);
        assert_eq!(g.plus.width, 30.0);
    }

    #[test]
    fn split_buttons_hug_the_right_edge() {
        let g = layout(&row(), &[], &[], None);
        assert_eq!(g.split_down.right(), row().right() - 12.0);
        assert_eq!(g.split_right.right() + GAP, g.split_down.x);
        assert_eq!(g.split_down.width, 34.0);
        assert_eq!(g.plus.x, 280.0, "no sessions: + leads the row");
    }

    #[test]
    fn hits_resolve_pill_close_plus_and_splits() {
        let pills = vec![pill(3, true), pill(5, false)];
        let g = layout(&row(), &pills, &[40.0, 30.0], None);
        let p0 = g.pills[0];
        assert_eq!(
            g.hit(&pills, None, p0.x + 4.0, p0.y + 15.0),
            Some(PillsHit::Pill(0))
        );
        let c = session_pill::close_rect(&p0);
        assert_eq!(
            g.hit(&pills, None, c.x + 2.0, c.y + 2.0),
            Some(PillsHit::Close(0))
        );
        // Idle pill: its right end is body, not close.
        let p1 = g.pills[1];
        assert_eq!(
            g.hit(&pills, None, p1.right() - 2.0, p1.y + 15.0),
            Some(PillsHit::Pill(1))
        );
        assert_eq!(
            g.hit(&pills, None, g.plus.x + 5.0, g.plus.y + 5.0),
            Some(PillsHit::NewSession)
        );
        assert_eq!(
            g.hit(&pills, None, g.split_right.x + 1.0, g.split_right.y + 1.0),
            Some(PillsHit::SplitRight)
        );
        assert_eq!(
            g.hit(&pills, None, g.split_down.x + 1.0, g.split_down.y + 1.0),
            Some(PillsHit::SplitDown)
        );
        assert_eq!(g.hit(&pills, None, 900.0, 110.0), None);
    }

    #[test]
    fn hovering_an_idle_pill_reveals_its_close() {
        let pills = vec![pill(0, true), pill(1, false)];
        let g = layout(&row(), &pills, &[40.0, 30.0], Some(1));
        assert_eq!(g.pills[1].width, session_pill::width(30.0, true, false));
        let c = session_pill::close_rect(&g.pills[1]);
        assert_eq!(
            g.hit(&pills, Some(1), c.x + 2.0, c.y + 2.0),
            Some(PillsHit::Close(1))
        );
    }

    #[test]
    fn pinned_sessions_never_show_a_close() {
        let mut p = pill(0, true);
        p.closable = false;
        assert!(!p.shows_close(true));
    }

    #[test]
    fn background_output_shows_the_dot_until_focused() {
        let mut p = pill(1, false);
        p.new_output = true;
        assert_eq!(p.state(false), PillState::NewOutput);
        assert_eq!(p.state(true), PillState::Hover);
        p.active = true;
        assert_eq!(p.state(false), PillState::Active);
        let g = layout(&row(), &[p.clone()], &[30.0], None);
        assert!(g.pills[0].width > 0.0);
    }

    #[test]
    fn labels_prefer_user_names_then_the_shell_title_without_its_prompt() {
        assert_eq!(session_label(Some("db"), Some("x"), "prod", 0), "db");
        assert_eq!(
            session_label(Some("prod"), Some("ubuntu@prod: ~/app"), "prod", 0),
            "~/app"
        );
        assert_eq!(session_label(None, Some("htop"), "prod", 0), "htop");
        assert_eq!(session_label(None, Some("a: b"), "prod", 0), "a: b");
        assert_eq!(session_label(None, Some("  "), "prod", 2), "Session 3");
        assert_eq!(session_label(Some("prod"), None, "prod", 0), "Session 1");
    }

    #[test]
    fn a_remote_tab_shows_the_machine_until_its_shell_sets_a_title() {
        // Still connecting: only the local `ssh` process is known, so its
        // local cwd/program title must not leak into the pill.
        assert_eq!(
            remote_session_label(None, None, "jerem prod", 0),
            "jerem prod"
        );
        assert_eq!(
            remote_session_label(Some("jerem prod"), Some(""), "jerem prod", 1),
            "jerem prod"
        );
        // The remote shell titled the tab: same rules as a local one.
        assert_eq!(
            remote_session_label(None, Some("ubuntu@prod: ~/app"), "jerem prod", 0),
            "~/app"
        );
        // A name the user gave still wins.
        assert_eq!(
            remote_session_label(Some("db"), None, "jerem prod", 0),
            "db"
        );
    }

    #[test]
    fn long_labels_are_clamped() {
        let g = layout(&row(), &[pill(0, false)], &[900.0], None);
        assert_eq!(g.label_w[0], MAX_LABEL);
    }
}
