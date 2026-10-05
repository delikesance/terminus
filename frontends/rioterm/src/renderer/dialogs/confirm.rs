//! Confirmation dialog painter (Confirm / Destructive / With-option).

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::button::{ButtonKind, ButtonSize};
use terminus_ui::components::overlay::{self as ov, DialogFocus, DialogKind};
use terminus_ui::confirm::{ConfirmLayout, ConfirmSpec};
use terminus_ui::theme::ChromeTheme;
use terminus_ui::tokens::{font_size, radius};

use super::{
    button_state, paint_button_rect, paint_checkbox_row, paint_shadow, DEPTH, ORDER,
};
use crate::renderer::chrome::{paint_flat, paint_surface_stroke};
use crate::renderer::ui_text::{draw_ui_text, UiWeight};

/// Pointer / keyboard state of one open dialog.
#[derive(Debug, Clone, Copy, Default)]
pub struct ConfirmView {
    pub focus: Option<DialogFocus>,
    pub hover: Option<DialogFocus>,
    pub option_checked: bool,
}

/// Paint `spec` at `layout`: scrim, panel, then (when `glyphs`) text and
/// controls. Lower layers of a modal stack pass `glyphs = false`.
pub fn paint_confirm(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    spec: &ConfirmSpec,
    layout: &ConfirmLayout,
    view: ConfirmView,
    glyphs: bool,
) {
    let l = &layout.dialog;
    paint_flat(sugarloaf, &l.scrim, ov::SCRIM, DEPTH - 0.02, ORDER);
    paint_shadow(sugarloaf, &l.dialog, radius::DIALOG, DEPTH);
    paint_surface_stroke(
        sugarloaf,
        &l.dialog,
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
    draw_ui_text(
        sugarloaf,
        l.title.x,
        l.title.y,
        &spec.title,
        font_size::TITLE,
        theme.text,
        UiWeight::SemiBold,
    );
    for (i, line) in layout.lines.iter().enumerate() {
        draw_ui_text(
            sugarloaf,
            l.body.x,
            l.body.y + i as f32 * ov::BODY_LINE,
            line,
            font_size::BODY_SM,
            theme.text_muted,
            UiWeight::Regular,
        );
    }
    if let (Some(row), Some(label)) = (l.option.as_ref(), spec.option.as_deref()) {
        paint_checkbox_row(
            sugarloaf,
            theme,
            row,
            label,
            view.option_checked,
            DEPTH + 0.05,
        );
    }
    let confirm_kind = if spec.kind == DialogKind::Destructive {
        ButtonKind::Danger
    } else {
        ButtonKind::Primary
    };
    let hov = |f| view.hover == Some(f);
    let foc = |f| view.focus == Some(f);
    paint_button_rect(
        sugarloaf,
        theme,
        &l.cancel,
        ButtonKind::Secondary,
        ButtonSize::Large,
        button_state(foc(DialogFocus::Cancel), hov(DialogFocus::Cancel)),
        &spec.cancel,
        theme.dialog,
        DEPTH + 0.05,
    );
    paint_button_rect(
        sugarloaf,
        theme,
        &l.confirm,
        confirm_kind,
        ButtonSize::Large,
        button_state(foc(DialogFocus::Confirm), hov(DialogFocus::Confirm)),
        &spec.confirm,
        theme.dialog,
        DEPTH + 0.05,
    );
}

/// The chrome's own confirmation (delete host / group), if open.
pub fn paint_chrome_confirm(
    sugarloaf: &mut Sugarloaf,
    chrome: &terminus_ui::Chrome,
    theme: &ChromeTheme,
    window: (f32, f32),
    glyphs: bool,
) {
    let Some(prompt) = chrome.confirm.as_ref() else {
        return;
    };
    let layout = prompt.layout(window);
    let view = ConfirmView {
        focus: Some(prompt.focus),
        hover: prompt.hover,
        option_checked: prompt.option_checked,
    };
    paint_confirm(sugarloaf, theme, &prompt.spec, &layout, view, glyphs);
}
