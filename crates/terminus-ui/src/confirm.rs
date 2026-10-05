//! Confirmation dialogs (delete host / group, quit, SFTP name conflict).
//!
//! Pure geometry and key semantics on top of the overlay dialog shell in
//! [`crate::components::overlay`]. The painter measures real glyphs, but
//! hit-testing has no font, so both sides use the *same* width estimate
//! ([`estimate_text_width`]) for the wrap and the button widths: a control
//! can never be drawn where it cannot be clicked.

use crate::components::overlay::{
    action_width, dialog_key, dialog_layout_at, wrap_text, DialogFocus, DialogHit,
    DialogKey, DialogKind, DialogLayout, DialogOutcome, DIALOG_PAD, DIALOG_WIDTH,
};
use crate::geom::Rect;

/// Average advance of the UI face as a fraction of the font size. Slightly
/// generous so an estimated line never overruns what the painter draws.
pub const AVG_ADVANCE: f32 = 0.55;
pub const BODY_FONT: f32 = 14.0;
pub const BUTTON_FONT: f32 = 14.0;
/// Longest title (chars) before it is elided to keep one line at 22px.
pub const TITLE_MAX_CHARS: usize = 30;

pub fn estimate_text_width(text: &str, size: f32) -> f32 {
    text.chars().count() as f32 * size * AVG_ADVANCE
}

/// Button labels are Medium/SemiBold and short: a tighter estimate keeps the
/// label (left-aligned by the Button painter) near the middle.
pub const BUTTON_ADVANCE: f32 = 0.54;

/// Content of one confirmation dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmSpec {
    pub kind: DialogKind,
    pub title: String,
    pub body: String,
    pub confirm: String,
    pub cancel: String,
    /// Checkbox row label (`DialogKind::WithOption`).
    pub option: Option<String>,
}

/// A spec laid out at a position: rects plus the wrapped body lines.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfirmLayout {
    pub dialog: DialogLayout,
    pub lines: Vec<String>,
}

impl ConfirmLayout {
    pub fn hit_test(&self, x: f32, y: f32) -> DialogHit {
        self.dialog.hit_test(x, y)
    }
}

/// Elide `title` to [`TITLE_MAX_CHARS`] with a trailing ellipsis.
pub fn elide_title(title: &str) -> String {
    if title.chars().count() <= TITLE_MAX_CHARS {
        return title.to_string();
    }
    let head: String = title.chars().take(TITLE_MAX_CHARS - 1).collect();
    format!("{head}\u{2026}")
}

impl ConfirmSpec {
    pub fn new(
        kind: DialogKind,
        title: &str,
        body: impl Into<String>,
        cancel: &str,
        confirm: &str,
    ) -> Self {
        Self {
            kind,
            title: elide_title(title),
            body: body.into(),
            confirm: confirm.to_string(),
            cancel: cancel.to_string(),
            option: None,
        }
    }

    pub fn with_option(mut self, label: &str) -> Self {
        self.kind = DialogKind::WithOption;
        self.option = Some(label.to_string());
        self
    }

    /// Dialog centred in `area` (a window, or the pane a modal covers).
    pub fn layout_in(&self, area: Rect) -> ConfirmLayout {
        let inner = DIALOG_WIDTH - 2.0 * DIALOG_PAD;
        let lines = wrap_text(&self.body, inner, |s| estimate_text_width(s, BODY_FONT));
        let cancel_w = action_width(self.cancel.chars().count() as f32 * BUTTON_FONT * BUTTON_ADVANCE);
        let confirm_w = action_width(self.confirm.chars().count() as f32 * BUTTON_FONT * BUTTON_ADVANCE);
        // Height first (position does not change it), then centre.
        let probe = dialog_layout_at(
            0.0,
            0.0,
            self.kind,
            lines.len(),
            cancel_w,
            confirm_w,
            (area.width, area.height),
        );
        let x = (area.x + (area.width - DIALOG_WIDTH) / 2.0).round();
        let y = (area.y + (area.height - probe.dialog.height) / 2.0).round();
        let mut dialog = dialog_layout_at(
            x,
            y,
            self.kind,
            lines.len(),
            cancel_w,
            confirm_w,
            (area.width, area.height),
        );
        dialog.scrim = area;
        ConfirmLayout { dialog, lines }
    }

    /// Dialog centred in a window of `window` logical pixels.
    pub fn layout(&self, window: (f32, f32)) -> ConfirmLayout {
        self.layout_in(Rect::new(0.0, 0.0, window.0, window.1))
    }
}

/// What a confirmed dialog does (the frontend owns the side effect).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmAction {
    DeleteHost(String),
    DeleteGroup(String),
    /// Quit the application (confirmed by the frontend, not the chrome).
    Quit,
}

/// Result of a key or click on an open prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmOutcome {
    /// Nothing visible changed.
    Idle,
    /// Focus or checkbox moved: repaint.
    Changed,
    Cancel,
    Confirm,
}

