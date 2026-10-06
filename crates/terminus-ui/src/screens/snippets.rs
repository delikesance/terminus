//! Snippets view: adapter from the shell's [`ViewInput`] to
//! [`crate::views::snippets::SnippetsView`] (filter, cards, Paste / Run,
//! delete, New snippet). The view itself lives in `views::snippets`.

pub use crate::views::snippets::{SnippetsAction, SnippetsHit, SnippetsView};

use super::{ViewAction, ViewInput, ViewKey, ViewOutcome};
use crate::geom::Rect;

/// Pixels scrolled per wheel line.
pub const WHEEL_LINE: f32 = 40.0;

fn act(a: Option<SnippetsAction>) -> Option<ViewOutcome> {
    a.map(|a| ViewOutcome::Action(ViewAction::Snippets(a)))
}

fn redraw_if(changed: bool) -> ViewOutcome {
    if changed {
        ViewOutcome::Redraw
    } else {
        ViewOutcome::Consumed
    }
}

/// Whether `(x, y)` is a button (hand cursor).
pub fn is_clickable(view: &SnippetsView, content: Rect, x: f32, y: f32) -> bool {
    matches!(
        view.hit_test(content, x, y),
        Some(
            SnippetsHit::NewButton
                | SnippetsHit::Paste(_)
                | SnippetsHit::Run(_)
                | SnippetsHit::Delete(_)
        )
    )
}

/// Route one shell input to the Snippets view.
pub fn handle(view: &mut SnippetsView, content: Rect, input: &ViewInput) -> ViewOutcome {
    match input {
        ViewInput::Press { x, y, .. } => {
            let was_focused = view.filter_focused;
            let a = view.press(content, *x, *y);
            act(a).unwrap_or(redraw_if(was_focused != view.filter_focused))
        }
        ViewInput::ContextPress { x, y } => {
            act(view.secondary_press(content, *x, *y)).unwrap_or(ViewOutcome::Consumed)
        }
        ViewInput::Move { x, y, .. } => redraw_if(view.hover_at(content, *x, *y)),
        // Shell: lines > 0 scrolls towards the end; the view: dy < 0 does.
        ViewInput::Wheel { lines, .. } => {
            redraw_if(view.wheel(content, -lines * WHEEL_LINE))
        }
        ViewInput::Release { .. } => ViewOutcome::Consumed,
        ViewInput::Key { key, mods } => {
            let consumed = match key {
                ViewKey::Text(t) if !mods.ctrl && !mods.logo && !mods.alt => {
                    // Typing anywhere in the view filters.
                    view.filter_focused = true;
                    view.type_text(t)
                }
                ViewKey::Escape => view.escape(),
                other => super::text_edit(other, *mods).is_some_and(|e| view.edit(e)),
            };
            if consumed {
                ViewOutcome::Redraw
            } else {
                ViewOutcome::Ignored
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::list::card_layout;
    use crate::screens::{Screens, ViewMods};
    use crate::shell::WorkspaceView;
    use crate::views::snippets::seed;

    fn content() -> Rect {
        Rect::new(260.0, 104.0, 1172.0, 788.0)
    }

    fn screens() -> Screens {
        Screens {
            snippets: SnippetsView::new(seed()),
            ..Default::default()
        }
    }

    fn key(k: ViewKey) -> ViewInput {
        ViewInput::Key {
            key: k,
            mods: ViewMods::default(),
        }
    }

    fn press(x: f32, y: f32) -> ViewInput {
        ViewInput::Press {
            x,
            y,
            double: false,
        }
    }

    #[test]
    fn pressing_run_on_a_card_runs_its_command() {
        let mut s = screens();
        let widths = s.snippets.action_widths();
        let spec = s.snippets.card_spec(&widths);
        let card = s.snippets.card_rect(content(), 0);
        let run = card_layout(card, &spec).actions[1].unwrap();
        let out = s.handle(
            WorkspaceView::Snippets,
            content(),
            &press(run.x + 2.0, run.y + 2.0),
        );
        let cmd = s.snippets.visible()[0].cmd.clone();
        assert_eq!(
            out,
            ViewOutcome::Action(ViewAction::Snippets(SnippetsAction::RunInSession(cmd)))
        );
    }

    #[test]
    fn new_snippet_button_opens_the_dialog() {
        let mut s = screens();
        let r = s.snippets.new_button(content()).rect();
        let out = s.handle(
            WorkspaceView::Snippets,
            content(),
            &press(r.x + 2.0, r.y + 2.0),
        );
        assert_eq!(
            out,
            ViewOutcome::Action(ViewAction::Snippets(SnippetsAction::OpenNewSnippet))
        );
    }

    #[test]
    fn typing_filters_and_escape_clears_then_falls_through() {
        let mut s = screens();
        let all = s.snippets.visible().len();
        let needle: String = s.snippets.items[0].name.chars().take(4).collect();
        let v = WorkspaceView::Snippets;
        assert_eq!(
            s.handle(v, content(), &key(ViewKey::Text(needle.clone()))),
            ViewOutcome::Redraw
        );
        assert_eq!(s.snippets.filter.value, needle);
        assert!(s.snippets.visible().len() < all);
        assert_eq!(
            s.handle(v, content(), &key(ViewKey::Escape)),
            ViewOutcome::Redraw
        );
        assert!(s.snippets.filter.value.is_empty());
        // Second Esc unfocuses, the third is left to the shell (→ Terminal).
        assert_eq!(
            s.handle(v, content(), &key(ViewKey::Escape)),
            ViewOutcome::Redraw
        );
        assert_eq!(
            s.handle(v, content(), &key(ViewKey::Escape)),
            ViewOutcome::Ignored
        );
    }

    #[test]
    fn ctrl_shortcuts_are_not_typed_into_the_filter() {
        let mut s = screens();
        let out = s.handle(
            WorkspaceView::Snippets,
            content(),
            &ViewInput::Key {
                key: ViewKey::Text("k".into()),
                mods: ViewMods {
                    ctrl: true,
                    ..Default::default()
                },
            },
        );
        assert_eq!(out, ViewOutcome::Ignored);
        assert!(s.snippets.filter.value.is_empty());
    }

    #[test]
    fn buttons_show_a_hand_cursor() {
        let s = screens();
        let r = s.snippets.new_button(content()).rect();
        assert!(s.is_clickable(WorkspaceView::Snippets, content(), r.x + 2.0, r.y + 2.0));
        assert!(!s.is_clickable(
            WorkspaceView::Snippets,
            content(),
            content().right() - 2.0,
            content().bottom() - 2.0
        ));
    }
}
