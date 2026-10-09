// Copyright (c) 2026-present, Terminus Contributors.

use super::color::{color_from_f32, with_alpha};
use super::surfaces::{paint_caret, paint_flat, paint_surface};
use super::text::{draw_text, elide, opts};
use super::{
    CARET_WIDTH, DEPTH_CONTENT, DEPTH_DIALOG, HINT_SIZE, ORDER_CONTENT, ORDER_DIALOG,
    ROW_SUB_SIZE, ROW_TITLE_SIZE,
};
use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::geom::Rect;
use terminus_ui::theme::ChromeTheme;

/// Shared settings field card (label + input row).
///
/// `trailing_slot` reserves space on the right of the input for an
/// adornment (e.g. passphrase eye) so value text never overlaps it.
///
/// When `paint_text` is false, only the card/input quads are drawn,
/// used while a popover covers the card so UI text (always last pass)
/// does not bleed through the menu.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_field_card(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    card: Rect,
    label: &str,
    value: &str,
    focused: bool,
    placeholder: bool,
    trailing_slot: f32,
    paint_text: bool,
) {
    let (depth, order) = (DEPTH_DIALOG, ORDER_DIALOG);
    paint_surface(
        sugarloaf,
        &card,
        theme.button_bg,
        None,
        12.0,
        depth + 0.03,
        order,
        false,
    );
    if paint_text {
        draw_text(
            sugarloaf,
            card.x + terminus_ui::settings::FIELD_CARD_PAD,
            card.y + terminus_ui::settings::FIELD_CARD_PAD,
            label,
            HINT_SIZE,
            theme.text_muted,
            false,
        );
    }
    let field_bg = theme.field_bg;
    let input = terminus_ui::settings::field_input_in_card(card);
    if focused {
        paint_surface(
            sugarloaf,
            &input,
            field_bg,
            Some(theme.field_border_focus),
            8.0,
            depth + 0.039,
            order,
            false,
        );
    } else {
        paint_surface(
            sugarloaf,
            &input,
            field_bg,
            None,
            8.0,
            depth + 0.04,
            order,
            false,
        );
    }
    if paint_text {
        let color = if placeholder {
            theme.text_placeholder
        } else {
            theme.text
        };
        let text_budget = (input.width
            - terminus_ui::settings::FIELD_TEXT_INSET
            - trailing_slot.max(terminus_ui::settings::FIELD_TEXT_INSET))
        .max(0.0);
        let shown = elide(
            sugarloaf,
            value,
            text_budget,
            &opts(ROW_SUB_SIZE, color, false),
        );
        let text_y = input.y + (input.height - ROW_SUB_SIZE) * 0.5;
        draw_text(
            sugarloaf,
            input.x + terminus_ui::settings::FIELD_TEXT_INSET,
            text_y,
            &shown,
            ROW_SUB_SIZE,
            color,
            false,
        );
    }
}

/// Accent wash behind the selected span of a field card. Text and
/// selection come from the shared field paint model, so a masked field
/// highlights the same character positions it displays.
pub(crate) fn paint_field_selection(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    card: Rect,
    paint: &terminus_ui::FieldPaint,
    trailing_slot: f32,
    depth: f32,
    order: u8,
) {
    let Some((start, end)) = paint.selection else {
        return;
    };
    if end <= start || paint.placeholder {
        return;
    }
    let input = terminus_ui::settings::field_input_in_card(card);
    let text_x = input.x + terminus_ui::settings::FIELD_TEXT_INSET;
    let text_budget = (input.width
        - terminus_ui::settings::FIELD_TEXT_INSET
        - trailing_slot.max(terminus_ui::settings::FIELD_TEXT_INSET))
    .max(0.0);
    if text_budget <= 0.0 {
        return;
    }
    let text_opts = opts(ROW_SUB_SIZE, theme.text, false);
    let before: String = paint.text.chars().take(start).collect();
    let through: String = paint.text.chars().take(end).collect();
    let before_shown = elide(sugarloaf, &before, text_budget, &text_opts);
    let through_shown = elide(sugarloaf, &through, text_budget, &text_opts);
    let start_x = sugarloaf.text_mut().measure(&before_shown, &text_opts);
    let end_x = sugarloaf.text_mut().measure(&through_shown, &text_opts);
    let x = (text_x + start_x).min(text_x + text_budget);
    let right = (text_x + end_x).min(text_x + text_budget);
    paint_flat(
        sugarloaf,
        &Rect::new(
            x,
            input.y + 6.0,
            (right - x).max(CARET_WIDTH),
            (input.height - 12.0).max(1.0),
        ),
        with_alpha(theme.accent, 0.35),
        depth,
        order,
    );
}