/// An open confirmation raised from the chrome (delete host / group).
#[derive(Debug, Clone, PartialEq)]
pub struct ConfirmPrompt {
    pub spec: ConfirmSpec,
    pub action: ConfirmAction,
    pub focus: DialogFocus,
    pub option_checked: bool,
    /// Button under the pointer, if any.
    pub hover: Option<DialogFocus>,
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

impl ConfirmPrompt {
    pub fn new(spec: ConfirmSpec, action: ConfirmAction) -> Self {
        let focus = spec.kind.default_focus();
        Self {
            spec,
            action,
            focus,
            option_checked: false,
            hover: None,
        }
    }

    /// Delete a stored host; `sessions` open tabs close with it.
    pub fn delete_host(id: &str, name: &str, sessions: usize) -> Self {
        let mut body = String::from(
            "It disappears from every computer you sync with. Your keys and snippets stay.",
        );
        if sessions > 0 {
            body.push_str(&format!(
                " Its {} close too.",
                plural(sessions, "open session", "open sessions")
            ));
        }
        Self::new(
            ConfirmSpec::new(
                DialogKind::Destructive,
                &format!("Delete {name}?"),
                body,
                "Cancel",
                "Delete",
            ),
            ConfirmAction::DeleteHost(id.to_string()),
        )
    }

    /// Delete a host group; its servers move to the top level.
    pub fn delete_group(id: &str, name: &str, hosts: usize) -> Self {
        let body = if hosts == 0 {
            "The group is empty. Nothing else is removed.".to_string()
        } else {
            format!(
                "Its {} move out of the group. Nothing else is removed.",
                plural(hosts, "server", "servers")
            )
        };
        Self::new(
            ConfirmSpec::new(
                DialogKind::Destructive,
                &format!("Delete {name}?"),
                body,
                "Cancel",
                "Delete",
            ),
            ConfirmAction::DeleteGroup(id.to_string()),
        )
    }

    /// Quit Terminus; `sessions` open tabs close with it.
    pub fn quit(sessions: usize) -> Self {
        let body = match sessions {
            0 => "Quitting closes this window.".to_string(),
            1 => "1 session is still open. Quitting closes it.".to_string(),
            n => format!("{n} sessions are still open. Quitting closes them."),
        };
        Self::new(
            ConfirmSpec::new(
                DialogKind::Confirm,
                "Quit Terminus?",
                body,
                "Keep working",
                "Quit",
            ),
            ConfirmAction::Quit,
        )
    }

    pub fn layout(&self, window: (f32, f32)) -> ConfirmLayout {
        self.spec.layout(window)
    }

    /// Esc cancels, Enter runs the focused button, Tab swaps focus.
    pub fn key(&mut self, key: DialogKey) -> ConfirmOutcome {
        match dialog_key(key, self.focus) {
            DialogOutcome::Cancel => ConfirmOutcome::Cancel,
            DialogOutcome::Confirm => ConfirmOutcome::Confirm,
            DialogOutcome::Focus(f) => {
                self.focus = f;
                ConfirmOutcome::Changed
            }
        }
    }

    /// Track the pointer; true when the hovered button changed.
    pub fn hover_at(&mut self, window: (f32, f32), x: f32, y: f32) -> bool {
        let hover = match self.layout(window).hit_test(x, y) {
            DialogHit::Confirm => Some(DialogFocus::Confirm),
            DialogHit::Cancel => Some(DialogFocus::Cancel),
            _ => None,
        };
        let changed = hover != self.hover;
        self.hover = hover;
        changed
    }

