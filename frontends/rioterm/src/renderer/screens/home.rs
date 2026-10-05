//! Home ("Where to?") painter (stub): search field and Add a server.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::geom::Rect;
use terminus_ui::icons::{Icon, IconPlacement};
use terminus_ui::screens::home::{
    HomeHit, HomeState, ADD, SEARCH_ICON, SEARCH_PLACEHOLDER, SEARCH_RADIUS, SEARCH_SIZE,
};
use terminus_ui::theme::ChromeTheme;

use crate::renderer::chrome::draw_icon;
use crate::renderer::ui_text::{draw_ui_text, UiWeight};

pub fn measure(sugarloaf: &mut Sugarloaf, state: &mut HomeState) {
    if state.add_label_w <= 0.0 {
        state.add_label_w = super::primary_label_w(sugarloaf, ADD);
    }
}

pub fn paint(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    content: Rect,
    state: &HomeState,
    device_scale: f32,
) {
    let s = state.search_rect(content);
    let bg = if state.hover == Some(HomeHit::Search) {
        theme.surface
    } else {
        theme.frame
    };
    sugarloaf.rounded_rect(None, s.x, s.y, s.width, s.height, bg, 0.0, SEARCH_RADIUS, 0);
    let muted = theme.text_muted.map(|v| v as f32 / 255.0);
    draw_icon(
        sugarloaf,
        Icon::Search,
        IconPlacement::new(
            s.x + 18.0,
            s.y + (s.height - SEARCH_ICON) * 0.5,
            SEARCH_ICON,
        ),
        muted,
        device_scale,
    );
    draw_ui_text(
        sugarloaf,
        s.x + 18.0 + SEARCH_ICON + 12.0,
        s.y + s.height * 0.5 - SEARCH_SIZE * 0.6,
        SEARCH_PLACEHOLDER,
        SEARCH_SIZE,
        theme.text_muted,
        UiWeight::Regular,
    );
    super::paint_primary(
        sugarloaf,
        theme,
        state.add_rect(content),
        ADD,
        state.add_label_w,
        state.hover == Some(HomeHit::Add),
    );
}
