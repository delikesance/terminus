// Copyright (c) 2026-present, Terminus Contributors.
//! Painter for the Terminus chrome.
//!
//! Every rectangle comes from `terminus_ui` (`activity_bar`, `sidebar`,
//! `add_host`), which is also what the mouse hit-tests against, so the
//! pixels and the click targets cannot drift apart. This module only
//! turns those rectangles into `sugarloaf` primitives and picks the
//! colors.
//!
//! Draw order notes for `sugarloaf`: `rect`/`line` take an `order` and
//! the quad batches are painted in ascending order (`arc` does not — it
//! always lands in the order-0 batch, *under* the terminal grid). The
//! grid uses order 3, so the chrome lives at 4 and up, and the overlays
//! (palette, search) at 20 sit above it. Icons do not go through the
//! quad batches at all: they are rasterized into coverage masks and
//! drawn with the UI text — see `draw_icon`.

use rio_backend::sugarloaf::text::{CoverageMask, DrawOpts};
use rio_backend::sugarloaf::Sugarloaf;

use terminus_ui::activity_bar;
use terminus_ui::add_host::{auth_method_label, Field};
use terminus_ui::chrome::Chrome;
use terminus_ui::connection::{NodeVisual, STEP_COUNT};
use terminus_ui::context_menu::{ContextMenu, ITEM_HEIGHT as CTX_ITEM_HEIGHT, MENU_PAD_X, MENU_RADIUS};
use terminus_ui::geom::Rect;
use terminus_ui::icons::{Cmd, Icon, IconPlacement, LUCIDE_STROKE};
use terminus_ui::loading::{breath_ring, orbit_dots, shimmer_bar};
use terminus_ui::os_icons::OsGlyph;
use terminus_ui::sidebar;
use terminus_ui::theme::ChromeTheme;

/// Chrome paint orders. The grid is 3 and the overlays are 20.
const ORDER_RAIL: u8 = 4;
const ORDER_PANEL: u8 = 6;
const ORDER_CONTENT: u8 = 7;
const ORDER_CONNECTING: u8 = 8;
const ORDER_DIALOG: u8 = 30;
/// Popovers inside the settings dialog (engine dropdown, …) — above dialog cards.
const ORDER_DIALOG_POPOVER: u8 = 31;
/// Host-drag phantom — above dialogs, rail, sticky header, everything.
const ORDER_GHOST: u8 = 50;

const DEPTH_BG: f32 = 0.05;
const DEPTH_CONTENT: f32 = 0.06;
/// Sticky drawer header (title + search) painted after the scrollable list
/// so host cards slide underneath instead of over it.
const DEPTH_STICKY: f32 = 0.085;
const DEPTH_DIALOG: f32 = 0.1;
const DEPTH_DIALOG_BG: f32 = 0.2;
const DEPTH_GHOST: f32 = 0.35;

const RAIL_ICON_SIZE: f32 = activity_bar::ICON_SIZE;
const ADD_ICON_SIZE: f32 = 15.0;

// Whole-pixel sizes on purpose: the atlas rasterises each size bucket
// separately and the glyph quads are whole pixels wide, so an integer
// size plus an integer origin samples 1:1. A half-pixel size (10.5,
// 12.5) only ever renders as a blurrier 10 or 13.
const TITLE_SIZE: f32 = 12.0;
const ROW_TITLE_SIZE: f32 = 13.0;
/// Secondary labels / endpoints — one step smaller than the title.
const ROW_SUB_SIZE: f32 = 11.0;
/// Section labels are headings: small, faint and letter-spaced by case.
const SECTION_LABEL_SIZE: f32 = 10.0;
const ADD_LABEL_SIZE: f32 = 12.0;

const DIALOG_TITLE_SIZE: f32 = 14.0;
const CAPTION_SIZE: f32 = 11.0;
/// Mock inputs are `text-xs` (12px).
const INPUT_SIZE: f32 = 12.0;
const HINT_SIZE: f32 = 11.0;

const BORDER_WIDTH: f32 = 1.0;
const INPUT_PAD_X: f32 = 10.0;
const CARET_WIDTH: f32 = 1.5;

/// Paint the whole chrome for one frame.
///
/// `window_width` / `window_height` are logical pixels (physical size
/// divided by the scale factor), matching `terminus_ui`'s geometry;
/// `device_scale` is that same factor, needed to rasterize icons into
/// masks that land 1:1 on the device grid.
///
/// `connecting_phase` is the looping `0.0..1.0` orbit phase when a
/// host session is starting; `None` skips the animation paints.
pub fn render(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    window_width: f32,
    window_height: f32,
    device_scale: f32,
    connecting_phase: Option<f32>,
) {
    if chrome.activity.collapsed {
        // Rail collapsed: still paint overlay dialogs (they are not part of
        // the panel). Same stacked overlay rules as the expanded path.
        paint_modal_stack(
            sugarloaf,
            chrome,
            theme,
            window_width,
            window_height,
            device_scale,
            connecting_phase,
        );
        return;
    }

    let origin_y = chrome.origin_y();
    let height = (window_height - origin_y).max(0.0);
    if height <= 0.0 {
        return;
    }

    render_rail(sugarloaf, chrome, theme, origin_y, height, device_scale);

    if chrome.panel_visible {
        render_panel(
            sugarloaf,
            chrome,
            theme,
            origin_y,
            height,
            window_width,
            window_height,
            device_scale,
            connecting_phase,
        );
    }

    paint_modal_stack(
        sugarloaf,
        chrome,
        theme,
        window_width,
        window_height,
        device_scale,
        connecting_phase,
    );

    if let Some(menu) = chrome.context_menu.as_ref() {
        render_context_menu(sugarloaf, menu, theme, device_scale);
    }

    // Insertion bar while dragging, then ghost at max z-order.
    paint_host_drag_insertion_bar(sugarloaf, chrome, theme);
    paint_host_drag_ghost(sugarloaf, chrome, theme, device_scale);
}

/// Single Sugarloaf overlay pass for every open dialog, back → front.
///
/// Only the front-most layer emits text/icons. Lower layers paint scrim +
/// panel shells so a translucent top scrim still dimly shows the form
/// underneath — without lower-modal glyphs floating above it.
fn paint_modal_stack(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    window_width: f32,
    window_height: f32,
    device_scale: f32,
    connecting_phase: Option<f32>,
) {
    let stack = chrome.modal_paint_stack();
    if stack.is_empty() {
        return;
    }
    let top = stack.last().copied();
    sugarloaf.begin_overlay();
    for layer in stack {
        let paint_glyphs = top == Some(layer);
        match layer {
            terminus_ui::ModalPaintLayer::Connection => {
                render_connection_modal(
                    sugarloaf,
                    chrome,
                    theme,
                    window_width,
                    window_height,
                    device_scale,
                    connecting_phase.unwrap_or(0.0),
                    paint_glyphs,
                );
            }
            terminus_ui::ModalPaintLayer::HostEditor => {
                render_add_host(
                    sugarloaf,
                    chrome,
                    theme,
                    window_width,
                    window_height,
                    device_scale,
                    paint_glyphs,
                );
            }
            terminus_ui::ModalPaintLayer::AddSnippet => {
                render_add_snippet(
                    sugarloaf,
                    chrome,
                    theme,
                    window_width,
                    window_height,
                    device_scale,
                    paint_glyphs,
                );
            }
            terminus_ui::ModalPaintLayer::Settings => {
                render_settings_modal(
                    sugarloaf,
                    chrome,
                    theme,
                    window_width,
                    window_height,
                    device_scale,
                    paint_glyphs,
                );
            }
            terminus_ui::ModalPaintLayer::VaultUnlock => {
                render_vault_unlock(
                    sugarloaf,
                    chrome,
                    theme,
                    window_width,
                    window_height,
                    device_scale,
                    paint_glyphs,
                );
            }
        }
    }
    sugarloaf.end_overlay();
}

fn render_rail(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    origin_y: f32,
    height: f32,
    device_scale: f32,
) {
    let rail = activity_bar::rect(origin_y, height);
    paint_flat(sugarloaf, &rail, theme.rail_bg, DEPTH_BG, ORDER_RAIL);
    // Right hairline against the drawer.
    paint_hairline_v(
        sugarloaf,
        rail.right() - BORDER_WIDTH,
        rail.y,
        rail.height,
        theme.panel_border,
        DEPTH_BG + 0.005,
        ORDER_RAIL,
    );

    for section in activity_bar::TOP_SECTIONS {
        let item = activity_bar::section_rect(origin_y, section);
        let selected = section == chrome.activity.selected && chrome.panel_visible;
        if selected {
            let pill = activity_bar::pill_rect(item);
            paint_surface(
                sugarloaf,
                &pill,
                theme.rail_active_bg,
                None,
                activity_bar::PILL_RADIUS,
                DEPTH_CONTENT,
                ORDER_CONTENT,
                false,
            );
        }
        let icon_rect = activity_bar::icon_in(item);
        let color = if selected {
            theme.accent
        } else {
            as_f32(theme.text_muted)
        };
        draw_icon(
            sugarloaf,
            section.icon(),
            IconPlacement::new(icon_rect.x, icon_rect.y, RAIL_ICON_SIZE),
            color,
            device_scale,
        );
    }

    for action in activity_bar::BOTTOM_ACTIONS {
        let item = activity_bar::action_rect(origin_y, height, action);
        let icon_rect = activity_bar::icon_in(item);
        let color = match action {
            activity_bar::RailAction::CloudSync if chrome.activity.cloud_sync_active => {
                theme.success
            }
            activity_bar::RailAction::Settings if chrome.settings_is_open() => theme.accent,
            _ => as_f32(theme.text_muted),
        };
        draw_icon(
            sugarloaf,
            action.icon(),
            IconPlacement::new(icon_rect.x, icon_rect.y, RAIL_ICON_SIZE),
            color,
            device_scale,
        );
    }
}

fn render_panel(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    origin_y: f32,
    height: f32,
    window_width: f32,
    window_height: f32,
    device_scale: f32,
    connecting_phase: Option<f32>,
) {
    let panel = chrome.panel.rect(origin_y, height);
    paint_flat(sugarloaf, &panel, theme.panel_bg, DEPTH_BG, ORDER_PANEL);
    // A hairline separator against the terminal, on the panel's right edge.
    paint_hairline_v(
        sugarloaf,
        panel.right() - BORDER_WIDTH,
        panel.y,
        panel.height,
        theme.panel_border,
        DEPTH_BG + 0.005,
        ORDER_PANEL,
    );

    // Sugarloaf UI text always composites above quads, so glyphs that sit
    // under an open dialog would float on top of it. Keep painting the
    // dimmed background chrome, and only suppress labels/icons whose
    // rects overlap the dialog (and its dropdowns).
    let label_cover = add_host_label_cover(chrome, window_width, window_height);

    let title = chrome.panel.title_rect(origin_y);
    if !text_blocked_by(label_cover.as_ref(), &title) {
        draw_text(
            sugarloaf,
            title.x + sidebar::PAD_X,
            title.y + 14.0,
            &chrome.panel_title().to_ascii_uppercase(),
            SECTION_LABEL_SIZE,
            [0xcb, 0xd5, 0xe1, 255], // slate-300
            true,
        );
        paint_hairline_h(
            sugarloaf,
            title.x,
            title.bottom() - BORDER_WIDTH,
            title.width,
            theme.panel_border,
            DEPTH_CONTENT,
            ORDER_CONTENT,
        );

        if chrome.hosts_visible() {
            paint_search_field(
                sugarloaf,
                chrome,
                theme,
                origin_y,
                device_scale,
                DEPTH_CONTENT,
            );
            let band = chrome.panel.search_band_rect(origin_y);
            paint_hairline_h(
                sugarloaf,
                band.x,
                band.bottom() - BORDER_WIDTH,
                band.width,
                theme.panel_border,
                DEPTH_CONTENT,
                ORDER_CONTENT,
            );
        }
    }

    if chrome.hosts_visible() {
        let cta = chrome.panel.add_button_rect(origin_y, height);
        render_new_host_cta(
            sugarloaf,
            chrome,
            theme,
            origin_y,
            height,
            !text_blocked_by(label_cover.as_ref(), &cta),
            device_scale,
        );
        render_host_rows(
            sugarloaf,
            chrome,
            theme,
            origin_y,
            height,
            label_cover.as_ref(),
            device_scale,
            connecting_phase,
        );
        // Re-paint the title + search band above the scrolled list so cards
        // tuck under the filter instead of covering it.
        if !text_blocked_by(label_cover.as_ref(), &title) {
            render_sticky_drawer_chrome(sugarloaf, chrome, theme, origin_y, device_scale);
        }
    } else if chrome.snippets_visible() {
        render_snippets(
            sugarloaf,
            chrome,
            theme,
            origin_y,
            height,
            label_cover.is_none(),
            device_scale,
        );
    }

    // Sticky footer: paint last so host rows scroll underneath it.
    let notice_labels = chrome
        .panel
        .notice_rect(origin_y, height)
        .map(|n| !text_blocked_by(label_cover.as_ref(), &n))
        .unwrap_or(true);
    render_notice(
        sugarloaf,
        chrome,
        theme,
        origin_y,
        height,
        notice_labels,
    );
}

fn render_notice(
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
        let wash = [0xf8 as f32 / 255.0, 0x71 as f32 / 255.0, 0x71 as f32 / 255.0, 0.18];
        let ring = [0xf8 as f32 / 255.0, 0x71 as f32 / 255.0, 0x71 as f32 / 255.0, 0.55];
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
    let lines = if is_error {
        wrap_lines(sugarloaf, message, max_w, &text_opts, 2)
    } else {
        vec![elide(sugarloaf, message, max_w, &text_opts)]
    };
    let line_gap = 4.0;
    let block_h = lines.len() as f32 * HINT_SIZE + (lines.len().saturating_sub(1) as f32) * line_gap;
    let mut y = rect.y + ((rect.height - block_h) * 0.5).max(pad * 0.5);
    for line in lines {
        draw_text(sugarloaf, rect.x + pad, y, &line, HINT_SIZE, color, false);
        y += HINT_SIZE + line_gap;
    }
}

/// Opaque title + search band redrawn after the list so scrolled cards pass
/// underneath the filter instead of painting over it.
fn render_sticky_drawer_chrome(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    origin_y: f32,
    device_scale: f32,
) {
    let title = chrome.panel.title_rect(origin_y);
    let band = chrome.panel.search_band_rect(origin_y);
    let cover_bottom = band.bottom();
    paint_flat(
        sugarloaf,
        &Rect::new(title.x, title.y, title.width, cover_bottom - title.y),
        theme.panel_bg,
        DEPTH_STICKY,
        ORDER_CONTENT,
    );

    draw_text(
        sugarloaf,
        title.x + sidebar::PAD_X,
        title.y + 14.0,
        &chrome.panel_title().to_ascii_uppercase(),
        SECTION_LABEL_SIZE,
        [0xcb, 0xd5, 0xe1, 255],
        true,
    );
    paint_hairline_h(
        sugarloaf,
        title.x,
        title.bottom() - BORDER_WIDTH,
        title.width,
        theme.panel_border,
        DEPTH_STICKY + 0.001,
        ORDER_CONTENT,
    );

    paint_search_field(
        sugarloaf,
        chrome,
        theme,
        origin_y,
        device_scale,
        DEPTH_STICKY + 0.002,
    );
    paint_hairline_h(
        sugarloaf,
        band.x,
        band.bottom() - BORDER_WIDTH,
        band.width,
        theme.panel_border,
        DEPTH_STICKY + 0.006,
        ORDER_CONTENT,
    );
}

