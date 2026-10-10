use super::CARD_ACTION_HEIGHT;
use crate::geom::Rect;

pub const HISTORY_ROW_HEIGHT: f32 = 52.0;
pub const HISTORY_GAP: f32 = 18.0;
pub const HISTORY_TIME_WIDTH: f32 = 90.0;
pub const HISTORY_DIVIDER: f32 = 1.0;

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryRowLayout {
    pub rect: Rect,
    pub command: Rect,
    pub cwd: Rect,
    pub time: Rect,
    /// Trailing action slot, `None` when `action_width` is 0.
    pub action: Option<Rect>,
    /// Hairline along the bottom edge.
    pub divider: Rect,
}

/// `text` in at most `max_chars` characters, cut at the start with an
/// ellipsis (paths: the end is what tells them apart).
pub fn elide_start(text: &str, max_chars: usize) -> String {
    let n = text.chars().count();
    if n <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    let tail: String = text.chars().skip(n - (max_chars - 1)).collect();
    format!("\u{2026}{tail}")
}

/// `text` in at most `max_chars` characters, cut at the end with an
/// ellipsis.
pub fn elide_end(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    let head: String = text.chars().take(max_chars - 1).collect();
    format!("{head}\u{2026}")
}

/// `cwd_width` is the measured width of the cwd text, `action_width` the
/// width of the trailing button (0 for none).
pub fn history_row_layout(
    rect: Rect,
    cwd_width: f32,
    action_width: f32,
) -> HistoryRowLayout {
    let mid = rect.y + rect.height / 2.0;
    let mut right = rect.right();
    let action = (action_width > 0.0).then(|| {
        let a = Rect::new(
            right - action_width,
            mid - CARD_ACTION_HEIGHT / 2.0,
            action_width,
            CARD_ACTION_HEIGHT,
        );
        right = a.x - HISTORY_GAP;
        a
    });
    let time = Rect::new(
        right - HISTORY_TIME_WIDTH,
        rect.y,
        HISTORY_TIME_WIDTH,
        rect.height,
    );
    // The cwd gets at most half of what is left of the time column, so a
    // deep path never runs over the command (the painter elides it).
    let room = (time.x - HISTORY_GAP - rect.x).max(0.0);
    let cwd_width = cwd_width.min(((room - HISTORY_GAP) / 2.0).max(0.0));
    let cwd = Rect::new(
        time.x - HISTORY_GAP - cwd_width,
        rect.y,
        cwd_width,
        rect.height,
    );
    let command = Rect::new(
        rect.x,
        rect.y,
        (cwd.x - HISTORY_GAP - rect.x).max(0.0),
        rect.height,
    );
    let divider = Rect::new(
        rect.x,
        rect.bottom() - HISTORY_DIVIDER,
        rect.width,
        HISTORY_DIVIDER,
    );
    HistoryRowLayout {
        rect,
        command,
        cwd,
        time,
        action,
        divider,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryHit {
    Action,
    Row,
}

pub fn history_hit(
    rect: Rect,
    cwd_width: f32,
    action_width: f32,
    x: f32,
    y: f32,
) -> Option<HistoryHit> {
    if !rect.contains(x, y) {
        return None;
    }
    let l = history_row_layout(rect, cwd_width, action_width);
    if l.action.is_some_and(|a| a.contains(x, y)) {
        Some(HistoryHit::Action)
    } else {
        Some(HistoryHit::Row)
    }
}
