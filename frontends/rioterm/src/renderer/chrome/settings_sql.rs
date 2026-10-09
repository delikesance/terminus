// Copyright (c) 2026-present, Terminus Contributors.

use super::color::{color_from_f32, with_alpha};
use super::fields::{paint_field_card, paint_field_caret_prefix, paint_field_selection};
use super::settings::content_origin;
use super::surfaces::paint_surface;
use super::text::{draw_text, elide, opts};
use super::{
    DEPTH_DIALOG, HINT_SIZE, ORDER_DIALOG, ORDER_DIALOG_POPOVER, ROW_SUB_SIZE, TITLE_SIZE,
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

pub(super) fn paint_sql_sync_tab(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    dialog: &Rect,
    window_width: f32,
    window_height: f32,
    device_scale: f32,
) {
    let (content_x, content_y) = content_origin(dialog);
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