/// Search field chrome + loupe + filter text (mock `pl-9` with left icon).
fn paint_search_field(
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

    let placeholder = if chrome.panel.filter.is_empty() && !chrome.panel.filter_focused {
        "Filter servers…"
    } else {
        ""
    };
    let filter_text = if chrome.panel.filter.is_empty() {
        placeholder
    } else {
        chrome.panel.filter.as_str()
    };
    let filter_color = if chrome.panel.filter.is_empty() && !chrome.panel.filter_focused {
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

fn render_new_host_cta(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    origin_y: f32,
    height: f32,
    labels: bool,
    device_scale: f32,
) {
    let cta = chrome.panel.add_button_rect(origin_y, height);
    let body = chrome.panel.body_rect(origin_y, height);
    let Some((top, bottom)) = cta.clip_rows(body.y, body.bottom()) else {
        return;
    };
    if bottom - top < 8.0 {
        return;
    }

    // Mock: `border-dashed border-appleBorder bg-appleCard/40 hover:bg-appleCard`
    let bg = if chrome.panel.add_hover {
        theme.button_bg
    } else {
        with_alpha(theme.button_bg, 0.40)
    };
    let border = if chrome.panel.add_hover {
        theme.accent
    } else {
        theme.panel_border
    };
    let title_color = if chrome.panel.add_hover {
        color_from_f32(theme.accent)
    } else {
        theme.text
    };
    paint_dashed_cta(
        sugarloaf,
        theme,
        &cta,
        sidebar::CARD_RADIUS,
        bg,
        border,
        "New Host",
        "Configure SSH connection",
        title_color,
        Icon::Plus,
        device_scale,
        labels,
        DEPTH_CONTENT,
        ORDER_CONTENT,
    );
}

/// Approximate a CSS `border-dashed` rounded rectangle.
///
/// Uses SDF `rounded_rect` capsules (not hard-edged `line` quads) so
/// straight dashes and corner arcs stay anti-aliased. Corners are a
/// chain of overlapping circular pills along the quarter-circle —
/// `sugarloaf.arc` still ignores paint order, so we cannot use it here.
fn draw_dashed_rounded_rect(
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

fn render_snippets(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    origin_y: f32,
    height: f32,
    labels: bool,
    device_scale: f32,
) {
    let body = chrome.snippets.body_rect(origin_y, height);
    if chrome.snippets.items.is_empty() && labels {
        draw_text(
            sugarloaf,
            body.x + 16.0,
            body.y + 18.0,
            "No snippets yet",
            ROW_TITLE_SIZE,
            theme.text_muted,
            false,
        );
        draw_text(
            sugarloaf,
            body.x + 16.0,
            body.y + 38.0,
            "Save frequent commands here.",
            ROW_SUB_SIZE,
            theme.text_muted,
            false,
        );
    }
    for (index, item) in chrome.snippets.items.iter().enumerate() {
        let row = chrome.snippets.item_rect(origin_y, index);
        let Some((top, bottom)) = row.clip_rows(body.y, body.bottom()) else {
            continue;
        };
        if bottom - top < 8.0 {
            continue;
        }
        let hovered = chrome.snippets.hover == Some(index);
        let bg = if hovered {
            theme.item_hover
        } else {
            theme.button_bg
        };
        paint_surface(
            sugarloaf,
            &row,
            bg,
            Some(theme.panel_border),
            sidebar::CARD_RADIUS,
            DEPTH_CONTENT,
            ORDER_CONTENT,
            false,
        );
        if labels {
            let pad_x = 12.0;
            let title_color = if hovered {
                color_from_f32(theme.accent)
            } else {
                theme.text
            };
            let title_w = row.width - pad_x * 2.0 - if hovered { 22.0 } else { 0.0 };
            let name = elide(
                sugarloaf,
                &item.name,
                title_w,
                &opts(ROW_TITLE_SIZE, title_color, true),
            );
            draw_text(
                sugarloaf,
                row.x + pad_x,
                row.y + 10.0,
                &name,
                ROW_TITLE_SIZE,
                title_color,
                true,
            );

            let cmd_y = row.y + 30.0;
            let max_cmd_w = row.width - pad_x * 2.0 - 22.0;
            let cmd = elide(
                sugarloaf,
                &item.cmd,
                max_cmd_w,
                &opts(HINT_SIZE, theme.text_muted, false),
            );
            draw_icon(
                sugarloaf,
                Icon::SquareTerminal,
                IconPlacement::new(row.x + pad_x, cmd_y, 14.0),
                [
                    theme.text_muted[0] as f32 / 255.0,
                    theme.text_muted[1] as f32 / 255.0,
                    theme.text_muted[2] as f32 / 255.0,
                    theme.text_muted[3] as f32 / 255.0,
                ],
                device_scale,
            );
            draw_text(
                sugarloaf,
                row.x + pad_x + 20.0,
                cmd_y + 1.0,
                &cmd,
                HINT_SIZE,
                theme.text_muted,
                false,
            );

            if hovered {
                let del_rect = chrome.snippets.delete_button_rect(origin_y, index);
                let del_hover = chrome.snippets.delete_hover == Some(index);
                let del_color = if del_hover {
                    theme.danger
                } else {
                    theme.text_muted
                };
                draw_icon(
                    sugarloaf,
                    Icon::X,
                    IconPlacement::new(del_rect.x, del_rect.y, 16.0),
                    [
                        del_color[0] as f32 / 255.0,
                        del_color[1] as f32 / 255.0,
                        del_color[2] as f32 / 255.0,
                        del_color[3] as f32 / 255.0,
                    ],
                    device_scale,
                );
            }
        }
    }

    render_footer_button(
        sugarloaf,
        chrome.snippets.add_button_rect(origin_y, height),
        chrome.snippets.add_hover,
        Icon::Plus,
        "Add snippet",
        theme,
        labels,
        device_scale,
    );
}

fn render_settings_modal(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    window_width: f32,
    window_height: f32,
    device_scale: f32,
    paint_glyphs: bool,
) {
    let dialog = chrome.settings.dialog_rect(window_width, window_height);
    paint_dialog_shell(
        sugarloaf,
        theme,
        window_width,
        window_height,
        &dialog,
        terminus_ui::settings::RADIUS,
        DialogBorderMode::Inset,
        DEPTH_DIALOG_BG,
        DEPTH_DIALOG,
        ORDER_DIALOG,
        None,
    );
    if !paint_glyphs {
        return;
    }

    // Header band
    let header = Rect::new(
        dialog.x + BORDER_WIDTH,
        dialog.y + BORDER_WIDTH,
        dialog.width - 2.0 * BORDER_WIDTH,
        64.0,
    );
    paint_surface(
        sugarloaf,
        &header,
        theme.dialog_header,
        None,
        terminus_ui::settings::RADIUS - 1.0,
        DEPTH_DIALOG + 0.02,
        ORDER_DIALOG,
        false,
    );
    // Square off header bottom corners
    paint_flat(
        sugarloaf,
        &Rect::new(
            dialog.x + BORDER_WIDTH,
            dialog.y + 40.0,
            dialog.width - 2.0 * BORDER_WIDTH,
            28.0,
        ),
        theme.dialog_header,
        DEPTH_DIALOG + 0.021,
        ORDER_DIALOG,
    );

    let badge = Rect::new(dialog.x + 20.0, dialog.y + 16.0, 36.0, 36.0);
    // Accent ring (`border-appleAccent/20`) then soft fill.
    let badge_shell = Rect::new(
        badge.x - BORDER_WIDTH,
        badge.y - BORDER_WIDTH,
        badge.width + 2.0 * BORDER_WIDTH,
        badge.height + 2.0 * BORDER_WIDTH,
    );
    paint_surface(
        sugarloaf,
        &badge_shell,
        theme.accent_soft,
        Some(with_alpha(theme.accent, 0.20)),
        13.0,
        DEPTH_DIALOG + 0.029,
        ORDER_DIALOG,
        false,
    );
    draw_icon(
        sugarloaf,
        Icon::Settings,
        IconPlacement::new(badge.x + 8.0, badge.y + 8.0, 20.0),
        theme.accent,
        device_scale,
    );
    draw_text(
        sugarloaf,
        badge.right() + 12.0,
        dialog.y + 20.0,
        "Settings",
        TITLE_SIZE,
        theme.text,
        true,
    );
    draw_text(
        sugarloaf,
        badge.right() + 12.0,
        dialog.y + 38.0,
        "Cryptographic identities and remote database synchronization.",
        HINT_SIZE,
        theme.text_muted,
        false,
    );

    // Close (×) — mock: w-7 h-7 rounded-full bordered card.
    let close = chrome
        .settings
        .close_button_rect(window_width, window_height);
    paint_surface(
        sugarloaf,
        &close,
        theme.button_bg,
        Some(theme.panel_border),
        close.width * 0.5,
        DEPTH_DIALOG + 0.03,
        ORDER_DIALOG,
        false,
    );
    draw_icon(
        sugarloaf,
        Icon::X,
        IconPlacement::new(close.x + 7.0, close.y + 7.0, 14.0),
        as_f32(theme.text_muted),
        device_scale,
    );

    // Sidebar
    let side_x = dialog.x + BORDER_WIDTH;
    let side_y = dialog.y + 66.0;
    let side_h = dialog.height - 66.0 - 48.0;
    paint_flat(
        sugarloaf,
        &Rect::new(
            side_x,
            side_y,
            terminus_ui::settings::SIDEBAR_WIDTH,
            side_h,
        ),
        theme.dialog_header,
        DEPTH_DIALOG + 0.02,
        ORDER_DIALOG,
    );

    for tab in [
        terminus_ui::SettingsTab::Keys,
        terminus_ui::SettingsTab::SqlSync,
    ] {
        let rect = chrome
            .settings
            .tab_rect(window_width, window_height, tab);
        let selected = chrome.settings.tab == tab;
        if selected {
            paint_surface(
                sugarloaf,
                &rect,
                theme.accent_soft,
                None,
                12.0,
                DEPTH_DIALOG + 0.03,
                ORDER_DIALOG,
                false,
            );
        }
        let (icon, label) = match tab {
            terminus_ui::SettingsTab::Keys => (Icon::KeyRound, "SSH Keys"),
            terminus_ui::SettingsTab::SqlSync => (Icon::Database, "Remote SQL Sync"),
        };
        let color = if selected {
            theme.accent
        } else {
            as_f32(theme.text_muted)
        };
        draw_icon(
            sugarloaf,
            icon,
            IconPlacement::new(rect.x + 10.0, rect.y + 8.0, 16.0),
            color,
            device_scale,
        );
        draw_text(
            sugarloaf,
            rect.x + 34.0,
            rect.y + 10.0,
            label,
            ROW_SUB_SIZE,
            if selected {
                color_from_f32(theme.accent)
            } else {
                theme.text_muted
            },
            selected,
        );
    }

    let content_x = dialog.x + terminus_ui::settings::SIDEBAR_WIDTH + 24.0;
    let content_y = dialog.y + 80.0;
    // Settings content pane: mock `bg-appleBg` (#18181b).
    paint_flat(
        sugarloaf,
        &Rect::new(
            dialog.x + terminus_ui::settings::SIDEBAR_WIDTH,
            dialog.y + 66.0,
            dialog.width - terminus_ui::settings::SIDEBAR_WIDTH - BORDER_WIDTH,
            dialog.height - 66.0 - 48.0,
        ),
        theme.shell_bg,
        DEPTH_DIALOG + 0.015,
        ORDER_DIALOG,
    );
    match chrome.settings.tab {
            terminus_ui::SettingsTab::Keys => {
            draw_text(
                sugarloaf,
                content_x,
                content_y,
                "Managed SSH Keys",
                TITLE_SIZE,
                theme.text,
                true,
            );
            draw_text(
                sugarloaf,
                content_x,
                content_y + 18.0,
                "Secure local keys for passwordless authentication.",
                HINT_SIZE,
                theme.text_muted,
                false,
            );
            let cta = chrome
                .settings
                .new_key_cta_rect(window_width, window_height);
            paint_dashed_cta(
                sugarloaf,
                theme,
                &cta,
                12.0,
                with_alpha(theme.button_bg, 0.40),
                theme.panel_border,
                "New SSH Key",
                "Generate or import cryptographic identity",
                theme.text,
                Icon::Plus,
                device_scale,
                true,
                DEPTH_DIALOG + 0.03,
                ORDER_DIALOG,
            );

            if let Some(draft) = chrome
                .settings
                .key_draft_rect(window_width, window_height)
            {
                paint_surface(
                    sugarloaf,
                    &draft,
                    theme.button_bg,
                    Some(theme.panel_border),
                    12.0,
                    DEPTH_DIALOG + 0.03,
                    ORDER_DIALOG,
                    false,
                );
                if let Some(field) = chrome
                    .settings
                    .key_draft_field_rect(window_width, window_height)
                {
                    let focused = chrome.settings.key_draft_focused;
                    let paint = terminus_ui::FieldPaint::from_draft(
                        &chrome.settings.key_label,
                        "Key label (e.g. Laptop Ed25519)",
                        focused,
                    );
                    paint_settings_field_card(
                        sugarloaf,
                        theme,
                        field,
                        "Label",
                        &paint.text,
                        focused,
                        paint.placeholder,
                        0.0,
                        true,
                    );
                    if paint.show_caret {
                        paint_field_caret_prefix(
                            sugarloaf,
                            theme,
                            field,
                            &chrome.settings.key_label.prefix_display(),
                            0.0,
                        );
                    }
                }
                if let Some(pem_card) = chrome
                    .settings
                    .key_draft_pem_rect(window_width, window_height)
                {
                    let focused = chrome.settings.key_draft_pem_focused;
                    let empty = chrome.settings.key_pem.value.is_empty();
                    let paint = if !empty && !focused {
                        terminus_ui::FieldPaint {
                            text: "••••••••  OpenSSH private key ready".into(),
                            placeholder: false,
                            show_caret: false,
                        }
                    } else {
                        terminus_ui::FieldPaint::from_draft(
                            &chrome.settings.key_pem,
                            "Paste OpenSSH private key to import (optional)",
                            focused,
                        )
                    };
                    paint_settings_field_card(
                        sugarloaf,
                        theme,
                        pem_card,
                        "Private key",
                        &paint.text,
                        focused,
                        paint.placeholder,
                        0.0,
                        true,
                    );
                    if paint.show_caret {
                        paint_field_caret_prefix(
                            sugarloaf,
                            theme,
                            pem_card,
                            &chrome.settings.key_pem.prefix_display(),
                            0.0,
                        );
                    }
                }
                if let Some(gen) = chrome
                    .settings
                    .key_draft_generate_rect(window_width, window_height)
                {
                    let gen_label = if chrome.settings.key_draft_wants_import() {
                        "Import"
                    } else {
                        "Generate"
                    };
                    paint_chrome_button(
                        sugarloaf,
                        theme,
                        terminus_ui::ButtonSpec::primary(gen),
                        gen_label,
                        ROW_SUB_SIZE,
                        DEPTH_DIALOG + 0.04,
                        ORDER_DIALOG,
                    );
                }
                if let Some(cancel) = chrome
                    .settings
                    .key_draft_cancel_rect(window_width, window_height)
                {
                    paint_chrome_button(
                        sugarloaf,
                        theme,
                        terminus_ui::ButtonSpec::secondary(cancel),
                        "Cancel",
                        ROW_SUB_SIZE,
                        DEPTH_DIALOG + 0.04,
                        ORDER_DIALOG,
                    );
                }
                if let Some(banner) = chrome
                    .settings
                    .key_draft_error_banner_rect(window_width, window_height)
                {
                    let err_text = chrome
                        .settings
                        .key_draft_error
                        .as_deref()
                        .unwrap_or("");
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
                        0.45,
                    ];
                    paint_surface(
                        sugarloaf,
                        &banner,
                        opaque_over(theme.button_bg, wash),
                        Some(opaque_over(theme.button_bg, ring)),
                        12.0,
                        DEPTH_DIALOG + 0.04,
                        ORDER_DIALOG,
                        true,
                    );
                    let shown = elide(
                        sugarloaf,
                        err_text,
                        banner.width - 2.0 * terminus_ui::settings::FIELD_CARD_PAD,
                        &opts(HINT_SIZE, theme.danger, false),
                    );
                    draw_text(
                        sugarloaf,
                        banner.x + terminus_ui::settings::FIELD_CARD_PAD,
                        banner.y + (banner.height - HINT_SIZE) * 0.5,
                        &shown,
                        HINT_SIZE,
                        theme.danger,
                        false,
                    );
                }
            }

            for (index, key) in chrome.settings.keys.iter().enumerate() {
                let row = chrome
                    .settings
                    .key_row_rect(window_width, window_height, index);
                paint_surface(
                    sugarloaf,
                    &row,
                    theme.button_bg,
                    Some(theme.panel_border),
                    12.0,
                    DEPTH_DIALOG + 0.03,
                    ORDER_DIALOG,
                    false,
                );
                draw_icon(
                    sugarloaf,
                    Icon::KeyRound,
                    IconPlacement::new(row.x + 14.0, row.y + 18.0, 16.0),
                    theme.accent,
                    device_scale,
                );
                let text_x = row.x + 40.0;
                let del = chrome
                    .settings
                    .key_delete_rect(window_width, window_height, index);
                draw_text(
                    sugarloaf,
                    text_x,
                    row.y + 14.0,
                    &key.name,
                    ROW_TITLE_SIZE,
                    theme.text,
                    true,
                );
                let fp_max = (del.x - text_x - 12.0).max(40.0);
                let fp_shown = elide(
                    sugarloaf,
                    &key.fingerprint,
                    fp_max,
                    &opts(HINT_SIZE, theme.text_muted, false),
                );
                draw_text(
                    sugarloaf,
                    text_x,
                    row.y + 32.0,
                    &fp_shown,
                    HINT_SIZE,
                    theme.text_muted,
                    false,
                );
                // Delete only appears while the row is hovered (mock:
                // opacity-0 group-hover:opacity-100). Turns red when the
                // pointer is on the control itself.
                if chrome.settings.key_row_hover == Some(index) {
                    let del_color = if chrome.settings.key_delete_hover == Some(index)
                    {
                        theme.danger
                    } else {
                        theme.text_muted
                    };
                    draw_text(
                        sugarloaf,
                        del.x,
                        del.y + (del.height - ROW_SUB_SIZE) * 0.5,
                        "Delete",
                        ROW_SUB_SIZE,
                        del_color,
                        false,
                    );
                }
            }
        }
            terminus_ui::SettingsTab::SqlSync => {
            draw_text(
                sugarloaf,
                content_x,
                content_y,
                "Remote Database Sync",
                TITLE_SIZE,
                theme.text,
                true,
            );
            if chrome.settings.sync_connected {
                let pill = Rect::new(content_x + 168.0, content_y - 2.0, 84.0, 20.0);
                paint_surface(
                    sugarloaf,
                    &pill,
                    rgba_u8(0x10, 0xb9, 0x81, 0.10),
                    None,
                    8.0,
                    DEPTH_DIALOG + 0.03,
                    ORDER_DIALOG,
                    false,
                );
                draw_text(
                    sugarloaf,
                    content_x + 180.0,
                    content_y + 2.0,
                    "Connected",
                    HINT_SIZE,
                    [0x34, 0xd3, 0x99, 255],
                    false,
                );
            }
            draw_text(
                sugarloaf,
                content_x,
                content_y + 22.0,
                "Configure your remote database engine and encrypted connection string.",
                HINT_SIZE,
                theme.text_muted,
                false,
            );

            let engine_card = chrome.settings.engine_card_rect(window_width, window_height);
            let uri_card = chrome.settings.uri_card_rect(window_width, window_height);
            let pass_card = chrome.settings.passphrase_card_rect(window_width, window_height);
            let status_row = chrome.settings.status_row_rect(window_width, window_height);

            // Engine selector card
            paint_settings_field_card(
                sugarloaf,
                theme,
                engine_card,
                "Database Engine",
                chrome.settings.engine_label(),
                chrome.settings.engine_menu_open,
                false,
                terminus_ui::settings::FIELD_EYE_SLOT,
                true,
            );
            let engine_input =
                terminus_ui::settings::field_input_in_card(engine_card);
            draw_text(
                sugarloaf,
                engine_input.right() - 18.0,
                engine_input.y + (engine_input.height - ROW_SUB_SIZE) * 0.5,
                if chrome.settings.engine_menu_open {
                    "▴"
                } else {
                    "▾"
                },
                ROW_SUB_SIZE,
                theme.text_muted,
                false,
            );

            // URI field: always paint the card quads so the "div" stays
            // behind the dropdown. Skip only the label/value text while
            // the menu is open — sugarloaf's UI text pass is always after
            // every quad, so URI glyphs would otherwise float on top of
            // the opaque menu (same pattern as tab-drag title hiding).
            let uri_paint = chrome.settings.uri_field_paint();
            let uri_paint_text = !chrome.settings.engine_menu_open;
            paint_settings_field_card(
                sugarloaf,
                theme,
                uri_card,
                "Connection URI / String",
                &uri_paint.text,
                chrome.settings.sql_focus == terminus_ui::SqlSyncFocus::Uri,
                uri_paint.placeholder,
                0.0,
                uri_paint_text,
            );
            if uri_paint_text && uri_paint.show_caret {
                paint_settings_caret(sugarloaf, theme, uri_card, &uri_paint.text, 0.0);
            }

            // Passphrase field
            let pass_paint = chrome.settings.passphrase_field_paint();
            paint_settings_field_card(
                sugarloaf,
                theme,
                pass_card,
                "Encryption Passphrase",
                &pass_paint.text,
                chrome.settings.sql_focus == terminus_ui::SqlSyncFocus::Passphrase,
                pass_paint.placeholder,
                terminus_ui::settings::FIELD_EYE_SLOT,
                true,
            );
            if pass_paint.show_caret {
                paint_settings_caret(
                    sugarloaf,
                    theme,
                    pass_card,
                    &pass_paint.text,
                    terminus_ui::settings::FIELD_EYE_SLOT,
                );
            }

            // Passphrase visibility toggle — eye / eye-off icon.
            let eye = chrome
                .settings
                .passphrase_toggle_rect(window_width, window_height);
            let eye_icon = if chrome.settings.passphrase_visible {
                Icon::EyeOff
            } else {
                Icon::Eye
            };
            let eye_size = terminus_ui::settings::FIELD_EYE_ICON;
            draw_icon(
                sugarloaf,
                eye_icon,
                IconPlacement::new(
                    eye.x + (eye.width - eye_size) * 0.5,
                    eye.y + (eye.height - eye_size) * 0.5,
                    eye_size,
                ),
                theme.accent,
                device_scale,
            );

            paint_surface(
                sugarloaf,
                &status_row,
                theme.button_bg,
                None,
                12.0,
                DEPTH_DIALOG + 0.03,
                ORDER_DIALOG,
                false,
            );
            // Status on its own line (full block width); Unlock / Test Sync sit
            // on the row below so long sync messages are not cropped.
            let status_label = if chrome.settings.vault_unlocked
                && !chrome.settings.sync_status.to_ascii_lowercase().contains("vault")
            {
                format!("Vault unlocked · {}", chrome.settings.sync_status)
            } else {
                chrome.settings.sync_status.clone()
            };
            let status_text = chrome
                .settings
                .status_text_rect(window_width, window_height);
            let status_shown = elide(
                sugarloaf,
                &status_label,
                status_text.width,
                &opts(HINT_SIZE, theme.text_muted, false),
            );
            draw_text(
                sugarloaf,
                status_text.x,
                status_text.y + (status_text.height - HINT_SIZE) * 0.5,
                &status_shown,
                HINT_SIZE,
                theme.text_muted,
                false,
            );

            let unlock = chrome
                .settings
                .unlock_vault_button_rect(window_width, window_height);
            let test = chrome
                .settings
                .test_sync_button_rect(window_width, window_height);

            paint_chrome_button(
                sugarloaf,
                theme,
                terminus_ui::ButtonSpec::secondary(unlock),
                "Unlock Vault",
                ROW_SUB_SIZE,
                DEPTH_DIALOG + 0.04,
                ORDER_DIALOG,
            );
            paint_chrome_button(
                sugarloaf,
                theme,
                terminus_ui::ButtonSpec::primary(test),
                "Test Sync",
                ROW_SUB_SIZE,
                DEPTH_DIALOG + 0.04,
                ORDER_DIALOG,
            );
            if let Some(forget) = chrome
                .settings
                .forget_passphrase_button_rect(window_width, window_height)
            {
                paint_chrome_button(
                    sugarloaf,
                    theme,
                    terminus_ui::ButtonSpec::secondary(forget),
                    "Forget passphrase",
                    ROW_SUB_SIZE,
                    DEPTH_DIALOG + 0.04,
                    ORDER_DIALOG,
                );
            }

            if let Some(banner) = chrome
                .settings
                .error_banner_rect(window_width, window_height)
            {
                let err_text = chrome
                    .settings
                    .sync_error
                    .as_deref()
                    .unwrap_or("");
                // Soft red wash — low alpha so the dialog chrome still reads.
                let wash = [
                    0xf8 as f32 / 255.0,
                    0x71 as f32 / 255.0,
                    0x71 as f32 / 255.0,
                    0.14,
                ];
                paint_surface(
                    sugarloaf,
                    &banner,
                    wash,
                    Some([
                        0xf8 as f32 / 255.0,
                        0x71 as f32 / 255.0,
                        0x71 as f32 / 255.0,
                        0.35,
                    ]),
                    12.0,
                    DEPTH_DIALOG + 0.03,
                    ORDER_DIALOG,
                    true,
                );
                let err_shown = elide(
                    sugarloaf,
                    err_text,
                    banner.width - 2.0 * terminus_ui::settings::FIELD_CARD_PAD,
                    &opts(HINT_SIZE, theme.danger, false),
                );
                draw_text(
                    sugarloaf,
                    banner.x + terminus_ui::settings::FIELD_CARD_PAD,
                    banner.y + (banner.height - HINT_SIZE) * 0.5,
                    &err_shown,
                    HINT_SIZE,
                    theme.danger,
                    false,
                );
            }

            // Engine dropdown: higher paint *order* than dialog cards.
            // (In sugarloaf, larger depth is further back — dialog BG is 0.2
            // while content is ~0.1 — so a bigger depth would go *behind*
            // the URI card. Order is what lifts the popover.)
            if chrome.settings.engine_menu_open {
                let menu = chrome
                    .settings
                    .engine_menu_rect(window_width, window_height);
                paint_surface(
                    sugarloaf,
                    &menu,
                    theme.button_bg,
                    Some(theme.panel_border),
                    10.0,
                    DEPTH_DIALOG,
                    ORDER_DIALOG_POPOVER,
                    false,
                );
                for i in 0..terminus_ui::SQL_ENGINES.len() {
                    let opt = chrome
                        .settings
                        .engine_option_rect(window_width, window_height, i);
                    let selected = chrome.settings.sql_engine == i;
                    let hovered = chrome.settings.engine_menu_hover == Some(i);
                    if selected || hovered {
                        paint_surface(
                            sugarloaf,
                            &opt,
                            if hovered {
                                theme.item_hover
                            } else {
                                theme.accent_soft
                            },
                            None,
                            6.0,
                            DEPTH_DIALOG + 0.002,
                            ORDER_DIALOG_POPOVER,
                            false,
                        );
                    }
                    draw_text(
                        sugarloaf,
                        opt.x + 12.0,
                        opt.y + 8.0,
                        terminus_ui::SQL_ENGINES[i],
                        ROW_SUB_SIZE,
                        if selected {
                            color_from_f32(theme.accent)
                        } else {
                            theme.text
                        },
                        selected,
                    );
                    if selected {
                        draw_text(
                            sugarloaf,
                            opt.right() - 22.0,
                            opt.y + 8.0,
                            "✓",
                            ROW_SUB_SIZE,
                            color_from_f32(theme.accent),
                            true,
                        );
                    }
                }
            }
        }
    }

    // Footer
    paint_flat(
        sugarloaf,
        &Rect::new(
            dialog.x + BORDER_WIDTH,
            dialog.bottom() - 48.0,
            dialog.width - 2.0 * BORDER_WIDTH,
            47.0,
        ),
        theme.dialog_header,
        DEPTH_DIALOG + 0.02,
        ORDER_DIALOG,
    );
    let done = chrome.settings.done_rect(window_width, window_height);
    paint_chrome_button(
        sugarloaf,
        theme,
        terminus_ui::ButtonSpec::secondary(done),
        "Done",
        ROW_SUB_SIZE,
        DEPTH_DIALOG + 0.04,
        ORDER_DIALOG,
    );
}

fn color_from_f32(c: [f32; 4]) -> [u8; 4] {
    [
        (c[0] * 255.0).round() as u8,
        (c[1] * 255.0).round() as u8,
        (c[2] * 255.0).round() as u8,
        (c[3] * 255.0).round() as u8,
    ]
}

/// Whether sugarloaf UI text for `item` would float on top of `cover`.
fn text_blocked_by(cover: Option<&Rect>, item: &Rect) -> bool {
    cover.is_some_and(|c| terminus_ui::rects_overlap(*c, *item))
}

/// Dialog (+ open auth/identity menus) that must not have background glyphs
/// painted underneath — sugarloaf text always composites above quads.
/// Prefer [`Sugarloaf::begin_overlay`] for full dialogs; this cover remains
/// for sidebar host-row labels under the add-host panel.
fn add_host_label_cover(
    chrome: &Chrome,
    window_width: f32,
    window_height: f32,
) -> Option<Rect> {
    if !chrome.add_host_is_open() {
        return None;
    }
    let layout = chrome.dialog_layout(window_width, window_height);
    let mut cover = layout.rect(chrome.form.height());
    if chrome.form.auth_menu_open() {
        if let Some(menu) = layout.auth_menu_rect(&chrome.form) {
            cover = rect_union(cover, menu);
        }
    }
    if chrome.form.identity_menu_open() {
        if let Some(menu) = layout.identity_menu_rect(&chrome.form) {
            cover = rect_union(cover, menu);
        }
    }
    Some(cover)
}

fn rect_union(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = a.right().max(b.right());
    let y1 = a.bottom().max(b.bottom());
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

fn rgba_u8(r: u8, g: u8, b: u8, a: f32) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a]
}

fn render_host_rows(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    origin_y: f32,
    height: f32,
    label_cover: Option<&Rect>,
    device_scale: f32,
    connecting_phase: Option<f32>,
) {
    let body = chrome.panel.body_rect(origin_y, height);
    if body.height <= 0.0 {
        return;
    }

    // Group trays paint as containing boxes; Local hosts stay discrete cards.

    let visible = chrome.panel.visible_row_indices();
    for &index in &visible {
        let row_kind = &chrome.panel.rows[index];
        let row = chrome.panel.item_rect(origin_y, index);
        // Clip: a partially scrolled row is drawn truncated rather than
        // over the header or the add-host row.
        let Some((top, bottom)) = row.clip_rows(body.y, body.bottom()) else {
            continue;
        };
        let labels = !text_blocked_by(label_cover, &row);

        // A section label is a heading, not a target: no hover, no
        // selection marker, no icon.
        if let Some(label) = row_kind.label() {
            let label_bottom = row.y + sidebar::SECTION_HEIGHT;
            if labels && bottom - top >= 10.0 && bottom >= row.y + 8.0 && top <= label_bottom {
                draw_text(
                    sugarloaf,
                    row.x,
                    row.y + (sidebar::SECTION_HEIGHT - SECTION_LABEL_SIZE) * 0.5,
                    &label.to_uppercase(),
                    SECTION_LABEL_SIZE,
                    theme.text_faint,
                    true,
                );
                if label.eq_ignore_ascii_case("Hosts") {
                    paint_new_group_button(
                        sugarloaf,
                        chrome,
                        theme,
                        origin_y,
                        height,
                        labels,
                        device_scale,
                    );
                }
            }
            if label.eq_ignore_ascii_case("Hosts") {
                paint_new_group_form(sugarloaf, chrome, theme, origin_y, labels, &body);
            }
            continue;
        }

        if let Some((name, host_count, collapsed)) = row_kind.group() {
            let card = chrome.panel.card_rect(origin_y, index);
            if bottom - top < 8.0 {
                continue;
            }
            let hovered = chrome.panel.hover == Some(index);

            // Real containing box: one tray around header + nested hosts.
            if let Some(tray) = chrome.panel.group_tray_rect(origin_y, index) {
                if let Some((tray_top, tray_bottom)) = tray.clip_rows(body.y, body.bottom()) {
                    if tray_bottom - tray_top >= 4.0 {
                        let group_id = match row_kind {
                            sidebar::Row::Group { id, .. } => Some(id.as_str()),
                            _ => None,
                        };
                        let drop_hl = chrome.panel.host_drag.as_ref().is_some_and(|d| {
                            d.started()
                                && matches!(
                                    &d.drop_target,
                                    Some(sidebar::HostDropTarget::Group(gid))
                                        | Some(sidebar::HostDropTarget::BeforeGroup(gid))
                                        if Some(gid.as_str()) == group_id
                                )
                        });
                        let border = if drop_hl {
                            Some(theme.accent)
                        } else {
                            Some(theme.panel_border)
                        };
                        let fill = if drop_hl {
                            opaque_over(theme.button_bg, theme.accent_soft)
                        } else {
                            theme.button_bg
                        };
                        paint_floating_surface(
                            sugarloaf,
                            &tray,
                            fill,
                            border,
                            sidebar::CARD_RADIUS,
                        );
                    }
                }
            }

            // Header wash only on hover — the tray already provides the fill.
            if hovered {
                let wash = Rect::new(
                    card.x + 1.0,
                    card.y + 1.0,
                    (card.width - 2.0).max(0.0),
                    (card.height - 2.0).max(0.0),
                );
                paint_surface(
                    sugarloaf,
                    &wash,
                    theme.item_hover,
                    None,
                    (sidebar::CARD_RADIUS - 1.0).max(0.0),
                    DEPTH_CONTENT + 0.012,
                    ORDER_CONTENT,
                    false,
                );
            }

            // Hairline under the folder header.
            if !collapsed {
                let sep_y = card.y + sidebar::ITEM_HEIGHT - 1.0;
                if sep_y >= top && sep_y <= bottom {
                    paint_hairline_h(
                        sugarloaf,
                        card.x + sidebar::CARD_PAD,
                        sep_y,
                        (card.width - 2.0 * sidebar::CARD_PAD).max(0.0),
                        with_alpha(theme.panel_border, 0.85),
                        DEPTH_CONTENT + 0.013,
                        ORDER_CONTENT,
                    );
                }
            }

            if labels && bottom - top >= 14.0 {
                let badge_x = card.x + sidebar::CARD_PAD;
                let badge_y =
                    card.y + (sidebar::ITEM_HEIGHT - sidebar::HOST_BADGE_TILE) / 2.0;
                let badge = Rect::new(
                    badge_x,
                    badge_y,
                    sidebar::HOST_BADGE_TILE,
                    sidebar::HOST_BADGE_TILE,
                );
                // Folder badge with soft accent ring (mock).
                paint_bordered_badge(
                    sugarloaf,
                    &badge,
                    theme.field_bg,
                    theme.accent,
                    8.0,
                    DEPTH_CONTENT + 0.02,
                    ORDER_CONTENT,
                );
                draw_icon(
                    sugarloaf,
                    Icon::Folder,
                    IconPlacement::new(
                        badge_x + (sidebar::HOST_BADGE_TILE - sidebar::ICON_SIZE) / 2.0,
                        badge_y + (sidebar::HOST_BADGE_TILE - sidebar::ICON_SIZE) / 2.0,
                        sidebar::ICON_SIZE,
                    ),
                    theme.accent,
                    device_scale,
                );
                let text_x = badge_x + sidebar::HOST_BADGE_TILE + sidebar::ICON_GAP;
                let renaming = chrome.panel.is_renaming(
                    match row_kind {
                        sidebar::Row::Group { id, .. } => id.as_str(),
                        _ => "",
                    },
                );
                if renaming {
                    if let Some(draft) = chrome.panel.rename.as_ref() {
                        paint_rename_text(sugarloaf, draft, text_x, card.y + 12.0, theme);
                    }
                } else {
                    draw_text(
                        sugarloaf,
                        text_x,
                        card.y + 12.0,
                        name,
                        ROW_TITLE_SIZE,
                        theme.text,
                        true,
                    );
                }
                let sessions = match row_kind {
                    sidebar::Row::Group { session_count, .. } => *session_count,
                    _ => 0,
                };
                let count = match (host_count, sessions) {
                    (0, _) => "Empty group".to_string(),
                    (1, 0) => "1 host".to_string(),
                    (1, s) => format!("1 host · {s} sessions"),
                    (n, 0) => format!("{n} hosts"),
                    (n, s) => format!("{n} hosts · {s} sessions"),
                };
                draw_text(
                    sugarloaf,
                    text_x,
                    card.y + 32.0,
                    &count,
                    ROW_SUB_SIZE,
                    theme.text_muted,
                    false,
                );
                let chevron = if collapsed {
                    Icon::ChevronRight
                } else {
                    Icon::ChevronDown
                };
                let icon_sz = sidebar::ICON_SIZE;
                let icon_x = card.right() - sidebar::CARD_PAD - icon_sz;
                let icon_y = card.y + (sidebar::ITEM_HEIGHT - icon_sz) / 2.0;
                draw_icon(
                    sugarloaf,
                    chevron,
                    IconPlacement::new(icon_x, icon_y, icon_sz),
                    as_f32(theme.text_muted),
                    device_scale,
                );
            }
            continue;
        }

        if let Some(session) = row_kind.session() {
            let card = chrome.panel.card_rect(origin_y, index);
            if bottom - top < 4.0 {
                continue;
            }
            let hovered = chrome.panel.hover == Some(index);
            let active = session.active
                || chrome.panel.selected_session == Some(session.tab_index);
            // Soft leaf + thin accent ring (follows the radius). No left bar —
            // straight bars fight rounded corners and kept regressing.
            let r = sidebar::SESSION_RADIUS;
            let inset = 2.0;
            let leaf = Rect::new(
                card.x + inset,
                card.y + 1.0,
                (card.width - inset * 2.0).max(0.0),
                (card.height - 2.0).max(0.0),
            );
            if active {
                let soft = opaque_over(theme.panel_bg, theme.accent_soft);
                paint_floating_surface(sugarloaf, &leaf, soft, Some(theme.accent), r);
            } else if hovered {
                paint_floating_surface(sugarloaf, &leaf, theme.item_hover, None, r);
            }

            if labels && bottom - top >= 10.0 {
                let icon_color = if active {
                    theme.accent
                } else {
                    as_f32(theme.text_muted)
                };
                let text_color = if active {
                    theme.text
                } else {
                    theme.text_muted
                };
                let content_x = leaf.x + sidebar::SESSION_CONTENT_PAD;
                draw_icon(
                    sugarloaf,
                    Icon::SquareTerminal,
                    IconPlacement::new(
                        content_x,
                        card.y + (card.height - 14.0) * 0.5,
                        14.0,
                    ),
                    icon_color,
                    device_scale,
                );
                draw_text(
                    sugarloaf,
                    content_x + 20.0,
                    card.y + (card.height - 11.0) * 0.5,
                    &session.title,
                    11.0,
                    text_color,
                    active,
                );
                if session.closable && hovered {
                    if let Some(close) = chrome.panel.session_close_rect(origin_y, index) {
                        draw_icon(
                            sugarloaf,
                            Icon::X,
                            IconPlacement::new(close.x + 3.0, close.y + 3.0, 14.0),
                            as_f32(theme.text_muted),
                            device_scale,
                        );
                    }
                }
            }
            continue;
        }

        let Some(host) = row_kind.host() else {
            continue;
        };
        let connecting = chrome.panel.is_connecting(&host.id);

        // Selection lives on the session leaf when a tab is open for this host.
        // Painting the host as "selected" with an accent shell + translucent
        // fill was the solid-blue brick on chevron expand.
        let selected = chrome.panel.selected == Some(index)
            && chrome.panel.selected_session.is_none()
            && !matches!(host.status, terminus_ui::HostStatus::Active);
        let hovered = chrome.panel.hover == Some(index);
        let card = chrome.panel.card_rect(origin_y, index);
        let nested = host.nested;
        let dragging_source = chrome.panel.host_drag.as_ref().is_some_and(|d| {
            d.started() && d.host_id == host.id
        });

        // Inside a folder tray: no second floating card — hover/selection wash only.
        // Root hosts keep the floating surface.
        if nested {
            if selected || connecting {
                let soft = if connecting {
                    opaque_over(theme.button_bg, with_alpha(theme.item_selected, 0.85))
                } else {
                    opaque_over(theme.button_bg, theme.item_selected)
                };
                paint_floating_surface(
                    sugarloaf,
                    &card,
                    soft,
                    Some(theme.accent),
                    sidebar::SESSION_RADIUS,
                );
            } else if hovered || dragging_source {
                paint_surface(
                    sugarloaf,
                    &card,
                    if dragging_source {
                        with_alpha(theme.item_hover, 0.55)
                    } else {
                        theme.item_hover
                    },
                    None,
                    sidebar::SESSION_RADIUS,
                    DEPTH_CONTENT + 0.014,
                    ORDER_CONTENT,
                    false,
                );
            }
        } else {
            let bg = if connecting {
                opaque_over(theme.button_bg, with_alpha(theme.item_selected, 0.85))
            } else if selected {
                opaque_over(theme.button_bg, theme.item_selected)
            } else if hovered {
                theme.item_hover
            } else if dragging_source {
                with_alpha(theme.button_bg, 0.55)
            } else {
                theme.button_bg
            };
            let border = if selected || connecting {
                Some(theme.accent)
            } else {
                None
            };
            paint_floating_surface(sugarloaf, &card, bg, border, sidebar::CARD_RADIUS);
        }

        if !labels {
            continue;
        }

        // Keep badges/text while the card is partially scrolled — an empty
        // shell at the viewport edge is worse than a clipped glyph. Still
        // skip when the content sits entirely under the sticky header.
        if bottom - top < 14.0 {
            continue;
        }
        let badge_tile = if nested {
            sidebar::HOST_BADGE_TILE - 2.0
        } else {
            sidebar::HOST_BADGE_TILE
        };
        let leading = chrome.panel.host_leading_inset(index);
        let badge_x = card.x + leading;
        let badge_y = card.y + (sidebar::ITEM_HEIGHT - badge_tile) / 2.0;
        if badge_y + badge_tile * 0.5 < top {
            continue;
        }

        // Soft badge tile — no gray ring; status lives on the luminous dot.
        paint_soft_badge_tile(sugarloaf, badge_x, badge_y, badge_tile, theme);
        if let Some(dot) = host.status.dot_color() {
            paint_status_dot(sugarloaf, badge_x, badge_y, badge_tile, dot);
        }

        let badge_color = if connecting {
            theme.accent
        } else {
            match host.badge {
                sidebar::Badge::Local => theme.accent,
                sidebar::Badge::Wsl | sidebar::Badge::Ssh => as_f32(theme.text_muted),
            }
        };
        let glyph = match host.badge {
            // "This computer" never matches a brand; prefer os_id, then the
            // subtitle which carries the OS label (`nixos@… · WSL`).
            sidebar::Badge::Local => OsGlyph::from_hint(
                host.os_id.as_deref(),
                host.os_id
                    .as_deref()
                    .filter(|id| !id.is_empty())
                    .unwrap_or(host.endpoint.as_str()),
            ),
            _ => OsGlyph::from_hint(host.os_id.as_deref(), &host.name),
        };
        let icon_x = badge_x + (badge_tile - sidebar::ICON_SIZE) / 2.0;
        let icon_y = badge_y + (badge_tile - sidebar::ICON_SIZE) / 2.0;
        let placement = IconPlacement::new(icon_x, icon_y, sidebar::ICON_SIZE);
        if glyph.has_mark() {
            draw_os_glyph(sugarloaf, glyph, placement, glyph.color(), device_scale);
        } else {
            draw_icon(
                sugarloaf,
                host.badge.icon(),
                placement,
                badge_color,
                device_scale,
            );
        }

        // One trailing affordance: connecting > hover + > disclosure.
        if connecting {
            // Orbit painted below.
        } else if hovered {
            if let Some(add) = chrome.panel.host_add_session_rect(origin_y, index) {
                draw_icon(
                    sugarloaf,
                    Icon::Plus,
                    IconPlacement::new(add.x + 2.0, add.y + 2.0, 16.0),
                    theme.accent,
                    device_scale,
                );
            }
            // Keep expand chevron visible beside + when sessions exist.
            if host.session_count > 0 {
                if let Some(ch) = chrome.panel.host_chevron_rect(origin_y, index) {
                    let collapsed = chrome.panel.collapsed_hosts.contains(&host.id);
                    let chevron = if collapsed {
                        Icon::ChevronRight
                    } else {
                        Icon::ChevronDown
                    };
                    draw_icon(
                        sugarloaf,
                        chevron,
                        IconPlacement::new(ch.x + 1.0, ch.y + 1.0, 16.0),
                        as_f32(theme.text_faint),
                        device_scale,
                    );
                }
            }
        } else if host.session_count > 0 {
            if let Some(ch) = chrome.panel.host_chevron_rect(origin_y, index) {
                let collapsed = chrome.panel.collapsed_hosts.contains(&host.id);
                let chevron = if collapsed {
                    Icon::ChevronRight
                } else {
                    Icon::ChevronDown
                };
                draw_icon(
                    sugarloaf,
                    chevron,
                    IconPlacement::new(ch.x + 1.0, ch.y + 1.0, 16.0),
                    as_f32(theme.text_faint),
                    device_scale,
                );
            }
        } else {
            let icon_sz = sidebar::ICON_SIZE;
            let icon_x = card.right() - sidebar::CARD_PAD - icon_sz;
            let icon_y = card.y + (sidebar::ITEM_HEIGHT - icon_sz) / 2.0;
            draw_icon(
                sugarloaf,
                Icon::ChevronRight,
                IconPlacement::new(icon_x, icon_y, icon_sz),
                as_f32(theme.text_faint),
                device_scale,
            );
        }

        let text_x = badge_x + badge_tile + sidebar::ICON_GAP;
        let trailing = if connecting {
            sidebar::CARD_PAD + sidebar::CONNECTING_SLOT + 6.0
        } else if host.session_count > 0 {
            sidebar::CARD_PAD + sidebar::CHEVRON_HIT + sidebar::HOST_ADD_HIT + 8.0
        } else {
            sidebar::CARD_PAD + sidebar::HOST_ADD_HIT + 8.0
        };
        let text_width = (card.right() - text_x - trailing).max(0.0);

        let renaming = chrome.panel.is_renaming(&host.id);
        if renaming {
            if let Some(draft) = chrome.panel.rename.as_ref() {
                paint_rename_text(sugarloaf, draft, text_x, card.y + 12.0, theme);
            }
        } else {
            let name = elide(
                sugarloaf,
                &host.name,
                text_width,
                &opts(ROW_TITLE_SIZE, theme.text, true),
            );
            draw_text(
                sugarloaf,
                text_x,
                card.y + 12.0,
                &name,
                ROW_TITLE_SIZE,
                theme.text,
                true,
            );
        }

        // While connecting, the subtitle becomes a status line and a
        // shimmer sweeps under it; otherwise it stays the endpoint.
        if connecting {
            let status = elide(
                sugarloaf,
                "Starting…",
                text_width,
                &opts(ROW_SUB_SIZE, theme.text_muted, false),
            );
            draw_text(
                sugarloaf,
                text_x,
                card.y + 32.0,
                &status,
                ROW_SUB_SIZE,
                theme.text_muted,
                false,
            );
            if let Some(phase) = connecting_phase {
                if let Some((sx, sy, sw)) =
                    chrome.panel.connecting_shimmer_track(origin_y, index)
                {
                    if sw > 8.0 {
                        let bar = shimmer_bar(sx, sy, sw, 2.0, phase);
                        let color = with_alpha(theme.accent, bar.alpha);
                        let track = Rect::new(bar.x, bar.y, bar.width, bar.height);
                        paint_surface(
                            sugarloaf,
                            &track,
                            color,
                            None,
                            1.0,
                            DEPTH_CONTENT + 0.02,
                            ORDER_CONNECTING,
                            false,
                        );
                    }
                }
                if let Some((cx, cy)) = chrome.panel.connecting_center(origin_y, index) {
                    draw_orbit_indicator(sugarloaf, theme, cx, cy, 5.5, 2.0, phase);
                }
            }
        } else {
            // The endpoint is the row's second line, not decoration: at the
            // faint colour it dropped to a 4:1 contrast against the panel and
            // read as smudge at 11px, so it gets the muted tone the header
            // uses.
            let endpoint = elide(
                sugarloaf,
                &host.endpoint,
                text_width,
                &opts(ROW_SUB_SIZE, theme.text_muted, false),
            );
            draw_text(
                sugarloaf,
                text_x,
                card.y + 32.0,
                &endpoint,
                ROW_SUB_SIZE,
                theme.text_muted,
                false,
            );
        }
    }

    render_panel_scrollbar(sugarloaf, chrome, theme, origin_y, height, body);
}

/// Accent insertion bar showing where a dragged host/group will land.
fn paint_host_drag_insertion_bar(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
) {
    let Some(drag) = chrome.panel.host_drag.as_ref() else {
        return;
    };
    if !drag.started() {
        return;
    }
    let Some(target) = drag.drop_target.as_ref() else {
        return;
    };
    let origin_y = chrome.origin_y();
    let Some(bar) = chrome.panel.insertion_bar_rect(origin_y, target) else {
        return;
    };
    if bar.width < 4.0 || bar.height < 1.0 {
        return;
    }
    paint_surface(
        sugarloaf,
        &bar,
        theme.accent,
        None,
        2.0,
        DEPTH_GHOST - 0.02,
        ORDER_GHOST,
        false,
    );
    // Soft glow wash under the bar for readability on dark cards.
    let wash = Rect::new(
        bar.x,
        bar.y - 3.0,
        bar.width,
        bar.height + 6.0,
    );
    paint_surface(
        sugarloaf,
        &wash,
        with_alpha(theme.accent, 0.22),
        None,
        4.0,
        DEPTH_GHOST - 0.03,
        ORDER_GHOST,
        false,
    );
}

/// Floating phantom of the dragged host — full card chrome, 50% opacity,
/// painted at max order so it sits above every other chrome layer.
fn paint_host_drag_ghost(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    device_scale: f32,
) {
    let Some(drag) = chrome.panel.host_drag.as_ref() else {
        return;
    };
    if !drag.ghost_visible() {
        return;
    }
    let card = drag.ghost_rect;
    if card.width < 8.0 || card.height < 8.0 {
        return;
    }
    const OPACITY: f32 = 0.5;
    let fill = with_alpha(theme.button_bg, OPACITY);
    let border = with_alpha(theme.accent, OPACITY);
    // Full solid card (background + accent ring), not a soft wash.
    paint_surface(
        sugarloaf,
        &card,
        fill,
        Some(border),
        sidebar::CARD_RADIUS,
        DEPTH_GHOST,
        ORDER_GHOST,
        false,
    );

    let badge_tile = sidebar::HOST_BADGE_TILE;
    let badge = Rect::new(
        card.x + sidebar::CARD_PAD,
        card.y + (card.height - badge_tile) * 0.5,
        badge_tile,
        badge_tile,
    );
    paint_surface(
        sugarloaf,
        &badge,
        with_alpha(theme.field_bg, OPACITY),
        None,
        8.0,
        DEPTH_GHOST + 0.02,
        ORDER_GHOST,
        false,
    );
    draw_icon(
        sugarloaf,
        if drag.is_group() {
            Icon::Folder
        } else {
            Icon::Server
        },
        IconPlacement::new(
            badge.x + (badge_tile - sidebar::ICON_SIZE) * 0.5,
            badge.y + (badge_tile - sidebar::ICON_SIZE) * 0.5,
            sidebar::ICON_SIZE,
        ),
        with_alpha(theme.accent, OPACITY),
        device_scale,
    );
    let text_x = badge.right() + sidebar::ICON_GAP;
    let mut title = theme.text;
    title[3] = (255.0 * OPACITY).round() as u8;
    let mut sub = theme.text_muted;
    sub[3] = (255.0 * OPACITY).round() as u8;
    draw_text(
        sugarloaf,
        text_x,
        card.y + 12.0,
        &drag.host_name,
        ROW_TITLE_SIZE,
        title,
        true,
    );
    draw_text(
        sugarloaf,
        text_x,
        card.y + 32.0,
        &drag.endpoint,
        ROW_SUB_SIZE,
        sub,
        false,
    );
}

/// How a dialog's 1px border is placed relative to the content rect.
enum DialogBorderMode {
    /// Stroke occupies `dialog`; fill is inset (settings modal).
    Inset,
    /// Stroke expands outside `dialog`; fill is the content rect (add-host / connection).
    Outward,
}

/// Full-window scrim + bordered dialog panel.
fn paint_dialog_shell(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    window_width: f32,
    window_height: f32,
    dialog: &Rect,
    radius: f32,
    mode: DialogBorderMode,
    scrim_depth: f32,
    panel_depth: f32,
    order: u8,
    scrim_alpha_clamp: Option<(f32, f32)>,
) {
    let mut scrim = theme.scrim;
    if let Some((lo, hi)) = scrim_alpha_clamp {
        scrim[3] = scrim[3].max(lo).min(hi);
    }
    paint_scrim(
        sugarloaf,
        window_width,
        window_height,
        scrim,
        scrim_depth,
        order,
    );
    let shell = match mode {
        DialogBorderMode::Inset => *dialog,
        DialogBorderMode::Outward => Rect::new(
            dialog.x - BORDER_WIDTH,
            dialog.y - BORDER_WIDTH,
            dialog.width + 2.0 * BORDER_WIDTH,
            dialog.height + 2.0 * BORDER_WIDTH,
        ),
    };
    paint_surface(
        sugarloaf,
        &shell,
        theme.dialog_bg,
        Some(theme.dialog_border),
        radius,
        panel_depth,
        order,
        false,
    );
}

/// Borderless floating card: soft fill, optional thin luminous ring.
///
/// When a border is requested the outer shell is accent-colored; the fill
/// must be **opaque**. A translucent `accent_soft` over that shell reads as
/// a solid blue brick (the bug on selected hosts / sessions).
fn paint_floating_surface(
    sugarloaf: &mut Sugarloaf,
    card: &Rect,
    bg: [f32; 4],
    border: Option<[f32; 4]>,
    radius: f32,
) {
    paint_surface(
        sugarloaf,
        card,
        bg,
        border,
        radius,
        DEPTH_CONTENT,
        ORDER_CONTENT,
        true,
    );
}

/// Shared bordered/plain rounded surface used by cards, chips, menus, badges.
///
/// `composite_alpha` matches [`paint_floating_surface`]: translucent fills are
/// composited over a dark card base so washes stay soft.
fn paint_surface(
    sugarloaf: &mut Sugarloaf,
    card: &Rect,
    bg: [f32; 4],
    border: Option<[f32; 4]>,
    radius: f32,
    depth: f32,
    order: u8,
    composite_alpha: bool,
) {
    paint_surface_stroke(
        sugarloaf,
        card,
        bg,
        border,
        radius,
        BORDER_WIDTH,
        depth,
        order,
        composite_alpha,
    );
}

/// Like [`paint_surface`], with an explicit border stroke width.
pub(crate) fn paint_surface_stroke(
    sugarloaf: &mut Sugarloaf,
    card: &Rect,
    bg: [f32; 4],
    border: Option<[f32; 4]>,
    radius: f32,
    stroke: f32,
    depth: f32,
    order: u8,
    composite_alpha: bool,
) {
    let fill = if composite_alpha && bg[3] < 0.999 {
        opaque_over([0.133, 0.133, 0.149, 1.0], bg) // ≈ button_bg
    } else {
        bg
    };
    if let Some(border_color) = border {
        sugarloaf.rounded_rect(
            None,
            card.x,
            card.y,
            card.width,
            card.height,
            border_color,
            depth,
            radius,
            order,
        );
        sugarloaf.rounded_rect(
            None,
            card.x + stroke,
            card.y + stroke,
            (card.width - 2.0 * stroke).max(0.0),
            (card.height - 2.0 * stroke).max(0.0),
            fill,
            depth + 0.01,
            (radius - stroke).max(0.0),
            order,
        );
    } else {
        sugarloaf.rounded_rect(
            None,
            card.x,
            card.y,
            card.width,
            card.height,
            fill,
            depth,
            radius,
            order,
        );
    }
}

/// Axis-aligned opaque fill (panels, rails, scrims, footers, notice bands).
pub(crate) fn paint_flat(
    sugarloaf: &mut Sugarloaf,
    rect: &Rect,
    color: [f32; 4],
    depth: f32,
    order: u8,
) {
    sugarloaf.rect(
        None,
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        color,
        depth,
        order,
    );
}

/// 1px horizontal separator.
pub(crate) fn paint_hairline_h(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    y: f32,
    width: f32,
    color: [f32; 4],
    depth: f32,
    order: u8,
) {
    paint_flat(
        sugarloaf,
        &Rect::new(x, y, width, BORDER_WIDTH),
        color,
        depth,
        order,
    );
}

/// 1px vertical separator.
pub(crate) fn paint_hairline_v(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    y: f32,
    height: f32,
    color: [f32; 4],
    depth: f32,
    order: u8,
) {
    paint_flat(
        sugarloaf,
        &Rect::new(x, y, BORDER_WIDTH, height),
        color,
        depth,
        order,
    );
}

/// Blinking text caret used by search, palette, settings, and rename fields.
pub(crate) fn paint_caret(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    y: f32,
    height: f32,
    color: [f32; 4],
    depth: f32,
    order: u8,
) {
    paint_flat(
        sugarloaf,
        &Rect::new(x, y, CARET_WIDTH, height),
        color,
        depth,
        order,
    );
}

/// Shared labeled text-field card (Settings SqlSync, SFTP name, …).
pub(crate) fn paint_field_card(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    card: Rect,
    label: &str,
    value: &str,
    focused: bool,
    placeholder: bool,
    trailing_slot: f32,
    paint_text: bool,
) {
    paint_field_card_at(
        sugarloaf,
        theme,
        card,
        label,
        value,
        focused,
        placeholder,
        trailing_slot,
        paint_text,
        DEPTH_DIALOG,
        ORDER_DIALOG,
    );
}

/// Field card with explicit depth/order (SFTP toolbar vs dialog chrome).
pub(crate) fn paint_field_card_at(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    card: Rect,
    label: &str,
    value: &str,
    focused: bool,
    placeholder: bool,
    trailing_slot: f32,
    paint_text: bool,
    depth: f32,
    order: u8,
) {
    paint_surface(
        sugarloaf,
        &card,
        theme.button_bg,
        None,
        12.0,
        depth + 0.03,
        order,
        false,
    );
    if paint_text {
        draw_text(
            sugarloaf,
            card.x + terminus_ui::settings::FIELD_CARD_PAD,
            card.y + terminus_ui::settings::FIELD_CARD_PAD,
            label,
            HINT_SIZE,
            theme.text_muted,
            false,
        );
    }
    let field_bg = theme.field_bg;
    let input = terminus_ui::settings::field_input_in_card(card);
    if focused {
        paint_surface(
            sugarloaf,
            &input,
            field_bg,
            Some(theme.field_border_focus),
            8.0,
            depth + 0.039,
            order,
            false,
        );
    } else {
        paint_surface(
            sugarloaf,
            &input,
            field_bg,
            None,
            8.0,
            depth + 0.04,
            order,
            false,
        );
    }
    if paint_text {
        let color = if placeholder {
            theme.text_placeholder
        } else {
            theme.text
        };
        let text_budget = (input.width
            - terminus_ui::settings::FIELD_TEXT_INSET
            - trailing_slot.max(terminus_ui::settings::FIELD_TEXT_INSET))
        .max(0.0);
        let shown = elide(
            sugarloaf,
            value,
            text_budget,
            &opts(ROW_SUB_SIZE, color, false),
        );
        let text_y = input.y + (input.height - ROW_SUB_SIZE) * 0.5;
        draw_text(
            sugarloaf,
            input.x + terminus_ui::settings::FIELD_TEXT_INSET,
            text_y,
            &shown,
            ROW_SUB_SIZE,
            color,
            false,
        );
    }
}

/// Caret inside a field card, positioned after `prefix` (not always end-of-value).
pub(crate) fn paint_field_caret_prefix(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    card: Rect,
    prefix: &str,
    trailing_slot: f32,
) {
    paint_field_caret_prefix_at(
        sugarloaf,
        theme,
        card,
        prefix,
        trailing_slot,
        DEPTH_DIALOG,
        ORDER_DIALOG,
    );
}

pub(crate) fn paint_field_caret_prefix_at(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    card: Rect,
    prefix: &str,
    trailing_slot: f32,
    depth: f32,
    order: u8,
) {
    let input = terminus_ui::settings::field_input_in_card(card);
    let text_x = input.x + terminus_ui::settings::FIELD_TEXT_INSET;
    let text_budget = (input.width
        - terminus_ui::settings::FIELD_TEXT_INSET
        - trailing_slot.max(terminus_ui::settings::FIELD_TEXT_INSET))
    .max(0.0);
    let shown = elide(
        sugarloaf,
        prefix,
        text_budget,
        &opts(ROW_SUB_SIZE, theme.text, false),
    );
    let advance = sugarloaf
        .text_mut()
        .measure(&shown, &opts(ROW_SUB_SIZE, theme.text, false));
    let caret_x = (text_x + advance).min(text_x + text_budget);
    let caret_y = input.y + (input.height - 16.0) * 0.5;
    paint_caret(
        sugarloaf,
        caret_x,
        caret_y,
        16.0,
        theme.accent,
        depth + 0.05,
        order,
    );
}

/// Inline rename name + optional selection wash + caret.
fn paint_rename_text(
    sugarloaf: &mut Sugarloaf,
    draft: &sidebar::RenameDraft,
    text_x: f32,
    text_y: f32,
    theme: &ChromeTheme,
) {
    let accent = color_from_f32(theme.accent);
    let title_opts = opts(ROW_TITLE_SIZE, accent, true);
    if let Some((start, end)) = draft.selection_range() {
        let before: String = draft.name.chars().take(start).collect();
        let selected: String = draft.name.chars().skip(start).take(end - start).collect();
        let bx = sugarloaf.text_mut().measure(&before, &title_opts);
        let sw = sugarloaf
            .text_mut()
            .measure(&selected, &title_opts)
            .max(2.0);
        paint_flat(
            sugarloaf,
            &Rect::new(text_x + bx, text_y - 1.0, sw, ROW_TITLE_SIZE + 2.0),
            with_alpha(theme.accent, 0.35),
            DEPTH_CONTENT + 0.02,
            ORDER_CONTENT,
        );
    }
    draw_text(
        sugarloaf,
        text_x,
        text_y,
        &draft.name,
        ROW_TITLE_SIZE,
        accent,
        true,
    );
    let prefix = draft.prefix();
    let w = sugarloaf.text_mut().measure(&prefix, &title_opts);
    paint_caret(
        sugarloaf,
        text_x + w,
        text_y,
        ROW_TITLE_SIZE,
        theme.accent,
        DEPTH_CONTENT + 0.03,
        ORDER_CONTENT,
    );
}

/// Full-window modal/overlay scrim.
pub(crate) fn paint_scrim(
    sugarloaf: &mut Sugarloaf,
    window_width: f32,
    window_height: f32,
    color: [f32; 4],
    depth: f32,
    order: u8,
) {
    paint_flat(
        sugarloaf,
        &Rect::new(0.0, 0.0, window_width, window_height),
        color,
        depth,
        order,
    );
}

/// Straight stroke (e.g. reset-swatch slash).
pub(crate) fn paint_line(
    sugarloaf: &mut Sugarloaf,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    stroke: f32,
    color: [f32; 4],
    depth: f32,
    order: u8,
) {
    sugarloaf.line(x0, y0, x1, y1, stroke, depth, color, order);
}

/// Apple HIG title-bar strip + bottom hairline (island / context bar).
pub(crate) fn paint_title_strip(
    sugarloaf: &mut Sugarloaf,
    logical_w: f32,
    height: f32,
) {
    let strip = [
        0x11 as f32 / 255.0,
        0x11 as f32 / 255.0,
        0x13 as f32 / 255.0,
        1.0,
    ];
    let strip_border = [
        0x2f as f32 / 255.0,
        0x2f as f32 / 255.0,
        0x35 as f32 / 255.0,
        1.0,
    ];
    paint_flat(
        sugarloaf,
        &Rect::new(0.0, 0.0, logical_w, height),
        strip,
        0.04,
        0,
    );
    paint_hairline_h(
        sugarloaf,
        0.0,
        height - 1.0,
        logical_w,
        strip_border,
        0.041,
        0,
    );
}

/// Bordered square icon badge (CTA / folder / key tiles).
fn paint_bordered_badge(
    sugarloaf: &mut Sugarloaf,
    badge: &Rect,
    fill: [f32; 4],
    border: [f32; 4],
    radius: f32,
    depth: f32,
    order: u8,
) {
    paint_surface(
        sugarloaf,
        badge,
        fill,
        Some(border),
        radius,
        depth,
        order,
        false,
    );
}

/// Dashed CTA row: wash fill, dashed stroke, bordered badge, title + subtitle.
fn paint_dashed_cta(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    cta: &Rect,
    radius: f32,
    bg: [f32; 4],
    border: [f32; 4],
    title: &str,
    subtitle: &str,
    title_color: [u8; 4],
    icon: Icon,
    device_scale: f32,
    labels: bool,
    depth: f32,
    order: u8,
) {
    paint_surface(
        sugarloaf,
        cta,
        bg,
        None,
        radius,
        depth,
        order,
        false,
    );
    draw_dashed_rounded_rect(
        sugarloaf,
        cta.x,
        cta.y,
        cta.width,
        cta.height,
        radius,
        border,
        depth + 0.01,
        order,
    );
    let badge = terminus_ui::dashed_cta_badge(*cta, sidebar::BADGE_TILE, sidebar::CARD_PAD);
    paint_bordered_badge(
        sugarloaf,
        &badge,
        theme.field_bg,
        theme.panel_border,
        8.0,
        depth + 0.02,
        order,
    );
    let icon_xy = (
        badge.x + (sidebar::BADGE_TILE - ADD_ICON_SIZE) * 0.5,
        badge.y + (sidebar::BADGE_TILE - ADD_ICON_SIZE) * 0.5,
    );
    draw_icon(
        sugarloaf,
        icon,
        IconPlacement::new(icon_xy.0, icon_xy.1, ADD_ICON_SIZE),
        theme.accent,
        device_scale,
    );
    if labels {
        let text_x = badge.right() + sidebar::ICON_GAP;
        // Same title/subtitle rhythm as host cards and key rows
        // (`+12` / `+32` on a 56px row), scaled when the CTA is taller
        // (New Host is 60). Centers the two-line stack with the badge.
        let base = cta.y + (cta.height - 56.0) * 0.5;
        let title_y = base + 12.0;
        let sub_y = base + 32.0;
        draw_text(
            sugarloaf,
            text_x,
            title_y,
            title,
            ROW_TITLE_SIZE,
            title_color,
            true,
        );
        draw_text(
            sugarloaf,
            text_x,
            sub_y,
            subtitle,
            ROW_SUB_SIZE,
            theme.text_muted,
            false,
        );
    }
}

/// Corner radius of the soft OS/folder badge tile.
const HOST_BADGE_RADIUS: f32 = 8.0;

/// Soft OS/folder badge without a gray outline ring.
fn paint_soft_badge_tile(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    y: f32,
    size: f32,
    theme: &ChromeTheme,
) {
    paint_surface(
        sugarloaf,
        &Rect::new(x, y, size, size),
        theme.field_bg,
        None,
        HOST_BADGE_RADIUS,
        DEPTH_CONTENT + 0.02,
        ORDER_CONTENT,
        false,
    );
}

/// Luminous status pastille at the badge's bottom-right corner.
fn paint_status_dot(
    sugarloaf: &mut Sugarloaf,
    badge_x: f32,
    badge_y: f32,
    badge_size: f32,
    color: [f32; 4],
) {
    let d = sidebar::STATUS_DOT;
    // Center on the *visible* rounded corner (45° on the arc), not the
    // AABB corner — otherwise the fill radius leaves the pastille floating
    // outside the tile.
    let corner_inset = HOST_BADGE_RADIUS * (1.0 - std::f32::consts::FRAC_1_SQRT_2);
    let x = badge_x + badge_size - corner_inset - d * 0.5;
    let y = badge_y + badge_size - corner_inset - d * 0.5;
    // Dark halo so the dot reads on both light badges and brand fills.
    paint_surface(
        sugarloaf,
        &Rect::new(x - 1.0, y - 1.0, d + 2.0, d + 2.0),
        [0.067, 0.067, 0.075, 1.0], // panel_bg
        None,
        (d + 2.0) * 0.5,
        DEPTH_CONTENT + 0.03,
        ORDER_CONTENT,
        false,
    );
    paint_surface(
        sugarloaf,
        &Rect::new(x, y, d, d),
        color,
        None,
        d * 0.5,
        DEPTH_CONTENT + 0.031,
        ORDER_CONTENT,
        false,
    );
}

fn paint_new_group_button(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    origin_y: f32,
    height: f32,
    labels: bool,
    device_scale: f32,
) {
    let button = chrome.panel.new_group_button_rect(origin_y, height);
    if button.width <= 0.0 {
        return;
    }
    if chrome.panel.new_group_hover {
        paint_surface(
            sugarloaf,
            &button,
            with_alpha([1.0, 1.0, 1.0, 1.0], 0.06),
            None,
            6.0,
            DEPTH_CONTENT + 0.02,
            ORDER_CONTENT,
            false,
        );
    }
    if !labels {
        return;
    }
    let color = if chrome.panel.new_group_hover {
        as_f32(theme.text)
    } else {
        as_f32(theme.text_muted)
    };
    draw_icon(
        sugarloaf,
        Icon::Plus,
        IconPlacement::new(button.x + 4.0, button.y + 3.0, 12.0),
        color,
        device_scale,
    );
    draw_text(
        sugarloaf,
        button.x + 18.0,
        button.y + 4.0,
        "New group",
        11.0,
        color_from_f32(color),
        false,
    );
}

fn paint_new_group_form(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    origin_y: f32,
    labels: bool,
    body: &Rect,
) {
    let Some(form) = chrome.panel.new_group_form_rect(origin_y) else {
        return;
    };
    let Some((top, bottom)) = form.clip_rows(body.y, body.bottom()) else {
        return;
    };
    if bottom - top < 8.0 {
        return;
    }
    paint_surface(
        sugarloaf,
        &form,
        with_alpha([1.0, 1.0, 1.0, 1.0], 0.025),
        Some(theme.panel_border),
        10.0,
        DEPTH_CONTENT + 0.02,
        ORDER_CONTENT,
        false,
    );

    let Some(field) = chrome.panel.new_group_field_rect(origin_y) else {
        return;
    };
    let Some(create) = chrome.panel.new_group_create_rect(origin_y) else {
        return;
    };
    let Some(cancel) = chrome.panel.new_group_cancel_rect(origin_y) else {
        return;
    };

    if chrome.panel.new_group_focused {
        paint_surface(
            sugarloaf,
            &field,
            theme.field_bg,
            Some(theme.field_border_focus),
            8.0,
            DEPTH_CONTENT + 0.03,
            ORDER_CONTENT,
            false,
        );
    } else {
        paint_surface(
            sugarloaf,
            &field,
            theme.field_bg,
            None,
            8.0,
            DEPTH_CONTENT + 0.03,
            ORDER_CONTENT,
            false,
        );
    }

    paint_chrome_button(
        sugarloaf,
        theme,
        terminus_ui::ButtonSpec::primary(create),
        if labels { "Create" } else { "" },
        11.0,
        DEPTH_CONTENT + 0.03,
        ORDER_CONTENT,
    );

    if !labels {
        return;
    }
    let placeholder = chrome.panel.new_group_name.is_empty();
    let text = if placeholder {
        "Group name…"
    } else {
        chrome.panel.new_group_name.as_str()
    };
    let color = if placeholder {
        theme.text_placeholder
    } else {
        theme.text
    };
    draw_text(
        sugarloaf,
        field.x + 8.0,
        field.y + (field.height - ROW_SUB_SIZE) * 0.5,
        text,
        ROW_SUB_SIZE,
        color,
        false,
    );
    draw_text(
        sugarloaf,
        cancel.x + 8.0,
        cancel.y + (cancel.height - ROW_SUB_SIZE) * 0.5,
        "Cancel",
        ROW_SUB_SIZE,
        theme.text_muted,
        false,
    );
}

fn render_panel_scrollbar(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    origin_y: f32,
    height: f32,
    body: Rect,
) {
    let content = chrome.panel.content_height();
    if content <= body.height || body.height <= 0.0 {
        return;
    }
    let max_scroll = chrome.panel.max_scroll(origin_y, height);
    if max_scroll <= 0.0 {
        return;
    }

    const TRACK: f32 = 3.0;
    let thumb_height = (body.height * (body.height / content)).max(24.0);
    let travel = body.height - thumb_height;
    let progress = (chrome.panel.scroll / max_scroll).clamp(0.0, 1.0);
    paint_flat(
        sugarloaf,
        &Rect::new(
            body.right() - TRACK - 2.0,
            body.y + travel * progress,
            TRACK,
            thumb_height,
        ),
        theme.panel_border,
        DEPTH_CONTENT + 0.01,
        ORDER_CONTENT,
    );
}

fn render_footer(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    origin_y: f32,
    height: f32,
    labels: bool,
    device_scale: f32,
) {
    let footer = chrome.panel.footer_rect(origin_y, height);
    paint_hairline_h(
        sugarloaf,
        footer.x,
        footer.y,
        footer.width,
        theme.panel_border,
        DEPTH_CONTENT,
        ORDER_CONTENT,
    );

    render_footer_button(
        sugarloaf,
        chrome.panel.add_button_rect(origin_y, height),
        chrome.panel.add_hover,
        terminus_ui::icons::Icon::Plus,
        "Add host",
        theme,
        labels,
        device_scale,
    );
    render_footer_button(
        sugarloaf,
        chrome.panel.new_group_button_rect(origin_y, height),
        chrome.panel.new_group_hover,
        terminus_ui::icons::Icon::Folder,
        "New group",
        theme,
        labels,
        device_scale,
    );
}

fn render_footer_button(
    sugarloaf: &mut Sugarloaf,
    button: Rect,
    hovered: bool,
    icon: Icon,
    label: &str,
    theme: &ChromeTheme,
    labels: bool,
    device_scale: f32,
) {
    if button.height <= 0.0 {
        return;
    }
    paint_surface(
        sugarloaf,
        &button,
        if hovered {
            theme.item_hover
        } else {
            theme.button_bg
        },
        None,
        5.0,
        DEPTH_CONTENT,
        ORDER_CONTENT,
        false,
    );

    const LABEL_GAP: f32 = 8.0;
    let label_width = if labels {
        sugarloaf
            .text_mut()
            .measure(label, &opts(ADD_LABEL_SIZE, theme.text, false))
    } else {
        0.0
    };
    let group_width = if labels {
        ADD_ICON_SIZE + LABEL_GAP + label_width
    } else {
        ADD_ICON_SIZE
    };
    let start_x = (button.x + (button.width - group_width) / 2.0).round();
    let icon_y = (button.y + (button.height - ADD_ICON_SIZE) / 2.0).round();
    draw_icon(
        sugarloaf,
        icon,
        IconPlacement::new(start_x, icon_y, ADD_ICON_SIZE),
        theme.accent,
        device_scale,
    );

    if labels {
        let text_y = (button.y + (button.height - ADD_LABEL_SIZE) / 2.0 - 1.0).round();
        draw_text(
            sugarloaf,
            start_x + ADD_ICON_SIZE + LABEL_GAP,
            text_y,
            label,
            ADD_LABEL_SIZE,
            theme.text,
            false,
        );
    }
}

fn render_add_host(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    window_width: f32,
    window_height: f32,
    device_scale: f32,
    paint_glyphs: bool,
) {
    let form = &chrome.form;
    let layout = chrome.dialog_layout(window_width, window_height);
    let dialog = layout.rect(form.height());

    let radius = terminus_ui::add_host::DIALOG_RADIUS;
    paint_dialog_shell(
        sugarloaf,
        theme,
        window_width,
        window_height,
        &dialog,
        radius,
        DialogBorderMode::Outward,
        DEPTH_DIALOG,
        DEPTH_DIALOG_BG,
        ORDER_DIALOG,
        None,
    );
    if !paint_glyphs {
        // Field shells only — keeps a dim silhouette under a higher modal.
        let input_radius = terminus_ui::add_host::INPUT_RADIUS;
        for field in form.visible_fields() {
            let Some(input) = layout.input_rect(form, field) else {
                continue;
            };
            let shell = Rect::new(
                input.x - BORDER_WIDTH,
                input.y - BORDER_WIDTH,
                input.width + 2.0 * BORDER_WIDTH,
                input.height + 2.0 * BORDER_WIDTH,
            );
            paint_surface(
                sugarloaf,
                &shell,
                theme.button_bg,
                Some(theme.field_border),
                input_radius,
                DEPTH_DIALOG_BG + 0.015,
                ORDER_DIALOG,
                false,
            );
        }
        return;
    }

    let title = layout.title_rect();
    let title_text = if form.is_editing() {
        "Edit Remote Host"
    } else {
        "Configure New Remote Host"
    };
    draw_text(
        sugarloaf,
        title.x,
        title.y + 4.0,
        title_text,
        DIALOG_TITLE_SIZE,
        theme.text,
        true,
    );

    let input_radius = terminus_ui::add_host::INPUT_RADIUS;
    for field in form.visible_fields() {
        let Some(input) = layout.input_rect(form, field) else {
            continue;
        };
        let focused = form.focused_field() == field;

        // Mock inputs: bg-appleCard (#222226) + border (outward stroke).
        let shell = Rect::new(
            input.x - BORDER_WIDTH,
            input.y - BORDER_WIDTH,
            input.width + 2.0 * BORDER_WIDTH,
            input.height + 2.0 * BORDER_WIDTH,
        );
        paint_surface(
            sugarloaf,
            &shell,
            theme.button_bg,
            Some(if focused {
                theme.field_border_focus
            } else {
                theme.field_border
            }),
            input_radius,
            DEPTH_DIALOG_BG + 0.015,
            ORDER_DIALOG,
            false,
        );

        if let Some(caption) = layout.caption_rect(form, field) {
            let skip_caption = (form.auth_menu_open()
                && matches!(field, Field::Identity | Field::Password))
                || (form.identity_menu_open() && matches!(field, Field::Password));
            if !skip_caption {
                draw_text(
                    sugarloaf,
                    caption.x,
                    caption.y + 2.0,
                    field.label(),
                    CAPTION_SIZE,
                    [0xcb, 0xd5, 0xe1, 255], // slate-300
                    false,
                );
            }
        }

        let text_y = input.y + (input.height - INPUT_SIZE) / 2.0;
        let text_x = input.x + INPUT_PAD_X;

        match field {
            Field::AuthMethod => {
                draw_text(
                    sugarloaf,
                    text_x,
                    text_y,
                    auth_method_label(form.auth_method()),
                    INPUT_SIZE,
                    theme.text,
                    false,
                );
                draw_text(
                    sugarloaf,
                    input.right() - 18.0,
                    text_y,
                    if form.auth_menu_open() {
                        "▴"
                    } else {
                        "▾"
                    },
                    INPUT_SIZE,
                    theme.text_muted,
                    false,
                );
            }
            Field::Identity => {
                // Skip glyph paint while the auth menu covers this row —
                // sugarloaf UI text always composites above quads.
                if form.auth_menu_open() {
                    continue;
                }
                let label = form
                    .selected_identity_name()
                    .unwrap_or_else(|| field.placeholder());
                let color = if form.selected_identity_name().is_some() {
                    theme.text
                } else {
                    theme.text_placeholder
                };
                draw_text(sugarloaf, text_x, text_y, label, INPUT_SIZE, color, false);
                draw_text(
                    sugarloaf,
                    input.right() - 18.0,
                    text_y,
                    if form.identity_menu_open() {
                        "▴"
                    } else {
                        "▾"
                    },
                    INPUT_SIZE,
                    theme.text_muted,
                    false,
                );
            }
            Field::Password => {
                if form.auth_menu_open() || form.identity_menu_open() {
                    continue;
                }
                let shown = if form.password_visible() {
                    form.password().to_string()
                } else {
                    "•".repeat(form.password().chars().count())
                };
                let text_budget = layout
                    .password_text_rect(form)
                    .map(|r| (r.width - 16.0).max(0.0))
                    .unwrap_or(input.width - 16.0);
                if shown.is_empty() && !focused {
                    let placeholder = if form.is_editing() {
                        "Leave blank to keep current"
                    } else {
                        field.placeholder()
                    };
                    draw_text(
                        sugarloaf,
                        text_x,
                        text_y,
                        placeholder,
                        INPUT_SIZE,
                        theme.text_placeholder,
                        false,
                    );
                } else {
                    let caret = form.cursor(Field::Password);
                    let before: String = if form.password_visible() {
                        form.password().chars().take(caret).collect()
                    } else {
                        "•".repeat(caret)
                    };
                    let after_len = shown.chars().count().saturating_sub(caret);
                    let after: String = if form.password_visible() {
                        form.password().chars().skip(caret).collect()
                    } else {
                        "•".repeat(after_len)
                    };
                    let _ = text_budget;
                    let drawn = sugarloaf.text_mut().draw(
                        text_x,
                        text_y,
                        &before,
                        &opts(INPUT_SIZE, theme.text, false),
                    );
                    if focused {
                        paint_caret(
                            sugarloaf,
                            text_x + drawn,
                            input.y + 6.0,
                            input.height - 12.0,
                            theme.accent,
                            DEPTH_DIALOG_BG + 0.03,
                            ORDER_DIALOG,
                        );
                    }
                    if !after.is_empty() {
                        sugarloaf.text_mut().draw(
                            text_x + drawn,
                            text_y,
                            &after,
                            &opts(INPUT_SIZE, theme.text, false),
                        );
                    }
                }
                if let Some(eye) = layout.password_toggle_rect(form) {
                    let eye_icon = if form.password_visible() {
                        Icon::EyeOff
                    } else {
                        Icon::Eye
                    };
                    let eye_size = terminus_ui::settings::FIELD_EYE_ICON;
                    draw_icon(
                        sugarloaf,
                        eye_icon,
                        IconPlacement::new(
                            eye.x + (eye.width - eye_size) * 0.5,
                            eye.y + (eye.height - eye_size) * 0.5,
                            eye_size,
                        ),
                        as_f32(theme.text_muted),
                        device_scale,
                    );
                }
            }
            _ => {
                if form.value(field).is_empty() && !focused {
                    draw_text(
                        sugarloaf,
                        text_x,
                        text_y,
                        field.placeholder(),
                        INPUT_SIZE,
                        theme.text_placeholder,
                        false,
                    );
                    continue;
                }

                let (before, after) = if focused {
                    form.split_at_cursor()
                } else {
                    (form.value(field), "")
                };
                let drawn = sugarloaf.text_mut().draw(
                    text_x,
                    text_y,
                    before,
                    &opts(INPUT_SIZE, theme.text, false),
                );
                if focused {
                    // Caret immediately after the text before it, so it lands
                    // on the true advance width rather than an estimate.
                    paint_caret(
                        sugarloaf,
                        text_x + drawn,
                        input.y + 6.0,
                        input.height - 12.0,
                        theme.accent,
                        DEPTH_DIALOG_BG + 0.03,
                        ORDER_DIALOG,
                    );
                }
                if !after.is_empty() {
                    sugarloaf.text_mut().draw(
                        text_x + drawn,
                        text_y,
                        after,
                        &opts(INPUT_SIZE, theme.text, false),
                    );
                }
            }
        }
    }

    let hint = layout.hint_rect(form.height());
    // Hairline above the footer actions.
    paint_hairline_h(
        sugarloaf,
        dialog.x + terminus_ui::add_host::PAD,
        hint.y - 8.0,
        dialog.width - 2.0 * terminus_ui::add_host::PAD,
        theme.panel_border,
        DEPTH_DIALOG_BG + 0.02,
        ORDER_DIALOG,
    );

    if let Some(error) = form.error() {
        let text = elide(
            sugarloaf,
            error,
            hint.width - 180.0,
            &opts(HINT_SIZE, theme.danger, false),
        );
        draw_text(
            sugarloaf,
            hint.x,
            hint.y + 12.0,
            &text,
            HINT_SIZE,
            theme.danger,
            false,
        );
    }

    // Cancel + Connect footer — shared ButtonSpec paint path.
    // Skip button *labels* while a dropdown covers them: sugarloaf's
    // UI text pass always composites after every quad, so Cancel/Connect
    // glyphs would otherwise float on top of the opaque popover (same
    // pattern as URI text under the SQL engine dropdown).
    let cancel = layout.cancel_button_rect(form.height());
    let connect = layout.connect_button_rect(form.height());
    let auth_menu = form
        .auth_menu_open()
        .then(|| layout.auth_menu_rect(form))
        .flatten();
    let identity_menu = form
        .identity_menu_open()
        .then(|| layout.identity_menu_rect(form))
        .flatten();
    let menu_covers = |btn: Rect| {
        auth_menu.is_some_and(|m| terminus_ui::rects_overlap(m, btn))
            || identity_menu.is_some_and(|m| terminus_ui::rects_overlap(m, btn))
    };
    let cancel_label = if menu_covers(cancel) {
        ""
    } else {
        "Cancel"
    };
    let connect_label = if menu_covers(connect) {
        ""
    } else if form.is_editing() {
        "Save"
    } else {
        "Connect"
    };
    paint_chrome_button(
        sugarloaf,
        theme,
        terminus_ui::ButtonSpec::secondary(cancel)
            .with_radius(input_radius)
            .muted(),
        cancel_label,
        HINT_SIZE,
        DEPTH_DIALOG_BG + 0.025,
        ORDER_DIALOG,
    );
    paint_chrome_button(
        sugarloaf,
        theme,
        terminus_ui::ButtonSpec::primary(connect).with_radius(input_radius),
        connect_label,
        HINT_SIZE,
        DEPTH_DIALOG_BG + 0.025,
        ORDER_DIALOG,
    );

    // Auth Method dropdown: higher paint order than dialog field text.
    if form.auth_menu_open() {
        if let Some(menu) = layout.auth_menu_rect(form) {
            paint_surface(
                sugarloaf,
                &menu,
                theme.button_bg,
                Some(theme.panel_border),
                10.0,
                DEPTH_DIALOG,
                ORDER_DIALOG_POPOVER,
                false,
            );
            for i in 0..terminus_ui::AUTH_METHODS.len() {
                let Some(opt) = layout.auth_option_rect(form, i) else {
                    continue;
                };
                let selected = form.auth_method() == terminus_ui::AUTH_METHODS[i];
                let hovered = form.auth_menu_hover() == Some(i);
                if selected || hovered {
                    paint_surface(
                        sugarloaf,
                        &opt,
                        if hovered {
                            theme.item_hover
                        } else {
                            theme.accent_soft
                        },
                        None,
                        6.0,
                        DEPTH_DIALOG + 0.002,
                        ORDER_DIALOG_POPOVER,
                        false,
                    );
                }
                draw_text(
                    sugarloaf,
                    opt.x + 12.0,
                    opt.y + 8.0,
                    auth_method_label(terminus_ui::AUTH_METHODS[i]),
                    ROW_SUB_SIZE,
                    if selected {
                        color_from_f32(theme.accent)
                    } else {
                        theme.text
                    },
                    selected,
                );
                if selected {
                    draw_text(
                        sugarloaf,
                        opt.right() - 22.0,
                        opt.y + 8.0,
                        "✓",
                        ROW_SUB_SIZE,
                        color_from_f32(theme.accent),
                        true,
                    );
                }
            }
        }
    }

    // Saved SSH key dropdown — same popover pattern as auth method.
    if form.identity_menu_open() {
        if let Some(menu) = layout.identity_menu_rect(form) {
            paint_surface(
                sugarloaf,
                &menu,
                theme.button_bg,
                Some(theme.panel_border),
                10.0,
                DEPTH_DIALOG,
                ORDER_DIALOG_POPOVER,
                false,
            );
            if form.identities().is_empty() {
                draw_text(
                    sugarloaf,
                    menu.x + 12.0,
                    menu.y + 12.0,
                    "No saved keys",
                    ROW_SUB_SIZE,
                    theme.text_muted,
                    false,
                );
            }
            for (i, (_id, name)) in form.identities().iter().enumerate() {
                let Some(opt) = layout.identity_option_rect(form, i) else {
                    continue;
                };
                let selected = form.identity_id() == Some(_id.as_str());
                let hovered = form.identity_menu_hover() == Some(i);
                if selected || hovered {
                    paint_surface(
                        sugarloaf,
                        &opt,
                        if hovered {
                            theme.item_hover
                        } else {
                            theme.accent_soft
                        },
                        None,
                        6.0,
                        DEPTH_DIALOG + 0.002,
                        ORDER_DIALOG_POPOVER,
                        false,
                    );
                }
                draw_text(
                    sugarloaf,
                    opt.x + 12.0,
                    opt.y + 8.0,
                    name,
                    ROW_SUB_SIZE,
                    if selected {
                        color_from_f32(theme.accent)
                    } else {
                        theme.text
                    },
                    selected,
                );
                if selected {
                    draw_text(
                        sugarloaf,
                        opt.right() - 22.0,
                        opt.y + 8.0,
                        "✓",
                        ROW_SUB_SIZE,
                        color_from_f32(theme.accent),
                        true,
                    );
                }
            }
        }
    }
}

fn render_vault_unlock(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    window_width: f32,
    window_height: f32,
    device_scale: f32,
    paint_glyphs: bool,
) {
    let prompt = &chrome.vault_unlock;
    let layout = terminus_ui::VaultUnlockLayout::centered(window_width, window_height);
    let dialog = layout.rect();
    let radius = terminus_ui::vault_unlock::RADIUS;

    paint_dialog_shell(
        sugarloaf,
        theme,
        window_width,
        window_height,
        &dialog,
        radius,
        DialogBorderMode::Outward,
        DEPTH_DIALOG,
        DEPTH_DIALOG_BG,
        ORDER_DIALOG,
        None,
    );
    if !paint_glyphs {
        return;
    }

    let title = layout.title_rect();
    draw_text(
        sugarloaf,
        title.x,
        title.y + 4.0,
        "Unlock Vault",
        DIALOG_TITLE_SIZE,
        theme.text,
        true,
    );

    let subtitle = layout.subtitle_rect();
    let subtitle_text = prompt
        .pending()
        .map(terminus_ui::PendingVaultAction::subtitle)
        .unwrap_or("Enter your vault passphrase.");
    draw_text(
        sugarloaf,
        subtitle.x,
        subtitle.y + 4.0,
        subtitle_text,
        HINT_SIZE,
        theme.text_muted,
        false,
    );

    // Same field card + caret path as Settings → Encryption Passphrase.
    let card = layout.passphrase_card_rect();
    let paint = prompt.field_paint();
    paint_settings_field_card(
        sugarloaf,
        theme,
        card,
        "Encryption Passphrase",
        &paint.text,
        true,
        paint.placeholder,
        terminus_ui::settings::FIELD_EYE_SLOT,
        true,
    );
    if paint.show_caret {
        let caret_prefix = if paint.placeholder {
            ""
        } else {
            paint.text.as_str()
        };
        paint_settings_caret(
            sugarloaf,
            theme,
            card,
            caret_prefix,
            terminus_ui::settings::FIELD_EYE_SLOT,
        );
    }

    let eye = layout.eye_rect();
    let eye_icon = if prompt.visible() {
        Icon::EyeOff
    } else {
        Icon::Eye
    };
    let eye_size = terminus_ui::VaultUnlockLayout::eye_icon_size();
    draw_icon(
        sugarloaf,
        eye_icon,
        IconPlacement::new(
            eye.x + (eye.width - eye_size) * 0.5,
            eye.y + (eye.height - eye_size) * 0.5,
            eye_size,
        ),
        theme.accent,
        device_scale,
    );

    if let Some(err) = prompt.error() {
        let hint = layout.hint_rect();
        draw_text(
            sugarloaf,
            hint.x,
            hint.y + 2.0,
            err,
            HINT_SIZE,
            theme.danger,
            false,
        );
    } else if prompt.unlocking() {
        let hint = layout.hint_rect();
        draw_text(
            sugarloaf,
            hint.x,
            hint.y + 2.0,
            "Unlocking…",
            HINT_SIZE,
            theme.text_muted,
            false,
        );
    }

    // Remember checkbox
    let check = layout.remember_box_rect();
    let row = layout.remember_row_rect();
    paint_surface(
        sugarloaf,
        &check,
        theme.button_bg,
        Some(if prompt.remember() {
            theme.accent
        } else {
            theme.field_border
        }),
        4.0,
        DEPTH_DIALOG_BG + 0.02,
        ORDER_DIALOG,
        false,
    );
    if prompt.remember() {
        draw_text(
            sugarloaf,
            check.x + 3.0,
            check.y + 1.0,
            "✓",
            HINT_SIZE,
            color_from_f32(theme.accent),
            true,
        );
    }
    draw_text(
        sugarloaf,
        check.right() + 10.0,
        row.y + (row.height - HINT_SIZE) * 0.5,
        "Remember on this device",
        HINT_SIZE,
        theme.text,
        false,
    );

    let cancel = layout.cancel_button_rect();
    let unlock = layout.unlock_button_rect();
    let input_radius = terminus_ui::vault_unlock::INPUT_RADIUS;
    paint_chrome_button(
        sugarloaf,
        theme,
        terminus_ui::ButtonSpec::secondary(cancel)
            .with_radius(input_radius)
            .muted(),
        "Cancel",
        HINT_SIZE,
        DEPTH_DIALOG_BG + 0.025,
        ORDER_DIALOG,
    );
    paint_chrome_button(
        sugarloaf,
        theme,
        terminus_ui::ButtonSpec::primary(unlock).with_radius(input_radius),
        "Unlock",
        HINT_SIZE,
        DEPTH_DIALOG_BG + 0.025,
        ORDER_DIALOG,
    );
}

// ---- primitives ------------------------------------------------------

fn opts(size: f32, color: [u8; 4], bold: bool) -> DrawOpts {
    DrawOpts {
        font_size: size,
        color,
        bold,
        ..DrawOpts::default()
    }
}

fn draw_text(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    y: f32,
    text: &str,
    size: f32,
    color: [u8; 4],
    bold: bool,
) {
    if text.is_empty() {
        return;
    }
    sugarloaf
        .text_mut()
        .draw(x, y, text, &opts(size, color, bold));
}

/// Shared settings field card (label + input row).
///
/// `trailing_slot` reserves space on the right of the input for an
/// adornment (e.g. passphrase eye) so value text never overlaps it.
///
/// When `paint_text` is false, only the card/input quads are drawn —
/// used while a popover covers the card so UI text (always last pass)
/// does not bleed through the menu.
fn paint_settings_field_card(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    card: Rect,
    label: &str,
    value: &str,
    focused: bool,
    placeholder: bool,
    trailing_slot: f32,
    paint_text: bool,
) {
    paint_field_card(
        sugarloaf,
        theme,
        card,
        label,
        value,
        focused,
        placeholder,
        trailing_slot,
        paint_text,
    );
}

fn paint_settings_caret(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    card: Rect,
    value: &str,
    trailing_slot: f32,
) {
    paint_field_caret_prefix(sugarloaf, theme, card, value, trailing_slot);
}

fn render_context_menu(
    sugarloaf: &mut Sugarloaf,
    menu: &ContextMenu,
    theme: &ChromeTheme,
    device_scale: f32,
) {
    let rect = menu.rect();


    // Quad shell still helps under terminal cells; the late UI-text pass
    // is what actually covers host labels/icons.
    paint_surface(
        sugarloaf,
        &rect,
        theme.button_bg,
        Some(theme.panel_border),
        MENU_RADIUS,
        DEPTH_DIALOG,
        ORDER_GHOST,
        false,
    );

    let scale = device_scale.max(1.0);
    let content_w = (rect.width * scale).round().max(1.0);
    let content_h = (rect.height * scale).round().max(1.0);
    // Atlas masks are square; side must cover the content. Artwork ids must
    // also encode content_w/content_h — otherwise a wider-than-tall host menu
    // and a shorter group menu share one cache slot and the taller fill sticks.
    let side = (content_w.max(content_h).ceil() as u16).max(1);
    let bg_id = ctx_menu_mask_id(CTX_MENU_BG_KIND, content_w, content_h);
    let border_id = ctx_menu_mask_id(CTX_MENU_BORDER_KIND, content_w, content_h);


    let bg = color_from_f32(theme.button_bg);
    let border = color_from_f32(theme.panel_border);
    let radius_px = MENU_RADIUS * scale;

    sugarloaf.text_mut().draw_mask_late(
        rect.x,
        rect.y,
        bg_id,
        side,
        bg,
        move |size| rasterize_rounded_rect_mask(size, content_w, content_h, radius_px, false),
    );
    sugarloaf.text_mut().draw_mask_late(
        rect.x,
        rect.y,
        border_id,
        side,
        border,
        move |size| rasterize_rounded_rect_mask(size, content_w, content_h, radius_px, true),
    );

    for (i, item) in menu.items.iter().enumerate() {
        let Some(row) = menu.item_rect(i) else {
            continue;
        };
        let hovered = menu.hover == Some(i);
        if hovered {
            let fill = if item.danger {
                [
                    theme.danger[0],
                    theme.danger[1],
                    theme.danger[2],
                    48,
                ]
            } else {
                color_from_f32(theme.item_hover)
            };
            let rw = (row.width * scale).round().max(1.0);
            let rh = (row.height * scale).round().max(1.0);
            let row_side = (rw.max(rh).ceil() as u16).max(1);
            let hover_id =
                ctx_menu_mask_id(CTX_MENU_HOVER_KIND.wrapping_add(i as u32), rw, rh);
            sugarloaf.text_mut().draw_mask_late(
                row.x,
                row.y,
                hover_id,
                row_side,
                fill,
                move |size| rasterize_rounded_rect_mask(size, rw, rh, 6.0 * scale, false),
            );
        }
        let color = if item.danger {
            theme.danger
        } else {
            theme.text
        };
        let text_y = row.y + (CTX_ITEM_HEIGHT - ROW_SUB_SIZE) * 0.5;
        sugarloaf.text_mut().draw_late(
            row.x + MENU_PAD_X + 4.0,
            text_y,
            &item.label,
            &opts(ROW_SUB_SIZE, color, false),
        );
    }
}

const CTX_MENU_BG_KIND: u32 = 0x01;
const CTX_MENU_BORDER_KIND: u32 = 0x02;
const CTX_MENU_HOVER_KIND: u32 = 0x10;

/// Atlas key for a rounded-rect mask.
///
/// `GlyphKey::glyph_id` is a **u32** (`artwork_id as u32` in sugarloaf), so
/// every bit that distinguishes shapes must fit in 32 bits. Packing height in
/// the high half of a u64 was truncated away — host (h=104) and group (h=72)
/// menus then shared one atlas slot (`side` is max(w,h) and usually the width).
fn ctx_menu_mask_id(kind: u32, content_w: f32, content_h: f32) -> u64 {
    let w = content_w.round().clamp(1.0, 0xFFF as f32) as u32;
    let h = content_h.round().clamp(1.0, 0xFFF as f32) as u32;
    // [kind:8][w:12][h:12]
    let id = (kind & 0xFF) | ((w & 0xFFF) << 8) | ((h & 0xFFF) << 20);
    id as u64
}

fn rasterize_rounded_rect_mask(
    size: u16,
    content_w: f32,
    content_h: f32,
    radius: f32,
    stroke_only: bool,
) -> Option<rio_backend::sugarloaf::text::CoverageMask> {
    let side = size as u32;
    let mut pixmap = tiny_skia::Pixmap::new(side, side)?;
    let w = content_w.min(size as f32).max(1.0);
    let h = content_h.min(size as f32).max(1.0);
    let r = radius.min(w * 0.5).min(h * 0.5).max(0.0);
    let path = rounded_rect_path(0.0, 0.0, w, h, r)?;
    let mut paint = tiny_skia::Paint::default();
    paint.anti_alias = true;
    paint.set_color_rgba8(255, 255, 255, 255);
    if stroke_only {
        let stroke = tiny_skia::Stroke {
            width: 1.0,
            ..tiny_skia::Stroke::default()
        };
        pixmap.stroke_path(
            &path,
            &paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    } else {
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            tiny_skia::Transform::identity(),
            None,
        );
    }
    let bytes: Vec<u8> = pixmap
        .pixels()
        .iter()
        .map(|p| p.alpha())
        .collect();
    rio_backend::sugarloaf::text::CoverageMask::new(size, bytes)
}

fn rounded_rect_path(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let mut b = tiny_skia::PathBuilder::new();
    if r <= 0.5 {
        b.move_to(x, y);
        b.line_to(x + w, y);
        b.line_to(x + w, y + h);
        b.line_to(x, y + h);
        b.close();
        return b.finish();
    }
    b.move_to(x + r, y);
    b.line_to(x + w - r, y);
    b.cubic_to(x + w, y, x + w, y, x + w, y + r);
    b.line_to(x + w, y + h - r);
    b.cubic_to(x + w, y + h, x + w, y + h, x + w - r, y + h);
    b.line_to(x + r, y + h);
    b.cubic_to(x, y + h, x, y + h, x, y + h - r);
    b.line_to(x, y + r);
    b.cubic_to(x, y, x, y, x + r, y);
    b.close();
    b.finish()
}

/// Paint a chrome button using [`terminus_ui::ButtonSpec`] centering math.
fn paint_chrome_button(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    spec: terminus_ui::ButtonSpec,
    label: &str,
    font_size: f32,
    depth: f32,
    order: u8,
) {
    let rect = spec.rect;
    if spec.has_border() {
        paint_surface(
            sugarloaf,
            &rect,
            spec.fill(theme.accent, theme.button_bg),
            Some(theme.panel_border),
            spec.radius,
            depth,
            order,
            false,
        );
    } else {
        paint_surface(
            sugarloaf,
            &rect,
            spec.fill(theme.accent, theme.button_bg),
            None,
            spec.radius,
            depth,
            order,
            false,
        );
    }
    if label.is_empty() {
        return;
    }
    let color = spec.label_color([255, 255, 255, 255], theme.text, theme.text_muted);
    let bold = matches!(spec.kind, terminus_ui::ButtonKind::Primary);
    let text_w = sugarloaf
        .text_mut()
        .measure(label, &opts(font_size, color, bold));
    let (x, y) = terminus_ui::centered_label_origin(rect, text_w, font_size);
    draw_text(sugarloaf, x, y, label, font_size, color, bold);
}

/// Draw a Lucide icon.
///
/// The outline is rasterized at device resolution and handed to `sugarloaf`
/// as a coverage mask, which the glyph shader then reads texel for texel.
/// That is the whole point of doing it this way: painting the same outline
/// out of `line`/`rect` primitives can only put straight edges on whole
/// pixels, so a 24-unit grid scaled to 20px gives a 2px stroke as two hard
/// columns with nothing in between — a staircase, however the endpoints are
/// snapped. A mask keeps the rasterizer's own per-pixel coverage, so the
/// same outline comes out antialiased at any size.
pub(crate) fn draw_icon(
    sugarloaf: &mut Sugarloaf,
    icon: Icon,
    placement: IconPlacement,
    color: [f32; 4],
    device_scale: f32,
) {
    let size = placement.device_size(device_scale);
    // Icons travel with the UI text (same shader, same atlas), which is
    // the one part of the painter that works in bytes rather than floats.
    sugarloaf.text_mut().draw_mask(
        placement.x,
        placement.y,
        icon.id(),
        size,
        as_u8(color),
        |size| rasterize_icon(icon, size),
    );
}

/// Draw a filled brand OS mark (Nix/Ubuntu/Windows/Linux).
pub(crate) fn draw_os_glyph(
    sugarloaf: &mut Sugarloaf,
    glyph: OsGlyph,
    placement: IconPlacement,
    color: [f32; 4],
    device_scale: f32,
) {
    if !glyph.has_mark() {
        return;
    }
    let size = placement.device_size(device_scale);
    sugarloaf.text_mut().draw_mask(
        placement.x,
        placement.y,
        glyph.mask_id(),
        size,
        as_u8(color),
        |size| rasterize_os_glyph(glyph, size),
    );
}

/// Rasterize a Lucide outline into an 8-bit coverage mask.
///
/// The artwork is scaled so its 24-unit grid fills the `size`-pixel mask
/// exactly, then stroked the way a browser strokes the SVG: Lucide's own
/// 2/24 width, round caps and round joins. `None` when the path is
/// degenerate.
fn rasterize_icon(icon: Icon, size: u16) -> Option<CoverageMask> {
    let unit = IconPlacement::unit(size);
    let mut builder = tiny_skia::PathBuilder::new();
    let (mut from, mut start) = ((0.0f32, 0.0f32), (0.0f32, 0.0f32));
    // Lucide spells a dot as a line with no length (`M6 6L6.01 6` in
    // `server`) and leans on the round cap to turn it into a disc. A
    // stroker has no direction to work with on a segment that short, so
    // those are filled as circles instead of being stroked.
    let mut dots = Vec::new();

    for cmd in icon.path() {
        let at = |x: f32, y: f32| (x * unit, y * unit);
        match cmd {
            Cmd::MoveTo { x, y } => {
                let point = at(x, y);
                builder.move_to(point.0, point.1);
                from = point;
                start = point;
            }
            Cmd::LineTo { x, y } => {
                let point = at(x, y);
                if (point.0 - from.0).abs() < unit * 0.05
                    && (point.1 - from.1).abs() < unit * 0.05
                {
                    dots.push(point);
                } else {
                    builder.line_to(point.0, point.1);
                }
                from = point;
            }
            Cmd::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let (c1, c2, point) = (at(x1, y1), at(x2, y2), at(x, y));
                builder.cubic_to(c1.0, c1.1, c2.0, c2.1, point.0, point.1);
                from = point;
            }
            Cmd::Close => {
                builder.close();
                from = start;
            }
        }
    }

    let path = builder.finish()?;
    let mut pixmap = tiny_skia::Pixmap::new(size as u32, size as u32)?;

    let stroke_width = LUCIDE_STROKE * unit;
    let mut paint = tiny_skia::Paint::default();
    paint.anti_alias = true;
    paint.set_color_rgba8(255, 255, 255, 255);
    let stroke = tiny_skia::Stroke {
        width: stroke_width,
        line_cap: tiny_skia::LineCap::Round,
        line_join: tiny_skia::LineJoin::Round,
        ..tiny_skia::Stroke::default()
    };
    pixmap.stroke_path(
        &path,
        &paint,
        &stroke,
        tiny_skia::Transform::identity(),
        None,
    );

    for (x, y) in dots {
        if let Some(dot) = tiny_skia::PathBuilder::from_circle(x, y, stroke_width / 2.0) {
            pixmap.fill_path(
                &dot,
                &paint,
                tiny_skia::FillRule::Winding,
                tiny_skia::Transform::identity(),
                None,
            );
        }
    }

    // The mask is the alpha channel: the paint is opaque white, so alpha
    // *is* the coverage, and the tint comes from the instance color.
    let bytes = pixmap.pixels().iter().map(|pixel| pixel.alpha()).collect();
    CoverageMask::new(size, bytes)
}

