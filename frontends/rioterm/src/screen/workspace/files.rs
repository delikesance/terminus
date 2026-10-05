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
                let Some(id) = self.chrome.shell.machine.as_ref().map(|m| m.id.clone())
                else {
                    return;
                };
                if let Err(err) = self.open_sftp_pane(&id) {
                    self.chrome.panel.error = Some(err);
                }
            }
        }
    }
}
