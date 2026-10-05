//! Home view actions.

use super::super::Screen;
use rio_backend::clipboard::Clipboard;
use terminus_ui::screens::home::HomeAction;

impl Screen<'_> {
    pub(super) fn home_view_action(
        &mut self,
        action: HomeAction,
        _clipboard: &mut Clipboard,
    ) {
        match action {
            HomeAction::Search => self.open_palette_hosts(),
            HomeAction::AddServer => self.chrome.open_add_host(),
        }
    }
}