/// Rasterize a filled brand mark into an 8-bit coverage mask.
fn rasterize_os_glyph(glyph: OsGlyph, size: u16) -> Option<CoverageMask> {
    let unit = IconPlacement::unit(size);
    let mut builder = tiny_skia::PathBuilder::new();
    let mut start = (0.0f32, 0.0f32);

    for cmd in glyph.path() {
        let at = |x: f32, y: f32| (x * unit, y * unit);
        match cmd {
            Cmd::MoveTo { x, y } => {
                let point = at(x, y);
                builder.move_to(point.0, point.1);
                start = point;
            }
            Cmd::LineTo { x, y } => {
                let point = at(x, y);
                builder.line_to(point.0, point.1);
            }
            Cmd::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let (c1, c2, point) = (at(x1, y1), at(x2, y2), at(x, y));
                builder.cubic_to(c1.0, c1.1, c2.0, c2.1, point.0, point.1);
            }
            Cmd::Close => {
                builder.close();
                let _ = start;
            }
        }
    }

    let path = builder.finish()?;
    let mut pixmap = tiny_skia::Pixmap::new(size as u32, size as u32)?;
    let mut paint = tiny_skia::Paint::default();
    paint.anti_alias = true;
    paint.set_color_rgba8(255, 255, 255, 255);
    pixmap.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        tiny_skia::Transform::identity(),
        None,
    );

    let bytes = pixmap.pixels().iter().map(|pixel| pixel.alpha()).collect();
    CoverageMask::new(size, bytes)
}

