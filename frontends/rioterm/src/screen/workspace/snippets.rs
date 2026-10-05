//! Snippets view actions: open the add dialog, paste / run into the
//! machine's terminal, delete.

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
        match action {
            SnippetsAction::New => {
                self.chrome.snippet_form.inner.closing = false;
            }
            SnippetsAction::Paste(cmd) => {
                self.show_view(WorkspaceView::Terminal);
                self.paste(&cmd, false);
            }
            SnippetsAction::Run(cmd) => {
                self.show_view(WorkspaceView::Terminal);
                // Non-bracketed write + CR so shells treat it as typed Enter.
                self.paste(&format!("{cmd}\r"), false);
            }
            SnippetsAction::Delete(id) => self.host_store.delete_snippet(id),
        }
    }
}
