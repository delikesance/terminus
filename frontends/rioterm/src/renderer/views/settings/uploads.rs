//! Uploads tab painter.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::button::{ButtonKind, ButtonSize, ButtonState};
use terminus_ui::components::input::{FieldKind, FieldState};
use terminus_ui::geom::Rect;
use terminus_ui::theme::ChromeTheme;
use terminus_ui::tokens::font_size;
use terminus_ui::views::settings::uploads::{
    UploadsField, UploadsRow, UploadsState, UploadsTarget, INTRO, RESET_LABEL,
};

use super::keys::{fit_tail, text_top};
use super::with_measure;
use crate::renderer::components::button::{label_spec, paint_button};
use crate::renderer::components::input::{paint_field, FieldContent};
use crate::renderer::ui_text::{draw_ui_text, measure_mono_text, UiWeight};

pub fn paint(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    content: Rect,
    s: &UploadsState,
) {
    let l = with_measure(sugarloaf, |m| s.layout(content, m));
    draw_ui_text(
        sugarloaf,
        l.intro.x,
        text_top(l.intro.y + l.intro.height / 2.0, font_size::BODY_SM),
        INTRO,
        font_size::BODY_SM,
        theme.text_muted,
        UiWeight::Regular,
    );
    for field in UploadsField::ALL {
        paint_row(sugarloaf, theme, s, field, &l.rows[field as usize]);
    }
}

fn paint_row(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    s: &UploadsState,
    field: UploadsField,
    row: &UploadsRow,
) {
    let scale = sugarloaf.scale_factor();
    let focused = s.focus == Some(field);
    let shown = fit_tail(&s.draft(field).value, row.field.text.width, |t| {
        measure_mono_text(sugarloaf, t, 14.0, UiWeight::Regular)
    });
    let caret =
        focused.then(|| measure_mono_text(sugarloaf, &shown, 14.0, UiWeight::Regular));
    let state = if focused {
        FieldState::Focus
    } else if s.hover == Some(UploadsTarget::Field(field)) {
        FieldState::Hover
    } else {
        FieldState::Default
    };
    paint_field(
        sugarloaf,
        theme,
        &row.field,
        &FieldContent {
            kind: FieldKind::Mono,
            state,
            label: Some(field.label()),
            value: &shown,
            placeholder: field.placeholder(),
            helper: None,
            revealed: false,
            caret_prefix_width: caret,
        },
        theme.canvas,
        scale,
    );
    let reset = label_spec(
        sugarloaf,
        (row.reset.x, row.reset.y),
        ButtonKind::Text,
        ButtonSize::Small,
        RESET_LABEL,
        false,
    );
    let hover = s.hover == Some(UploadsTarget::Reset(field));
    paint_button(
        sugarloaf,
        theme,
        &reset,
        ButtonState::resolve(hover, false, false, false),
        RESET_LABEL,
        None,
    );
}