/// Soft overlay over the terminal while a host session is starting.
fn render_connection_modal(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    window_width: f32,
    window_height: f32,
    device_scale: f32,
    phase: f32,
    paint_glyphs: bool,
) {
    let Some(conn) = chrome.connection.as_ref() else {
        return;
    };

    // Measure step labels first so the dialog / columns hug real glyph
    // advances (no text clamp — width grows with content + padding).
    let mut label_widths = [0.0f32; STEP_COUNT];
    if paint_glyphs {
        for i in 0..STEP_COUNT {
            let label = conn.step_label(i);
            label_widths[i] = sugarloaf
                .text_mut()
                .measure(label, &opts(10.0, theme.text, false));
        }
        conn.set_label_widths(label_widths);
    }

    let dialog = conn.dialog_rect(window_width, window_height);
    // Outer radius 13 → fill at 12 via paint_surface's inset shrink.
    paint_dialog_shell(
        sugarloaf,
        theme,
        window_width,
        window_height,
        &dialog,
        13.0,
        DialogBorderMode::Outward,
        DEPTH_DIALOG,
        DEPTH_DIALOG_BG,
        ORDER_DIALOG,
        Some((0.55, 0.70)),
    );
    if !paint_glyphs {
        return;
    }

    // Header: host badge + title + endpoint + Show logs.
    let header_icon = conn.header_icon_rect(dialog);
    paint_surface(
        sugarloaf,
        &header_icon,
        with_alpha(theme.accent, 0.22),
        None,
        10.0,
        DEPTH_DIALOG_BG + 0.02,
        ORDER_DIALOG,
        false,
    );
    let header_glyph = match conn.kind {
        terminus_ui::ConnectKind::Ssh => Icon::Server,
        terminus_ui::ConnectKind::Wsl => Icon::SquareTerminal,
    };
    draw_icon(
        sugarloaf,
        header_glyph,
        IconPlacement::new(
            header_icon.x + (header_icon.width - 22.0) * 0.5,
            header_icon.y + (header_icon.height - 22.0) * 0.5,
            22.0,
        ),
        theme.accent,
        device_scale,
    );

    let text_x = header_icon.right() + 12.0;
    draw_text(
        sugarloaf,
        text_x,
        header_icon.y + 8.0,
        &conn.title,
        DIALOG_TITLE_SIZE,
        theme.text,
        true,
    );
    draw_text(
        sugarloaf,
        text_x,
        header_icon.y + 28.0,
        &conn.endpoint,
        HINT_SIZE,
        theme.text_muted,
        false,
    );

    let logs_btn = conn.logs_button_rect(dialog);
    let logs_label = if conn.logs_open {
        "Hide logs"
    } else {
        "Show logs"
    };
    paint_chrome_button(
        sugarloaf,
        theme,
        terminus_ui::ButtonSpec::ghost(logs_btn).muted(),
        logs_label,
        HINT_SIZE,
        DEPTH_DIALOG_BG + 0.02,
        ORDER_DIALOG,
    );

    // Progress track.
    let track = conn.track_line_rect(dialog);
    paint_surface(
        sugarloaf,
        &track,
        theme.panel_border,
        None,
        2.0,
        DEPTH_DIALOG_BG + 0.02,
        ORDER_DIALOG,
        false,
    );
    let fill = conn.progress_fill_rect(dialog);
    if fill.width > 0.5 {
        paint_surface(
            sugarloaf,
            &fill,
            theme.accent,
            None,
            2.0,
            DEPTH_DIALOG_BG + 0.03,
            ORDER_DIALOG,
            false,
        );
    }

    let success = success_color();
    for i in 0..STEP_COUNT {
        let node = conn.node_rect(dialog, i);
        let state = conn.node_state(i);
        let (fill_c, border_c, icon_c) = match state {
            NodeVisual::Pending => (
                theme.dialog_bg,
                theme.panel_border,
                as_f32(theme.text_faint),
            ),
            NodeVisual::Active => (theme.accent, theme.accent, [1.0, 1.0, 1.0, 1.0]),
            NodeVisual::Done => (success, success, [1.0, 1.0, 1.0, 1.0]),
        };

        // Soft glow behind the active node (pulse via phase).
        if state == NodeVisual::Active {
            let pulse = (phase * std::f32::consts::TAU).sin() * 0.5 + 0.5;
            let glow = 4.0 + 4.0 * pulse;
            let glow_rect = Rect::new(
                node.x - glow,
                node.y - glow,
                node.width + 2.0 * glow,
                node.height + 2.0 * glow,
            );
            paint_surface(
                sugarloaf,
                &glow_rect,
                with_alpha(theme.accent, 0.18 + 0.16 * pulse),
                None,
                (node.width + 2.0 * glow) * 0.5,
                DEPTH_DIALOG_BG + 0.035,
                ORDER_DIALOG,
                false,
            );
        }

        let node_shell = Rect::new(
            node.x - 1.5,
            node.y - 1.5,
            node.width + 3.0,
            node.height + 3.0,
        );
        paint_surface_stroke(
            sugarloaf,
            &node_shell,
            fill_c,
            Some(border_c),
            (node.width + 3.0) * 0.5,
            1.5,
            DEPTH_DIALOG_BG + 0.04,
            ORDER_DIALOG,
            false,
        );

        let icon_size = 16.0;
        draw_icon(
            sugarloaf,
            conn.step_icon(i),
            IconPlacement::new(
                node.x + (node.width - icon_size) * 0.5,
                node.y + (node.height - icon_size) * 0.5,
                icon_size,
            ),
            icon_c,
            device_scale,
        );

        let label = conn.step_label(i);
        let label_color = match state {
            NodeVisual::Pending => theme.text_faint,
            NodeVisual::Active => as_u8(theme.accent),
            NodeVisual::Done => as_u8(success),
        };
        let lw = label_widths[i];
        // Center under the node — columns share the widest label's width.
        let label_x = node.x + (node.width - lw) * 0.5;
        draw_text(
            sugarloaf,
            label_x,
            node.bottom() + 8.0,
            label,
            10.0,
            label_color,
            false,
        );
    }

    // Status line.
    let status = conn.status_rect(dialog);
    let status_color = if conn.succeeded {
        as_u8(success)
    } else {
        as_u8(theme.accent)
    };
    let sw = sugarloaf
        .text_mut()
        .measure(&conn.status, &opts(HINT_SIZE, status_color, false));
    draw_text(
        sugarloaf,
        status.x + (status.width - sw) * 0.5,
        status.y + 10.0,
        &conn.status,
        HINT_SIZE,
        status_color,
        false,
    );

    // Collapsible logs drawer.
    if let Some(logs) = conn.logs_rect(dialog) {
        paint_surface(
            sugarloaf,
            &logs,
            theme.field_bg,
            None,
            8.0,
            DEPTH_DIALOG_BG + 0.02,
            ORDER_DIALOG,
            false,
        );
        let mut y = logs.y + 8.0;
        let line_h = 14.0;
        let max_lines = ((logs.height - 12.0) / line_h).floor() as usize;
        let start = conn.logs.len().saturating_sub(max_lines);
        for line in conn.logs.iter().skip(start) {
            let clipped = elide(
                sugarloaf,
                line,
                logs.width - 16.0,
                &opts(10.0, theme.text_muted, false),
            );
            draw_text(
                sugarloaf,
                logs.x + 8.0,
                y,
                &clipped,
                10.0,
                theme.text_muted,
                false,
            );
            y += line_h;
            if y > logs.bottom() - 10.0 {
                break;
            }
        }
    }

    // Close button.
    let close = conn.close_button_rect(dialog);
    paint_chrome_button(
        sugarloaf,
        theme,
        terminus_ui::ButtonSpec::ghost(close).with_radius(10.0),
        "Close",
        HINT_SIZE,
        DEPTH_DIALOG_BG + 0.02,
        ORDER_DIALOG,
    );
}