/// Caret inside a field card, positioned after `prefix` (not always end-of-value).
pub(crate) fn paint_field_caret_prefix(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    card: Rect,
    prefix: &str,
    trailing_slot: f32,
) {
    paint_field_caret_prefix_at(
        sugarloaf,
        theme,
        card,
        prefix,
        trailing_slot,
        DEPTH_DIALOG,
        ORDER_DIALOG,
    );
}

pub(crate) fn paint_field_caret_prefix_at(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    card: Rect,
    prefix: &str,
    trailing_slot: f32,
    depth: f32,
    order: u8,
) {
    let input = terminus_ui::settings::field_input_in_card(card);
    let text_x = input.x + terminus_ui::settings::FIELD_TEXT_INSET;
    let text_budget = (input.width
        - terminus_ui::settings::FIELD_TEXT_INSET
        - trailing_slot.max(terminus_ui::settings::FIELD_TEXT_INSET))
    .max(0.0);
    let shown = elide(
        sugarloaf,
        prefix,
        text_budget,
        &opts(ROW_SUB_SIZE, theme.text, false),
    );
    let advance = sugarloaf
        .text_mut()
        .measure(&shown, &opts(ROW_SUB_SIZE, theme.text, false));
    let caret_x = (text_x + advance).min(text_x + text_budget);
    let caret_y = input.y + (input.height - 16.0) * 0.5;
    paint_caret(
        sugarloaf,
        caret_x,
        caret_y,
        16.0,
        theme.accent,
        depth + 0.05,
        order,
    );
}

/// Inline rename name + optional selection wash + caret.
pub(crate) fn paint_rename_text(
    sugarloaf: &mut Sugarloaf,
    draft: &terminus_ui::TextDraft,
    text_x: f32,
    text_y: f32,
    theme: &ChromeTheme,
) {
    let accent = color_from_f32(theme.accent);
    let title_opts = opts(ROW_TITLE_SIZE, accent, true);
    if let Some((start, end)) = draft.selection_range() {
        let before: String = draft.value.chars().take(start).collect();
        let selected: String =
            draft.value.chars().skip(start).take(end - start).collect();
        let bx = sugarloaf.text_mut().measure(&before, &title_opts);
        let sw = sugarloaf
            .text_mut()
            .measure(&selected, &title_opts)
            .max(2.0);
        paint_flat(
            sugarloaf,
            &Rect::new(text_x + bx, text_y - 1.0, sw, ROW_TITLE_SIZE + 2.0),
            with_alpha(theme.accent, 0.35),
            DEPTH_CONTENT + 0.02,
            ORDER_CONTENT,
        );
    }
    draw_text(
        sugarloaf,
        text_x,
        text_y,
        &draft.value,
        ROW_TITLE_SIZE,
        accent,
        true,
    );
    let prefix = draft.prefix();
    let w = sugarloaf.text_mut().measure(&prefix, &title_opts);
    paint_caret(
        sugarloaf,
        text_x + w,
        text_y,
        ROW_TITLE_SIZE,
        theme.accent,
        DEPTH_CONTENT + 0.03,
        ORDER_CONTENT,
    );
}
