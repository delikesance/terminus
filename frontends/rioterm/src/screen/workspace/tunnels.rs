//! Tunnels view actions (none yet).

use super::super::Screen;
use rio_backend::clipboard::Clipboard;
use terminus_ui::screens::tunnels::TunnelsAction;

impl Screen<'_> {
    pub(super) fn tunnels_view_action(
        &mut self,
        action: TunnelsAction,
        _clipboard: &mut Clipboard,
    ) {
        match action {}
    }
}