fn success_color() -> [f32; 4] {
    // Emerald-500 — matches the Termius mock's validated state.
    [16.0 / 255.0, 185.0 / 255.0, 129.0 / 255.0, 1.0]
}

/// Three chasing dots + a breathing ring around `(cx, cy)`.
fn draw_orbit_indicator(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    cx: f32,
    cy: f32,
    orbit_r: f32,
    dot_r: f32,
    phase: f32,
) {
    let ring = breath_ring(cx, cy, orbit_r + dot_r * 1.4, phase);
    let ring_color = with_alpha(theme.accent, ring.alpha);
    let diam = ring.radius * 2.0;
    paint_surface(
        sugarloaf,
        &Rect::new(ring.x - ring.radius, ring.y - ring.radius, diam, diam),
        ring_color,
        None,
        ring.radius,
        DEPTH_CONTENT + 0.05,
        ORDER_CONNECTING,
        false,
    );

    for dot in orbit_dots(cx, cy, orbit_r, dot_r, phase) {
        let color = with_alpha(theme.accent, dot.alpha);
        let d = dot.radius * 2.0;
        paint_surface(
            sugarloaf,
            &Rect::new(dot.x - dot.radius, dot.y - dot.radius, d, d),
            color,
            None,
            dot.radius,
            DEPTH_CONTENT + 0.06,
            ORDER_CONNECTING,
            false,
        );
    }
}

