//! Confirmation dialog painter (Confirm / Destructive / With-option).

use crate::renderer::components::button::{button_state, paint_button_in_rect};
use crate::renderer::components::selection::paint_checkbox_on;
use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::button::{ButtonKind, ButtonSize};
use terminus_ui::components::overlay::{self as ov, DialogFocus, DialogKind};
use terminus_ui::components::selection::ControlState;
use terminus_ui::confirm::{ConfirmLayout, ConfirmSpec};
use terminus_ui::theme::ChromeTheme;
use terminus_ui::tokens::font_size;

use super::{dialog_layer, paint_dialog_frame};
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
    paint_dialog_frame(sugarloaf, theme, Some(&l.scrim), &l.dialog);
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
        paint_checkbox_on(
            sugarloaf,
            theme,
            (row.x, row.y),
            label,
            view.option_checked,
            ControlState::Default,
            dialog_layer(theme),
        );
    }
    let confirm_kind = if spec.kind == DialogKind::Destructive {
        ButtonKind::Danger
    } else {
        ButtonKind::Primary
    };
    let hov = |f| view.hover == Some(f);
    let foc = |f| view.focus == Some(f);
    paint_button_in_rect(
        sugarloaf,
        theme,
        &l.cancel,
        ButtonKind::Secondary,
        ButtonSize::Large,
        button_state(foc(DialogFocus::Cancel), hov(DialogFocus::Cancel)),
        &spec.cancel,
        dialog_layer(theme),
    );
    paint_button_in_rect(
        sugarloaf,
        theme,
        &l.confirm,
        confirm_kind,
        ButtonSize::Large,
        button_state(foc(DialogFocus::Confirm), hov(DialogFocus::Confirm)),
        &spec.confirm,
        dialog_layer(theme),
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
