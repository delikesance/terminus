// Copyright (c) 2026-present, Terminus Contributors.

use super::color::{as_f32, opaque_over};
use super::surfaces::paint_surface;
use super::text::{draw_text, opts, wrap_lines};
use super::{
    BORDER_WIDTH, DEPTH_CONTENT, HINT_SIZE, ORDER_CONTENT, ROW_SUB_SIZE, ROW_TITLE_SIZE,
};
use crate::renderer::icons::draw_icon;
use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::chrome::Chrome;
use terminus_ui::geom::Rect;
use terminus_ui::icons::Icon;
use terminus_ui::icons::IconPlacement;
use terminus_ui::sidebar;
use terminus_ui::theme::ChromeTheme;

/// "No saved hosts yet" / "No matches" under the list.
pub(crate) fn render_empty_hint(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    origin_y: f32,
    height: f32,
    cover: Option<&Rect>,
) {
    let (Some(hint), Some(rect)) = (
        chrome.panel.empty_hint(),
        chrome.panel.empty_hint_rect(origin_y, height),
    ) else {
        return;
    };
    let body = chrome.panel.body_rect(origin_y, height);
    if rect.bottom() > body.bottom() || text_blocked_by(cover, &rect) {
        return;
    }
    draw_text(
        sugarloaf,
        rect.x + 6.0,
        rect.y + 4.0,
        hint.title,
        ROW_TITLE_SIZE,
        theme.text_muted,
        false,
    );
    let text_opts = opts(ROW_SUB_SIZE, theme.text_muted, false);
    let lines = wrap_lines(sugarloaf, &hint.body, rect.width - 12.0, &text_opts, 2);
    let mut y = rect.y + 24.0;
    for line in lines {
        draw_text(
            sugarloaf,
            rect.x + 6.0,
            y,
            &line,
            ROW_SUB_SIZE,
            theme.text_muted,
            false,
        );
        y += ROW_SUB_SIZE + 4.0;
    }
}

pub(crate) fn render_notice(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    origin_y: f32,
    height: f32,
    labels: bool,
) {
    let Some(rect) = chrome.panel.notice_rect(origin_y, height) else {
        return;
    };
    let is_error = chrome.panel.error.is_some();
    if is_error {
        // Soft red wash composited over the panel — same recipe as Settings,
        // but opaque so scrolled host rows cannot bleed through the footer.
        let wash = [
            0xf8 as f32 / 255.0,
            0x71 as f32 / 255.0,
            0x71 as f32 / 255.0,
            0.18,
        ];
        let ring = [
            0xf8 as f32 / 255.0,
            0x71 as f32 / 255.0,
            0x71 as f32 / 255.0,
            0.55,
        ];
        paint_surface(
            sugarloaf,
            &rect,
            opaque_over(theme.panel_bg, wash),
            Some(opaque_over(theme.panel_bg, ring)),
            12.0,
            DEPTH_CONTENT + 0.05,
            ORDER_CONTENT,
            true,
        );
    } else {
        paint_surface(
            sugarloaf,
            &rect,
            theme.notice_bg,
            Some(theme.panel_border),
            12.0,
            DEPTH_CONTENT + 0.05,
            ORDER_CONTENT,
            true,
        );
    }
    if !labels {
        return;
    }
    let message = chrome
        .panel
        .error
        .as_deref()
        .or(chrome.panel.notice.as_deref())
        .unwrap_or_default();
    let color = if is_error {
        theme.danger
    } else {
        theme.text_muted
    };
    let pad = 12.0;
    let max_w = (rect.width - 2.0 * pad).max(0.0);
    let text_opts = opts(HINT_SIZE, color, false);
    let max_lines = if is_error {
        2
    } else {
        chrome.panel.notice_lines()
    };
    let lines = wrap_lines(sugarloaf, message, max_w, &text_opts, max_lines);
    let line_gap = 4.0;
    let block_h = lines.len() as f32 * HINT_SIZE
        + (lines.len().saturating_sub(1) as f32) * line_gap;
    let mut y = rect.y + ((rect.height - block_h) * 0.5).max(pad * 0.5);
    for line in lines {
        draw_text(sugarloaf, rect.x + pad, y, &line, HINT_SIZE, color, false);
        y += HINT_SIZE + line_gap;
    }
}

/// Search field chrome + loupe + filter text (mock `pl-9` with left icon).
pub(crate) fn paint_search_field(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    origin_y: f32,
    device_scale: f32,
    depth: f32,
) {
    let search = chrome.panel.search_rect(origin_y);
    paint_surface(
        sugarloaf,
        &search,
        theme.field_bg,
        Some(theme.panel_border),
        sidebar::CARD_RADIUS,
        depth,
        ORDER_CONTENT,
        false,
    );
    if chrome.panel.filter_focused {
        let focus_shell = Rect::new(
            search.x - BORDER_WIDTH,
            search.y - BORDER_WIDTH,
            search.width + 2.0 * BORDER_WIDTH,
            search.height + 2.0 * BORDER_WIDTH,
        );
        paint_surface(
            sugarloaf,
            &focus_shell,
            theme.button_bg,
            Some(theme.field_border_focus),
            sidebar::CARD_RADIUS + 1.0,
            depth + 0.002,
            ORDER_CONTENT,
            false,
        );
    }

    const SEARCH_ICON: f32 = 14.0;
    let icon_x = search.x + 12.0;
    let icon_y = search.y + (sidebar::SEARCH_HEIGHT - SEARCH_ICON) * 0.5;
    draw_icon(
        sugarloaf,
        Icon::Search,
        IconPlacement::new(icon_x, icon_y, SEARCH_ICON),
        as_f32(theme.text_faint),
        device_scale,
    );

    let placeholder =
        if chrome.panel.filter.value.is_empty() && !chrome.panel.filter_focused {
            "Filter servers…"
        } else {
            ""
        };
    let filter_text = if chrome.panel.filter.value.is_empty() {
        placeholder
    } else {
        chrome.panel.filter.value.as_str()
    };
    let filter_color =
        if chrome.panel.filter.value.is_empty() && !chrome.panel.filter_focused {
            theme.text_placeholder
        } else {
            theme.text
        };
    draw_text(
        sugarloaf,
        search.x + 12.0 + SEARCH_ICON + 8.0,
        search.y + ((sidebar::SEARCH_HEIGHT - ROW_SUB_SIZE) * 0.5).round(),
        filter_text,
        ROW_SUB_SIZE,
        filter_color,
        false,
    );
}

