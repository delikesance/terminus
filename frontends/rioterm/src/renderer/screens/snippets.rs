//! Snippets view painter (stub): New snippet, then one card per snippet
//! with Paste / Run / Delete.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::button::{ButtonSize, ButtonState};
use terminus_ui::geom::Rect;
use terminus_ui::screens::snippets::{
    SnippetsHit, SnippetsState, DELETE, NEW, PASTE, ROW_PAD_X, ROW_RADIUS, RUN, TITLE,
};
use terminus_ui::theme::ChromeTheme;

use crate::renderer::components::button::paint_button;
use crate::renderer::ui_text::{draw_mono_text, draw_ui_text, measure_ui_text, UiWeight};

pub fn measure(sugarloaf: &mut Sugarloaf, state: &mut SnippetsState) {
    if state.label_w[0] > 0.0 {
        return;
    }
    state.label_w[0] = super::primary_label_w(sugarloaf, NEW);
    let fs = ButtonSize::Medium.font_size();
    for (i, label) in [PASTE, RUN, DELETE].iter().enumerate() {
        state.label_w[i + 1] = measure_ui_text(sugarloaf, label, fs, UiWeight::Medium);
    }
}

pub fn paint(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    content: Rect,
    state: &SnippetsState,
    _device_scale: f32,
) {
    let new = state.new_button(content);
    let state_of = |hit: SnippetsHit| {
        if state.hover == Some(hit) {
            ButtonState::Hover
        } else {
            ButtonState::Default
        }
    };
    paint_button(
        sugarloaf,
        theme,
        &new,
        state_of(SnippetsHit::New),
        NEW,
        None,
    );
    if state.items.is_empty() {
        super::paint_empty(
            sugarloaf,
            theme,
            content,
            TITLE,
            "Save commands you run often, then paste or run them in one click.",
        );
        return;
    }
    let list = state.list_rect(content);
    for (i, item) in state.items.iter().enumerate() {
        let row = state.row_rect(content, i);
        if row.y < list.y - 0.5 || row.bottom() > list.bottom() + 0.5 {
            continue;
        }
        sugarloaf.rounded_rect(
            None,
            row.x,
            row.y,
            row.width,
            row.height,
            theme.frame,
            0.0,
            ROW_RADIUS,
            0,
        );
        draw_ui_text(
            sugarloaf,
            row.x + ROW_PAD_X,
            row.y + 16.0,
            &item.name,
            15.0,
            theme.text,
            UiWeight::Medium,
        );
        draw_mono_text(
            sugarloaf,
            row.x + ROW_PAD_X,
            row.y + 16.0 + 15.0 + 8.0,
            &item.cmd,
            11.0,
            [0xE4, 0xDE, 0xEF, 255],
            UiWeight::Regular,
        );
        let [paste, run, delete] = state.row_buttons(content, i);
        paint_button(
            sugarloaf,
            theme,
            &paste,
            state_of(SnippetsHit::Paste(i)),
            PASTE,
            None,
        );
        paint_button(
            sugarloaf,
            theme,
            &run,
            state_of(SnippetsHit::Run(i)),
            RUN,
            None,
        );
        paint_button(
            sugarloaf,
            theme,
            &delete,
            state_of(SnippetsHit::Delete(i)),
            DELETE,
            None,
        );
    }
}
