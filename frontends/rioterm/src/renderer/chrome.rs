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
//! always lands in the order-0 batch, *under* the terminal grid, which
//! is why the Lucide icons are tessellated into `line` strips). The grid
//! uses order 3, so the chrome lives at 4 and up, and the overlays
//! (palette, search) at 20 sit above it.

use rio_backend::sugarloaf::text::DrawOpts;
use rio_backend::sugarloaf::Sugarloaf;

use terminus_ui::activity_bar;
use terminus_ui::add_host::FIELDS;
use terminus_ui::chrome::Chrome;
use terminus_ui::geom::Rect;
use terminus_ui::icons::{self, IconPlacement};
use terminus_ui::sidebar;
use terminus_ui::theme::ChromeTheme;

/// Chrome paint orders. The grid is 3 and the overlays are 20.
const ORDER_RAIL: u8 = 4;
const ORDER_PANEL: u8 = 6;
const ORDER_CONTENT: u8 = 7;
const ORDER_DIALOG: u8 = 30;

const DEPTH_BG: f32 = 0.05;
const DEPTH_CONTENT: f32 = 0.06;
const DEPTH_DIALOG: f32 = 0.1;
const DEPTH_DIALOG_BG: f32 = 0.2;

const RAIL_ICON_SIZE: f32 = activity_bar::ICON_SIZE;
const ADD_ICON_SIZE: f32 = 13.0;

const TITLE_SIZE: f32 = 11.0;
const COUNT_SIZE: f32 = 10.0;
const ROW_TITLE_SIZE: f32 = 12.0;
const ROW_SUB_SIZE: f32 = 10.5;

const DIALOG_TITLE_SIZE: f32 = 13.0;
const CAPTION_SIZE: f32 = 10.5;
const INPUT_SIZE: f32 = 12.0;
const HINT_SIZE: f32 = 10.5;

const BORDER_WIDTH: f32 = 1.0;
const INPUT_PAD_X: f32 = 10.0;
const CARET_WIDTH: f32 = 1.5;

/// Paint the whole chrome for one frame.
///
/// `window_width` / `window_height` are logical pixels (physical size
/// divided by the scale factor), matching `terminus_ui`'s geometry.
pub fn render(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    window_width: f32,
    window_height: f32,
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

    render_rail(sugarloaf, chrome, theme, origin_y, height);

    if chrome.panel_visible {
        render_panel(sugarloaf, chrome, theme, origin_y, height);
    }

    if chrome.add_host_is_open() {
        render_add_host(sugarloaf, chrome, theme, window_width, window_height);
    }
}

fn render_rail(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    origin_y: f32,
    height: f32,
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

    for section in activity_bar::SECTIONS {
        let index = section.index();
        let selected = section == chrome.activity.selected;

        if selected {
            let item = activity_bar::item_rect(origin_y, index);
            sugarloaf.rounded_rect(
                None,
                item.x + 4.0,
                item.y,
                item.width - 8.0,
                item.height,
                theme.rail_active_bg,
                DEPTH_CONTENT,
                6.0,
                ORDER_CONTENT,
            );
            let marker = activity_bar::marker_rect(origin_y, index);
            sugarloaf.rect(
                None,
                marker.x,
                marker.y,
                marker.width,
                marker.height,
                theme.rail_marker,
                DEPTH_CONTENT + 0.01,
                ORDER_CONTENT,
            );
        }

        let icon_rect = activity_bar::icon_rect(origin_y, index);
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
            DEPTH_CONTENT + 0.02,
            ORDER_CONTENT,
        );
    }
}

