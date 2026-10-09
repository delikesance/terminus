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

use terminus_ui::chrome::Chrome;
use terminus_ui::components::button::{ButtonKind, ButtonSize, ButtonState};
use terminus_ui::geom::Rect;
use terminus_ui::icons::{Cmd, Icon, IconPlacement, LUCIDE_STROKE};
use terminus_ui::loading::{breath_ring, orbit_dots};
use terminus_ui::os_icons::OsGlyph;
use terminus_ui::sidebar;
use terminus_ui::theme::ChromeTheme;

use super::components::button::paint_button_in_rect;
use super::components::Layer;

/// Chrome paint orders. The grid is 3 and the overlays are 20.
const ORDER_CONTENT: u8 = 7;
const ORDER_CONNECTING: u8 = 8;
const ORDER_DIALOG: u8 = 30;
/// Popovers inside the settings dialog (engine dropdown, …) — above dialog cards.
const ORDER_DIALOG_POPOVER: u8 = 31;
/// Host-drag phantom — above dialogs, rail, sticky header, everything.
const ORDER_GHOST: u8 = 50;

#[allow(dead_code)]
const DEPTH_BG: f32 = 0.05;
const DEPTH_CONTENT: f32 = 0.06;
/// Sticky drawer header (title + search) painted after the scrollable list
/// so host cards slide underneath instead of over it.
#[allow(dead_code)]
const DEPTH_STICKY: f32 = 0.085;
const DEPTH_DIALOG: f32 = 0.1;
const DEPTH_DIALOG_BG: f32 = 0.2;
const DEPTH_GHOST: f32 = 0.35;

const ADD_ICON_SIZE: f32 = 15.0;

// Whole-pixel sizes on purpose: the atlas rasterises each size bucket
// separately and the glyph quads are whole pixels wide, so an integer
// size plus an integer origin samples 1:1. A half-pixel size (10.5,
// 12.5) only ever renders as a blurrier 10 or 13.
const TITLE_SIZE: f32 = 12.0;
const ROW_TITLE_SIZE: f32 = 13.0;
/// Secondary labels / endpoints — one step smaller than the title.
const ROW_SUB_SIZE: f32 = 11.0;
#[allow(dead_code)]
const DIALOG_TITLE_SIZE: f32 = 14.0;
#[allow(dead_code)]
const CAPTION_SIZE: f32 = 11.0;
/// Mock inputs are `text-xs` (12px).
#[allow(dead_code)]
const INPUT_SIZE: f32 = 12.0;
const HINT_SIZE: f32 = 11.0;

const BORDER_WIDTH: f32 = 1.0;
#[allow(dead_code)]
const INPUT_PAD_X: f32 = 10.0;
const CARET_WIDTH: f32 = 1.5;

