//! General tab painter.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::selection::ControlState;
use terminus_ui::geom::Rect;
use terminus_ui::theme::ChromeTheme;
use terminus_ui::views::settings::general::GeneralState;

use super::updates::{card, two_lines};
use crate::renderer::components::selection::paint_toggle;

pub fn paint(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    content: Rect,
    s: &GeneralState,
) {
    let l = s.layout(content);
    card(sugarloaf, theme, &l.restore.card, s.hover);
    two_lines(
        sugarloaf,
        theme,
        &l.restore.text,
        "Reopen tabs on launch",
        "Bring back the tabs you had open when Terminus last closed.",
        theme.text_muted,
    );
    paint_toggle(
        sugarloaf,
        theme,
        (l.restore.toggle.x, l.restore.toggle.y),
        s.restore_tabs,
        ControlState::Default,
    );
}