/// Approximate a CSS `border-dashed` rounded rectangle.
///
/// Uses SDF `rounded_rect` capsules (not hard-edged `line` quads) so
/// straight dashes and corner arcs stay anti-aliased. Corners are a
/// chain of overlapping circular pills along the quarter-circle —
/// `sugarloaf.arc` still ignores paint order, so we cannot use it here.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_dashed_rounded_rect(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    radius: f32,
    color: [f32; 4],
    depth: f32,
    order: u8,
) {
    let stroke: f32 = 1.25;
    let dash: f32 = 5.0;
    let gap: f32 = 3.5;
    let r = radius.min(w / 2.0).min(h / 2.0).max(0.0);
    let inset = stroke * 0.5;
    let x0 = x + inset;
    let y0 = y + inset;
    let x1 = x + w - inset;
    let y1 = y + h - inset;
    let rr = (r - inset).max(0.0);
    let pill_r = stroke * 0.5;

    let mut paint_capsule = |cx: f32, cy: f32, len: f32, horizontal: bool| {
        if len < 0.35 {
            return;
        }
        if horizontal {
            paint_surface(
                sugarloaf,
                &Rect::new(cx, cy - pill_r, len, stroke),
                color,
                None,
                pill_r,
                depth,
                order,
                false,
            );
        } else {
            paint_surface(
                sugarloaf,
                &Rect::new(cx - pill_r, cy, stroke, len),
                color,
                None,
                pill_r,
                depth,
                order,
                false,
            );
        }
    };

    let mut paint_edge = |mut a: f32, b: f32, horizontal: bool, fixed: f32| {
        let mut on = true;
        let mut remain = dash;
        while a < b - 0.01 {
            let len = remain.min(b - a);
            if on && len > 0.4 {
                if horizontal {
                    paint_capsule(a, fixed, len, true);
                } else {
                    paint_capsule(fixed, a, len, false);
                }
            }
            a += len;
            remain -= len;
            if remain <= 0.0 {
                on = !on;
                remain = if on { dash } else { gap };
            }
        }
    };

    paint_edge(x0 + rr, x1 - rr, true, y0);
    paint_edge(x0 + rr, x1 - rr, true, y1);
    paint_edge(y0 + rr, y1 - rr, false, x0);
    paint_edge(y0 + rr, y1 - rr, false, x1);

    if rr > 0.5 {
        // Dense overlapping circular SDF pills ≈ a smooth AA arc.
        let arc_len = std::f32::consts::FRAC_PI_2 * rr;
        let steps = ((arc_len / (stroke * 0.45)).ceil() as usize).clamp(12, 48);
        for (cx, cy, start, end) in [
            (x0 + rr, y0 + rr, 180.0_f32, 270.0_f32),
            (x1 - rr, y0 + rr, 270.0, 360.0),
            (x1 - rr, y1 - rr, 0.0, 90.0),
            (x0 + rr, y1 - rr, 90.0, 180.0),
        ] {
            for i in 0..=steps {
                let t = i as f32 / steps as f32;
                let a = (start + (end - start) * t).to_radians();
                let px = cx + rr * a.cos();
                let py = cy + rr * a.sin();
                paint_surface(
                    sugarloaf,
                    &Rect::new(px - pill_r, py - pill_r, stroke, stroke),
                    color,
                    None,
                    pill_r,
                    depth,
                    order,
                    false,
                );
            }
        }
    }
}

/// Whether sugarloaf UI text for `item` would float on top of `cover`.
pub(super) fn text_blocked_by(cover: Option<&Rect>, item: &Rect) -> bool {
    cover.is_some_and(|c| terminus_ui::rects_overlap(*c, *item))
}

/// Dialog (+ open auth/identity menus) that must not have background glyphs
/// painted underneath — sugarloaf text always composites above quads.
/// Prefer [`Sugarloaf::begin_overlay`] for full dialogs; this cover remains
/// for sidebar host-row labels under the add-host panel.
#[allow(dead_code)]
pub(super) fn add_host_label_cover(
    chrome: &Chrome,
    window_width: f32,
    window_height: f32,
) -> Option<Rect> {
    if !chrome.add_host_is_open() {
        return None;
    }
    let layout = chrome.dialog_layout(window_width, window_height);
    let mut cover = layout.rect(chrome.form.height());
    if let Some(menu) = layout.menu_rect(&chrome.form) {
        cover = rect_union(cover, menu);
    }
    Some(cover)
}

#[allow(dead_code)]
pub(super) fn rect_union(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = a.right().max(b.right());
    let y1 = a.bottom().max(b.bottom());
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}
