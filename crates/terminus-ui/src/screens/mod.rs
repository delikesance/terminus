//! Input dispatch of the workspace views that replace the terminal in
//! the main card. Files (empty state), Snippets and Home are routed here;
//! Tunnels, History and Settings (whose input needs the frontend's text
//! measure or process runtime) are routed by `rioterm::screen::workspace`.
//!
//! **Contract** (see `SHELL_CONTRACT.md`): each view owns one file here
//! and one painter in `rioterm::renderer::screens`. The shell gives a view
//! its `content` rect ([`crate::shell::Shell::content_rect`]) and routes
//! every pointer / key event inside it to [`Screens::handle`] while that
//! view is shown; the terminal never sees them. A view answers with a
//! [`ViewOutcome`]: nothing, a repaint, or a [`ViewAction`] the frontend
//! carries out (open SFTP, paste a snippet, …) in
//! `rioterm::screen::workspace::<view>`.
//!
//! The views themselves live in [`crate::views`].

pub mod files;
pub mod home;
pub mod snippets;

use crate::geom::Rect;
use crate::shell::WorkspaceView;
use crate::text_field::{TextEdit, TextMoveKind};

/// Content padding of a view (the mock's 28px).
pub const PAD: f32 = 28.0;
/// Empty-state title / body sizes and the gap between blocks.
pub const EMPTY_TITLE_SIZE: f32 = 22.0;
pub const EMPTY_BODY_SIZE: f32 = 14.0;
pub const EMPTY_GAP: f32 = 12.0;

/// Modifier keys held with a [`ViewKey`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ViewMods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub logo: bool,
}

/// A key, already decoded by the frontend (no winit types here).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ViewKey {
    /// Printable text (one key press, IME commit, paste).
    Text(String),
    Enter,
    Escape,
    Backspace,
    Delete,
    Tab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    F2,
}

/// Map a key to the shared text edit every field understands: Backspace,
/// Delete, caret moves (Shift extends the selection, Ctrl jumps by word)
/// and Ctrl+A. `None` for keys a field does not edit with.
pub fn text_edit(key: &ViewKey, mods: ViewMods) -> Option<TextEdit> {
    let kind = if mods.shift {
        TextMoveKind::Extend
    } else {
        TextMoveKind::Collapse
    };
    let by_word = mods.ctrl;
    Some(match key {
        ViewKey::Backspace => TextEdit::Backspace { by_word },
        ViewKey::Delete => TextEdit::Delete { by_word },
        ViewKey::Left => TextEdit::Left { kind, by_word },
        ViewKey::Right => TextEdit::Right { kind, by_word },
        ViewKey::Home => TextEdit::Home { kind },
        ViewKey::End => TextEdit::End { kind },
        ViewKey::Text(t) if mods.ctrl && t.eq_ignore_ascii_case("a") => {
            TextEdit::SelectAll
        }
        _ => return None,
    })
}

/// One input event for the shown view, in logical window pixels.
#[derive(Debug, Clone, PartialEq)]
pub enum ViewInput {
    /// Primary button press (`double` for a double-click).
    Press {
        x: f32,
        y: f32,
        double: bool,
    },
    /// Primary button release.
    Release {
        x: f32,
        y: f32,
    },
    /// Secondary button press (context menu).
    ContextPress {
        x: f32,
        y: f32,
    },
    /// Pointer move (with `dragging` while the primary button is held).
    Move {
        x: f32,
        y: f32,
        dragging: bool,
    },
    /// Wheel, in lines (positive = scroll content down / towards the end).
    Wheel {
        x: f32,
        y: f32,
        lines: f32,
    },
    Key {
        key: ViewKey,
        mods: ViewMods,
    },
}

impl ViewInput {
    /// Pointer position of a pointer event.
    pub fn position(&self) -> Option<(f32, f32)> {
        match *self {
            ViewInput::Press { x, y, .. }
            | ViewInput::Release { x, y }
            | ViewInput::ContextPress { x, y }
            | ViewInput::Move { x, y, .. }
            | ViewInput::Wheel { x, y, .. } => Some((x, y)),
            ViewInput::Key { .. } => None,
        }
    }
}

/// Something a view asks the frontend to do.
#[derive(Debug, Clone, PartialEq)]
pub enum ViewAction {
    Files(files::FilesAction),
    Snippets(snippets::SnippetsAction),
    Home(home::HomeAction),
}

/// A view's answer to one [`ViewInput`].
#[derive(Debug, Clone, PartialEq)]
pub enum ViewOutcome {
    /// Not handled. Keys fall back to the shell (Esc returns to the
    /// terminal); pointer events are still swallowed.
    Ignored,
    /// Handled; view state changed and needs a repaint.
    Redraw,
    /// Handled; nothing visible changed.
    Consumed,
    /// Handled; the frontend must perform this action (then repaint).
    Action(ViewAction),
}

/// The state of every view. Lives in `Chrome::screens`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Screens {
    pub files: files::FilesState,
    pub snippets: snippets::SnippetsView,
    pub home: home::HomeState,
}