/// Paint the whole chrome for one frame.
///
/// `window_width` / `window_height` are logical pixels (physical size
/// divided by the scale factor), matching `terminus_ui`'s geometry;
/// `device_scale` is that same factor, needed to rasterize icons into
/// masks that land 1:1 on the device grid.
pub fn render(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    window_width: f32,
    window_height: f32,
    device_scale: f32,
) {
    // Register Sora / Martian Mono (once per library) so every `opts()`
    // below resolves to the UI faces.
    super::ui_text::sync_ui_fonts(sugarloaf);
    // The sidebar itself is painted by `renderer::shell` (under the grid
    // decorations); here only what floats above it.
    paint_modal_stack(
        sugarloaf,
        chrome,
        theme,
        window_width,
        window_height,
        device_scale,
    );

    if let Some(menu) = chrome.context_menu.as_ref() {
        sugarloaf.begin_overlay();
        super::components::overlay::paint_menu(sugarloaf, theme, menu);
        sugarloaf.end_overlay();
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
            terminus_ui::ModalPaintLayer::HostEditor => {
                crate::renderer::screens::add_server::render(
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
                super::dialogs::snippet::paint_add_snippet(
                    sugarloaf,
                    chrome,
                    theme,
                    (window_width, window_height),
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
            terminus_ui::ModalPaintLayer::Confirm => {
                super::dialogs::confirm::paint_chrome_confirm(
                    sugarloaf,
                    chrome,
                    theme,
                    (window_width, window_height),
                    paint_glyphs,
                );
            }
            terminus_ui::ModalPaintLayer::VaultUnlock => {
                super::dialogs::vault::paint_vault_unlock(
                    sugarloaf,
                    chrome,
                    theme,
                    (window_width, window_height),
                    paint_glyphs,
                );
            }
        }
    }
    sugarloaf.end_overlay();
}

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
        &Rect::new(side_x, side_y, terminus_ui::settings::SIDEBAR_WIDTH, side_h),
        theme.dialog_header,
        DEPTH_DIALOG + 0.02,
        ORDER_DIALOG,
    );

    for tab in [
        terminus_ui::SettingsTab::Keys,
        terminus_ui::SettingsTab::SqlSync,
    ] {
        let rect = chrome.settings.tab_rect(window_width, window_height, tab);
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

            if let Some(draft) =
                chrome.settings.key_draft_rect(window_width, window_height)
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
                    paint_field_card(
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
                            caret_prefix: String::new(),
                            selection: None,
                        }
                    } else {
                        terminus_ui::FieldPaint::from_draft(
                            &chrome.settings.key_pem,
                            "Paste a private key or its path, e.g. ~/.ssh/id_ed25519",
                            focused,
                        )
                    };
                    paint_field_card(
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
                if let Some(pass_card) = chrome
                    .settings
                    .key_draft_passphrase_rect(window_width, window_height)
                {
                    let focused = chrome.settings.key_draft_passphrase_focused;
                    let value = &chrome.settings.key_passphrase.value;
                    let paint = if value.is_empty() {
                        terminus_ui::FieldPaint::from_draft(
                            &chrome.settings.key_passphrase,
                            "Key passphrase (only for an encrypted key)",
                            focused,
                        )
                    } else {
                        let masked = "•".repeat(value.chars().count());
                        terminus_ui::FieldPaint {
                            caret_prefix: masked.clone(),
                            text: masked,
                            placeholder: false,
                            show_caret: focused,
                            selection: None,
                        }
                    };
                    paint_field_card(
                        sugarloaf,
                        theme,
                        pass_card,
                        "Passphrase",
                        &paint.text,
                        focused,
                        paint.placeholder,
                        0.0,
                        true,
                    );
                    if paint.show_caret {
                        let masked_prefix = "•".repeat(
                            chrome
                                .settings
                                .key_passphrase
                                .prefix_display()
                                .chars()
                                .count(),
                        );
                        paint_field_caret_prefix(
                            sugarloaf,
                            theme,
                            pass_card,
                            &masked_prefix,
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
                    paint_button_in_rect(
                        sugarloaf,
                        theme,
                        &gen,
                        ButtonKind::Primary,
                        ButtonSize::Small,
                        ButtonState::Default,
                        gen_label,
                        Layer {
                            order: ORDER_DIALOG,
                            depth: DEPTH_DIALOG + 0.04,
                            backdrop: theme.dialog,
                        },
                    );
                }
                if let Some(cancel) = chrome
                    .settings
                    .key_draft_cancel_rect(window_width, window_height)
                {
                    paint_button_in_rect(
                        sugarloaf,
                        theme,
                        &cancel,
                        ButtonKind::Secondary,
                        ButtonSize::Small,
                        ButtonState::Default,
                        "Cancel",
                        Layer {
                            order: ORDER_DIALOG,
                            depth: DEPTH_DIALOG + 0.04,
                            backdrop: theme.dialog,
                        },
                    );
                }
                if let Some(banner) = chrome
                    .settings
                    .key_draft_error_banner_rect(window_width, window_height)
                {
                    let err_text =
                        chrome.settings.key_draft_error.as_deref().unwrap_or("");
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
                let row =
                    chrome
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
                let del =
                    chrome
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
                let copy =
                    chrome
                        .settings
                        .key_copy_rect(window_width, window_height, index);
                let has_public = !key.public_key.is_empty();
                let fp_right = if has_public { copy.x } else { del.x };
                let fp_max = (fp_right - text_x - 12.0).max(40.0);
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
                if has_public {
                    paint_surface(
                        sugarloaf,
                        &copy,
                        theme.panel_bg,
                        Some(theme.panel_border),
                        8.0,
                        DEPTH_DIALOG + 0.04,
                        ORDER_DIALOG,
                        false,
                    );
                    let label = "Copy public key";
                    let label_w = sugarloaf
                        .text_mut()
                        .measure(label, &opts(ROW_SUB_SIZE, theme.text, false));
                    draw_text(
                        sugarloaf,
                        copy.x + (copy.width - label_w) * 0.5,
                        copy.y + (copy.height - ROW_SUB_SIZE) * 0.5,
                        label,
                        ROW_SUB_SIZE,
                        theme.text,
                        false,
                    );
                }
                // Delete only appears while the row is hovered (mock:
                // opacity-0 group-hover:opacity-100). Turns red when the
                // pointer is on the control itself.
                if chrome.settings.key_row_hover == Some(index) {
                    let del_color = if chrome.settings.key_delete_hover == Some(index) {
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
            if let (Some(notice), Some(last)) = (
                chrome.settings.keys_notice.as_deref(),
                chrome.settings.keys.len().checked_sub(1),
            ) {
                let row = chrome
                    .settings
                    .key_row_rect(window_width, window_height, last);
                let color = color_from_f32(theme.success);
                let text_opts = opts(HINT_SIZE, color, false);
                let lines = wrap_lines(sugarloaf, notice, row.width, &text_opts, 2);
                let mut y = row.bottom() + 12.0;
                for line in lines {
                    draw_text(sugarloaf, row.x, y, &line, HINT_SIZE, color, false);
                    y += HINT_SIZE + 4.0;
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
                    with_alpha(theme.success, 0.12),
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
                    terminus_ui::theme::text_color(theme.success),
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

            let engine_card = chrome
                .settings
                .engine_card_rect(window_width, window_height);
            let uri_card = chrome.settings.uri_card_rect(window_width, window_height);
            let pass_card = chrome
                .settings
                .passphrase_card_rect(window_width, window_height);
            let status_row = chrome.settings.status_row_rect(window_width, window_height);

            // Engine selector card
            paint_field_card(
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
            let engine_input = terminus_ui::settings::field_input_in_card(engine_card);
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
            paint_field_card(
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
            if uri_paint_text {
                paint_field_selection(
                    sugarloaf,
                    theme,
                    uri_card,
                    &uri_paint,
                    0.0,
                    DEPTH_DIALOG + 0.045,
                    ORDER_DIALOG,
                );
                if uri_paint.show_caret {
                    paint_field_caret_prefix(
                        sugarloaf,
                        theme,
                        uri_card,
                        &uri_paint.caret_prefix,
                        0.0,
                    );
                }
            }

            // Passphrase field
            let pass_paint = chrome.settings.passphrase_field_paint();
            paint_field_card(
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
            paint_field_selection(
                sugarloaf,
                theme,
                pass_card,
                &pass_paint,
                terminus_ui::settings::FIELD_EYE_SLOT,
                DEPTH_DIALOG + 0.045,
                ORDER_DIALOG,
            );
            if pass_paint.show_caret {
                paint_field_caret_prefix(
                    sugarloaf,
                    theme,
                    pass_card,
                    &pass_paint.caret_prefix,
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
                && !chrome
                    .settings
                    .sync_status
                    .to_ascii_lowercase()
                    .contains("vault")
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

            paint_button_in_rect(
                sugarloaf,
                theme,
                &unlock,
                ButtonKind::Secondary,
                ButtonSize::Small,
                ButtonState::Default,
                "Unlock Vault",
                Layer {
                    order: ORDER_DIALOG,
                    depth: DEPTH_DIALOG + 0.04,
                    backdrop: theme.dialog,
                },
            );
            paint_button_in_rect(
                sugarloaf,
                theme,
                &test,
                ButtonKind::Primary,
                ButtonSize::Small,
                ButtonState::Default,
                "Test Sync",
                Layer {
                    order: ORDER_DIALOG,
                    depth: DEPTH_DIALOG + 0.04,
                    backdrop: theme.dialog,
                },
            );
            if let Some(forget) = chrome
                .settings
                .forget_passphrase_button_rect(window_width, window_height)
            {
                paint_button_in_rect(
                    sugarloaf,
                    theme,
                    &forget,
                    ButtonKind::Secondary,
                    ButtonSize::Small,
                    ButtonState::Default,
                    "Forget passphrase",
                    Layer {
                        order: ORDER_DIALOG,
                        depth: DEPTH_DIALOG + 0.04,
                        backdrop: theme.dialog,
                    },
                );
            }

            if let Some(banner) = chrome
                .settings
                .error_banner_rect(window_width, window_height)
            {
                let err_text = chrome.settings.sync_error.as_deref().unwrap_or("");
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
                    let opt = chrome.settings.engine_option_rect(
                        window_width,
                        window_height,
                        i,
                    );
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
    paint_button_in_rect(
        sugarloaf,
        theme,
        &done,
        ButtonKind::Secondary,
        ButtonSize::Small,
        ButtonState::Default,
        "Done",
        Layer {
            order: ORDER_DIALOG,
            depth: DEPTH_DIALOG + 0.04,
            backdrop: theme.dialog,
        },
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
#[allow(dead_code)]
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
    if let Some(menu) = layout.menu_rect(&chrome.form) {
        cover = rect_union(cover, menu);
    }
    Some(cover)
}

#[allow(dead_code)]
fn rect_union(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.min(b.x);
    let y0 = a.y.min(b.y);
    let x1 = a.right().max(b.right());
    let y1 = a.bottom().max(b.bottom());
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}

/// Accent insertion bar showing where a dragged host/group will land.
pub(crate) fn paint_host_drag_insertion_bar(
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
    let wash = Rect::new(bar.x, bar.y - 3.0, bar.width, bar.height + 6.0);
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
    #[allow(dead_code)]
    Outward,
}

/// Full-window scrim + bordered dialog panel.
#[allow(clippy::too_many_arguments)]
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
#[allow(dead_code)]
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
#[allow(clippy::too_many_arguments)]
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
#[allow(clippy::too_many_arguments)]
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
#[allow(dead_code)]
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

/// Shared settings field card (label + input row).
///
/// `trailing_slot` reserves space on the right of the input for an
/// adornment (e.g. passphrase eye) so value text never overlaps it.
///
/// When `paint_text` is false, only the card/input quads are drawn,
/// used while a popover covers the card so UI text (always last pass)
/// does not bleed through the menu.
#[allow(clippy::too_many_arguments)]
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
    let (depth, order) = (DEPTH_DIALOG, ORDER_DIALOG);
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

/// Accent wash behind the selected span of a field card. Text and
/// selection come from the shared field paint model, so a masked field
/// highlights the same character positions it displays.
pub(crate) fn paint_field_selection(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    card: Rect,
    paint: &terminus_ui::FieldPaint,
    trailing_slot: f32,
    depth: f32,
    order: u8,
) {
    let Some((start, end)) = paint.selection else {
        return;
    };
    if end <= start || paint.placeholder {
        return;
    }
    let input = terminus_ui::settings::field_input_in_card(card);
    let text_x = input.x + terminus_ui::settings::FIELD_TEXT_INSET;
    let text_budget = (input.width
        - terminus_ui::settings::FIELD_TEXT_INSET
        - trailing_slot.max(terminus_ui::settings::FIELD_TEXT_INSET))
    .max(0.0);
    if text_budget <= 0.0 {
        return;
    }
    let text_opts = opts(ROW_SUB_SIZE, theme.text, false);
    let before: String = paint.text.chars().take(start).collect();
    let through: String = paint.text.chars().take(end).collect();
    let before_shown = elide(sugarloaf, &before, text_budget, &text_opts);
    let through_shown = elide(sugarloaf, &through, text_budget, &text_opts);
    let start_x = sugarloaf.text_mut().measure(&before_shown, &text_opts);
    let end_x = sugarloaf.text_mut().measure(&through_shown, &text_opts);
    let x = (text_x + start_x).min(text_x + text_budget);
    let right = (text_x + end_x).min(text_x + text_budget);
    paint_flat(
        sugarloaf,
        &Rect::new(
            x,
            input.y + 6.0,
            (right - x).max(CARET_WIDTH),
            (input.height - 12.0).max(1.0),
        ),
        with_alpha(theme.accent, 0.35),
        depth,
        order,
    );
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
pub(crate) fn paint_rename_text(
    sugarloaf: &mut Sugarloaf,
    draft: &terminus_ui::TextDraft,
    text_x: f32,
    text_y: f32,
    theme: &ChromeTheme,
) {
    let accent = color_from_f32(theme.accent);
    let title_opts = opts(ROW_TITLE_SIZE, accent, true);
    if let Some((start, end)) = draft.selection_range() {
        let before: String = draft.value.chars().take(start).collect();
        let selected: String =
            draft.value.chars().skip(start).take(end - start).collect();
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
        &draft.value,
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

/// Apple HIG title-bar strip + bottom hairline (island / context bar).
pub(crate) fn paint_title_strip(sugarloaf: &mut Sugarloaf, logical_w: f32, height: f32) {
    let theme = ChromeTheme::default();
    let strip = theme.frame;
    let strip_border = theme.divider;
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
#[allow(clippy::too_many_arguments)]
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
#[allow(clippy::too_many_arguments)]
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
    paint_surface(sugarloaf, cta, bg, None, radius, depth, order, false);
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
    let badge = terminus_ui::components::button::dashed_cta_badge(
        *cta,
        sidebar::BADGE_TILE,
        sidebar::CARD_PAD,
    );
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

pub(crate) fn paint_new_group_form(
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
        // A faint lift off the panel: lighter on ink, darker on paper.
        if theme.is_light() {
            with_alpha([0.0, 0.0, 0.0, 1.0], 0.025)
        } else {
            with_alpha([1.0, 1.0, 1.0, 1.0], 0.025)
        },
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

    paint_button_in_rect(
        sugarloaf,
        theme,
        &create,
        ButtonKind::Primary,
        ButtonSize::Small,
        ButtonState::Default,
        if labels { "Create" } else { "" },
        Layer {
            order: ORDER_CONTENT,
            depth: DEPTH_CONTENT + 0.03,
            backdrop: theme.dialog,
        },
    );

    if !labels {
        return;
    }
    let placeholder = chrome.panel.new_group_name.value.is_empty();
    let text = if placeholder {
        "Group name…"
    } else {
        chrome.panel.new_group_name.value.as_str()
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

// ---- primitives ------------------------------------------------------

/// Chrome text options: Sora (semibold when `bold`, else regular).
/// Weight comes from the face, never from synthetic bold.
fn opts(size: f32, color: [u8; 4], bold: bool) -> DrawOpts {
    use super::ui_text::{ui_opts, UiFamily, UiWeight};
    let weight = if bold {
        UiWeight::SemiBold
    } else {
        UiWeight::Regular
    };
    ui_opts(UiFamily::Sans, size, color, weight)
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

#[allow(dead_code)]
const CTX_MENU_BG_KIND: u32 = 0x01;
#[allow(dead_code)]
const CTX_MENU_BORDER_KIND: u32 = 0x02;
#[allow(dead_code)]
const CTX_MENU_HOVER_KIND: u32 = 0x10;

/// Atlas key for a rounded-rect mask.
///
/// `GlyphKey::glyph_id` is a **u32** (`artwork_id as u32` in sugarloaf), so
/// every bit that distinguishes shapes must fit in 32 bits. Packing height in
/// the high half of a u64 was truncated away — host (h=104) and group (h=72)
/// menus then shared one atlas slot (`side` is max(w,h) and usually the width).
#[allow(dead_code)]
fn ctx_menu_mask_id(kind: u32, content_w: f32, content_h: f32) -> u64 {
    let w = content_w.round().clamp(1.0, 0xFFF as f32) as u32;
    let h = content_h.round().clamp(1.0, 0xFFF as f32) as u32;
    // [kind:8][w:12][h:12]
    let id = (kind & 0xFF) | ((w & 0xFFF) << 8) | ((h & 0xFFF) << 20);
    id as u64
}

#[allow(dead_code)]
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
    let mut paint = tiny_skia::Paint {
        anti_alias: true,
        ..Default::default()
    };
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
    let bytes: Vec<u8> = pixmap.pixels().iter().map(|p| p.alpha()).collect();
    rio_backend::sugarloaf::text::CoverageMask::new(size, bytes)
}

#[allow(dead_code)]
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
    let mut paint = tiny_skia::Paint {
        anti_alias: true,
        ..Default::default()
    };
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
    let mut paint = tiny_skia::Paint {
        anti_alias: true,
        ..Default::default()
    };
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
#[allow(dead_code)]
fn success_color() -> [f32; 4] {
    // Emerald-500 — matches the Termius mock's validated state.
    [16.0 / 255.0, 185.0 / 255.0, 129.0 / 255.0, 1.0]
}

/// Three chasing dots + a breathing ring around `(cx, cy)`.
pub(crate) fn draw_orbit_indicator(
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
    [
        color[0],
        color[1],
        color[2],
        (color[3] * alpha).clamp(0.0, 1.0),
    ]
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
pub(crate) fn wrap_lines(
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
    #[allow(clippy::assertions_on_constants)]
    fn chrome_orders_sit_above_the_grid_and_below_the_overlays() {
        // The terminal grid paints at order 3; the command palette,
        // search and hint tooltip at 20. Chrome goes in between, and the
        // editor above them all so it is never occluded. The host-drag
        // ghost sits above dialogs so the phantom is always on top.
        assert!(ORDER_CONTENT > 3);
        assert!(ORDER_CONTENT < 20);
        assert!(ORDER_DIALOG > 20);
        assert!(ORDER_GHOST > ORDER_DIALOG);
        assert!(DEPTH_GHOST > DEPTH_DIALOG_BG);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn the_dialog_depth_is_above_the_scrim() {
        assert!(DEPTH_DIALOG_BG > DEPTH_DIALOG);
    }
}