fn with_alpha(color: [f32; 4], alpha: f32) -> [f32; 4] {
    [color[0], color[1], color[2], (color[3] * alpha).clamp(0.0, 1.0)]
}

/// Composite `fg` (possibly translucent) over opaque `bg` → opaque result.
fn opaque_over(bg: [f32; 4], fg: [f32; 4]) -> [f32; 4] {
    let a = fg[3].clamp(0.0, 1.0);
    let inv = 1.0 - a;
    [
        fg[0] * a + bg[0] * inv,
        fg[1] * a + bg[1] * inv,
        fg[2] * a + bg[2] * inv,
        1.0,
    ]
}

fn as_u8(color: [f32; 4]) -> [u8; 4] {
    color.map(|channel| (channel * 255.0).round().clamp(0.0, 255.0) as u8)
}

fn as_f32(color: [u8; 4]) -> [f32; 4] {
    [
        color[0] as f32 / 255.0,
        color[1] as f32 / 255.0,
        color[2] as f32 / 255.0,
        color[3] as f32 / 255.0,
    ]
}

/// Truncate `text` to fit `max_width`, ending in an ellipsis.
///
/// Measured with the real shaper, so a proportional font cannot
/// overflow its panel the way a character count would allow.
fn elide(
    sugarloaf: &mut Sugarloaf,
    text: &str,
    max_width: f32,
    opts: &DrawOpts,
) -> String {
    if max_width <= 0.0 {
        return String::new();
    }
    if sugarloaf.text_mut().measure(text, opts) <= max_width {
        return text.to_string();
    }
    let mut kept = String::new();
    for ch in text.chars() {
        let mut candidate = kept.clone();
        candidate.push(ch);
        candidate.push('\u{2026}');
        if sugarloaf.text_mut().measure(&candidate, opts) > max_width {
            break;
        }
        kept.push(ch);
    }
    kept.push('\u{2026}');
    kept
}

