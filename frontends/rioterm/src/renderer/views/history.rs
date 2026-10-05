//! History view painter and input entry points.
//!
//! Geometry and state come from `terminus_ui::views::history`; rows are the
//! list component's history row (with a real "Run again" button) and the
//! filter is the input component's search box.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::components::input::SearchKind;
use terminus_ui::components::list::HISTORY_ROW_HEIGHT;
use terminus_ui::geom::Rect;
use terminus_ui::theme::ChromeTheme;
use terminus_ui::tokens::{font_size, radius};
use terminus_ui::views::history::{
    self as ui, EmptyKind, HistoryAction, HistoryHit, HistoryItem, HistoryState,
};

use crate::renderer::chrome::{paint_flat, paint_surface_stroke};
use crate::renderer::components::button::label_spec;
use crate::renderer::components::input::paint_search;
use crate::renderer::components::list::{paint_history_row, Action};
use crate::renderer::ui_text::{
    draw_ui_text, measure_mono_text, measure_ui_text, UiWeight,
};
use terminus_ui::components::button::{ButtonKind, ButtonSize};

const RUN_AGAIN: &str = "Run again";
const PLACEHOLDER: &str = "Filter history";

fn run_again_width(s: &mut Sugarloaf) -> f32 {
    label_spec(
        s,
        (0.0, 0.0),
        ButtonKind::Secondary,
        ButtonSize::Medium,
        RUN_AGAIN,
        false,
    )
    .width()
}

pub fn paint(
    s: &mut Sugarloaf,
    theme: &ChromeTheme,
    content: Rect,
    state: &HistoryState,
    now: i64,
) {
    let scale = s.scale_factor();
    let l = ui::layout(content);
    paint_search(
        s,
        theme,
        &l.filter,
        SearchKind::Search,
        &state.filter.value,
        PLACEHOLDER,
        scale,
    );
    if state.filter_focused {
        // Focus outline: four hairlines (the stroke helper fills opaque).
        let r = l.filter.box_rect;
        for edge in [
            Rect::new(r.x, r.y, r.width, 1.0),
            Rect::new(r.x, r.bottom() - 1.0, r.width, 1.0),
            Rect::new(r.x, r.y, 1.0, r.height),
            Rect::new(r.right() - 1.0, r.y, 1.0, r.height),
        ] {
            paint_flat(s, &edge, theme.hover_border, 0.15, 7);
        }
        let prefix = state.filter.prefix();
        let w = measure_ui_text(s, &prefix, 14.0, UiWeight::Regular);
        let caret = Rect::new(
            l.filter.text.x + w,
            l.filter.text.y + 1.0,
            1.0,
            l.filter.text.height - 2.0,
        );
        paint_flat(s, &caret, [0.93, 0.92, 0.96, 1.0], 0.2, 7);
    }

    if let Some(kind) = state.empty_kind() {
        paint_empty(s, theme, l.list, kind, state.recording);
        return;
    }

    let visible = state.visible();
    for i in ui::row_range(content, state.scroll, visible.len()) {
        let item = &state.items[visible[i]];
        let rect = ui::row_rect(content, state.scroll, i);
        // No GPU clipping: skip rows that are not fully inside the list.
        if rect.y < l.list.y || rect.bottom() > l.list.bottom() {
            continue;
        }
        if state.hover == Some(i) {
            let mut wash = rect;
            wash.x -= 8.0;
            wash.width += 16.0;
            paint_surface_stroke(
                s,
                &wash,
                theme.surface,
                None,
                radius::SMALL,
                0.0,
                0.05,
                0,
                false,
            );
        }
        paint_history_row(
            s,
            theme,
            rect,
            &item.command,
            &item.cwd,
            &ui::relative_time(now, item.at),
            Some(&Action::Secondary(RUN_AGAIN)),
        );
    }
}

fn paint_empty(
    s: &mut Sugarloaf,
    theme: &ChromeTheme,
    list: Rect,
    kind: EmptyKind,
    recording: bool,
) {
    let (title, body) = match kind {
        EmptyKind::NoMatch => (ui::NO_MATCH_TITLE, "Try a different filter."),
        EmptyKind::NoHistory if !recording => (ui::EMPTY_TITLE, ui::EMPTY_BODY_OFF),
        EmptyKind::NoHistory => (ui::EMPTY_TITLE, ui::EMPTY_BODY),
    };
    let x = list.x;
    let mut y = list.y + 24.0;
    draw_ui_text(
        s,
        x,
        y,
        title,
        font_size::BODY,
        theme.text,
        UiWeight::SemiBold,
    );
    y += 28.0;
    let max_w = list.width.min(560.0);
    for line in wrap(s, body, font_size::LABEL, max_w) {
        draw_ui_text(
            s,
            x,
            y,
            &line,
            font_size::LABEL,
            theme.text_muted,
            UiWeight::Regular,
        );
        y += 20.0;
    }
}

