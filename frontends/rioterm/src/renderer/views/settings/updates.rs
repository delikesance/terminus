//! Updates tab painter.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::button::{ButtonSize, ButtonState};
use terminus_ui::components::list::{CardState, CARD_RADIUS};
use terminus_ui::components::selection::ControlState;
use terminus_ui::geom::Rect;
use terminus_ui::theme::ChromeTheme;
use terminus_ui::tokens::font_size;
use terminus_ui::views::settings::updates::{ToggleRow, UpdatesState, UpdatesTarget};

use super::keys::text_top;
use super::with_measure;
use crate::renderer::components::button::{label_spec, paint_button};
use crate::renderer::components::selection::paint_toggle;
use crate::renderer::ui_text::{draw_ui_text, UiWeight};

fn card(sugarloaf: &mut Sugarloaf, theme: &ChromeTheme, r: &Rect, hover: bool) {
    let bg = if hover {
        CardState::Hover
    } else {
        CardState::Default
    }
    .background(theme);
    sugarloaf.rounded_rect(None, r.x, r.y, r.width, r.height, bg, 0.1, CARD_RADIUS, 0);
}

fn two_lines(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    text: &Rect,
    title: &str,
    sub: &str,
    sub_color: [u8; 4],
) {
    // 15px title, 4px gap, 13px sub-line, centred as a block.
    let block = 20.0 + 4.0 + 17.0;
    let top = text.y + (text.height - block) / 2.0;
    draw_ui_text(
        sugarloaf,
        text.x,
        top,
        title,
        font_size::BODY,
        theme.text,
        UiWeight::Medium,
    );
    draw_ui_text(
        sugarloaf,
        text.x,
        top + 24.0,
        sub,
        font_size::LABEL,
        sub_color,
        UiWeight::Regular,
    );
}

pub fn paint(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    content: Rect,
    s: &UpdatesState,
) {
    let l = with_measure(sugarloaf, |m| s.layout(content, m));
    card(sugarloaf, theme, &l.card, false);
    let sub_color = if s.status.is_error() {
        theme.danger_text
    } else {
        theme.text_muted
    };
    two_lines(
        sugarloaf,
        theme,
        &l.text,
        &s.title(),
        &s.status.text(),
        sub_color,
    );
    let (target, label, kind) = s.primary_button();
    let spec = label_spec(
        sugarloaf,
        (l.button.x, l.button.y),
        kind,
        ButtonSize::Medium,
        label,
        false,
    );
    paint_button(
        sugarloaf,
        theme,
        &spec,
        ButtonState::resolve(s.hover == Some(target), false, false, s.button_disabled()),
        label,
        None,
    );

    let rows: [(&ToggleRow, &str, &str, bool, UpdatesTarget); 2] = [
        (
            &l.check,
            "Check for updates",
            "When Terminus starts, then once a day.",
            s.check,
            UpdatesTarget::CheckToggle,
        ),
        (
            &l.auto,
            "Install automatically",
            "Swap in the new version without asking.",
            s.auto_install,
            UpdatesTarget::AutoToggle,
        ),
    ];
    for (row, title, desc, on, target) in rows {
        card(sugarloaf, theme, &row.card, s.hover == Some(target));
        two_lines(sugarloaf, theme, &row.text, title, desc, theme.text_muted);
        paint_toggle(
            sugarloaf,
            theme,
            (row.toggle.x, row.toggle.y),
            on,
            ControlState::Default,
        );
    }
    let _ = text_top;
}