fn render_panel(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    origin_y: f32,
    height: f32,
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
        let header = chrome.panel.header_rect(origin_y);
        draw_text(
            sugarloaf,
            header.x + sidebar::PAD_X,
            header.y + 12.0,
            chrome.panel_title(),
            TITLE_SIZE,
            theme.text_muted,
            true,
        );
        let count = if chrome.hosts_visible() {
            chrome.panel.count_label()
        } else {
            String::new()
        };
        if !count.is_empty() {
            let width = sugarloaf
                .text_mut()
                .measure(&count, &opts(COUNT_SIZE, theme.text_faint, false));
            draw_text(
                sugarloaf,
                header.right() - sidebar::PAD_X - width,
                header.y + 13.0,
                &count,
                COUNT_SIZE,
                theme.text_faint,
                false,
            );
        }
    }

    render_notice(sugarloaf, chrome, theme, origin_y, labels);

    if chrome.hosts_visible() {
        render_host_rows(sugarloaf, chrome, theme, origin_y, height, labels);
    } else if labels {
        let body = chrome.panel.body_rect(origin_y, height);
        draw_text(
            sugarloaf,
            body.x + sidebar::PAD_X,
            body.y + 8.0,
            "Nothing here yet",
            ROW_SUB_SIZE,
            theme.text_faint,
            false,
        );
    }

    render_footer(sugarloaf, chrome, theme, origin_y, height, labels);
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