impl Screens {
    /// Route `input` to the view on screen. Terminal is not a view here:
    /// its input goes to the PTY and this returns [`ViewOutcome::Ignored`].
    pub fn handle(
        &mut self,
        view: WorkspaceView,
        content: Rect,
        input: &ViewInput,
    ) -> ViewOutcome {
        match view {
            // Terminal input goes to the PTY; Tunnels, History and Settings
            // need the frontend (text measure, process runtime) and are
            // routed by `rioterm::screen::workspace` before reaching here.
            WorkspaceView::Terminal
            | WorkspaceView::Tunnels
            | WorkspaceView::History
            | WorkspaceView::Settings(_) => ViewOutcome::Ignored,
            WorkspaceView::Files => self.files.handle(content, input),
            WorkspaceView::Snippets => {
                snippets::handle(&mut self.snippets, content, input)
            }
            WorkspaceView::Home => self.home.handle(content, input),
        }
    }

    /// Whether the pointer at `(x, y)` should show a hand cursor.
    pub fn is_clickable(
        &self,
        view: WorkspaceView,
        content: Rect,
        x: f32,
        y: f32,
    ) -> bool {
        match view {
            WorkspaceView::Terminal
            | WorkspaceView::Tunnels
            | WorkspaceView::History
            | WorkspaceView::Settings(_) => false,
            WorkspaceView::Files => self.files.is_clickable(content, x, y),
            WorkspaceView::Snippets => {
                snippets::is_clickable(&self.snippets, content, x, y)
            }
            WorkspaceView::Home => self.home.is_clickable(content, x, y),
        }
    }
}

/// Shared stub layout: a centred empty state (title, one line of body,
/// optional button) inside `content`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EmptyState {
    /// Top-left of the title line.
    pub title: (f32, f32),
    /// Top-left of the body line.
    pub body: (f32, f32),
    /// Top-left of the call-to-action button.
    pub button: (f32, f32),
}

/// Lay an empty state out at the top-left of `content` (28px padding, like
/// the mock's list views).
pub fn empty_state(content: Rect) -> EmptyState {
    let x = content.x + PAD;
    let y = content.y + PAD;
    let body_y = y + EMPTY_TITLE_SIZE + EMPTY_GAP;
    EmptyState {
        title: (x, y),
        body: (x, body_y),
        button: (x, body_y + EMPTY_BODY_SIZE + 2.0 * EMPTY_GAP),
    }
}

/// Large primary button rect at `origin` for a label of `label_w`.
pub fn primary_button(origin: (f32, f32), label_w: f32) -> Rect {
    use crate::components::button::{ButtonKind, ButtonSize, ButtonSpec};
    ButtonSpec::label(
        origin,
        ButtonKind::Primary,
        ButtonSize::Large,
        label_w,
        false,
    )
    .rect()
}

/// Rough width of a 14px Medium/SemiBold label until measured.
pub fn estimate_label(label: &str) -> f32 {
    label.chars().count() as f32 * 8.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::SettingsPage;

    fn content() -> Rect {
        Rect::new(260.0, 104.0, 1172.0, 788.0)
    }

    #[test]
    fn the_terminal_is_not_a_view() {
        let mut s = Screens::default();
        let press = ViewInput::Press {
            x: 400.0,
            y: 400.0,
            double: false,
        };
        assert_eq!(
            s.handle(WorkspaceView::Terminal, content(), &press),
            ViewOutcome::Ignored
        );
    }

    #[test]
    fn every_view_answers_input_without_panicking() {
        let mut s = Screens::default();
        let views = [
            WorkspaceView::Files,
            WorkspaceView::Tunnels,
            WorkspaceView::Snippets,
            WorkspaceView::History,
            WorkspaceView::Home,
            WorkspaceView::Settings(SettingsPage::Keys),
            WorkspaceView::Settings(SettingsPage::Updates),
        ];
        let inputs = [
            ViewInput::Press {
                x: 500.0,
                y: 500.0,
                double: false,
            },
            ViewInput::Move {
                x: 500.0,
                y: 500.0,
                dragging: false,
            },
            ViewInput::Wheel {
                x: 500.0,
                y: 500.0,
                lines: 3.0,
            },
            ViewInput::Key {
                key: ViewKey::Text("a".into()),
                mods: ViewMods::default(),
            },
        ];
        for v in views {
            for i in &inputs {
                let _ = s.handle(v, content(), i);
                let _ = s.is_clickable(v, content(), 500.0, 500.0);
            }
        }
    }

    #[test]
    fn empty_state_is_padded_inside_the_content() {
        let e = empty_state(content());
        assert_eq!(e.title, (288.0, 132.0));
        assert!(e.body.1 > e.title.1);
        assert!(e.button.1 > e.body.1);
    }

    #[test]
    fn positions_come_from_pointer_events_only() {
        assert_eq!(
            ViewInput::Release { x: 1.0, y: 2.0 }.position(),
            Some((1.0, 2.0))
        );
        assert_eq!(
            ViewInput::Key {
                key: ViewKey::Enter,
                mods: ViewMods::default()
            }
            .position(),
            None
        );
    }
}
