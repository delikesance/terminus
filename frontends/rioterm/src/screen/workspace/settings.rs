//! Settings page actions.

use super::super::Screen;
use crate::renderer::command_palette::PaletteAction;
use rio_backend::clipboard::Clipboard;
use terminus_ui::screens::settings::SettingsAction;

impl Screen<'_> {
    pub(super) fn settings_view_action(
        &mut self,
        action: SettingsAction,
        clipboard: &mut Clipboard,
    ) {
        match action {
            SettingsAction::OpenDialog(page) => {
                if let Some(tab) = page.legacy_tab() {
                    self.chrome.open_settings(tab);
                }
            }
            SettingsAction::CheckForUpdates => {
                self.execute_palette_action(PaletteAction::CheckForUpdates, clipboard)
            }
            SettingsAction::OpenConfig => {
                self.execute_palette_action(PaletteAction::ConfigEditor, clipboard)
            }
        }
    }
}
