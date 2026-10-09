// Copyright (c) 2026-present, Terminus Contributors.

use super::color::{as_f32, color_from_f32, with_alpha};
use super::settings_keys::paint_keys_tab;
use super::settings_sql::paint_sql_sync_tab;
use super::surfaces::{paint_dialog_shell, paint_flat, paint_surface, DialogBorderMode};
use super::text::draw_text;
use super::{
    BORDER_WIDTH, DEPTH_DIALOG, DEPTH_DIALOG_BG, HINT_SIZE, ORDER_DIALOG, ROW_SUB_SIZE,
    TITLE_SIZE,
};
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
use terminus_ui::theme::ChromeTheme;

pub(super) fn render_settings_modal(
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
        terminus_ui::SettingsTab::Keys => paint_keys_tab(
            sugarloaf,
            chrome,
            theme,
            &dialog,
            window_width,
            window_height,
            device_scale,
        ),
        terminus_ui::SettingsTab::SqlSync => paint_sql_sync_tab(
            sugarloaf,
            chrome,
            theme,
            &dialog,
            window_width,
            window_height,
            device_scale,
        ),
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

pub(super) fn content_origin(dialog: &Rect) -> (f32, f32) {
    (
        dialog.x + terminus_ui::settings::SIDEBAR_WIDTH + 24.0,
        dialog.y + 80.0,
    )
}
