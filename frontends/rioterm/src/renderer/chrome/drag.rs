// Copyright (c) 2026-present, Terminus Contributors.

use super::color::with_alpha;
use super::surfaces::paint_surface;
use super::text::draw_text;
use super::{DEPTH_GHOST, ORDER_GHOST, ROW_SUB_SIZE, ROW_TITLE_SIZE};
use crate::renderer::icons::draw_icon;
use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::chrome::Chrome;
use terminus_ui::geom::Rect;
use terminus_ui::icons::Icon;
use terminus_ui::icons::IconPlacement;
use terminus_ui::sidebar;
use terminus_ui::theme::ChromeTheme;

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
pub(super) fn paint_host_drag_ghost(
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