fn wrap(s: &mut Sugarloaf, text: &str, size: f32, max_w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let cand = if cur.is_empty() {
            word.to_string()
        } else {
            format!("{cur} {word}")
        };
        if !cur.is_empty() && measure_ui_text(s, &cand, size, UiWeight::Regular) > max_w {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_string();
        } else {
            cur = cand;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

fn hit(
    s: &mut Sugarloaf,
    content: Rect,
    state: &HistoryState,
    x: f32,
    y: f32,
) -> HistoryHit {
    let act = run_again_width(s);
    let mut widths = std::collections::HashMap::new();
    for it in &state.items {
        widths
            .entry(it.cwd.clone())
            .or_insert_with(|| measure_mono_text(s, &it.cwd, 11.0, UiWeight::Regular));
    }
    ui::hit_test(
        content,
        state,
        &|t: &str| widths.get(t).copied().unwrap_or(0.0),
        act,
        x,
        y,
    )
}

// ---- input entry points (same `content` as paint; coordinates logical) ----

/// Pointer press. Returns the action the app must execute.
pub fn pointer_press(
    s: &mut Sugarloaf,
    content: Rect,
    state: &mut HistoryState,
    x: f32,
    y: f32,
) -> Option<HistoryAction> {
    let h = hit(s, content, state, x, y);
    ui::press(state, h)
}

/// Pointer shape at `(x, y)` (hand over "Run again", I-beam on the filter).
pub fn cursor_at(
    s: &mut Sugarloaf,
    content: Rect,
    state: &HistoryState,
    x: f32,
    y: f32,
) -> terminus_ui::ChromeCursor {
    ui::cursor_for(hit(s, content, state, x, y))
}

/// Pointer move; `true` when the hover state changed (needs a redraw).
pub fn pointer_move(
    s: &mut Sugarloaf,
    content: Rect,
    state: &mut HistoryState,
    x: f32,
    y: f32,
) -> bool {
    let (hover, action) = match hit(s, content, state, x, y) {
        HistoryHit::Row(i) => (Some(i), false),
        HistoryHit::RunAgain(i) => (Some(i), true),
        _ => (None, false),
    };
    let changed = hover != state.hover || action != state.hover_action;
    state.hover = hover;
    state.hover_action = action;
    changed
}

pub fn wheel(content: Rect, state: &mut HistoryState, dy: f32) {
    state.wheel(dy, content);
}

/// Typed text (while the filter is focused).
pub fn text(state: &mut HistoryState, text: &str) -> bool {
    state.type_text(text)
}

// ---- preview (TERMINUS_VIEW_PREVIEW=history) ----

pub fn preview_requested() -> bool {
    std::env::var("TERMINUS_VIEW_PREVIEW").is_ok_and(|v| v == "history")
}

fn seeded(now: i64) -> Vec<HistoryItem> {
    let row = |id: &str, cmd: &str, cwd: &str, ago: i64| HistoryItem {
        id: id.into(),
        command: cmd.into(),
        cwd: cwd.into(),
        at: now - ago,
    };
    vec![
        row("1", "git pull", "~/app", 120),
        row("2", "docker compose up -d", "~/app", 300),
        row("3", "sudo systemctl restart nginx", "~", 3600),
        row("4", "tail -f /var/log/nginx/error.log", "~", 30 * 3600),
        row("5", "df -h", "/var/log", 3 * 86_400),
    ]
}

thread_local! {
    static PREVIEW: std::cell::RefCell<Option<HistoryState>> = const { std::cell::RefCell::new(None) };
}

/// Paint the view full-window: content = window minus a 260px left strip and
/// a 96px top strip. Reads the real local history; seeds samples when the
/// store has none (or `TERMINUS_VIEW_PREVIEW_SEED=1`).
pub fn paint_preview(s: &mut Sugarloaf, theme: &ChromeTheme) {
    let scale = s.scale_factor();
    let size = s.window_size();
    let (w, h) = (size.width / scale, size.height / scale);
    crate::renderer::ui_text::sync_ui_fonts(s);
    paint_flat(s, &Rect::new(0.0, 0.0, w, h), theme.frame, 0.0, 0);
    let content = Rect::new(260.0, 96.0, (w - 260.0).max(0.0), (h - 96.0).max(0.0));
    paint_flat(s, &content, theme.canvas, 0.01, 0);
    let now = chrono::Utc::now().timestamp();
    PREVIEW.with(|p| {
        let mut p = p.borrow_mut();
        let state = p.get_or_insert_with(|| {
            let home = std::env::var("HOME").ok();
            let real = std::env::var_os("TERMINUS_VIEW_PREVIEW_SEED")
                .is_none()
                .then(|| {
                    crate::history_worker::to_items(
                        &crate::history_worker::load_blocking("local", 500),
                        home.as_deref(),
                    )
                })
                .unwrap_or_default();
            let items = if real.is_empty() { seeded(now) } else { real };
            let mut st = HistoryState::new(items, true);
            if let Ok(f) = std::env::var("TERMINUS_VIEW_PREVIEW_FILTER") {
                st.filter = terminus_ui::text_field::TextDraft::new(f);
                st.filter_focused = true;
            }
            st
        });
        if std::env::var_os("TERMINUS_VIEW_PREVIEW_EMPTY").is_some() {
            state.items.clear();
        }
        paint(s, theme, content, state, now);
    });
    let _ = HISTORY_ROW_HEIGHT;
}
