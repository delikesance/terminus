//! History view actions (none yet).

use super::super::Screen;
use rio_backend::clipboard::Clipboard;
use terminus_ui::screens::history::HistoryAction;

impl Screen<'_> {
    pub(super) fn history_view_action(
        &mut self,
        action: HistoryAction,
        _clipboard: &mut Clipboard,
    ) {
        match action {}
    }
}
