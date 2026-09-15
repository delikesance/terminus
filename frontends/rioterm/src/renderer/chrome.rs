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
        // The rail is collapsed, so nothing reserves space and nothing
        // is painted — but the editor is a dialog, not part of the
        // panel, and must still show if it is open.
        if chrome.add_host_is_open() {
            render_add_host(sugarloaf, chrome, theme, window_width, window_height);
        }
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
            device_scale,
            connecting_phase,
        );
    }

    if chrome.connection.is_some() {
        render_connection_modal(
            sugarloaf,
            chrome,
            theme,
            window_width,
            window_height,
            device_scale,
            connecting_phase.unwrap_or(0.0),
        );
    }

    if chrome.add_host_is_open() {
        render_add_host(sugarloaf, chrome, theme, window_width, window_height);
    }

    if chrome.settings_is_open() {
        render_settings_modal(sugarloaf, chrome, theme, window_width, window_height, device_scale);
    }

    // Ghost last, max z-order — always above rail, panel, dialogs, sticky header.
    paint_host_drag_ghost(sugarloaf, chrome, theme, device_scale);
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
    sugarloaf.rect(
        None,
        rail.x,
        rail.y,
        rail.width,
        rail.height,
        theme.rail_bg,
        DEPTH_BG,
        ORDER_RAIL,
    );
    // Right hairline against the drawer.
    sugarloaf.rect(
        None,
        rail.right() - BORDER_WIDTH,
        rail.y,
        BORDER_WIDTH,
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
            sugarloaf.rounded_rect(
                None,
                pill.x,
                pill.y,
                pill.width,
                pill.height,
                theme.rail_active_bg,
                DEPTH_CONTENT,
                activity_bar::PILL_RADIUS,
                ORDER_CONTENT,
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
    device_scale: f32,
    connecting_phase: Option<f32>,
) {
    let panel = chrome.panel.rect(origin_y, height);
    sugarloaf.rect(
        None,
        panel.x,
        panel.y,
        panel.width,
        panel.height,
        theme.panel_bg,
        DEPTH_BG,
        ORDER_PANEL,
    );
    // A hairline separator against the terminal, on the panel's right edge.
    sugarloaf.rect(
        None,
        panel.right() - BORDER_WIDTH,
        panel.y,
        BORDER_WIDTH,
        panel.height,
        theme.panel_border,
        DEPTH_BG + 0.005,
        ORDER_PANEL,
    );

    let labels = !chrome.add_host_is_open();

    if labels {
        let title = chrome.panel.title_rect(origin_y);
        draw_text(
            sugarloaf,
            title.x + sidebar::PAD_X,
            title.y + 14.0,
            &chrome.panel_title().to_ascii_uppercase(),
            SECTION_LABEL_SIZE,
            [0xcb, 0xd5, 0xe1, 255], // slate-300
            true,
        );
        sugarloaf.rect(
            None,
            title.x,
            title.bottom() - BORDER_WIDTH,
            title.width,
            BORDER_WIDTH,
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
            sugarloaf.rect(
                None,
                band.x,
                band.bottom() - BORDER_WIDTH,
                band.width,
                BORDER_WIDTH,
                theme.panel_border,
                DEPTH_CONTENT,
                ORDER_CONTENT,
            );
        }
    }

    render_notice(sugarloaf, chrome, theme, origin_y, labels);

    if chrome.hosts_visible() {
        render_new_host_cta(
            sugarloaf,
            chrome,
            theme,
            origin_y,
            height,
            labels,
            device_scale,
        );
        render_host_rows(
            sugarloaf,
            chrome,
            theme,
            origin_y,
            height,
            labels,
            device_scale,
            connecting_phase,
        );
        // Re-paint the title + search band above the scrolled list so cards
        // tuck under the filter instead of covering it.
        if labels {
            render_sticky_drawer_chrome(sugarloaf, chrome, theme, origin_y, device_scale);
        }
    } else if chrome.snippets_visible() {
        render_snippets(sugarloaf, chrome, theme, origin_y, height, labels, device_scale);
    }
}

fn render_notice(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    origin_y: f32,
    labels: bool,
) {
    let Some(rect) = chrome.panel.notice_rect(origin_y) else {
        return;
    };
    let is_error = chrome.panel.error.is_some();
    sugarloaf.rect(
        None,
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        theme.notice_bg,
        DEPTH_BG + 0.01,
        ORDER_PANEL,
    );
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
    let text = elide(
        sugarloaf,
        message,
        rect.width - 2.0 * sidebar::PAD_X,
        &opts(ROW_SUB_SIZE, color, false),
    );
    draw_text(
        sugarloaf,
        rect.x + sidebar::PAD_X,
        rect.y + 6.0,
        &text,
        ROW_SUB_SIZE,
        color,
        false,
    );
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
    sugarloaf.rect(
        None,
        title.x,
        title.y,
        title.width,
        cover_bottom - title.y,
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
    sugarloaf.rect(
        None,
        title.x,
        title.bottom() - BORDER_WIDTH,
        title.width,
        BORDER_WIDTH,
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
    sugarloaf.rect(
        None,
        band.x,
        band.bottom() - BORDER_WIDTH,
        band.width,
        BORDER_WIDTH,
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
    sugarloaf.rounded_rect(
        None,
        search.x,
        search.y,
        search.width,
        search.height,
        theme.panel_border,
        depth,
        sidebar::CARD_RADIUS,
        ORDER_CONTENT,
    );
    sugarloaf.rounded_rect(
        None,
        search.x + BORDER_WIDTH,
        search.y + BORDER_WIDTH,
        search.width - 2.0 * BORDER_WIDTH,
        search.height - 2.0 * BORDER_WIDTH,
        theme.field_bg,
        depth + 0.001,
        sidebar::CARD_RADIUS - 1.0,
        ORDER_CONTENT,
    );
    if chrome.panel.filter_focused {
        sugarloaf.rounded_rect(
            None,
            search.x - BORDER_WIDTH,
            search.y - BORDER_WIDTH,
            search.width + 2.0 * BORDER_WIDTH,
            search.height + 2.0 * BORDER_WIDTH,
            theme.field_border_focus,
            depth + 0.002,
            sidebar::CARD_RADIUS + 1.0,
            ORDER_CONTENT,
        );
        sugarloaf.rounded_rect(
            None,
            search.x + BORDER_WIDTH,
            search.y + BORDER_WIDTH,
            search.width - 2.0 * BORDER_WIDTH,
            search.height - 2.0 * BORDER_WIDTH,
            theme.button_bg,
            depth + 0.003,
            sidebar::CARD_RADIUS - 1.0,
            ORDER_CONTENT,
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
    sugarloaf.rounded_rect(
        None,
        cta.x,
        cta.y,
        cta.width,
        cta.height,
        bg,
        DEPTH_CONTENT,
        sidebar::CARD_RADIUS,
        ORDER_CONTENT,
    );
    let border = if chrome.panel.add_hover {
        theme.accent
    } else {
        theme.panel_border
    };
    draw_dashed_rounded_rect(
        sugarloaf,
        cta.x,
        cta.y,
        cta.width,
        cta.height,
        sidebar::CARD_RADIUS,
        border,
        DEPTH_CONTENT + 0.01,
        ORDER_CONTENT,
    );

    let badge = Rect::new(
        cta.x + sidebar::CARD_PAD,
        cta.y + (cta.height - sidebar::BADGE_TILE) / 2.0,
        sidebar::BADGE_TILE,
        sidebar::BADGE_TILE,
    );
    sugarloaf.rounded_rect(
        None,
        badge.x,
        badge.y,
        badge.width,
        badge.height,
        theme.field_bg,
        DEPTH_CONTENT + 0.02,
        8.0,
        ORDER_CONTENT,
    );
    sugarloaf.rounded_rect(
        None,
        badge.x,
        badge.y,
        badge.width,
        badge.height,
        theme.panel_border,
        DEPTH_CONTENT + 0.021,
        8.0,
        ORDER_CONTENT,
    );
    sugarloaf.rounded_rect(
        None,
        badge.x + BORDER_WIDTH,
        badge.y + BORDER_WIDTH,
        badge.width - 2.0 * BORDER_WIDTH,
        badge.height - 2.0 * BORDER_WIDTH,
        theme.field_bg,
        DEPTH_CONTENT + 0.022,
        7.0,
        ORDER_CONTENT,
    );
    draw_icon(
        sugarloaf,
        Icon::Plus,
        IconPlacement::new(
            badge.x + (sidebar::BADGE_TILE - ADD_ICON_SIZE) / 2.0,
            badge.y + (sidebar::BADGE_TILE - ADD_ICON_SIZE) / 2.0,
            ADD_ICON_SIZE,
        ),
        theme.accent,
        device_scale,
    );

    if labels {
        let title_color = if chrome.panel.add_hover {
            color_from_f32(theme.accent)
        } else {
            theme.text
        };
        let text_x = badge.right() + sidebar::ICON_GAP;
        draw_text(
            sugarloaf,
            text_x,
            cta.y + 16.0,
            "New Host",
            ROW_TITLE_SIZE,
            title_color,
            true,
        );
        draw_text(
            sugarloaf,
            text_x,
            cta.y + 36.0,
            "Configure SSH connection",
            ROW_SUB_SIZE,
            theme.text_muted,
            false,
        );
    }
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
            sugarloaf.rounded_rect(
                None,
                cx,
                cy - pill_r,
                len,
                stroke,
                color,
                depth,
                pill_r,
                order,
            );
        } else {
            sugarloaf.rounded_rect(
                None,
                cx - pill_r,
                cy,
                stroke,
                len,
                color,
                depth,
                pill_r,
                order,
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
                sugarloaf.rounded_rect(
                    None,
                    px - pill_r,
                    py - pill_r,
                    stroke,
                    stroke,
                    color,
                    depth,
                    pill_r,
                    order,
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
    let _ = device_scale;
    let body = chrome.snippets.body_rect(origin_y, height);
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
        sugarloaf.rounded_rect(
            None,
            row.x,
            row.y,
            row.width,
            row.height,
            theme.panel_border,
            DEPTH_CONTENT,
            sidebar::CARD_RADIUS,
            ORDER_CONTENT,
        );
        sugarloaf.rounded_rect(
            None,
            row.x + BORDER_WIDTH,
            row.y + BORDER_WIDTH,
            row.width - 2.0 * BORDER_WIDTH,
            row.height - 2.0 * BORDER_WIDTH,
            bg,
            DEPTH_CONTENT + 0.01,
            sidebar::CARD_RADIUS - 1.0,
            ORDER_CONTENT,
        );
        if labels {
            draw_text(
                sugarloaf,
                row.x + 14.0,
                row.y + 14.0,
                &item.name,
                ROW_TITLE_SIZE,
                if hovered {
                    color_from_f32(theme.accent)
                } else {
                    theme.text
                },
                true,
            );
            // Run chip — mock bordered pill.
            let chip_w = 48.0;
            let chip_h = 18.0;
            let chip_x = row.right() - 14.0 - chip_w;
            let chip_y = row.y + 12.0;
            sugarloaf.rounded_rect(
                None,
                chip_x,
                chip_y,
                chip_w,
                chip_h,
                theme.panel_border,
                DEPTH_CONTENT + 0.02,
                8.0,
                ORDER_CONTENT,
            );
            sugarloaf.rounded_rect(
                None,
                chip_x + BORDER_WIDTH,
                chip_y + BORDER_WIDTH,
                chip_w - 2.0 * BORDER_WIDTH,
                chip_h - 2.0 * BORDER_WIDTH,
                theme.field_bg,
                DEPTH_CONTENT + 0.021,
                7.0,
                ORDER_CONTENT,
            );
            draw_text(
                sugarloaf,
                chip_x + 8.0,
                chip_y + 3.0,
                "Run ↵",
                HINT_SIZE,
                [0x22, 0xd3, 0xee, 255], // cyan-400
                false,
            );
            sugarloaf.rounded_rect(
                None,
                row.x + 12.0,
                row.y + 40.0,
                row.width - 24.0,
                36.0,
                theme.panel_border,
                DEPTH_CONTENT + 0.02,
                8.0,
                ORDER_CONTENT,
            );
            sugarloaf.rounded_rect(
                None,
                row.x + 12.0 + BORDER_WIDTH,
                row.y + 40.0 + BORDER_WIDTH,
                row.width - 24.0 - 2.0 * BORDER_WIDTH,
                36.0 - 2.0 * BORDER_WIDTH,
                theme.field_bg,
                DEPTH_CONTENT + 0.021,
                7.0,
                ORDER_CONTENT,
            );
            let cmd = elide(
                sugarloaf,
                &item.cmd,
                row.width - 48.0,
                &opts(HINT_SIZE, theme.text_muted, false),
            );
            draw_text(
                sugarloaf,
                row.x + 20.0,
                row.y + 50.0,
                &cmd,
                HINT_SIZE,
                theme.text_muted,
                false,
            );
        }
    }
}

fn render_settings_modal(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    window_width: f32,
    window_height: f32,
    device_scale: f32,
) {
    sugarloaf.rect(
        None,
        0.0,
        0.0,
        window_width,
        window_height,
        theme.scrim,
        DEPTH_DIALOG_BG,
        ORDER_DIALOG,
    );

    let dialog = chrome.settings.dialog_rect(window_width, window_height);
    sugarloaf.rounded_rect(
        None,
        dialog.x,
        dialog.y,
        dialog.width,
        dialog.height,
        theme.dialog_border,
        DEPTH_DIALOG,
        terminus_ui::settings::RADIUS,
        ORDER_DIALOG,
    );
    sugarloaf.rounded_rect(
        None,
        dialog.x + BORDER_WIDTH,
        dialog.y + BORDER_WIDTH,
        dialog.width - 2.0 * BORDER_WIDTH,
        dialog.height - 2.0 * BORDER_WIDTH,
        theme.dialog_bg,
        DEPTH_DIALOG + 0.01,
        terminus_ui::settings::RADIUS - 1.0,
        ORDER_DIALOG,
    );

    // Header band
    sugarloaf.rounded_rect(
        None,
        dialog.x + BORDER_WIDTH,
        dialog.y + BORDER_WIDTH,
        dialog.width - 2.0 * BORDER_WIDTH,
        64.0,
        theme.dialog_header,
        DEPTH_DIALOG + 0.02,
        terminus_ui::settings::RADIUS - 1.0,
        ORDER_DIALOG,
    );
    // Square off header bottom corners
    sugarloaf.rect(
        None,
        dialog.x + BORDER_WIDTH,
        dialog.y + 40.0,
        dialog.width - 2.0 * BORDER_WIDTH,
        28.0,
        theme.dialog_header,
        DEPTH_DIALOG + 0.021,
        ORDER_DIALOG,
    );

    let badge = Rect::new(dialog.x + 20.0, dialog.y + 16.0, 36.0, 36.0);
    // Accent ring (`border-appleAccent/20`) then soft fill.
    sugarloaf.rounded_rect(
        None,
        badge.x - BORDER_WIDTH,
        badge.y - BORDER_WIDTH,
        badge.width + 2.0 * BORDER_WIDTH,
        badge.height + 2.0 * BORDER_WIDTH,
        with_alpha(theme.accent, 0.20),
        DEPTH_DIALOG + 0.029,
        13.0,
        ORDER_DIALOG,
    );
    sugarloaf.rounded_rect(
        None,
        badge.x,
        badge.y,
        badge.width,
        badge.height,
        theme.accent_soft,
        DEPTH_DIALOG + 0.03,
        12.0,
        ORDER_DIALOG,
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
    sugarloaf.rounded_rect(
        None,
        close.x,
        close.y,
        close.width,
        close.height,
        theme.panel_border,
        DEPTH_DIALOG + 0.03,
        close.width * 0.5,
        ORDER_DIALOG,
    );
    sugarloaf.rounded_rect(
        None,
        close.x + BORDER_WIDTH,
        close.y + BORDER_WIDTH,
        close.width - 2.0 * BORDER_WIDTH,
        close.height - 2.0 * BORDER_WIDTH,
        theme.button_bg,
        DEPTH_DIALOG + 0.031,
        (close.width - 2.0) * 0.5,
        ORDER_DIALOG,
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
    sugarloaf.rect(
        None,
        side_x,
        side_y,
        terminus_ui::settings::SIDEBAR_WIDTH,
        side_h,
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
            sugarloaf.rounded_rect(
                None,
                rect.x,
                rect.y,
                rect.width,
                rect.height,
                theme.accent_soft,
                DEPTH_DIALOG + 0.03,
                12.0,
                ORDER_DIALOG,
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
    sugarloaf.rect(
        None,
        dialog.x + terminus_ui::settings::SIDEBAR_WIDTH,
        dialog.y + 66.0,
        dialog.width - terminus_ui::settings::SIDEBAR_WIDTH - BORDER_WIDTH,
        dialog.height - 66.0 - 48.0,
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
            let mut y = content_y + 48.0;
            let cta_w = dialog.right() - content_x - 24.0;
            let cta_h = 56.0;
            // Dashed New SSH Key CTA (same language as drawer New Host).
            sugarloaf.rounded_rect(
                None,
                content_x,
                y,
                cta_w,
                cta_h,
                with_alpha(theme.button_bg, 0.40),
                DEPTH_DIALOG + 0.03,
                12.0,
                ORDER_DIALOG,
            );
            draw_dashed_rounded_rect(
                sugarloaf,
                content_x,
                y,
                cta_w,
                cta_h,
                12.0,
                theme.panel_border,
                DEPTH_DIALOG + 0.031,
                ORDER_DIALOG,
            );
            let badge = Rect::new(content_x + 14.0, y + (cta_h - 32.0) / 2.0, 32.0, 32.0);
            sugarloaf.rounded_rect(
                None,
                badge.x,
                badge.y,
                badge.width,
                badge.height,
                theme.panel_border,
                DEPTH_DIALOG + 0.04,
                8.0,
                ORDER_DIALOG,
            );
            sugarloaf.rounded_rect(
                None,
                badge.x + BORDER_WIDTH,
                badge.y + BORDER_WIDTH,
                badge.width - 2.0 * BORDER_WIDTH,
                badge.height - 2.0 * BORDER_WIDTH,
                theme.field_bg,
                DEPTH_DIALOG + 0.041,
                7.0,
                ORDER_DIALOG,
            );
            draw_icon(
                sugarloaf,
                Icon::Plus,
                IconPlacement::new(badge.x + 8.5, badge.y + 8.5, 15.0),
                theme.accent,
                device_scale,
            );
            draw_text(
                sugarloaf,
                badge.right() + 12.0,
                y + 16.0,
                "New SSH Key",
                ROW_TITLE_SIZE,
                theme.text,
                true,
            );
            draw_text(
                sugarloaf,
                badge.right() + 12.0,
                y + 34.0,
                "Generate or import cryptographic identity",
                HINT_SIZE,
                theme.text_muted,
                false,
            );
            y += 68.0;
            for key in &chrome.settings.keys {
                sugarloaf.rounded_rect(
                    None,
                    content_x,
                    y,
                    cta_w,
                    56.0,
                    theme.panel_border,
                    DEPTH_DIALOG + 0.03,
                    12.0,
                    ORDER_DIALOG,
                );
                sugarloaf.rounded_rect(
                    None,
                    content_x + BORDER_WIDTH,
                    y + BORDER_WIDTH,
                    cta_w - 2.0 * BORDER_WIDTH,
                    56.0 - 2.0 * BORDER_WIDTH,
                    theme.button_bg,
                    DEPTH_DIALOG + 0.031,
                    11.0,
                    ORDER_DIALOG,
                );
                draw_icon(
                    sugarloaf,
                    Icon::KeyRound,
                    IconPlacement::new(content_x + 14.0, y + 18.0, 16.0),
                    theme.accent,
                    device_scale,
                );
                draw_text(
                    sugarloaf,
                    content_x + 40.0,
                    y + 14.0,
                    &key.name,
                    ROW_TITLE_SIZE,
                    theme.text,
                    true,
                );
                draw_text(
                    sugarloaf,
                    content_x + 40.0,
                    y + 32.0,
                    &key.fingerprint,
                    HINT_SIZE,
                    theme.text_muted,
                    false,
                );
                draw_text(
                    sugarloaf,
                    dialog.right() - 100.0,
                    y + 22.0,
                    "Delete",
                    ROW_SUB_SIZE,
                    theme.text_muted,
                    false,
                );
                y += 64.0;
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
                sugarloaf.rounded_rect(
                    None,
                    content_x + 168.0,
                    content_y - 2.0,
                    84.0,
                    20.0,
                    rgba_u8(0x10, 0xb9, 0x81, 0.10),
                    DEPTH_DIALOG + 0.03,
                    8.0,
                    ORDER_DIALOG,
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
            let card_w = engine_card.width;

            // Engine selector card
            paint_settings_field_card(
                sugarloaf,
                theme,
                engine_card,
                "Database Engine",
                chrome.settings.engine_label(),
                chrome.settings.engine_menu_open,
                false,
                true,
                true,
            );
            draw_text(
                sugarloaf,
                engine_card.right() - 28.0,
                engine_card.y + 36.0,
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
                false,
                uri_paint_text,
            );
            if uri_paint_text && uri_paint.show_caret {
                paint_settings_caret(sugarloaf, theme, uri_card, &uri_paint.text);
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
                false,
                true,
            );
            if pass_paint.show_caret {
                paint_settings_caret(sugarloaf, theme, pass_card, &pass_paint.text);
            }

            // Passphrase visibility toggle.
            let eye = chrome
                .settings
                .passphrase_toggle_rect(window_width, window_height);
            draw_text(
                sugarloaf,
                eye.x + 2.0,
                eye.y + 6.0,
                if chrome.settings.passphrase_visible {
                    "Hide"
                } else {
                    "Show"
                },
                HINT_SIZE,
                color_from_f32(theme.accent),
                false,
            );

            sugarloaf.rounded_rect(
                None,
                status_row.x,
                status_row.y,
                status_row.width,
                status_row.height,
                theme.button_bg,
                DEPTH_DIALOG + 0.03,
                12.0,
                ORDER_DIALOG,
            );
            let status_label = if chrome.settings.vault_unlocked
                && !chrome.settings.sync_status.to_ascii_lowercase().contains("vault")
            {
                format!("Vault unlocked · {}", chrome.settings.sync_status)
            } else {
                chrome.settings.sync_status.clone()
            };
            let status_shown = elide(
                sugarloaf,
                &status_label,
                card_w - 250.0,
                &opts(HINT_SIZE, theme.text_muted, false),
            );
            draw_text(
                sugarloaf,
                status_row.x + 16.0,
                status_row.y + 18.0,
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
            );
            paint_chrome_button(
                sugarloaf,
                theme,
                terminus_ui::ButtonSpec::primary(test),
                "Test Sync",
                ROW_SUB_SIZE,
            );

            // Engine dropdown: higher paint *order* than dialog cards.
            // (In sugarloaf, larger depth is further back — dialog BG is 0.2
            // while content is ~0.1 — so a bigger depth would go *behind*
            // the URI card. Order is what lifts the popover.)
            if chrome.settings.engine_menu_open {
                let menu = chrome
                    .settings
                    .engine_menu_rect(window_width, window_height);
                sugarloaf.rounded_rect(
                    None,
                    menu.x,
                    menu.y,
                    menu.width,
                    menu.height,
                    theme.panel_border,
                    DEPTH_DIALOG,
                    10.0,
                    ORDER_DIALOG_POPOVER,
                );
                sugarloaf.rounded_rect(
                    None,
                    menu.x + BORDER_WIDTH,
                    menu.y + BORDER_WIDTH,
                    menu.width - 2.0 * BORDER_WIDTH,
                    menu.height - 2.0 * BORDER_WIDTH,
                    theme.button_bg,
                    DEPTH_DIALOG + 0.001,
                    9.0,
                    ORDER_DIALOG_POPOVER,
                );
                for i in 0..terminus_ui::SQL_ENGINES.len() {
                    let opt = chrome
                        .settings
                        .engine_option_rect(window_width, window_height, i);
                    let selected = chrome.settings.sql_engine == i;
                    let hovered = chrome.settings.engine_menu_hover == Some(i);
                    if selected || hovered {
                        sugarloaf.rounded_rect(
                            None,
                            opt.x,
                            opt.y,
                            opt.width,
                            opt.height,
                            if hovered {
                                theme.item_hover
                            } else {
                                theme.accent_soft
                            },
                            DEPTH_DIALOG + 0.002,
                            6.0,
                            ORDER_DIALOG_POPOVER,
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
    sugarloaf.rect(
        None,
        dialog.x + BORDER_WIDTH,
        dialog.bottom() - 48.0,
        dialog.width - 2.0 * BORDER_WIDTH,
        47.0,
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

fn rgba_u8(r: u8, g: u8, b: u8, a: f32) -> [f32; 4] {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, a]
}

fn render_host_rows(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    origin_y: f32,
    height: f32,
    labels: bool,
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
                sugarloaf.rounded_rect(
                    None,
                    card.x + 1.0,
                    card.y + 1.0,
                    (card.width - 2.0).max(0.0),
                    (card.height - 2.0).max(0.0),
                    theme.item_hover,
                    DEPTH_CONTENT + 0.012,
                    (sidebar::CARD_RADIUS - 1.0).max(0.0),
                    ORDER_CONTENT,
                );
            }

            // Hairline under the folder header.
            if !collapsed {
                let sep_y = card.y + sidebar::ITEM_HEIGHT - 1.0;
                if sep_y >= top && sep_y <= bottom {
                    sugarloaf.rect(
                        None,
                        card.x + sidebar::CARD_PAD,
                        sep_y,
                        (card.width - 2.0 * sidebar::CARD_PAD).max(0.0),
                        1.0,
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
                // Folder badge with soft accent ring (mock).
                sugarloaf.rounded_rect(
                    None,
                    badge_x,
                    badge_y,
                    sidebar::HOST_BADGE_TILE,
                    sidebar::HOST_BADGE_TILE,
                    theme.accent,
                    DEPTH_CONTENT + 0.02,
                    8.0,
                    ORDER_CONTENT,
                );
                sugarloaf.rounded_rect(
                    None,
                    badge_x + BORDER_WIDTH,
                    badge_y + BORDER_WIDTH,
                    sidebar::HOST_BADGE_TILE - 2.0 * BORDER_WIDTH,
                    sidebar::HOST_BADGE_TILE - 2.0 * BORDER_WIDTH,
                    theme.field_bg,
                    DEPTH_CONTENT + 0.021,
                    7.0,
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
                draw_text(
                    sugarloaf,
                    text_x,
                    card.y + 12.0,
                    name,
                    ROW_TITLE_SIZE,
                    theme.text,
                    true,
                );
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
                sugarloaf.rounded_rect(
                    None,
                    card.x,
                    card.y,
                    card.width,
                    card.height,
                    if dragging_source {
                        with_alpha(theme.item_hover, 0.55)
                    } else {
                        theme.item_hover
                    },
                    DEPTH_CONTENT + 0.014,
                    sidebar::SESSION_RADIUS,
                    ORDER_CONTENT,
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
                        sugarloaf.rounded_rect(
                            None,
                            bar.x,
                            bar.y,
                            bar.width,
                            bar.height,
                            color,
                            DEPTH_CONTENT + 0.02,
                            1.0,
                            ORDER_CONNECTING,
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
    sugarloaf.rounded_rect(
        None,
        card.x,
        card.y,
        card.width,
        card.height,
        border,
        DEPTH_GHOST,
        sidebar::CARD_RADIUS,
        ORDER_GHOST,
    );
    sugarloaf.rounded_rect(
        None,
        card.x + BORDER_WIDTH,
        card.y + BORDER_WIDTH,
        (card.width - 2.0 * BORDER_WIDTH).max(0.0),
        (card.height - 2.0 * BORDER_WIDTH).max(0.0),
        fill,
        DEPTH_GHOST + 0.01,
        (sidebar::CARD_RADIUS - 1.0).max(0.0),
        ORDER_GHOST,
    );

    let badge_tile = sidebar::HOST_BADGE_TILE;
    let badge_x = card.x + sidebar::CARD_PAD;
    let badge_y = card.y + (card.height - badge_tile) * 0.5;
    sugarloaf.rounded_rect(
        None,
        badge_x,
        badge_y,
        badge_tile,
        badge_tile,
        with_alpha(theme.field_bg, OPACITY),
        DEPTH_GHOST + 0.02,
        8.0,
        ORDER_GHOST,
    );
    draw_icon(
        sugarloaf,
        Icon::Server,
        IconPlacement::new(
            badge_x + (badge_tile - sidebar::ICON_SIZE) * 0.5,
            badge_y + (badge_tile - sidebar::ICON_SIZE) * 0.5,
            sidebar::ICON_SIZE,
        ),
        with_alpha(theme.accent, OPACITY),
        device_scale,
    );
    let text_x = badge_x + badge_tile + sidebar::ICON_GAP;
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
    let fill = if bg[3] < 0.999 {
        // Composite over a dark card base so alpha washes stay soft, not neon.
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
            DEPTH_CONTENT,
            radius,
            ORDER_CONTENT,
        );
        sugarloaf.rounded_rect(
            None,
            card.x + BORDER_WIDTH,
            card.y + BORDER_WIDTH,
            (card.width - 2.0 * BORDER_WIDTH).max(0.0),
            (card.height - 2.0 * BORDER_WIDTH).max(0.0),
            fill,
            DEPTH_CONTENT + 0.01,
            (radius - 1.0).max(0.0),
            ORDER_CONTENT,
        );
    } else {
        sugarloaf.rounded_rect(
            None,
            card.x,
            card.y,
            card.width,
            card.height,
            fill,
            DEPTH_CONTENT,
            radius,
            ORDER_CONTENT,
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
    sugarloaf.rounded_rect(
        None,
        x,
        y,
        size,
        size,
        theme.field_bg,
        DEPTH_CONTENT + 0.02,
        HOST_BADGE_RADIUS,
        ORDER_CONTENT,
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
    sugarloaf.rounded_rect(
        None,
        x - 1.0,
        y - 1.0,
        d + 2.0,
        d + 2.0,
        [0.067, 0.067, 0.075, 1.0], // panel_bg
        DEPTH_CONTENT + 0.03,
        (d + 2.0) * 0.5,
        ORDER_CONTENT,
    );
    sugarloaf.rounded_rect(
        None,
        x,
        y,
        d,
        d,
        color,
        DEPTH_CONTENT + 0.031,
        d * 0.5,
        ORDER_CONTENT,
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
        sugarloaf.rounded_rect(
            None,
            button.x,
            button.y,
            button.width,
            button.height,
            with_alpha([1.0, 1.0, 1.0, 1.0], 0.06),
            DEPTH_CONTENT + 0.02,
            6.0,
            ORDER_CONTENT,
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
    sugarloaf.rounded_rect(
        None,
        form.x,
        form.y,
        form.width,
        form.height,
        theme.panel_border,
        DEPTH_CONTENT + 0.02,
        10.0,
        ORDER_CONTENT,
    );
    sugarloaf.rounded_rect(
        None,
        form.x + BORDER_WIDTH,
        form.y + BORDER_WIDTH,
        form.width - 2.0 * BORDER_WIDTH,
        form.height - 2.0 * BORDER_WIDTH,
        with_alpha([1.0, 1.0, 1.0, 1.0], 0.025),
        DEPTH_CONTENT + 0.021,
        9.0,
        ORDER_CONTENT,
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

    sugarloaf.rounded_rect(
        None,
        field.x,
        field.y,
        field.width,
        field.height,
        theme.field_bg,
        DEPTH_CONTENT + 0.03,
        8.0,
        ORDER_CONTENT,
    );
    if chrome.panel.new_group_focused {
        sugarloaf.rounded_rect(
            None,
            field.x,
            field.y,
            field.width,
            field.height,
            theme.field_border_focus,
            DEPTH_CONTENT + 0.031,
            8.0,
            ORDER_CONTENT,
        );
        sugarloaf.rounded_rect(
            None,
            field.x + BORDER_WIDTH,
            field.y + BORDER_WIDTH,
            field.width - 2.0 * BORDER_WIDTH,
            field.height - 2.0 * BORDER_WIDTH,
            theme.field_bg,
            DEPTH_CONTENT + 0.032,
            7.0,
            ORDER_CONTENT,
        );
    }

    sugarloaf.rounded_rect(
        None,
        create.x,
        create.y,
        create.width,
        create.height,
        theme.accent,
        DEPTH_CONTENT + 0.03,
        8.0,
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
        create.x + 10.0,
        create.y + (create.height - ROW_SUB_SIZE) * 0.5,
        "Create",
        ROW_SUB_SIZE,
        [255, 255, 255, 255],
        true,
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
    sugarloaf.rect(
        None,
        body.right() - TRACK - 2.0,
        body.y + travel * progress,
        TRACK,
        thumb_height,
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
    sugarloaf.rect(
        None,
        footer.x,
        footer.y,
        footer.width,
        BORDER_WIDTH,
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
    sugarloaf.rounded_rect(
        None,
        button.x,
        button.y,
        button.width,
        button.height,
        if hovered {
            theme.item_hover
        } else {
            theme.button_bg
        },
        DEPTH_CONTENT,
        5.0,
        ORDER_CONTENT,
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
) {
    // Dim everything behind the dialog. The scrim is a quad, so it is
    // painted over the grid and over the chrome's own backgrounds.
    sugarloaf.rect(
        None,
        0.0,
        0.0,
        window_width,
        window_height,
        theme.scrim,
        DEPTH_DIALOG,
        ORDER_DIALOG,
    );

    let form = &chrome.form;
    let layout = chrome.dialog_layout(window_width, window_height);
    let dialog = layout.rect(form.height());

    // Border first, then the body inset by the border width, which is
    // how a 1px outline is drawn with filled-only primitives.
    let radius = terminus_ui::add_host::DIALOG_RADIUS;
    sugarloaf.rounded_rect(
        None,
        dialog.x - BORDER_WIDTH,
        dialog.y - BORDER_WIDTH,
        dialog.width + 2.0 * BORDER_WIDTH,
        dialog.height + 2.0 * BORDER_WIDTH,
        theme.dialog_border,
        DEPTH_DIALOG_BG,
        radius,
        ORDER_DIALOG,
    );
    sugarloaf.rounded_rect(
        None,
        dialog.x,
        dialog.y,
        dialog.width,
        dialog.height,
        theme.dialog_bg,
        DEPTH_DIALOG_BG + 0.01,
        radius - 1.0,
        ORDER_DIALOG,
    );

    let title = layout.title_rect();
    draw_text(
        sugarloaf,
        title.x,
        title.y + 4.0,
        "Configure New Remote Host",
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

        // Mock inputs: bg-appleCard (#222226) + border.
        sugarloaf.rounded_rect(
            None,
            input.x - BORDER_WIDTH,
            input.y - BORDER_WIDTH,
            input.width + 2.0 * BORDER_WIDTH,
            input.height + 2.0 * BORDER_WIDTH,
            if focused {
                theme.field_border_focus
            } else {
                theme.field_border
            },
            DEPTH_DIALOG_BG + 0.015,
            input_radius,
            ORDER_DIALOG,
        );
        sugarloaf.rounded_rect(
            None,
            input.x,
            input.y,
            input.width,
            input.height,
            theme.button_bg,
            DEPTH_DIALOG_BG + 0.02,
            input_radius - 1.0,
            ORDER_DIALOG,
        );

        if let Some(caption) = layout.caption_rect(form, field) {
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
            }
            Field::Identity => {
                let label = form
                    .selected_identity_name()
                    .unwrap_or_else(|| field.placeholder());
                let color = if form.selected_identity_name().is_some() {
                    theme.text
                } else {
                    theme.text_placeholder
                };
                draw_text(sugarloaf, text_x, text_y, label, INPUT_SIZE, color, false);
            }
            Field::Password => {
                let masked: String = "•".repeat(form.password().chars().count());
                if masked.is_empty() && !focused {
                    draw_text(
                        sugarloaf,
                        text_x,
                        text_y,
                        field.placeholder(),
                        INPUT_SIZE,
                        theme.text_placeholder,
                        false,
                    );
                } else {
                    let caret = form.cursor(Field::Password);
                    let before: String = "•".repeat(caret);
                    let after: String = "•".repeat(masked.chars().count().saturating_sub(caret));
                    let drawn = sugarloaf.text_mut().draw(
                        text_x,
                        text_y,
                        &before,
                        &opts(INPUT_SIZE, theme.text, false),
                    );
                    if focused {
                        sugarloaf.rect(
                            None,
                            text_x + drawn,
                            input.y + 6.0,
                            CARET_WIDTH,
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
                    sugarloaf.rect(
                        None,
                        text_x + drawn,
                        input.y + 6.0,
                        CARET_WIDTH,
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
    sugarloaf.rect(
        None,
        dialog.x + terminus_ui::add_host::PAD,
        hint.y - 8.0,
        dialog.width - 2.0 * terminus_ui::add_host::PAD,
        BORDER_WIDTH,
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

    // Cancel + Connect footer — geometry owned by AddHostLayout.
    let cancel = layout.cancel_button_rect(form.height());
    let connect = layout.connect_button_rect(form.height());

    sugarloaf.rounded_rect(
        None,
        cancel.x,
        cancel.y,
        cancel.width,
        cancel.height,
        theme.panel_border,
        DEPTH_DIALOG_BG + 0.025,
        input_radius,
        ORDER_DIALOG,
    );
    sugarloaf.rounded_rect(
        None,
        cancel.x + BORDER_WIDTH,
        cancel.y + BORDER_WIDTH,
        cancel.width - 2.0 * BORDER_WIDTH,
        cancel.height - 2.0 * BORDER_WIDTH,
        theme.button_bg,
        DEPTH_DIALOG_BG + 0.026,
        input_radius - 1.0,
        ORDER_DIALOG,
    );
    draw_text(
        sugarloaf,
        cancel.x + 18.0,
        cancel.y + 8.0,
        "Cancel",
        HINT_SIZE,
        theme.text_muted,
        false,
    );

    sugarloaf.rounded_rect(
        None,
        connect.x,
        connect.y,
        connect.width,
        connect.height,
        theme.accent,
        DEPTH_DIALOG_BG + 0.025,
        input_radius,
        ORDER_DIALOG,
    );
    draw_text(
        sugarloaf,
        connect.x + 18.0,
        connect.y + 8.0,
        "Connect",
        HINT_SIZE,
        [255, 255, 255, 255],
        true,
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
    _selector: bool,
    paint_text: bool,
) {
    sugarloaf.rounded_rect(
        None,
        card.x,
        card.y,
        card.width,
        card.height,
        theme.button_bg,
        DEPTH_DIALOG + 0.03,
        12.0,
        ORDER_DIALOG,
    );
    if paint_text {
        draw_text(
            sugarloaf,
            card.x + 16.0,
            card.y + 12.0,
            label,
            HINT_SIZE,
            theme.text_muted,
            false,
        );
    }
    let field_bg = theme.field_bg;
    let input_x = card.x + 16.0;
    let input_y = card.y + 30.0;
    let input_w = card.width - 32.0;
    let input_h = 26.0;
    if focused {
        sugarloaf.rounded_rect(
            None,
            input_x,
            input_y,
            input_w,
            input_h,
            theme.field_border_focus,
            DEPTH_DIALOG + 0.039,
            8.0,
            ORDER_DIALOG,
        );
        sugarloaf.rounded_rect(
            None,
            input_x + 1.0,
            input_y + 1.0,
            input_w - 2.0,
            input_h - 2.0,
            field_bg,
            DEPTH_DIALOG + 0.041,
            7.0,
            ORDER_DIALOG,
        );
    } else {
        sugarloaf.rounded_rect(
            None,
            input_x,
            input_y,
            input_w,
            input_h,
            field_bg,
            DEPTH_DIALOG + 0.04,
            8.0,
            ORDER_DIALOG,
        );
    }
    if paint_text {
        let color = if placeholder {
            theme.text_placeholder
        } else {
            theme.text
        };
        let shown = elide(
            sugarloaf,
            value,
            input_w - 16.0,
            &opts(ROW_SUB_SIZE, color, false),
        );
        draw_text(
            sugarloaf,
            card.x + 24.0,
            card.y + 36.0,
            &shown,
            ROW_SUB_SIZE,
            color,
            false,
        );
    }
}

fn paint_settings_caret(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    card: Rect,
    value: &str,
) {
    let text_x = card.x + 24.0;
    let advance = sugarloaf
        .text_mut()
        .measure(value, &opts(ROW_SUB_SIZE, theme.text, false));
    sugarloaf.rect(
        None,
        text_x + advance,
        card.y + 34.0,
        CARET_WIDTH,
        16.0,
        theme.accent,
        DEPTH_DIALOG + 0.05,
        ORDER_DIALOG,
    );
}

/// Paint a chrome button using [`terminus_ui::ButtonSpec`] centering math.
fn paint_chrome_button(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    spec: terminus_ui::ButtonSpec,
    label: &str,
    font_size: f32,
) {
    let rect = spec.rect;
    if spec.has_border() {
        sugarloaf.rounded_rect(
            None,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            theme.panel_border,
            DEPTH_DIALOG + 0.04,
            spec.radius,
            ORDER_DIALOG,
        );
        sugarloaf.rounded_rect(
            None,
            rect.x + BORDER_WIDTH,
            rect.y + BORDER_WIDTH,
            rect.width - 2.0 * BORDER_WIDTH,
            rect.height - 2.0 * BORDER_WIDTH,
            spec.fill(theme.accent, theme.button_bg),
            DEPTH_DIALOG + 0.041,
            (spec.radius - 1.0).max(0.0),
            ORDER_DIALOG,
        );
    } else {
        sugarloaf.rounded_rect(
            None,
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            spec.fill(theme.accent, theme.button_bg),
            DEPTH_DIALOG + 0.04,
            spec.radius,
            ORDER_DIALOG,
        );
    }
    let color = spec.label_color([255, 255, 255, 255], theme.text);
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
) {
    let Some(conn) = chrome.connection.as_ref() else {
        return;
    };

    // Full-window scrim — same stack as the add-host dialog.
    let mut scrim = theme.scrim;
    scrim[3] = scrim[3].max(0.55).min(0.70);
    sugarloaf.rect(
        None,
        0.0,
        0.0,
        window_width,
        window_height,
        scrim,
        DEPTH_DIALOG,
        ORDER_DIALOG,
    );

    let dialog = conn.dialog_rect(window_width, window_height);
    sugarloaf.rounded_rect(
        None,
        dialog.x - BORDER_WIDTH,
        dialog.y - BORDER_WIDTH,
        dialog.width + 2.0 * BORDER_WIDTH,
        dialog.height + 2.0 * BORDER_WIDTH,
        theme.dialog_border,
        DEPTH_DIALOG_BG,
        13.0,
        ORDER_DIALOG,
    );
    sugarloaf.rounded_rect(
        None,
        dialog.x,
        dialog.y,
        dialog.width,
        dialog.height,
        theme.dialog_bg,
        DEPTH_DIALOG_BG + 0.01,
        12.0,
        ORDER_DIALOG,
    );

    // Header: host badge + title + endpoint + Show logs.
    let header_icon = conn.header_icon_rect(dialog);
    sugarloaf.rounded_rect(
        None,
        header_icon.x,
        header_icon.y,
        header_icon.width,
        header_icon.height,
        with_alpha(theme.accent, 0.22),
        DEPTH_DIALOG_BG + 0.02,
        10.0,
        ORDER_DIALOG,
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
    sugarloaf.rounded_rect(
        None,
        logs_btn.x,
        logs_btn.y,
        logs_btn.width,
        logs_btn.height,
        theme.button_bg,
        DEPTH_DIALOG_BG + 0.02,
        8.0,
        ORDER_DIALOG,
    );
    let logs_label = if conn.logs_open {
        "Hide logs"
    } else {
        "Show logs"
    };
    let logs_w = sugarloaf
        .text_mut()
        .measure(logs_label, &opts(HINT_SIZE, theme.text_muted, false));
    draw_text(
        sugarloaf,
        logs_btn.x + (logs_btn.width - logs_w) * 0.5,
        logs_btn.y + 8.0,
        logs_label,
        HINT_SIZE,
        theme.text_muted,
        false,
    );

    // Progress track.
    let track = conn.track_line_rect(dialog);
    sugarloaf.rounded_rect(
        None,
        track.x,
        track.y,
        track.width,
        track.height,
        theme.panel_border,
        DEPTH_DIALOG_BG + 0.02,
        2.0,
        ORDER_DIALOG,
    );
    let fill = conn.progress_fill_rect(dialog);
    if fill.width > 0.5 {
        sugarloaf.rounded_rect(
            None,
            fill.x,
            fill.y,
            fill.width,
            fill.height,
            theme.accent,
            DEPTH_DIALOG_BG + 0.03,
            2.0,
            ORDER_DIALOG,
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
            sugarloaf.rounded_rect(
                None,
                node.x - glow,
                node.y - glow,
                node.width + 2.0 * glow,
                node.height + 2.0 * glow,
                with_alpha(theme.accent, 0.18 + 0.16 * pulse),
                DEPTH_DIALOG_BG + 0.035,
                (node.width + 2.0 * glow) * 0.5,
                ORDER_DIALOG,
            );
        }

        sugarloaf.rounded_rect(
            None,
            node.x - 1.5,
            node.y - 1.5,
            node.width + 3.0,
            node.height + 3.0,
            border_c,
            DEPTH_DIALOG_BG + 0.04,
            (node.width + 3.0) * 0.5,
            ORDER_DIALOG,
        );
        sugarloaf.rounded_rect(
            None,
            node.x,
            node.y,
            node.width,
            node.height,
            fill_c,
            DEPTH_DIALOG_BG + 0.05,
            node.width * 0.5,
            ORDER_DIALOG,
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
        let lw = sugarloaf
            .text_mut()
            .measure(label, &opts(10.0, label_color, false));
        draw_text(
            sugarloaf,
            node.x + (node.width - lw) * 0.5,
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
        sugarloaf.rounded_rect(
            None,
            logs.x,
            logs.y,
            logs.width,
            logs.height,
            theme.field_bg,
            DEPTH_DIALOG_BG + 0.02,
            8.0,
            ORDER_DIALOG,
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
    sugarloaf.rounded_rect(
        None,
        close.x,
        close.y,
        close.width,
        close.height,
        theme.button_bg,
        DEPTH_DIALOG_BG + 0.02,
        10.0,
        ORDER_DIALOG,
    );
    let close_label = "Close";
    let cw = sugarloaf
        .text_mut()
        .measure(close_label, &opts(HINT_SIZE, theme.text, false));
    draw_text(
        sugarloaf,
        close.x + (close.width - cw) * 0.5,
        close.y + 11.0,
        close_label,
        HINT_SIZE,
        theme.text,
        false,
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
    sugarloaf.rounded_rect(
        None,
        ring.x - ring.radius,
        ring.y - ring.radius,
        diam,
        diam,
        ring_color,
        DEPTH_CONTENT + 0.05,
        ring.radius,
        ORDER_CONNECTING,
    );

    for dot in orbit_dots(cx, cy, orbit_r, dot_r, phase) {
        let color = with_alpha(theme.accent, dot.alpha);
        let d = dot.radius * 2.0;
        sugarloaf.rounded_rect(
            None,
            dot.x - dot.radius,
            dot.y - dot.radius,
            d,
            d,
            color,
            DEPTH_CONTENT + 0.06,
            dot.radius,
            ORDER_CONNECTING,
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

/// Soft left→right status wash inside a host card.
///
/// Retired from the live host chrome (status dots replaced washes). Kept
/// for reference / potential settings accent surfaces.
#[allow(dead_code)]
fn paint_status_wash(
    sugarloaf: &mut Sugarloaf,
    card: &Rect,
    color: [f32; 4],
    device_scale: f32,
) {
    let inset = BORDER_WIDTH;
    let x0 = card.x + inset;
    let y0 = card.y + inset;
    let w = (card.width - 2.0 * inset).max(0.0);
    let h = (card.height - 2.0 * inset).max(0.0);
    if w < 8.0 || h < 8.0 {
        return;
    }

    // Match the inner fill radius used by the host card chrome.
    let radius = (sidebar::CARD_RADIUS - 1.0).clamp(0.0, h * 0.5).min(w * 0.5);
    let scale = device_scale.max(1.0);
    let wash_w = (w * 0.40).max(24.0).min(w);
    let cols = (wash_w * scale).round().max(1.0) as usize;
    let ramp = status_wash_alpha_ramp(cols);
    let col_w = wash_w / cols as f32;

    for (i, &alpha) in ramp.iter().enumerate() {
        // Peak opacity reduced ~20% vs the baked ramp.
        let alpha = alpha * 0.35;
        if alpha < 0.008 {
            continue;
        }
        // i = 0 is the leftmost column (strongest in the ramp).
        let x = x0 + col_w * i as f32;
        let x_mid = x + col_w * 0.5;
        let (top, bottom) = rounded_rect_column_y(x0, y0, w, h, radius, x_mid);
        let col_h = bottom - top;
        if col_h < 0.5 {
            continue;
        }
        sugarloaf.rect(
            None,
            x,
            top,
            col_w + 0.35,
            col_h,
            with_alpha(color, alpha),
            DEPTH_CONTENT + 0.015,
            ORDER_CONTENT,
        );
    }
}

/// Vertical span of a rounded-rect interior at horizontal position `x_mid`.
///
/// Returns `(top, bottom)` clipped to the circular corners so a column fill
/// stays inside the card's radius.
fn rounded_rect_column_y(
    x0: f32,
    y0: f32,
    w: f32,
    h: f32,
    radius: f32,
    x_mid: f32,
) -> (f32, f32) {
    let mut top = y0;
    let mut bottom = y0 + h;
    if radius <= 0.5 {
        return (top, bottom);
    }

    let left = x0 + radius;
    let right = x0 + w - radius;
    if x_mid < left {
        let dx = (left - x_mid).min(radius);
        let dy = radius - (radius * radius - dx * dx).max(0.0).sqrt();
        top += dy;
        bottom -= dy;
    } else if x_mid > right {
        let dx = (x_mid - right).min(radius);
        let dy = radius - (radius * radius - dx * dx).max(0.0).sqrt();
        top += dy;
        bottom -= dy;
    }
    (top, bottom.max(top))
}

/// Horizontal alpha ramp via tiny-skia's linear gradient shader.
///
/// Index 0 = left edge (peak alpha), last = right of the wash (transparent).
fn status_wash_alpha_ramp(cols: usize) -> Vec<f32> {
    let w = cols.max(1) as u32;
    let Some(mut pixmap) = tiny_skia::Pixmap::new(w, 1) else {
        return vec![0.0; cols.max(1)];
    };
    let mut paint = tiny_skia::Paint::default();
    paint.anti_alias = false;
    // Gradient runs left → right: opaque at x=0, clear at x=w.
    paint.shader = tiny_skia::LinearGradient::new(
        tiny_skia::Point::from_xy(0.0, 0.5),
        tiny_skia::Point::from_xy(w as f32, 0.5),
        vec![
            tiny_skia::GradientStop::new(0.0, tiny_skia::Color::from_rgba8(255, 255, 255, 72)),
            tiny_skia::GradientStop::new(0.55, tiny_skia::Color::from_rgba8(255, 255, 255, 28)),
            tiny_skia::GradientStop::new(1.0, tiny_skia::Color::from_rgba8(255, 255, 255, 0)),
        ],
        tiny_skia::SpreadMode::Pad,
        tiny_skia::Transform::identity(),
    )
    .unwrap_or(tiny_skia::Shader::SolidColor(tiny_skia::Color::TRANSPARENT));

    if let Some(rect) = tiny_skia::Rect::from_xywh(0.0, 0.0, w as f32, 1.0) {
        let path = tiny_skia::PathBuilder::from_rect(rect);
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            tiny_skia::Transform::identity(),
            None,
        );
    }

    let mut out = Vec::with_capacity(cols.max(1));
    for x in 0..w {
        // Premultiplied RGBA — alpha is in the last channel.
        let px = pixmap.pixel(x, 0).unwrap_or(tiny_skia::PremultipliedColorU8::TRANSPARENT);
        out.push(px.alpha() as f32 / 255.0);
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

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
