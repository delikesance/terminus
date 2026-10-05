//! Sync tab painter.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::button::{ButtonKind, ButtonSize, ButtonSpec, ButtonState};
use terminus_ui::components::feedback::status_dot_rect;
use terminus_ui::components::input::{FieldKind, FieldState};
use terminus_ui::components::list::CARD_RADIUS;
use terminus_ui::components::selection::SegmentedSize;
use terminus_ui::geom::Rect;
use terminus_ui::icons::Icon;
use terminus_ui::theme::ChromeTheme;
use terminus_ui::tokens::font_size;
use terminus_ui::views::settings::sync::{SyncState, SyncTarget, INTRO};

use super::keys::{fit_tail, text_top};
use super::with_measure;
use crate::renderer::components::button::{label_spec, paint_button};
use crate::renderer::components::feedback::paint_status_dot;
use crate::renderer::components::input::{paint_field, FieldContent};
use crate::renderer::components::selection::paint_segmented;
use crate::renderer::ui_text::{draw_ui_text, measure_mono_text, UiWeight};

pub fn paint(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    content: Rect,
    s: &SyncState,
) {
    let l = with_measure(sugarloaf, |m| s.layout(content, m));
    let scale = sugarloaf.scale_factor();
    let _ = Icon::Trash2;

    draw_ui_text(
        sugarloaf,
        l.intro.x,
        text_top(l.intro.y + l.intro.height / 2.0, font_size::BODY_SM),
        INTRO,
        font_size::BODY_SM,
        theme.text_muted,
        UiWeight::Regular,
    );
    draw_ui_text(
        sugarloaf,
        l.database_label.x,
        text_top(
            l.database_label.y + l.database_label.height / 2.0,
            font_size::LABEL,
        ),
        "Database",
        font_size::LABEL,
        theme.text,
        UiWeight::Medium,
    );
    paint_segmented(
        sugarloaf,
        theme,
        (l.segmented.track.x, l.segmented.track.y),
        &terminus_ui::settings::SQL_ENGINES,
        s.engine,
        SegmentedSize::Medium,
    );

    // The value, tail-fitted so the end being typed stays visible.
    let shown = fit_tail(&s.uri.value, l.field.text.width, |t| {
        measure_mono_text(sugarloaf, t, 14.0, UiWeight::Regular)
    });
    let caret = s
        .focused
        .then(|| measure_mono_text(sugarloaf, &shown, 14.0, UiWeight::Regular));
    let fstate = if s.field_error.is_some() {
        FieldState::Error
    } else if s.focused {
        FieldState::Focus
    } else if s.hover == Some(SyncTarget::Field) {
        FieldState::Hover
    } else {
        FieldState::Default
    };
    paint_field(
        sugarloaf,
        theme,
        &l.field,
        &FieldContent {
            kind: FieldKind::Mono,
            state: fstate,
            label: Some(s.field_label()),
            value: &shown,
            placeholder: s.placeholder(),
            helper: s.field_error.as_deref(),
            revealed: false,
            caret_prefix_width: caret,
        },
        theme.canvas,
        scale,
    );

    sugarloaf.rounded_rect(
        None,
        l.status.x,
        l.status.y,
        l.status.width,
        l.status.height,
        theme.frame,
        0.1,
        CARD_RADIUS,
        0,
    );
    let dot = status_dot_rect(l.status_dot.x, l.status.y, l.status.height);
    paint_status_dot(sugarloaf, theme, &dot, s.status_kind(), theme.frame);
    let color = if s.status.is_error {
        theme.danger_text
    } else {
        theme.text
    };
    draw_ui_text(
        sugarloaf,
        l.status_text.x,
        text_top(l.status.y + l.status.height / 2.0, font_size::BODY_SM),
        &s.status_text(),
        font_size::BODY_SM,
        color,
        UiWeight::Regular,
    );
    let label = s.status_button_label();
    let spec = label_spec(
        sugarloaf,
        (l.status_button.x, l.status_button.y),
        ButtonKind::Secondary,
        ButtonSize::Medium,
        label,
        false,
    );
    paint_button(
        sugarloaf,
        theme,
        &spec,
        ButtonState::resolve(
            s.hover == Some(SyncTarget::StatusButton),
            false,
            false,
            false,
        ),
        label,
        None,
    );
    let save: ButtonSpec = label_spec(
        sugarloaf,
        (l.save.x, l.save.y),
        ButtonKind::Primary,
        ButtonSize::Large,
        "Save",
        false,
    );
    paint_button(
        sugarloaf,
        theme,
        &save,
        ButtonState::resolve(s.hover == Some(SyncTarget::Save), false, false, false),
        "Save",
        None,
    );
}
