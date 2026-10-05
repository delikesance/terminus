//! New-snippet dialog (Overlay + Input look).

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::button::{ButtonKind, ButtonSize};
use terminus_ui::components::input as ui;
use terminus_ui::components::overlay as ov;
use terminus_ui::dialog_form::{DialogFormLayout, DynamicFormHit};
use terminus_ui::geom::Rect;
use terminus_ui::theme::ChromeTheme;
use terminus_ui::tokens::{font_size, radius};
use terminus_ui::Chrome;

use super::{
    button_state, paint_button_rect, paint_shadow, paint_text_field, DEPTH, ORDER,
};
use crate::renderer::chrome::{paint_flat, paint_surface_stroke};
use crate::renderer::ui_text::{draw_ui_text, measure_ui_text, UiWeight};

pub fn paint_add_snippet(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    window: (f32, f32),
    glyphs: bool,
) {
    let form = &chrome.snippet_form.inner;
    let layout = DialogFormLayout::compute(form, window.0, window.1);
    paint_flat(
        sugarloaf,
        &Rect::new(0.0, 0.0, window.0, window.1),
        ov::SCRIM,
        DEPTH - 0.02,
        ORDER,
    );
    paint_shadow(sugarloaf, &layout.dialog, radius::DIALOG, DEPTH);
    paint_surface_stroke(
        sugarloaf,
        &layout.dialog,
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
        layout.title.x,
        layout.title.y,
        &form.title,
        24.0,
        theme.text,
        UiWeight::SemiBold,
    );
    let font = ui::SANS_VALUE_FONT;
    for (i, field) in form.fields.iter().enumerate() {
        let frame = layout.fields[i];
        let input = layout.input_rect(i);
        let focused = form.focused_index == i;
        draw_ui_text(
            sugarloaf,
            frame.x,
            frame.y,
            &field.label,
            ui::LABEL_FONT,
            theme.text,
            UiWeight::Medium,
        );
        let shown = field.draft.display_line();
        let empty = shown.is_empty();
        let prefix = field.draft.prefix_display();
        paint_text_field(
            sugarloaf,
            theme,
            &input,
            if empty { &field.placeholder } else { &shown },
            empty,
            focused,
            focused.then_some(prefix.as_str()),
            0.0,
            DEPTH + 0.04,
        );
        if focused {
            if let Some((start, end)) = field.draft.selection_range() {
                let before: String = shown.chars().take(start).collect();
                let sel: String = shown.chars().skip(start).take(end - start).collect();
                let bx = measure_ui_text(sugarloaf, &before, font, UiWeight::Regular);
                let sw =
                    measure_ui_text(sugarloaf, &sel, font, UiWeight::Regular).max(2.0);
                let h = (font * 1.25).round();
                let mut c = theme.accent;
                c[3] = 0.35;
                paint_flat(
                    sugarloaf,
                    &Rect::new(
                        input.x + ui::PAD_LEFT + bx,
                        input.y + (input.height - h) / 2.0,
                        sw,
                        h,
                    ),
                    c,
                    DEPTH + 0.052,
                    ORDER,
                );
            }
        }
    }
    if let (Some(rect), Some(err)) = (layout.error_line, form.error.as_deref()) {
        draw_ui_text(
            sugarloaf,
            rect.x,
            rect.y,
            err,
            font_size::CAPTION,
            theme.danger_text,
            UiWeight::Regular,
        );
    }
    let hov = |h| form.btn_hover == Some(h);
    paint_button_rect(
        sugarloaf,
        theme,
        &layout.cancel_btn,
        ButtonKind::Secondary,
        ButtonSize::Large,
        button_state(false, hov(DynamicFormHit::Cancel)),
        "Cancel",
        theme.dialog,
        DEPTH + 0.05,
    );
    paint_button_rect(
        sugarloaf,
        theme,
        &layout.save_btn,
        ButtonKind::Primary,
        ButtonSize::Large,
        button_state(false, hov(DynamicFormHit::Save)),
        &form.save_label,
        theme.dialog,
        DEPTH + 0.05,
    );
}
