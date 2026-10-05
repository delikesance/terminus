//! Files view actions.

use super::super::Screen;
use rio_backend::clipboard::Clipboard;
use terminus_ui::screens::files::FilesAction;

impl Screen<'_> {
    pub(super) fn files_view_action(
        &mut self,
        action: FilesAction,
        _clipboard: &mut Clipboard,
    ) {
        match action {
            FilesAction::OpenBrowser => {
                self.open_files_browser();
            }
        }
    }

    /// Open the SFTP browser for the selected machine; a failure is shown
    /// in the Files view with a Retry button.
    pub(in crate::screen) fn open_files_browser(&mut self) {
        let Some(id) = self.chrome.shell.machine.as_ref().map(|m| m.id.clone()) else {
            return;
        };
        if let Err(err) = self.open_sftp_pane(&id) {
            self.chrome.screens.files.fail(err);
        }
        self.mark_dirty();
    }

    /// After a pump: a browser whose server side never connected is
    /// closed, and its error moves to the Files view (with Retry).
    pub(in crate::screen) fn settle_failed_files_browser(&mut self) -> bool {
        let Some(err) = self
            .sftp
            .as_ref()
            .filter(|s| s.state.left.backend.is_local())
            .and_then(|s| s.state.right.connect_error.clone())
        else {
            return false;
        };
        if let Some(session) = self.sftp.take() {
            session.close();
        }
        self.chrome.screens.files.fail(err);
        self.mark_dirty();
        true
    }
}
