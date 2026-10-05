//! Identity component gallery: OS tiles and the machine / workspace header.
//!
//! Geometry comes from `terminus_ui::components::identity`; this file only
//! paints the rects.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::identity::{
    self as id, HeaderRects, TileGlyph, BOARD, HEADER_ADDRESS_SIZE, HEADER_TAB_GAP,
    HEADER_TITLE_SIZE,
};
use terminus_ui::geom::Rect;
use terminus_ui::icons::IconPlacement;
use terminus_ui::theme::{text_color, ChromeTheme};
use terminus_ui::tokens::{font_size, space};

use crate::renderer::chrome::{draw_icon, draw_os_glyph};
use crate::renderer::ui_text::{
    draw_mono_text, draw_ui_text, measure_mono_text, measure_ui_text, UiWeight,
};

/// Paint one identity tile (`rect` is the tile square).
pub fn paint_tile(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    rect: &Rect,
    glyph: TileGlyph,
    selected: bool,
) {
    let (bg, fg) = id::tile_style_with(theme, glyph, selected);
    sugarloaf.rounded_rect(
        None,
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        bg,
        0.0,
        id::tile_radius(rect.width),
        1,
    );
    let mark = id::mark_rect(rect);
    let place = IconPlacement::new(mark.x, mark.y, mark.width);
    let scale = sugarloaf.scale_factor();
    if let Some(os) = glyph.os_glyph() {
        draw_os_glyph(sugarloaf, os, place, fg, scale);
    } else if let Some(icon) = glyph.icon() {
        draw_icon(sugarloaf, icon, place, fg, scale);
    }
}

/// A view tab stand-in (the real tabs belong to the Navigation family).
struct StandInTab {
    label: &'static str,
    active: bool,
    badge: &'static str,
}

fn tab_width(sugarloaf: &mut Sugarloaf, tab: &StandInTab) -> f32 {
    let w = measure_ui_text(sugarloaf, tab.label, font_size::BODY_SM, UiWeight::Medium);
    if tab.badge.is_empty() {
        w
    } else {
        w + 6.0
            + measure_ui_text(sugarloaf, tab.badge, font_size::BODY_SM, UiWeight::Regular)
    }
}

fn tabs_total(sugarloaf: &mut Sugarloaf, tabs: &[StandInTab]) -> f32 {
    let widths: f32 = tabs.iter().map(|t| tab_width(sugarloaf, t)).sum();
    widths + HEADER_TAB_GAP * tabs.len().saturating_sub(1) as f32
}

/// Paint a header with stand-in tabs; returns the height used.
pub fn paint_header(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    origin: (f32, f32),
    width: f32,
    title: &str,
    address: Option<&str>,
    tabs: &[(&'static str, bool, &'static str)],
) -> f32 {
    let tabs: Vec<StandInTab> = tabs
        .iter()
        .map(|&(label, active, badge)| StandInTab {
            label,
            active,
            badge,
        })
        .collect();
    let mut block_w =
        measure_ui_text(sugarloaf, title, HEADER_TITLE_SIZE, UiWeight::SemiBold);
    if let Some(a) = address {
        block_w = block_w.max(measure_mono_text(
            sugarloaf,
            a,
            HEADER_ADDRESS_SIZE,
            UiWeight::Regular,
        ));
    }
    let tabs_w = tabs_total(sugarloaf, &tabs);
    let h: HeaderRects = id::header_rects(
        origin.0,
        origin.1,
        width,
        address.is_some(),
        block_w,
        tabs_w,
    );
    sugarloaf.rect(
        None,
        h.border.x,
        h.border.y,
        h.border.width,
        h.border.height,
        theme.divider,
        0.0,
        1,
    );
    draw_ui_text(
        sugarloaf,
        h.title.x,
        h.title.y,
        title,
        HEADER_TITLE_SIZE,
        theme.text,
        UiWeight::SemiBold,
    );
    if let (Some(a), Some(r)) = (address, h.address) {
        draw_mono_text(
            sugarloaf,
            r.x,
            r.y,
            a,
            HEADER_ADDRESS_SIZE,
            theme.text_faint,
            UiWeight::Regular,
        );
    }
    let mut x = h.tabs.x;
    for tab in &tabs {
        let w = tab_width(sugarloaf, tab);
        let (color, weight) = if tab.active {
            (theme.text, UiWeight::Medium)
        } else {
            (theme.text_muted, UiWeight::Regular)
        };
        draw_ui_text(
            sugarloaf,
            x,
            h.tabs.y,
            tab.label,
            font_size::BODY_SM,
            color,
            weight,
        );
        if !tab.badge.is_empty() {
            let lw = measure_ui_text(sugarloaf, tab.label, font_size::BODY_SM, weight);
            draw_ui_text(
                sugarloaf,
                x + lw + 6.0,
                h.tabs.y,
                tab.badge,
                font_size::BODY_SM,
                text_color(theme.success),
                UiWeight::Regular,
            );
        }
        if tab.active {
            sugarloaf.rect(None, x, h.border.y - 2.0, w, 2.0, theme.accent, 0.0, 2);
        }
        x += w + HEADER_TAB_GAP;
    }
    h.bounds.height
}

/// Paint the `identity` gallery at `origin` within `width`; returns the height used.
pub fn paint_gallery(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    origin: (f32, f32),
    width: f32,
) -> f32 {
    let mut y =
        origin.1 + super::paint_section_title(sugarloaf, theme, origin, "Identity");
    let caption = |sugarloaf: &mut Sugarloaf, x: f32, y: f32, s: &str, c: [u8; 4]| {
        draw_ui_text(sugarloaf, x, y, s, font_size::CAPTION, c, UiWeight::Regular);
    };

    caption(
        sugarloaf,
        origin.0,
        y,
        "OS tile: 28 (lists), 36 (cards), selected",
        theme.text_muted,
    );
    y += font_size::CAPTION + space::LG;
    for (i, (glyph, label)) in BOARD.iter().enumerate() {
        let c = id::cell_rects(i, origin.0, y, width);
        caption(sugarloaf, c.caption.x, c.caption.y, label, theme.text_faint);
        paint_tile(sugarloaf, theme, &c.small, *glyph, false);
        paint_tile(sugarloaf, theme, &c.card, *glyph, false);
        paint_tile(sugarloaf, theme, &c.selected, *glyph, true);
    }
    y += id::grid_height(BOARD.len()) + space::XXL;

    caption(
        sugarloaf,
        origin.0,
        y,
        "Machine header: name, address, view tabs on the border",
        theme.text_muted,
    );
    y += font_size::CAPTION + space::LG;
    y += paint_header(
        sugarloaf,
        theme,
        (origin.0, y),
        width,
        "jerem prod",
        Some("ubuntu@137.74.42.224"),
        &[
            ("Terminal", true, ""),
            ("Files", false, ""),
            ("Tunnels", false, "1"),
            ("Snippets", false, ""),
            ("History", false, ""),
        ],
    ) + space::XXL;

    caption(
        sugarloaf,
        origin.0,
        y,
        "Settings header: title and tabs only",
        theme.text_muted,
    );
    y += font_size::CAPTION + space::LG;
    y += paint_header(
        sugarloaf,
        theme,
        (origin.0, y),
        width,
        "Settings",
        None,
        &[
            ("SSH keys", true, ""),
            ("Sync", false, ""),
            ("Appearance", false, ""),
            ("Updates", false, ""),
        ],
    );
    y - origin.1
}
