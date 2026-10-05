//! History view (`terminus_ui::views::history`): loads the selected
//! machine's recorded commands when the tab opens, routes input, and runs
//! "Run again" in the machine's active session.

use super::super::Screen;
use crate::renderer::views::history as painter;
use terminus_ui::screens::{ViewInput, ViewKey, ViewOutcome};
use terminus_ui::shell::WorkspaceView;
use terminus_ui::views::history::HistoryAction;

/// Rows loaded per machine.
const LIMIT: usize = 500;

impl Screen<'_> {
    /// Load the rows of `machine_id` when the History tab is shown for it
    /// (tab opened, or machine switched while on it). Leaving the tab
    /// forgets the machine so the next visit shows new commands.
    pub(super) fn sync_history_view(&mut self, machine_id: &str) {
        if self.chrome.shell.view() != WorkspaceView::History {
            self.history_for = None;
            return;
        }
        if self.history_for.as_deref() == Some(machine_id) {
            return;
        }
        let entries = crate::history_worker::load_blocking(machine_id, LIMIT);
        let home = dirs::home_dir().map(|p| p.to_string_lossy().into_owned());
        let items = crate::history_worker::to_items(&entries, home.as_deref());
        self.history_view.set_items(items);
        self.history_view.scroll = 0.0;
        // A filter typed for another machine (or visit) would hide rows.
        self.history_view.filter.clear();
        self.history_view.filter_focused = false;
        self.history_for = Some(machine_id.to_string());
    }

    /// Route one input to the History view.
    pub(super) fn history_view_input(&mut self, input: &ViewInput) -> ViewOutcome {
        let content = self.chrome.shell.content_rect();
        let redraw = |changed: bool| {
            if changed {
                ViewOutcome::Redraw
            } else {
                ViewOutcome::Consumed
            }
        };
        match input {
            ViewInput::Press { x, y, .. } => {
                let action = painter::pointer_press(
                    &mut self.sugarloaf,
                    content,
                    &mut self.history_view,
                    *x,
                    *y,
                );
                if let Some(HistoryAction::RunAgain { command }) = action {
                    self.show_view(WorkspaceView::Terminal);
                    self.run_history_command(&command);
                }
                ViewOutcome::Redraw
            }
            ViewInput::Move { x, y, .. } => redraw(painter::pointer_move(
                &mut self.sugarloaf,
                content,
                &mut self.history_view,
                *x,
                *y,
            )),
            // Shell: lines > 0 scrolls towards the end; the view: dy < 0.
            ViewInput::Wheel { lines, .. } => {
                painter::wheel(content, &mut self.history_view, -lines);
                ViewOutcome::Redraw
            }
            ViewInput::Release { .. } | ViewInput::ContextPress { .. } => {
                ViewOutcome::Consumed
            }
            ViewInput::Key { key, mods } => {
                let state = &mut self.history_view;
                let consumed = match key {
                    ViewKey::Text(t) if !mods.ctrl && !mods.logo && !mods.alt => {
                        // Typing anywhere in the view filters.
                        state.filter_focused = true;
                        painter::text(state, t)
                    }
                    ViewKey::Backspace => state.backspace(),
                    ViewKey::Escape => state.escape(),
                    _ => false,
                };
                if consumed {
                    ViewOutcome::Redraw
                } else {
                    ViewOutcome::Ignored
                }
            }
        }
    }
}