    /// Press at `(x, y)`; the scrim and Cancel dismiss, the dialog body is inert.
    pub fn press(&mut self, window: (f32, f32), x: f32, y: f32) -> ConfirmOutcome {
        match self.layout(window).hit_test(x, y) {
            DialogHit::Confirm => ConfirmOutcome::Confirm,
            DialogHit::Cancel | DialogHit::Scrim => ConfirmOutcome::Cancel,
            DialogHit::Option => {
                self.option_checked = !self.option_checked;
                ConfirmOutcome::Changed
            }
            DialogHit::Inside => ConfirmOutcome::Idle,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIN: (f32, f32) = (1440.0, 900.0);

    #[test]
    fn delete_prompts_focus_cancel_so_a_stray_enter_is_safe() {
        let mut p = ConfirmPrompt::delete_host("h1", "jerem prod", 0);
        assert_eq!(p.focus, DialogFocus::Cancel);
        assert_eq!(p.key(DialogKey::Enter), ConfirmOutcome::Cancel);
    }

    #[test]
    fn tab_moves_focus_then_enter_confirms_the_delete() {
        let mut p = ConfirmPrompt::delete_group("g1", "prod", 2);
        assert_eq!(p.key(DialogKey::Tab), ConfirmOutcome::Changed);
        assert_eq!(p.focus, DialogFocus::Confirm);
        assert_eq!(p.key(DialogKey::Enter), ConfirmOutcome::Confirm);
        assert_eq!(p.action, ConfirmAction::DeleteGroup("g1".into()));
    }

    #[test]
    fn quit_prompt_counts_sessions_and_defaults_to_quit() {
        let mut p = ConfirmPrompt::quit(3);
        assert_eq!(p.spec.title, "Quit Terminus?");
        assert_eq!(
            p.spec.body,
            "3 sessions are still open. Quitting closes them."
        );
        assert_eq!(
            (p.spec.cancel.as_str(), p.spec.confirm.as_str()),
            ("Keep working", "Quit")
        );
        assert_eq!(p.spec.kind, DialogKind::Confirm);
        assert_eq!(p.focus, DialogFocus::Confirm);
        assert_eq!(p.key(DialogKey::Enter), ConfirmOutcome::Confirm);
        assert!(ConfirmPrompt::quit(1).spec.body.starts_with("1 session is"));
        assert_eq!(
            ConfirmPrompt::quit(0).spec.body,
            "Quitting closes this window."
        );
    }

    #[test]
    fn escape_always_cancels() {
        let mut p = ConfirmPrompt::delete_host("h1", "x", 0);
        p.focus = DialogFocus::Confirm;
        assert_eq!(p.key(DialogKey::Escape), ConfirmOutcome::Cancel);
    }

    #[test]
    fn copy_names_the_host_and_its_open_sessions() {
        let p = ConfirmPrompt::delete_host("h1", "jerem prod", 2);
        assert_eq!(p.spec.title, "Delete jerem prod?");
        assert!(p.spec.body.contains("2 open sessions"));
        let one = ConfirmPrompt::delete_host("h1", "x", 1);
        assert!(one.spec.body.contains("1 open session close"));
        let none = ConfirmPrompt::delete_host("h1", "x", 0);
        assert!(!none.spec.body.contains("session"));
        assert_eq!(p.spec.confirm, "Delete");
        assert_eq!(p.spec.kind, DialogKind::Destructive);
    }

    #[test]
    fn long_titles_are_elided() {
        let name = "a-very-long-host-name-that-keeps-going-forever";
        let p = ConfirmPrompt::delete_host("h", name, 0);
        assert!(p.spec.title.chars().count() <= TITLE_MAX_CHARS);
        assert!(p.spec.title.ends_with('\u{2026}'));
    }

    #[test]
    fn layout_is_centred_and_buttons_sit_inside_the_dialog() {
        let p = ConfirmPrompt::delete_host("h1", "jerem prod", 3);
        let l = p.layout(WIN);
        let d = l.dialog.dialog;
        assert!((d.x + d.width / 2.0 - 720.0).abs() <= 1.0);
        assert!((d.y + d.height / 2.0 - 450.0).abs() <= 1.0);
        assert!(l.dialog.cancel.x >= d.x && l.dialog.confirm.right() <= d.right());
        assert!(l.lines.len() >= 2, "long body wraps");
    }

    #[test]
    fn layout_in_a_pane_centres_in_that_pane_and_scrims_it() {
        let pane = Rect::new(300.0, 100.0, 800.0, 600.0);
        let l =
            ConfirmSpec::new(DialogKind::Confirm, "t", "b", "No", "Yes").layout_in(pane);
        let d = l.dialog.dialog;
        assert!((d.x + d.width / 2.0 - 700.0).abs() <= 1.0);
        assert!((d.y + d.height / 2.0 - 400.0).abs() <= 1.0);
        assert_eq!(l.dialog.scrim, pane);
    }

    #[test]
    fn press_hits_buttons_scrim_and_the_inert_body() {
        let mut p = ConfirmPrompt::delete_host("h1", "x", 0);
        let l = p.layout(WIN).dialog;
        let c = l.confirm;
        assert_eq!(p.press(WIN, c.x + 3.0, c.y + 3.0), ConfirmOutcome::Confirm);
        let k = l.cancel;
        assert_eq!(p.press(WIN, k.x + 3.0, k.y + 3.0), ConfirmOutcome::Cancel);
        assert_eq!(p.press(WIN, 2.0, 2.0), ConfirmOutcome::Cancel);
        let t = l.title;
        assert_eq!(p.press(WIN, t.x + 3.0, t.y + 3.0), ConfirmOutcome::Idle);
    }

    #[test]
    fn option_row_toggles_on_click() {
        let spec = ConfirmSpec::new(DialogKind::Confirm, "t", "b", "No", "Yes")
            .with_option("Do this for every conflict");
        let mut p = ConfirmPrompt::new(spec, ConfirmAction::DeleteHost("h".into()));
        let o = p.layout(WIN).dialog.option.unwrap();
        assert_eq!(p.press(WIN, o.x + 2.0, o.y + 2.0), ConfirmOutcome::Changed);
        assert!(p.option_checked);
    }
}
