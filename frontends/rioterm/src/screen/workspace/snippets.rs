//! Snippets view actions (`terminus_ui::views::snippets`): open the add
//! dialog, paste / run into the machine's terminal, delete.

use super::super::Screen;
use rio_backend::clipboard::Clipboard;
use terminus_ui::screens::snippets::SnippetsAction;
use terminus_ui::shell::WorkspaceView;

impl Screen<'_> {
    pub(super) fn snippets_view_action(
        &mut self,
        action: SnippetsAction,
        _clipboard: &mut Clipboard,
    ) {
        self.execute_snippets_action(action);
    }

    /// Run one action returned by the Snippets view. Paste types into the
    /// active session without Enter; Run adds a carriage return
    /// (non-bracketed, so shells see a typed Enter). Both bring the
    /// Terminal view forward so the result is visible.
    pub fn execute_snippets_action(&mut self, action: SnippetsAction) {
        match action {
            SnippetsAction::PasteToSession(cmd) => {
                self.show_view(WorkspaceView::Terminal);
                self.paste(&cmd, false);
            }
            SnippetsAction::RunInSession(cmd) => {
                self.show_view(WorkspaceView::Terminal);
                self.paste(&format!("{cmd}\r"), false);
            }
            SnippetsAction::OpenNewSnippet => {
                self.chrome.snippet_form.inner.closing = false;
            }
            SnippetsAction::Delete(id) => self.host_store.delete_snippet(id),
            // The store only creates snippets today (no update command).
            SnippetsAction::Edit(_) => {}
        }
    }
}
