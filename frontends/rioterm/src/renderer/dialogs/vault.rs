//! Vault unlock / create dialog.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::button::{ButtonKind, ButtonSize};
use terminus_ui::components::overlay as ov;
use terminus_ui::confirm::{estimate_text_width, BODY_FONT};
use terminus_ui::icons::{Icon, IconPlacement};
use terminus_ui::theme::ChromeTheme;
use terminus_ui::tokens::{font_size, radius};
use terminus_ui::vault_unlock::{self as vu, VaultUnlockLayout};
use terminus_ui::Chrome;

use super::{
    button_state, paint_button_rect, paint_checkbox_row, paint_shadow, paint_text_field,
    DEPTH, ORDER,
};
use crate::renderer::chrome::{draw_icon, paint_flat, paint_surface_stroke};
use crate::renderer::ui_text::{draw_ui_text, UiWeight};

pub fn paint_vault_unlock(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    window: (f32, f32),
    glyphs: bool,
) {
    let prompt = &chrome.vault_unlock;
    let layout = VaultUnlockLayout::for_prompt(window.0, window.1, prompt);
    let d = layout.rect();
    paint_flat(
        sugarloaf,
        &terminus_ui::Rect::new(0.0, 0.0, window.0, window.1),
        ov::SCRIM,
        DEPTH - 0.02,
        ORDER,
    );
    paint_shadow(sugarloaf, &d, radius::DIALOG, DEPTH);
    paint_surface_stroke(
        sugarloaf,
        &d,
        theme.dialog,
        Some(theme.dialog_line),
        radius::DIALOG,
        1.0,
        DEPTH + 0.02,
        ORDER,
        false,
    );
    if !glyphs {
        return;
    }
    let scale = sugarloaf.scale_factor();

    // Lock tile.
    let tile = layout.tile_rect();
    sugarloaf.rounded_rect(
        None,
        tile.x,
        tile.y,
        tile.width,
        tile.height,
        theme.choice_selected_bg,
        DEPTH + 0.04,
        radius::tile(tile.width).max(14.0),
        ORDER,
    );
    let icon = 22.0;
    draw_icon(
        sugarloaf,
        Icon::Lock,
        IconPlacement::new(
            tile.x + (tile.width - icon) / 2.0,
            tile.y + (tile.height - icon) / 2.0,
            icon,
        ),
        theme.accent,
        scale,
    );

    let title = layout.title_rect();
    draw_ui_text(
        sugarloaf,
        title.x,
        title.y,
        prompt.title(),
        font_size::TITLE,
        theme.text,
        UiWeight::SemiBold,
    );
    let body = layout.subtitle_rect();
    let lines = ov::wrap_text(&prompt.subtitle(), body.width, |s| {
        estimate_text_width(s, BODY_FONT)
    });
    for (i, line) in lines.iter().enumerate() {
        draw_ui_text(
            sugarloaf,
            body.x,
            body.y + i as f32 * vu::BODY_LINE,
            line,
            font_size::BODY_SM,
            theme.text_muted,
            UiWeight::Regular,
        );
    }

    let field = layout.passphrase_card_rect();
    let paint = prompt.field_paint();
    let pass_focus = !prompt.confirm_focused();
    paint_text_field(
        sugarloaf,
        theme,
        &field,
        &paint.text,
        paint.placeholder,
        pass_focus,
        (pass_focus && paint.show_caret).then_some(paint.caret_prefix.as_str()),
        vu::EYE_SLOT,
        DEPTH + 0.04,
    );
    let eye = layout.eye_rect();
    let eye_icon = if prompt.visible() {
        Icon::EyeOff
    } else {
        Icon::Eye
    };
    let es = VaultUnlockLayout::eye_icon_size();
    draw_icon(
        sugarloaf,
        eye_icon,
        IconPlacement::new(
            eye.x + (eye.width - es) / 2.0,
            eye.y + (eye.height - es) / 2.0,
            es,
        ),
        [
            theme.text_muted[0] as f32 / 255.0,
            theme.text_muted[1] as f32 / 255.0,
            theme.text_muted[2] as f32 / 255.0,
            1.0,
        ],
        scale,
    );
    if let Some(confirm_rect) = layout.confirm_card_rect() {
        let c = prompt.confirm_field_paint();
        paint_text_field(
            sugarloaf,
            theme,
            &confirm_rect,
            &c.text,
            c.placeholder,
            prompt.confirm_focused(),
            (prompt.confirm_focused() && c.show_caret).then_some(c.caret_prefix.as_str()),
            0.0,
            DEPTH + 0.04,
        );
    }

    paint_checkbox_row(
        sugarloaf,
        theme,
        &layout.remember_row_rect(),
        vu::REMEMBER_LABEL,
        prompt.remember(),
        DEPTH + 0.05,
    );

    let hint = layout.hint_rect();
    if hint.height > 0.0 {
        let (text, color) = if let Some(err) = prompt.error() {
            (err.to_string(), theme.danger_text)
        } else {
            (
                if prompt.creating() {
                    "Creating\u{2026}"
                } else {
                    "Unlocking\u{2026}"
                }
                .to_string(),
                theme.text_muted,
            )
        };
        draw_ui_text(
            sugarloaf,
            hint.x,
            hint.y,
            &text,
            font_size::CAPTION,
            color,
            UiWeight::Regular,
        );
    }

    let (cancel, ok) = layout.button_rects("Cancel", prompt.action_label());
    paint_button_rect(
        sugarloaf,
        theme,
        &cancel,
        ButtonKind::Secondary,
        ButtonSize::Large,
        button_state(false, false),
        "Cancel",
        theme.dialog,
        DEPTH + 0.05,
    );
    paint_button_rect(
        sugarloaf,
        theme,
        &ok,
        ButtonKind::Primary,
        ButtonSize::Large,
        button_state(false, false),
        prompt.action_label(),
        theme.dialog,
        DEPTH + 0.05,
    );
}
