// Copyright (c) 2026-present, Terminus Contributors.

use super::color::{color_from_f32, opaque_over, with_alpha};
use super::fields::{paint_field_card, paint_field_caret_prefix};
use super::forms::paint_dashed_cta;
use super::settings::content_origin;
use super::surfaces::paint_surface;
use super::text::{draw_text, elide, opts, wrap_lines};
use super::{
    DEPTH_DIALOG, HINT_SIZE, ORDER_DIALOG, ROW_SUB_SIZE, ROW_TITLE_SIZE, TITLE_SIZE,
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

pub(super) fn paint_keys_tab(
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

    if let Some(draft) = chrome.settings.key_draft_rect(window_width, window_height) {
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
            let err_text = chrome.settings.key_draft_error.as_deref().unwrap_or("");
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
        let copy = chrome
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