/// Word-wrap `text` into at most `max_lines` lines that fit `max_width`.
/// The final line is elided when the message still overflows.
fn wrap_lines(
    sugarloaf: &mut Sugarloaf,
    text: &str,
    max_width: f32,
    opts: &DrawOpts,
    max_lines: usize,
) -> Vec<String> {
    if max_lines == 0 || max_width <= 0.0 {
        return Vec::new();
    }
    if sugarloaf.text_mut().measure(text, opts) <= max_width {
        return vec![text.to_string()];
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return vec![elide(sugarloaf, text, max_width, opts)];
    }

    let mut lines: Vec<String> = Vec::new();
    let mut idx = 0;
    while idx < words.len() && lines.len() < max_lines {
        let last_line = lines.len() + 1 == max_lines;
        if last_line {
            let rest = words[idx..].join(" ");
            lines.push(elide(sugarloaf, &rest, max_width, opts));
            break;
        }
        let mut current = String::new();
        while idx < words.len() {
            let candidate = if current.is_empty() {
                words[idx].to_string()
            } else {
                format!("{} {}", current, words[idx])
            };
            if sugarloaf.text_mut().measure(&candidate, opts) <= max_width {
                current = candidate;
                idx += 1;
            } else {
                break;
            }
        }
        if current.is_empty() {
            lines.push(elide(sugarloaf, words[idx], max_width, opts));
            idx += 1;
        } else {
            lines.push(current);
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_blocked_only_when_cover_overlaps() {
        let cover = Rect::new(100.0, 100.0, 200.0, 200.0);
        let under = Rect::new(120.0, 120.0, 40.0, 40.0);
        let aside = Rect::new(10.0, 10.0, 40.0, 40.0);
        assert!(text_blocked_by(Some(&cover), &under));
        assert!(!text_blocked_by(Some(&cover), &aside));
        assert!(!text_blocked_by(None, &under));
    }

    #[test]
    fn rect_union_expands_to_bounds() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        let b = Rect::new(5.0, 5.0, 20.0, 20.0);
        let u = rect_union(a, b);
        assert_eq!(u.x, 0.0);
        assert_eq!(u.y, 0.0);
        assert_eq!(u.right(), 25.0);
        assert_eq!(u.bottom(), 25.0);
    }

    #[test]
    fn colors_convert_from_bytes_without_clipping() {
        assert_eq!(as_f32([0, 0, 0, 0]), [0.0, 0.0, 0.0, 0.0]);
        assert_eq!(as_f32([255, 255, 255, 255]), [1.0, 1.0, 1.0, 1.0]);
        let half = as_f32([128, 128, 128, 128]);
        for channel in half {
            assert!((channel - 0.502).abs() < 0.01, "{channel}");
        }
    }

    /// The round trip has to be exact, or a theme colour would drift every
    /// time it passed through the icon path.
    #[test]
    fn colors_survive_the_round_trip_to_bytes() {
        for value in 0u8..=255 {
            let byte = [value, value, value, value];
            assert_eq!(as_u8(as_f32(byte)), byte, "{value} drifted");
        }
    }

    #[test]
    fn chrome_orders_sit_above_the_grid_and_below_the_overlays() {
        // The terminal grid paints at order 3; the command palette,
        // search and hint tooltip at 20. Chrome goes in between, and the
        // editor above them all so it is never occluded. The host-drag
        // ghost sits above dialogs so the phantom is always on top.
        assert!(ORDER_RAIL > 3);
        assert!(ORDER_PANEL > ORDER_RAIL);
        assert!(ORDER_CONTENT > ORDER_PANEL);
        assert!(ORDER_CONTENT < 20);
        assert!(ORDER_DIALOG > 20);
        assert!(ORDER_GHOST > ORDER_DIALOG);
        assert!(DEPTH_GHOST > DEPTH_DIALOG_BG);
    }

    #[test]
    fn the_dialog_depth_is_above_the_scrim() {
        assert!(DEPTH_DIALOG_BG > DEPTH_DIALOG);
    }
}


fn render_add_snippet(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    window_width: f32,
    window_height: f32,
    device_scale: f32,
    paint_glyphs: bool,
) {
    let form = &chrome.snippet_form.inner;
    paint_scrim(
        sugarloaf,
        window_width,
        window_height,
        theme.scrim,
        DEPTH_DIALOG_BG,
        ORDER_DIALOG,
    );

    let layout = terminus_ui::dialog_form::DialogFormLayout::compute(
        form,
        window_width,
        window_height,
    );

    paint_dialog_shell(
        sugarloaf,
        theme,
        window_width,
        window_height,
        &layout.dialog,
        terminus_ui::dialog_form::DIALOG_RADIUS,
        DialogBorderMode::Outward,
        DEPTH_DIALOG_BG,
        DEPTH_DIALOG,
        ORDER_DIALOG,
        None,
    );
    if !paint_glyphs {
        return;
    }

    draw_text(
        sugarloaf,
        layout.title.x,
        layout.title.y + 4.0,
        &form.title,
        DIALOG_TITLE_SIZE,
        theme.text,
        true,
    );

    for (i, input_data) in form.fields.iter().enumerate() {
        let field_frame = layout.fields[i];
        let focused = form.focused_index == i;

        let input_top = 18.0;
        let input_height = 32.0;

        let caption = Rect::new(field_frame.x + 2.0, field_frame.y, field_frame.width - 4.0, input_top);
        let input = Rect::new(field_frame.x, field_frame.y + input_top, field_frame.width, input_height);

        let shell = Rect::new(
            input.x - BORDER_WIDTH,
            input.y - BORDER_WIDTH,
            input.width + 2.0 * BORDER_WIDTH,
            input.height + 2.0 * BORDER_WIDTH,
        );
        paint_surface(
            sugarloaf,
            &shell,
            theme.button_bg,
            Some(if focused {
                theme.field_border_focus
            } else {
                theme.field_border
            }),
            12.0,
            DEPTH_DIALOG_BG + 0.015,
            ORDER_DIALOG,
            false,
        );

        draw_text(
            sugarloaf,
            caption.x,
            caption.y + 2.0,
            &input_data.label,
            CAPTION_SIZE,
            [0xcb, 0xd5, 0xe1, 255],
            false,
        );

        let text_x = input.x + 8.0;
        let text_y = input.y + (input.height - 12.0) / 2.0;
        let draft = &input_data.draft;
        let shown = draft.display_line();
        let text_opts = opts(12.0, theme.text, false);

        if focused {
            if let Some((start, end)) = draft.selection_range() {
                let before: String = shown.chars().take(start).collect();
                let selected: String = shown.chars().skip(start).take(end - start).collect();
                let bx = sugarloaf.text_mut().measure(&before, &text_opts);
                let sw = sugarloaf
                    .text_mut()
                    .measure(&selected, &text_opts)
                    .max(2.0);
                paint_flat(
                    sugarloaf,
                    &Rect::new(text_x + bx, input.y + 6.0, sw, input.height - 12.0),
                    with_alpha(theme.accent, 0.35),
                    DEPTH_DIALOG_BG + 0.02,
                    ORDER_DIALOG,
                );
            }
        }

        sugarloaf
            .text_mut()
            .draw(text_x, text_y, &shown, &text_opts);

        if focused {
            let prefix = draft.prefix_display();
            let drawn = sugarloaf.text_mut().measure(&prefix, &text_opts);
            paint_caret(
                sugarloaf,
                text_x + drawn,
                input.y + 6.0,
                input.height - 12.0,
                theme.accent,
                DEPTH_DIALOG_BG + 0.03,
                ORDER_DIALOG,
            );
        }
    }
    
    if let Some(err_box) = layout.error_line {
        draw_text(
            sugarloaf,
            err_box.x,
            err_box.y + 16.0,
            form.error.as_deref().unwrap_or(""),
            HINT_SIZE,
            theme.danger,
            true,
        );
    }

    paint_chrome_button(
        sugarloaf,
        theme,
        terminus_ui::ButtonSpec::primary(layout.save_btn).hovered(
            form.btn_hover == Some(terminus_ui::dialog_form::DynamicFormHit::Save),
        ),
        &form.save_label,
        ROW_TITLE_SIZE,
        DEPTH_DIALOG,
        ORDER_DIALOG,
    );

    paint_chrome_button(
        sugarloaf,
        theme,
        terminus_ui::ButtonSpec::ghost(layout.cancel_btn).hovered(
            form.btn_hover == Some(terminus_ui::dialog_form::DynamicFormHit::Cancel),
        ),
        "Cancel",
        ROW_TITLE_SIZE,
        DEPTH_DIALOG,
        ORDER_DIALOG,
    );
}