fn render_host_rows(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    origin_y: f32,
    height: f32,
    labels: bool,
) {
    let body = chrome.panel.body_rect(origin_y, height);
    if body.height <= 0.0 {
        return;
    }

    for (index, host) in chrome.panel.items.iter().enumerate() {
        let row = chrome.panel.item_rect(origin_y, index);
        // Clip: a partially scrolled row is drawn truncated rather than
        // over the header or the add-host row.
        let Some((top, bottom)) = row.clip_rows(body.y, body.bottom()) else {
            continue;
        };

        let selected = chrome.panel.selected == Some(index);
        let hovered = chrome.panel.hover == Some(index);
        if selected || hovered {
            let background = if selected {
                theme.item_selected
            } else {
                theme.item_hover
            };
            sugarloaf.rect(
                None,
                row.x,
                top,
                row.width,
                bottom - top,
                background,
                DEPTH_CONTENT,
                ORDER_CONTENT,
            );
        }
        if selected {
            sugarloaf.rect(
                None,
                row.x,
                top,
                activity_bar::MARKER_WIDTH,
                bottom - top,
                theme.rail_marker,
                DEPTH_CONTENT + 0.005,
                ORDER_CONTENT,
            );
        }

        if !labels {
            continue;
        }

        // Text is painted above every quad, so a clipped row must be
        // skipped entirely rather than drawn past the viewport.
        if top > row.y + 1.0 || bottom < row.bottom() - 1.0 {
            continue;
        }

        let text_width = row.width - 2.0 * sidebar::PAD_X;
        let name = elide(
            sugarloaf,
            &host.name,
            text_width,
            &opts(ROW_TITLE_SIZE, theme.text, false),
        );
        draw_text(
            sugarloaf,
            row.x + sidebar::PAD_X,
            row.y + 7.0,
            &name,
            ROW_TITLE_SIZE,
            theme.text,
            false,
        );

        let endpoint = elide(
            sugarloaf,
            &host.endpoint,
            text_width,
            &opts(ROW_SUB_SIZE, theme.text_faint, false),
        );
        draw_text(
            sugarloaf,
            row.x + sidebar::PAD_X,
            row.y + 24.0,
            &endpoint,
            ROW_SUB_SIZE,
            theme.text_faint,
            false,
        );
    }

    render_panel_scrollbar(sugarloaf, chrome, theme, origin_y, height, body);
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

    let button = chrome.panel.add_button_rect(origin_y, height);
    if button.height <= 0.0 {
        return;
    }
    sugarloaf.rounded_rect(
        None,
        button.x,
        button.y,
        button.width,
        button.height,
        if chrome.panel.add_hover {
            theme.item_hover
        } else {
            theme.button_bg
        },
        DEPTH_CONTENT,
        5.0,
        ORDER_CONTENT,
    );

    // The `plus` icon is Lucide geometry, laid out beside its label.
    let icon_x = button.x + 10.0;
    let icon_y = button.y + (button.height - ADD_ICON_SIZE) / 2.0;
    draw_icon(
        sugarloaf,
        terminus_ui::icons::Icon::Plus,
        IconPlacement::new(icon_x, icon_y, ADD_ICON_SIZE),
        theme.accent,
        DEPTH_CONTENT + 0.02,
        ORDER_CONTENT,
    );

    if labels {
        draw_text(
            sugarloaf,
            icon_x + ADD_ICON_SIZE + 8.0,
            button.y + (button.height - INPUT_SIZE) / 2.0 - 1.0,
            "Add host",
            INPUT_SIZE,
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
    sugarloaf.rounded_rect(
        None,
        dialog.x - BORDER_WIDTH,
        dialog.y - BORDER_WIDTH,
        dialog.width + 2.0 * BORDER_WIDTH,
        dialog.height + 2.0 * BORDER_WIDTH,
        theme.dialog_border,
        DEPTH_DIALOG_BG,
        9.0,
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
        8.0,
        ORDER_DIALOG,
    );

    let title = layout.title_rect();
    draw_text(
        sugarloaf,
        title.x,
        title.y + 6.0,
        "Add host",
        DIALOG_TITLE_SIZE,
        theme.text,
        true,
    );

    for field in FIELDS {
        let input = layout.input_rect(field);
        let focused = form.focused_field() == field;

        // Focus ring behind the box, then the box itself inset by the
        // border width — a 1px outline out of filled-only primitives.
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
            6.0,
            ORDER_DIALOG,
        );
        sugarloaf.rounded_rect(
            None,
            input.x,
            input.y,
            input.width,
            input.height,
            theme.field_bg,
            DEPTH_DIALOG_BG + 0.02,
            5.0,
            ORDER_DIALOG,
        );

        let caption = layout.caption_rect(field);
        draw_text(
            sugarloaf,
            caption.x,
            caption.y + 2.0,
            field.label(),
            CAPTION_SIZE,
            if focused {
                theme.text
            } else {
                theme.text_faint
            },
            false,
        );

        let text_y = input.y + (input.height - INPUT_SIZE) / 2.0;
        let text_x = input.x + INPUT_PAD_X;
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

    let hint = layout.hint_rect(form.height());
    match form.error() {
        Some(error) => {
            let text = elide(
                sugarloaf,
                error,
                hint.width,
                &opts(HINT_SIZE, theme.danger, false),
            );
            draw_text(
                sugarloaf,
                hint.x,
                hint.y + 4.0,
                &text,
                HINT_SIZE,
                theme.danger,
                false,
            );
        }
        None => draw_text(
            sugarloaf,
            hint.x,
            hint.y + 4.0,
            "Tab next field  ·  Enter to save  ·  Esc to cancel",
            HINT_SIZE,
            theme.text_faint,
            false,
        ),
    }
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

fn draw_icon(
    sugarloaf: &mut Sugarloaf,
    icon: terminus_ui::icons::Icon,
    placement: IconPlacement,
    color: [f32; 4],
    depth: f32,
    order: u8,
) {
    let stroke = placement.stroke();
    for ((x1, y1), (x2, y2)) in icons::flatten(icon) {
        let (px1, py1) = placement.map(x1, y1);
        let (px2, py2) = placement.map(x2, y2);
        sugarloaf.line(px1, py1, px2, py2, stroke, depth, color, order);
    }
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

    #[test]
    fn chrome_orders_sit_above_the_grid_and_below_the_overlays() {
        // The terminal grid paints at order 3; the command palette,
        // search and hint tooltip at 20. Chrome goes in between, and the
        // editor above them all so it is never occluded.
        assert!(ORDER_RAIL > 3);
        assert!(ORDER_PANEL > ORDER_RAIL);
        assert!(ORDER_CONTENT > ORDER_PANEL);
        assert!(ORDER_CONTENT < 20);
        assert!(ORDER_DIALOG > 20);
    }

    #[test]
    fn the_dialog_depth_is_above_the_scrim() {
        assert!(DEPTH_DIALOG_BG > DEPTH_DIALOG);
    }
}
