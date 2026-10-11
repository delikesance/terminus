//! Quick-find box under each pane header.

use super::*;

pub(super) fn paint_search(
    s: &mut Sugarloaf,
    theme: &ChromeTheme,
    p: &terminus_ui::views::files::PaneRects,
    side: &terminus_ui::sftp_pane::SftpSideState,
    under: [f32; 4],
) {
    let cy = p.search.y + p.search.height / 2.0;
    let field = Rect::new(
        p.search.x + geo::SEARCH_PAD_X,
        p.search.y + 4.0,
        (p.search.width - 2.0 * geo::SEARCH_PAD_X).max(0.0),
        p.search.height - 8.0,
    );
    let border = if side.filter_focused {
        theme.accent
    } else {
        unit_color(theme.text_faint)
    };
    rrect(s, &field, blend(under, border, 0.35), 8.0, D_CTRL);
    let inner = Rect::new(
        field.x + 1.0,
        field.y + 1.0,
        field.width - 2.0,
        field.height - 2.0,
    );
    rrect(
        s,
        &inner,
        blend(under, unit_color(theme.text), 0.04),
        7.0,
        D_CTRL + 0.01,
    );
    let (text, color) = if side.filter.value.is_empty() {
        ("Search files", theme.text_faint)
    } else {
        (side.filter.value.as_str(), theme.text)
    };
    draw_ui_text(
        s,
        field.x + 8.0,
        text_top(cy, font_size::LABEL),
        text,
        font_size::LABEL,
        color,
        UiWeight::Regular,
    );
    if side.filter_focused {
        let cx = caret_x(s, &field, &side.filter.prefix());
        paint_caret(s, theme, cx, cy);
    }
}
