//! Settings page painter (stub). SSH keys / Sync open the legacy dialog
//! over this page.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::geom::Rect;
use terminus_ui::screens::settings::{cta, title, SettingsState};
use terminus_ui::shell::SettingsPage;
use terminus_ui::theme::ChromeTheme;

pub fn measure(sugarloaf: &mut Sugarloaf, page: SettingsPage, state: &mut SettingsState) {
    state.cta_label_w = super::primary_label_w(sugarloaf, cta(page));
}

pub fn paint(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    content: Rect,
    page: SettingsPage,
    state: &SettingsState,
    _device_scale: f32,
) {
    super::paint_empty(sugarloaf, theme, content, title(page), &state.body(page));
    super::paint_primary(
        sugarloaf,
        theme,
        state.cta_rect(page, content),
        cta(page),
        state.cta_label_w,
        state.cta_hover,
    );
}
