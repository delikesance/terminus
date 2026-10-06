//! History view: command rows of one machine, newest first, with a filter.
//!
//! Pure state, geometry and hit-testing. Everything is computed from the
//! `content` rect the shell gives the view; the painter walks the same rects.

use crate::components::input::{search_layout, SearchKind, SearchLayout, SEARCH_HEIGHT};
use crate::components::list::HISTORY_ROW_HEIGHT;
use crate::geom::Rect;
use crate::text_field::{TextDraft, TextEdit};

/// Padding around the list (design: `padding: 20px 28px`).
pub const PAD_X: f32 = 28.0;
pub const PAD_Y: f32 = 20.0;
/// Subtle filter field: capped width, gap below it.
pub const FILTER_MAX_WIDTH: f32 = 320.0;
pub const FILTER_GAP: f32 = 12.0;
pub const FILTER_MAX_BYTES: usize = 200;
pub const WHEEL_STEP: f32 = 40.0;

/// One recorded command, ready to show.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoryItem {
    pub id: String,
    pub command: String,
    /// Display form of the working directory (`~/app`), may be empty.
    pub cwd: String,
    /// Unix seconds.
    pub at: i64,
}

/// What the app must do after an input event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryAction {
    /// Type `command` into the machine's active session and press Enter.
    RunAgain { command: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryHit {
    Filter,
    RunAgain(usize),
    Row(usize),
    Background,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmptyKind {
    /// Nothing recorded yet: explain how history gets recorded.
    NoHistory,
    /// The filter hides everything.
    NoMatch,
}

#[derive(Debug, Clone, Default)]
pub struct HistoryState {
    pub items: Vec<HistoryItem>,
    pub filter: TextDraft,
    pub filter_focused: bool,
    pub scroll: f32,
    /// Hovered row, as an index into [`HistoryState::visible`].
    pub hover: Option<usize>,
    /// Hovered "Run again" button.
    pub hover_action: bool,
    /// `false` when recording is switched off (empty state says so).
    pub recording: bool,
}

impl HistoryState {
    pub fn new(items: Vec<HistoryItem>, recording: bool) -> Self {
        Self {
            items,
            recording,
            ..Self::default()
        }
    }

    /// Indices into `items` matching the filter (case-insensitive substring
    /// of command or cwd), in stored order (newest first).
    pub fn visible(&self) -> Vec<usize> {
        let q = self.filter.value.trim().to_lowercase();
        self.items
            .iter()
            .enumerate()
            .filter(|(_, it)| {
                q.is_empty()
                    || it.command.to_lowercase().contains(&q)
                    || it.cwd.to_lowercase().contains(&q)
            })
            .map(|(i, _)| i)
            .collect()
    }

    pub fn empty_kind(&self) -> Option<EmptyKind> {
        if self.items.is_empty() {
            Some(EmptyKind::NoHistory)
        } else if self.visible().is_empty() {
            Some(EmptyKind::NoMatch)
        } else {
            None
        }
    }

    /// Replace the entries (keeps the filter), clamping scroll.
    pub fn set_items(&mut self, items: Vec<HistoryItem>) {
        self.items = items;
        self.hover = None;
    }

    /// Typed text goes to the filter while it is focused.
    pub fn type_text(&mut self, text: &str) -> bool {
        if !self.filter_focused {
            return false;
        }
        let changed = self.filter.insert(text, FILTER_MAX_BYTES, false);
        if changed {
            self.scroll = 0.0;
        }
        changed
    }

    pub fn backspace(&mut self) -> bool {
        self.edit(TextEdit::Backspace { by_word: false })
    }

    /// Shared text editing (Backspace, Delete, caret, selection).
    pub fn edit(&mut self, edit: TextEdit) -> bool {
        let changed = self.filter_focused && self.filter.apply(edit);
        if changed {
            self.scroll = 0.0;
        }
        changed
    }

    /// Escape: clear the filter first, then drop focus. `true` if consumed.
    pub fn escape(&mut self) -> bool {
        if !self.filter.value.is_empty() {
            self.filter.clear();
            self.scroll = 0.0;
            true
        } else if self.filter_focused {
            self.filter_focused = false;
            true
        } else {
            false
        }
    }

    pub fn wheel(&mut self, dy: f32, content: Rect) {
        let max = max_scroll(self.visible().len(), content);
        self.scroll = (self.scroll - dy * WHEEL_STEP).clamp(0.0, max);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HistoryLayout {
    pub filter: SearchLayout,
    /// Clip rect of the scrolling list.
    pub list: Rect,
}

pub fn layout(content: Rect) -> HistoryLayout {
    let width = (content.width - 2.0 * PAD_X).clamp(0.0, FILTER_MAX_WIDTH);
    let filter = search_layout(
        (content.x + PAD_X, content.y + PAD_Y),
        width,
        SearchKind::Search,
        0.0,
    );
    let top = content.y + PAD_Y + SEARCH_HEIGHT + FILTER_GAP;
    let list = Rect::new(
        content.x + PAD_X,
        top,
        (content.width - 2.0 * PAD_X).max(0.0),
        (content.bottom() - top).max(0.0),
    );
    HistoryLayout { filter, list }
}

pub fn max_scroll(rows: usize, content: Rect) -> f32 {
    let list = layout(content).list;
    (rows as f32 * HISTORY_ROW_HEIGHT - list.height).max(0.0)
}

/// Rect of the `i`-th visible row at the given scroll offset.
pub fn row_rect(content: Rect, scroll: f32, i: usize) -> Rect {
    let list = layout(content).list;
    Rect::new(
        list.x,
        list.y + i as f32 * HISTORY_ROW_HEIGHT - scroll,
        list.width,
        HISTORY_ROW_HEIGHT,
    )
}

/// Range of visible-row indices that intersect the list clip.
pub fn row_range(content: Rect, scroll: f32, count: usize) -> std::ops::Range<usize> {
    let list = layout(content).list;
    let first = (scroll / HISTORY_ROW_HEIGHT).floor().max(0.0) as usize;
    let last = ((scroll + list.height) / HISTORY_ROW_HEIGHT).ceil() as usize;
    first.min(count)..last.min(count)
}

/// What is under the pointer. `measure_cwd` / `action_width` are the painter's
/// measured text and button widths (the geometry of a row depends on them).
pub fn hit_test(
    content: Rect,
    state: &HistoryState,
    measure_cwd: &dyn Fn(&str) -> f32,
    action_width: f32,
    x: f32,
    y: f32,
) -> HistoryHit {
    let l = layout(content);
    if l.filter.box_rect.contains(x, y) {
        return HistoryHit::Filter;
    }
    if !l.list.contains(x, y) {
        return HistoryHit::Background;
    }
    let visible = state.visible();
    for i in row_range(content, state.scroll, visible.len()) {
        let item = &state.items[visible[i]];
        let rect = row_rect(content, state.scroll, i);
        if let Some(hit) = crate::components::list::history_hit(
            rect,
            measure_cwd(&item.cwd),
            action_width,
            x,
            y,
        ) {
            return match hit {
                crate::components::list::HistoryHit::Action => HistoryHit::RunAgain(i),
                crate::components::list::HistoryHit::Row => HistoryHit::Row(i),
            };
        }
    }
    HistoryHit::Background
}

/// Pointer shape for what is under it: "Run again" is a hand, the filter
/// an I-beam.
pub fn cursor_for(hit: HistoryHit) -> crate::chrome::ChromeCursor {
    use crate::chrome::ChromeCursor;
    match hit {
        HistoryHit::RunAgain(_) => ChromeCursor::Pointer,
        HistoryHit::Filter => ChromeCursor::Text,
        HistoryHit::Row(_) | HistoryHit::Background => ChromeCursor::Default,
    }
}

/// Press: focus handling plus the action a "Run again" press produces.
pub fn press(state: &mut HistoryState, hit: HistoryHit) -> Option<HistoryAction> {
    state.filter_focused = hit == HistoryHit::Filter;
    match hit {
        HistoryHit::RunAgain(i) => {
            let visible = state.visible();
            let idx = *visible.get(i)?;
            Some(HistoryAction::RunAgain {
                command: state.items[idx].command.clone(),
            })
        }
        _ => None,
    }
}

/// Relative time in the design's voice: `2 min ago`, `1 h ago`, `Yesterday`.
pub fn relative_time(now: i64, then: i64) -> String {
    let secs = (now - then).max(0);
    let mins = secs / 60;
    let hours = secs / 3600;
    let days = secs / 86_400;
    if secs < 60 {
        "Just now".to_string()
    } else if hours < 1 {
        format!("{mins} min ago")
    } else if days < 1 {
        format!("{hours} h ago")
    } else if days < 2 {
        "Yesterday".to_string()
    } else if days < 30 {
        format!("{days} d ago")
    } else {
        format!("{} mo ago", days / 30)
    }
}

/// `/home/me/app` -> `~/app` when under `home`.
pub fn display_cwd(cwd: &str, home: Option<&str>) -> String {
    if let Some(home) = home.filter(|h| !h.is_empty() && *h != "/") {
        if cwd == home {
            return "~".to_string();
        }
        if let Some(rest) = cwd.strip_prefix(home) {
            if rest.starts_with('/') {
                return format!("~{rest}");
            }
        }
    }
    cwd.to_string()
}

pub const EMPTY_TITLE: &str = "No commands yet";
pub const EMPTY_BODY: &str = "Commands you run here are recorded when the shell reports them (OSC 133). Local shells are set up for you. On servers, add the Terminus line from misc/shell-integration to your shell rc. Commands starting with a space are never recorded.";
pub const EMPTY_BODY_OFF: &str =
    "Recording is turned off (TERMINUS_HISTORY=0). Unset it to record commands again.";
pub const NO_MATCH_TITLE: &str = "No matching commands";

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, cmd: &str, cwd: &str, at: i64) -> HistoryItem {
        HistoryItem {
            id: id.into(),
            command: cmd.into(),
            cwd: cwd.into(),
            at,
        }
    }

    fn sample() -> HistoryState {
        HistoryState::new(
            vec![
                item("1", "git pull", "~/app", 100),
                item("2", "docker compose up -d", "~/app", 90),
                item("3", "tail -f /var/log/nginx/error.log", "~", 80),
            ],
            true,
        )
    }

    const CONTENT: Rect = Rect {
        x: 260.0,
        y: 96.0,
        width: 900.0,
        height: 400.0,
    };

    #[test]
    fn relative_time_buckets() {
        let now = 1_000_000;
        assert_eq!(relative_time(now, now - 5), "Just now");
        assert_eq!(relative_time(now, now + 30), "Just now");
        assert_eq!(relative_time(now, now - 120), "2 min ago");
        assert_eq!(relative_time(now, now - 3_600), "1 h ago");
        assert_eq!(relative_time(now, now - 7 * 3_600), "7 h ago");
        assert_eq!(relative_time(now, now - 30 * 3_600), "Yesterday");
        assert_eq!(relative_time(now, now - 3 * 86_400), "3 d ago");
        assert_eq!(relative_time(now, now - 65 * 86_400), "2 mo ago");
    }

    #[test]
    fn cwd_is_shown_relative_to_home() {
        assert_eq!(display_cwd("/home/me/app", Some("/home/me")), "~/app");
        assert_eq!(display_cwd("/home/me", Some("/home/me")), "~");
        assert_eq!(display_cwd("/home/meta", Some("/home/me")), "/home/meta");
        assert_eq!(display_cwd("/srv", None), "/srv");
    }

    #[test]
    fn filter_matches_command_or_cwd_case_insensitively() {
        let mut s = sample();
        s.filter_focused = true;
        assert_eq!(s.visible(), vec![0, 1, 2]);
        s.type_text("DOCKER");
        assert_eq!(s.visible(), vec![1]);
        s.filter.clear();
        s.type_text("~/app");
        assert_eq!(s.visible(), vec![0, 1]);
    }

    #[test]
    fn typing_is_ignored_without_focus() {
        let mut s = sample();
        assert!(!s.type_text("x"));
        assert_eq!(s.filter.value, "");
    }

    #[test]
    fn escape_clears_then_unfocuses() {
        let mut s = sample();
        s.filter_focused = true;
        s.type_text("git");
        assert!(s.escape());
        assert_eq!(s.filter.value, "");
        assert!(s.filter_focused);
        assert!(s.escape());
        assert!(!s.filter_focused);
        assert!(!s.escape());
    }

    #[test]
    fn empty_kinds() {
        let mut s = HistoryState::new(vec![], true);
        assert_eq!(s.empty_kind(), Some(EmptyKind::NoHistory));
        let mut s2 = sample();
        assert_eq!(s2.empty_kind(), None);
        s2.filter_focused = true;
        s2.type_text("zzz");
        assert_eq!(s2.empty_kind(), Some(EmptyKind::NoMatch));
        s.recording = false;
        assert_eq!(s.empty_kind(), Some(EmptyKind::NoHistory));
    }

    #[test]
    fn layout_follows_content_and_rows_stack_under_filter() {
        let l = layout(CONTENT);
        assert_eq!(l.filter.box_rect.x, CONTENT.x + PAD_X);
        assert_eq!(l.filter.box_rect.y, CONTENT.y + PAD_Y);
        assert!(l.filter.box_rect.width <= FILTER_MAX_WIDTH);
        assert!(l.list.y >= l.filter.box_rect.bottom());
        let r0 = row_rect(CONTENT, 0.0, 0);
        let r1 = row_rect(CONTENT, 0.0, 1);
        assert_eq!(r0.y, l.list.y);
        assert_eq!(r1.y, r0.bottom());
        assert_eq!(row_rect(CONTENT, 26.0, 0).y, l.list.y - 26.0);
    }

    #[test]
    fn scroll_is_clamped_and_range_follows() {
        let mut s = HistoryState::new(
            (0..50)
                .map(|i| item(&i.to_string(), "ls", "~", i))
                .collect(),
            true,
        );
        s.wheel(-100.0, CONTENT);
        assert_eq!(s.scroll, max_scroll(50, CONTENT));
        s.wheel(1000.0, CONTENT);
        assert_eq!(s.scroll, 0.0);
        let r = row_range(CONTENT, 0.0, 50);
        assert_eq!(r.start, 0);
        assert!(r.end < 50 && r.end >= 5);
        assert_eq!(max_scroll(2, CONTENT), 0.0);
    }

    #[test]
    fn hit_test_finds_filter_action_and_row() {
        let s = sample();
        let cwd_w = |t: &str| t.len() as f32 * 7.0;
        let btn = 90.0;
        let f = layout(CONTENT).filter.box_rect;
        assert_eq!(
            hit_test(CONTENT, &s, &cwd_w, btn, f.x + 5.0, f.y + 5.0),
            HistoryHit::Filter
        );
        let row = row_rect(CONTENT, 0.0, 1);
        let mid = row.y + row.height / 2.0;
        assert_eq!(
            hit_test(CONTENT, &s, &cwd_w, btn, row.right() - 10.0, mid),
            HistoryHit::RunAgain(1)
        );
        assert_eq!(
            hit_test(CONTENT, &s, &cwd_w, btn, row.x + 10.0, mid),
            HistoryHit::Row(1)
        );
        assert_eq!(
            hit_test(CONTENT, &s, &cwd_w, btn, 5.0, 5.0),
            HistoryHit::Background
        );
    }

    #[test]
    fn run_again_is_a_hand_and_the_filter_an_i_beam() {
        use crate::chrome::ChromeCursor;
        assert_eq!(cursor_for(HistoryHit::RunAgain(0)), ChromeCursor::Pointer);
        assert_eq!(cursor_for(HistoryHit::Filter), ChromeCursor::Text);
        assert_eq!(cursor_for(HistoryHit::Row(0)), ChromeCursor::Default);
        assert_eq!(cursor_for(HistoryHit::Background), ChromeCursor::Default);
    }

    #[test]
    fn run_again_returns_the_filtered_row_command() {
        let mut s = sample();
        s.filter_focused = true;
        s.type_text("docker");
        let action = press(&mut s, HistoryHit::RunAgain(0));
        assert_eq!(
            action,
            Some(HistoryAction::RunAgain {
                command: "docker compose up -d".into()
            })
        );
        assert!(!s.filter_focused, "pressing a row drops filter focus");
        assert_eq!(press(&mut s, HistoryHit::Filter), None);
        assert!(s.filter_focused);
    }
}
