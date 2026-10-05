//! Execution of Snippets-view actions (`terminus_ui::views::snippets`).

use super::Screen;
use terminus_ui::views::snippets::SnippetsAction;

impl Screen<'_> {
    /// Run one action returned by the Snippets view. Paste types into the
    /// active terminal session without Enter; Run adds a carriage return
    /// (non-bracketed, so shells see a typed Enter).
    #[allow(dead_code)]
    pub fn execute_snippets_action(&mut self, action: SnippetsAction) {
        match action {
            SnippetsAction::PasteToSession(cmd) => self.paste(&cmd, false),
            SnippetsAction::RunInSession(cmd) => self.paste(&format!("{cmd}\r"), false),
            SnippetsAction::OpenNewSnippet => {
                self.chrome.snippet_form.inner.closing = false;
            }
            SnippetsAction::Delete(id) => self.host_store.delete_snippet(id),
            // The store only creates snippets today (no update command).
            SnippetsAction::Edit(_) => {}
        }
    }
}
