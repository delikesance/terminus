//! Snippets view painter + preview harness.
//!
//! Geometry and hit-testing come from `terminus_ui::views::snippets`; this
//! file walks the same rects with the Input / Button / List components.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::button::{ButtonKind, ButtonSize, ButtonState};
use terminus_ui::components::input::SearchKind;
use terminus_ui::components::list::{CardState, CARD_HEIGHT};
use terminus_ui::geom::Rect;
use terminus_ui::icons::Icon;
use terminus_ui::theme::ChromeTheme;
use terminus_ui::tokens::font_size;
use terminus_ui::views::snippets::{
    seed, tag_of, LabelWidths, SnippetsHit, SnippetsView,
};

use crate::renderer::chrome::paint_flat;
use crate::renderer::components::button::{label_spec, paint_button};
use crate::renderer::components::input::paint_search;
use crate::renderer::components::list::{paint_card, Action, CardContent};
use crate::renderer::ui_text::{draw_ui_text, measure_ui_text, UiWeight};

/// Measure the three button labels (call before hit-testing if the font
/// changed; `paint` does it every frame).
pub fn measure_labels(s: &mut Sugarloaf) -> LabelWidths {
    let mut w = |t: &str, k: ButtonKind, sz: ButtonSize| {
        label_spec(s, (0.0, 0.0), k, sz, t, false);
        let weight = if k.semibold() {
            UiWeight::SemiBold
        } else {
            UiWeight::Medium
        };
        measure_ui_text(s, t, sz.font_size(), weight)
    };
    LabelWidths {
        new_snippet: w("New snippet", ButtonKind::Primary, ButtonSize::Large),
        paste: w("Paste", ButtonKind::Secondary, ButtonSize::Medium),
        run: w("Run", ButtonKind::Text, ButtonSize::Medium),
    }
}

/// Paint the Snippets view into `content`.
pub fn paint(
    s: &mut Sugarloaf,
    theme: &ChromeTheme,
    content: Rect,
    state: &mut SnippetsView,
    device_scale: f32,
) {
    state.labels = measure_labels(s);
    paint_measured(s, theme, content, state, device_scale);
}

/// Paint with label widths already in `state.labels` (the shell measures
/// them in `renderer::screens::measure`, before hit-testing).
pub fn paint_measured(
    s: &mut Sugarloaf,
    theme: &ChromeTheme,
    content: Rect,
    state: &SnippetsView,
    device_scale: f32,
) {
    // Toolbar: filter field + New snippet.
    let search = state.search(content);
    paint_search(
        s,
        theme,
        &search,
        SearchKind::Search,
        &state.filter,
        "Filter snippets",
        device_scale,
    );
    if state.filter_focused {
        let w = measure_ui_text(s, &state.filter, 14.0, UiWeight::Regular);
        paint_flat(
            s,
            &Rect::new(
                search.text.x + w,
                search.text.y + 1.0,
                1.5,
                search.text.height - 2.0,
            ),
            theme.accent,
            0.12,
            7,
        );
    }
    let nb = state.new_button(content);
    let nb_state = if state.hover == Some(SnippetsHit::NewButton) {
        ButtonState::Hover
    } else {
        ButtonState::Default
    };
    paint_button(s, theme, &nb, nb_state, "New snippet", None);

    // Cards.
    let area = state.list_area(content);
    let visible = state.visible();
    if visible.is_empty() {
        let (title, hint) = if state.items.is_empty() {
            (
                "No snippets yet",
                "Save the commands you type often, then paste or run them in one click.",
            )
        } else {
            (
                "No snippet matches",
                "Try another word from the title, command or tag.",
            )
        };
        let tw = measure_ui_text(s, title, font_size::BODY, UiWeight::Medium);
        let hw = measure_ui_text(s, hint, font_size::LABEL, UiWeight::Regular);
        let cx = area.x + area.width / 2.0;
        let cy = area.y + (area.height / 2.0).min(120.0);
        draw_ui_text(
            s,
            cx - tw / 2.0,
            cy - 14.0,
            title,
            font_size::BODY,
            theme.text,
            UiWeight::Medium,
        );
        draw_ui_text(
            s,
            cx - hw / 2.0,
            cy + 10.0,
            hint,
            font_size::LABEL,
            theme.text_muted,
            UiWeight::Regular,
        );
        return;
    }
    for (i, item) in visible.iter().enumerate() {
        let rect = state.card_rect(content, i);
        if rect.y < area.y - 0.5 || rect.y + CARD_HEIGHT > area.bottom() + CARD_HEIGHT {
            continue;
        }
        let hovered = matches!(
            state.hover,
            Some(SnippetsHit::Card(j) | SnippetsHit::Paste(j) | SnippetsHit::Run(j)
                | SnippetsHit::Delete(j)) if j == i
        );
        let actions = [Action::Secondary("Paste"), Action::Ghost("Run")];
        paint_card(
            s,
            theme,
            rect,
            if hovered {
                CardState::Hover
            } else {
                CardState::Default
            },
            &CardContent {
                title: &item.name,
                detail: &item.cmd,
                meta: tag_of(item),
                dot: None,
                actions: &actions,
            },
        );
        let d = state.delete_rect(content, i);
        let spec = terminus_ui::components::button::ButtonSpec::icon_only(
            (d.x, d.y),
            ButtonKind::Quiet,
            ButtonSize::Medium,
        );
        let st = if state.hover == Some(SnippetsHit::Delete(i)) {
            ButtonState::Hover
        } else {
            ButtonState::Default
        };
        paint_button(s, theme, &spec, st, "Delete snippet", Some(Icon::Trash2));
    }
}

/// `TERMINUS_VIEW_PREVIEW=snippets`.
pub fn preview_selected() -> bool {
    std::env::var("TERMINUS_VIEW_PREVIEW")
        .map(|v| v.split(',').any(|p| p.trim() == "snippets"))
        .unwrap_or(false)
}

/// Full-window preview: content = window minus a 260 px strip on the left
/// and a 96 px strip on top. `TERMINUS_VIEW_PREVIEW_FILTER` seeds the filter;
/// `TERMINUS_VIEW_PREVIEW_EMPTY=1` shows the empty state.
pub fn paint_preview(s: &mut Sugarloaf, theme: &ChromeTheme) {
    let scale = s.scale_factor();
    let size = s.window_size();
    let (w, h) = (size.width / scale, size.height / scale);
    crate::renderer::ui_text::sync_ui_fonts(s);
    paint_flat(s, &Rect::new(0.0, 0.0, w, h), theme.canvas, 0.0, 0);
    let content = Rect::new(260.0, 96.0, (w - 260.0).max(0.0), (h - 96.0).max(0.0));
    let items = if std::env::var_os("TERMINUS_VIEW_PREVIEW_EMPTY").is_some() {
        Vec::new()
    } else {
        seed()
    };
    let mut state = SnippetsView::new(items);
    if let Ok(f) = std::env::var("TERMINUS_VIEW_PREVIEW_FILTER") {
        state.filter = f;
        state.filter_focused = true;
    }
    paint(s, theme, content, &mut state, scale);
}
