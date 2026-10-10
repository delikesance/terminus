use super::*;

pub(super) struct IslandFills {
    pub(super) inactive: [f32; 4],
    pub(super) active: [f32; 4],
    pub(super) outline: Option<[f32; 4]>,
    pub(super) close_hover: [f32; 4],
}

pub(super) fn island_fills(bg: [f32; 4]) -> IslandFills {
    let luminance = 0.2126 * bg[0] + 0.7152 * bg[1] + 0.0722 * bg[2];
    if luminance > 0.5 {
        IslandFills {
            inactive: [0.0, 0.0, 0.0, 0.06],
            active: [1.0, 1.0, 1.0, 0.92],
            outline: Some([0.0, 0.0, 0.0, 0.14]),
            close_hover: [0.0, 0.0, 0.0, 0.09],
        }
    } else {
        // Dark strip: inactive pills stay readable (soft fill + hairline);
        // active is the elevated card.
        IslandFills {
            inactive: [
                0x1c as f32 / 255.0,
                0x1c as f32 / 255.0,
                0x20 as f32 / 255.0,
                1.0,
            ],
            active: [
                0x2a as f32 / 255.0,
                0x2a as f32 / 255.0,
                0x30 as f32 / 255.0,
                1.0,
            ],
            outline: Some([
                0x3a as f32 / 255.0,
                0x3a as f32 / 255.0,
                0x42 as f32 / 255.0,
                1.0,
            ]),
            close_hover: [1.0, 1.0, 1.0, 0.12],
        }
    }
}

#[inline]
pub(super) fn over(dst: [f32; 4], src: [f32; 4]) -> [f32; 4] {
    let a = src[3];
    [
        src[0] * a + dst[0] * (1.0 - a),
        src[1] * a + dst[1] * (1.0 - a),
        src[2] * a + dst[2] * (1.0 - a),
        dst[3],
    ]
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_island(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radius: f32,
    fill: [f32; 4],
    outline: Option<[f32; 4]>,
    punch: Option<[f32; 4]>,
    order: u8,
) {
    let card = terminus_ui::Rect::new(x, y, w, h);
    match outline {
        Some(ring) => {
            if let Some(bg) = punch {
                // Punch under the fill so translucent fills keep a solid backing.
                crate::renderer::chrome::paint_surface_stroke(
                    sugarloaf,
                    &card,
                    bg,
                    Some(ring),
                    radius,
                    1.0,
                    0.05,
                    order,
                    false,
                );
                let inner = terminus_ui::Rect::new(
                    x + 1.0,
                    y + 1.0,
                    (w - 2.0).max(0.0),
                    (h - 2.0).max(0.0),
                );
                crate::renderer::chrome::paint_surface_stroke(
                    sugarloaf,
                    &inner,
                    fill,
                    None,
                    (radius - 1.0).clamp(0.0, inner.width.min(inner.height) / 2.0),
                    1.0,
                    0.05,
                    order,
                    false,
                );
            } else {
                crate::renderer::chrome::paint_surface_stroke(
                    sugarloaf,
                    &card,
                    fill,
                    Some(ring),
                    radius,
                    1.0,
                    0.05,
                    order,
                    false,
                );
            }
        }
        None => {
            crate::renderer::chrome::paint_surface_stroke(
                sugarloaf, &card, fill, None, radius, 1.0, 0.05, order, false,
            );
        }
    }
}

#[inline]
pub(super) fn island_rect(slot_x: f32, tab_width: f32) -> (f32, f32, f32, f32, f32) {
    let x = slot_x + TAB_GAP / 2.0;
    let w = (tab_width - TAB_GAP).max(0.0);
    let y = TAB_INSET_Y;
    let h = ISLAND_HEIGHT - TAB_INSET_Y * 2.0;
    let radius = TAB_RADIUS.min(w / 2.0).min(h / 2.0);
    (x, y, w, h, radius)
}

#[inline]
pub(super) fn close_button_center(island_x: f32, island_w: f32) -> Option<f32> {
    (island_w >= CLOSE_MIN_ISLAND_WIDTH)
        .then_some(island_x + island_w - CLOSE_MARGIN_RIGHT)
}

#[inline]
pub(super) fn close_button_center_x(
    layout: &TabStripLayout,
    tab_index: usize,
) -> Option<f32> {
    let slot_x = layout.slot_x(tab_index);
    let (ix, _, iw, _, _) = island_rect(slot_x, layout.width_at(tab_index));
    close_button_center(ix, iw)
}

#[inline]
pub fn close_button_hit(
    layout: &TabStripLayout,
    tab_index: usize,
    x_unscaled: f32,
) -> bool {
    close_button_center_x(layout, tab_index)
        .is_some_and(|cx| (x_unscaled - cx).abs() <= CLOSE_HIT_HALF_WIDTH)
}

/// Which tab slot contains `x_unscaled`, if any.
pub fn tab_index_at(
    layout: &TabStripLayout,
    x_unscaled: f32,
    num_tabs: usize,
) -> Option<usize> {
    if num_tabs == 0 || layout.is_empty() {
        return None;
    }
    let x_in = x_unscaled - layout.left_margin;
    if x_in < 0.0 || x_in >= layout.tabs_width() {
        return None;
    }
    let mut cursor = 0.0;
    for (i, &w) in layout.widths.iter().enumerate().take(num_tabs) {
        if x_in < cursor + w {
            return Some(i);
        }
        cursor += w;
    }
    None
}

pub(super) fn draw_close_button(
    sugarloaf: &mut Sugarloaf,
    cx: f32,
    color: [f32; 4],
    hover: bool,
    scale_factor: f32,
) {
    use crate::renderer::chrome;
    use terminus_ui::icons::{Icon, IconPlacement};

    let alpha = if hover {
        CLOSE_ALPHA_HOVER
    } else {
        CLOSE_ALPHA_IDLE
    };
    let color = [color[0], color[1], color[2], color[3] * alpha];
    let ix = cx - CLOSE_ICON_SIZE / 2.0;
    let iy = (ISLAND_HEIGHT - CLOSE_ICON_SIZE) / 2.0;
    // Lucide mask (same path as window-control ×) — AA via tiny-skia,
    // not two diagonal `line` strokes that stair-step and clip.
    chrome::draw_icon(
        sugarloaf,
        Icon::X,
        IconPlacement::new(ix, iy, CLOSE_ICON_SIZE),
        color,
        scale_factor,
    );
}

#[inline]
pub(super) fn color_u8(c: [f32; 4]) -> [u8; 4] {
    [
        (c[0].clamp(0.0, 1.0) * 255.0) as u8,
        (c[1].clamp(0.0, 1.0) * 255.0) as u8,
        (c[2].clamp(0.0, 1.0) * 255.0) as u8,
        (c[3].clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

pub(super) fn render_title_bar_chrome(
    _sugarloaf: &mut Sugarloaf,
    _window_width_logical: f32,
    _scale_factor: f32,
    _icon_color: [f32; 4],
) {
    // Apple HIG mock: title bar is strip + tabs (+ Windows captions) only.
}
