// Copyright (c) 2026-present, Terminus Contributors.

use super::color::with_alpha;
use super::notices::draw_dashed_rounded_rect;
use super::surfaces::paint_surface;
use super::text::draw_text;
use super::{ADD_ICON_SIZE, DEPTH_CONTENT, ORDER_CONTENT, ROW_SUB_SIZE, ROW_TITLE_SIZE};
use crate::renderer::components::button::paint_button_in_rect;
use crate::renderer::components::Layer;
use crate::renderer::icons::draw_icon;
use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::chrome::Chrome;
use terminus_ui::components::button::ButtonKind;
use terminus_ui::components::button::ButtonSize;
use terminus_ui::components::button::ButtonState;
use terminus_ui::geom::Rect;
use terminus_ui::icons::Icon;
use terminus_ui::icons::IconPlacement;
use terminus_ui::sidebar;
use terminus_ui::theme::ChromeTheme;

/// Bordered square icon badge (CTA / folder / key tiles).
pub(super) fn paint_bordered_badge(
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
pub(super) fn paint_dashed_cta(
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
