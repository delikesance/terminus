//! Lists: rows, cards, tables and their hit-testing.
//!
//! Three row families from the design board:
//!
//! * the **list card** (tunnel, snippet, SSH key, status row): optional
//!   leading status dot, title + mono detail, right meta, then 0-2 action
//!   slots laid out right to left;
//! * the **file row** (SFTP browser): icon, name, size and date columns,
//!   with default / hover / selected / drop-target / renaming states;
//! * the **history row**: mono command, mono cwd, relative time, one
//!   trailing action slot, hairline divider below.
//!
//! Everything is in logical pixels. Painters walk the `*_layout` functions
//! and the mouse uses the `*_hit` functions, so they cannot disagree.

use crate::geom::Rect;
use crate::theme::ChromeTheme;
use crate::tokens::{height, radius};

// ---------------------------------------------------------------------
// List card
// ---------------------------------------------------------------------

/// Vertical padding of a card.
pub const CARD_PAD_Y: f32 = 16.0;
/// Horizontal padding of a card.
pub const CARD_PAD_X: f32 = 20.0;
/// Gap between the flex items of a card (dot, text, meta, actions).
pub const CARD_GAP: f32 = 18.0;
/// Gap between two action buttons.
pub const CARD_ACTION_GAP: f32 = 10.0;
/// Status dot diameter.
pub const CARD_DOT: f32 = 8.0;
/// Title line (15 px) + 4 px gap + mono detail line (11 px).
pub const CARD_CONTENT_HEIGHT: f32 = 37.0;
/// Total card height.
pub const CARD_HEIGHT: f32 = 2.0 * CARD_PAD_Y + CARD_CONTENT_HEIGHT;
/// Card corner radius.
pub const CARD_RADIUS: f32 = radius::CARD;
/// Height of an action slot (a medium button).
pub const CARD_ACTION_HEIGHT: f32 = height::CONTROL_MD;

/// Card background state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardState {
    Default,
    Hover,
}

impl CardState {
    pub fn background(self, theme: &ChromeTheme) -> [f32; 4] {
        match self {
            CardState::Default => theme.frame,
            CardState::Hover => theme.surface,
        }
    }
}

/// Content-dependent inputs of a card layout (text widths are measured by
/// the painter and passed in, keeping this crate font-free).
#[derive(Debug, Clone, Copy)]
pub struct CardSpec<'a> {
    pub has_dot: bool,
    /// Measured width of the right meta text (0 when empty).
    pub meta_width: f32,
    /// Widths of 0-2 action slots, in reading order (leftmost first).
    pub action_widths: &'a [f32],
}

/// Resolved card rectangles.
#[derive(Debug, Clone, PartialEq)]
pub struct CardLayout {
    pub rect: Rect,
    pub dot: Option<Rect>,
    /// Title + detail block.
    pub text: Rect,
    pub meta: Rect,
    /// Action slots in reading order; `None` past the number of actions.
    pub actions: [Option<Rect>; 2],
}

pub fn card_layout(rect: Rect, spec: &CardSpec) -> CardLayout {
    let mid = rect.y + rect.height / 2.0;
    let mut right = rect.right() - CARD_PAD_X;
    let mut actions: [Option<Rect>; 2] = [None, None];
    let n = spec.action_widths.len().min(2);
    for i in (0..n).rev() {
        let w = spec.action_widths[i];
        actions[i] = Some(Rect::new(
            right - w,
            mid - CARD_ACTION_HEIGHT / 2.0,
            w,
            CARD_ACTION_HEIGHT,
        ));
        right -= w + CARD_ACTION_GAP;
    }
    if n > 0 {
        right = actions[0].map_or(right, |a| a.x);
    }
    let meta_right = if n > 0 { right - CARD_GAP } else { right };
    let meta = Rect::new(
        meta_right - spec.meta_width,
        rect.y + CARD_PAD_Y,
        spec.meta_width,
        CARD_CONTENT_HEIGHT,
    );
    let mut left = rect.x + CARD_PAD_X;
    let dot = if spec.has_dot {
        let d = Rect::new(left, mid - CARD_DOT / 2.0, CARD_DOT, CARD_DOT);
        left = d.right() + CARD_GAP;
        Some(d)
    } else {
        None
    };
    let text = Rect::new(
        left,
        rect.y + CARD_PAD_Y,
        (meta.x - CARD_GAP - left).max(0.0),
        CARD_CONTENT_HEIGHT,
    );
    CardLayout {
        rect,
        dot,
        text,
        meta,
        actions,
    }
}

/// What a point hit on a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardHit {
    /// Action slot index (reading order).
    Action(usize),
    Body,
}

pub fn card_hit(rect: Rect, spec: &CardSpec, x: f32, y: f32) -> Option<CardHit> {
    if !rect.contains(x, y) {
        return None;
    }
    let l = card_layout(rect, spec);
    for (i, a) in l.actions.iter().enumerate() {
        if a.is_some_and(|a| a.contains(x, y)) {
            return Some(CardHit::Action(i));
        }
    }
    Some(CardHit::Body)
}

// ---------------------------------------------------------------------
// File row
// ---------------------------------------------------------------------

pub const FILE_ROW_HEIGHT: f32 = 40.0;
pub const FILE_ROW_RADIUS: f32 = radius::SMALL;
pub const FILE_ROW_PAD_X: f32 = 10.0;
pub const FILE_ROW_GAP: f32 = 12.0;
pub const FILE_ICON_SIZE: f32 = 15.0;
pub const FILE_SIZE_WIDTH: f32 = 70.0;
pub const FILE_DATE_WIDTH: f32 = 80.0;
pub const FILE_RENAME_HEIGHT: f32 = 28.0;
pub const FILE_RENAME_RADIUS: f32 = 6.0;
pub const FILE_DROP_STROKE: f32 = 1.5;
/// Alpha of the drop-target accent fill.
pub const FILE_DROP_FILL_ALPHA: f32 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Folder,
    File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileRowState {
    Default,
    Hover,
    Selected,
    DropTarget,
    Renaming,
}

impl FileRowState {
    /// Row fill, `None` when transparent.
    pub fn background(self, theme: &ChromeTheme) -> Option<[f32; 4]> {
        match self {
            FileRowState::Default | FileRowState::Renaming => None,
            FileRowState::Hover => Some(theme.surface),
            FileRowState::Selected => Some(theme.selected),
            FileRowState::DropTarget => {
                let a = theme.accent;
                Some([a[0], a[1], a[2], FILE_DROP_FILL_ALPHA])
            }
        }
    }

    /// Inset accent stroke width, if any.
    pub fn inset_stroke(self) -> Option<f32> {
        (self == FileRowState::DropTarget).then_some(FILE_DROP_STROKE)
    }

    pub fn shows_rename_field(self) -> bool {
        self == FileRowState::Renaming
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FileRowLayout {
    pub rect: Rect,
    pub icon: Rect,
    pub name: Rect,
    /// The inline rename field (28 px, centered over the name column).
    pub rename_field: Rect,
    pub size: Rect,
    pub date: Rect,
}

pub fn file_row_layout(rect: Rect) -> FileRowLayout {
    let mid = rect.y + rect.height / 2.0;
    let date = Rect::new(
        rect.right() - FILE_ROW_PAD_X - FILE_DATE_WIDTH,
        rect.y,
        FILE_DATE_WIDTH,
        rect.height,
    );
    let size = Rect::new(
        date.x - FILE_ROW_GAP - FILE_SIZE_WIDTH,
        rect.y,
        FILE_SIZE_WIDTH,
        rect.height,
    );
    let icon = Rect::new(
        rect.x + FILE_ROW_PAD_X,
        mid - FILE_ICON_SIZE / 2.0,
        FILE_ICON_SIZE,
        FILE_ICON_SIZE,
    );
    let name_x = icon.right() + FILE_ROW_GAP;
    let name = Rect::new(
        name_x,
        rect.y,
        (size.x - FILE_ROW_GAP - name_x).max(0.0),
        rect.height,
    );
    let rename_field = Rect::new(
        name.x,
        mid - FILE_RENAME_HEIGHT / 2.0,
        name.width,
        FILE_RENAME_HEIGHT,
    );
    FileRowLayout {
        rect,
        icon,
        name,
        rename_field,
        size,
        date,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileColumn {
    Icon,
    Name,
    Size,
    Date,
    /// The row's padding / gaps.
    Row,
}

pub fn file_row_hit(rect: Rect, x: f32, y: f32) -> Option<FileColumn> {
    if !rect.contains(x, y) {
        return None;
    }
    let l = file_row_layout(rect);
    let in_x = |r: &Rect| x >= r.x && x < r.right();
    Some(if in_x(&l.icon) {
        FileColumn::Icon
    } else if in_x(&l.name) {
        FileColumn::Name
    } else if in_x(&l.size) {
        FileColumn::Size
    } else if in_x(&l.date) {
        FileColumn::Date
    } else {
        FileColumn::Row
    })
}

// ---------------------------------------------------------------------
// History row
// ---------------------------------------------------------------------

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;
    use crate::theme::ChromeTheme;

    fn card_spec<'a>(widths: &'a [f32]) -> CardSpec<'a> {
        CardSpec {
            has_dot: true,
            meta_width: 40.0,
            action_widths: widths,
        }
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn card_height_covers_padding_and_text_block() {
        assert_eq!(CARD_HEIGHT, 2.0 * 16.0 + CARD_CONTENT_HEIGHT);
        assert!(CARD_CONTENT_HEIGHT >= 36.0, "fits a medium button");
    }

    #[test]
    fn card_dot_is_8px_inside_left_padding_and_centered() {
        let l = card_layout(
            Rect::new(10.0, 20.0, 600.0, CARD_HEIGHT),
            &card_spec(&[60.0]),
        );
        let dot = l.dot.expect("dot");
        assert_eq!((dot.width, dot.height), (8.0, 8.0));
        assert_eq!(dot.x, 10.0 + 20.0);
        assert_eq!(dot.y + 4.0, 20.0 + CARD_HEIGHT / 2.0);
        // text starts after dot + 18 gap
        assert_eq!(l.text.x, dot.right() + 18.0);
    }

    #[test]
    fn card_without_dot_starts_text_at_padding() {
        let mut s = card_spec(&[]);
        s.has_dot = false;
        let l = card_layout(Rect::new(0.0, 0.0, 600.0, CARD_HEIGHT), &s);
        assert!(l.dot.is_none());
        assert_eq!(l.text.x, 20.0);
    }

    #[test]
    fn card_actions_lay_out_right_to_left_with_10px_gaps() {
        let r = Rect::new(0.0, 0.0, 600.0, CARD_HEIGHT);
        let l = card_layout(r, &card_spec(&[80.0, 36.0]));
        let a0 = l.actions[0].unwrap();
        let a1 = l.actions[1].unwrap();
        // slot 0 is the leftmost (first listed), slot 1 hugs the right padding
        assert_eq!(a1.right(), 600.0 - 20.0);
        assert_eq!(a0.right() + 10.0, a1.x);
        assert_eq!((a0.width, a1.width), (80.0, 36.0));
        assert_eq!(a0.height, 36.0);
        assert_eq!(a0.y + 18.0, CARD_HEIGHT / 2.0);
    }

    #[test]
    fn card_meta_sits_left_of_actions_and_text_fills_the_rest() {
        let r = Rect::new(0.0, 0.0, 600.0, CARD_HEIGHT);
        let l = card_layout(r, &card_spec(&[80.0]));
        let a = l.actions[0].unwrap();
        assert_eq!(l.meta.right() + 18.0, a.x);
        assert_eq!(l.meta.width, 40.0);
        assert_eq!(l.text.right() + 18.0, l.meta.x);
        assert!(l.text.width > 0.0);
    }

    #[test]
    fn card_without_actions_meta_hugs_right_padding() {
        let l = card_layout(Rect::new(0.0, 0.0, 600.0, CARD_HEIGHT), &card_spec(&[]));
        assert!(l.actions.iter().all(|a| a.is_none()));
        assert_eq!(l.meta.right(), 580.0);
    }

    #[test]
    fn card_hit_test_prefers_actions_then_body() {
        let r = Rect::new(0.0, 0.0, 600.0, CARD_HEIGHT);
        let spec = card_spec(&[80.0, 36.0]);
        let l = card_layout(r, &spec);
        let a1 = l.actions[1].unwrap();
        assert_eq!(
            card_hit(r, &spec, a1.x + 1.0, a1.y + 1.0),
            Some(CardHit::Action(1))
        );
        let a0 = l.actions[0].unwrap();
        assert_eq!(
            card_hit(r, &spec, a0.x + 1.0, a0.y + 1.0),
            Some(CardHit::Action(0))
        );
        assert_eq!(card_hit(r, &spec, 5.0, 5.0), Some(CardHit::Body));
        // the gap between actions is body, outside is nothing
        assert_eq!(
            card_hit(r, &spec, a0.right() + 5.0, a0.y + 1.0),
            Some(CardHit::Body)
        );
        assert_eq!(card_hit(r, &spec, 700.0, 5.0), None);
    }

    #[test]
    fn card_background_follows_state() {
        let t = ChromeTheme::default();
        assert_eq!(CardState::Default.background(&t), t.frame);
        assert_eq!(CardState::Hover.background(&t), t.surface);
    }

    #[test]
    fn file_row_columns_have_board_widths() {
        let r = Rect::new(5.0, 7.0, 500.0, 40.0);
        let l = file_row_layout(r);
        assert_eq!(l.size.width, 70.0);
        assert_eq!(l.date.width, 80.0);
        assert_eq!(l.date.right(), 500.0 + 5.0 - 10.0);
        assert_eq!(l.size.right() + 12.0, l.date.x);
        assert_eq!(l.icon.x, 5.0 + 10.0);
        assert_eq!(l.icon.width, 15.0);
        assert_eq!(l.name.x, l.icon.right() + 12.0);
        assert_eq!(l.name.right() + 12.0, l.size.x);
    }

    #[test]
    fn file_row_rename_field_is_28px_centered_over_name() {
        let r = Rect::new(0.0, 100.0, 500.0, 40.0);
        let l = file_row_layout(r);
        assert_eq!(l.rename_field.height, 28.0);
        assert_eq!(l.rename_field.y, 106.0);
        assert_eq!(l.rename_field.x, l.name.x);
        assert_eq!(l.rename_field.width, l.name.width);
    }

    #[test]
    fn file_row_hit_test_reports_columns() {
        let r = Rect::new(0.0, 0.0, 500.0, 40.0);
        let l = file_row_layout(r);
        assert_eq!(
            file_row_hit(r, l.icon.x + 1.0, 20.0),
            Some(FileColumn::Icon)
        );
        assert_eq!(
            file_row_hit(r, l.name.x + 1.0, 20.0),
            Some(FileColumn::Name)
        );
        assert_eq!(
            file_row_hit(r, l.size.x + 1.0, 20.0),
            Some(FileColumn::Size)
        );
        assert_eq!(
            file_row_hit(r, l.date.x + 1.0, 20.0),
            Some(FileColumn::Date)
        );
        assert_eq!(file_row_hit(r, 2.0, 20.0), Some(FileColumn::Row));
        assert_eq!(file_row_hit(r, 2.0, 41.0), None);
    }

    #[test]
    fn file_row_state_visuals() {
        let t = ChromeTheme::default();
        assert_eq!(FileRowState::Default.background(&t), None);
        assert_eq!(FileRowState::Renaming.background(&t), None);
        assert_eq!(FileRowState::Hover.background(&t), Some(t.surface));
        assert_eq!(FileRowState::Selected.background(&t), Some(t.selected));
        let drop = FileRowState::DropTarget.background(&t).unwrap();
        assert_eq!(&drop[..3], &t.accent[..3]);
        assert!((drop[3] - 0.1).abs() < 1e-6);
        assert_eq!(FileRowState::DropTarget.inset_stroke(), Some(1.5));
        assert_eq!(FileRowState::Selected.inset_stroke(), None);
        assert!(FileRowState::Renaming.shows_rename_field());
        assert!(!FileRowState::Hover.shows_rename_field());
    }

    #[test]
    fn history_row_is_52_high_with_bottom_divider() {
        let r = Rect::new(0.0, 10.0, 600.0, HISTORY_ROW_HEIGHT);
        let l = history_row_layout(r, 50.0, 100.0);
        assert_eq!(HISTORY_ROW_HEIGHT, 52.0);
        assert_eq!(l.divider, Rect::new(0.0, 10.0 + 51.0, 600.0, 1.0));
    }

    #[test]
    fn history_row_columns_flow_right_to_left() {
        let r = Rect::new(0.0, 0.0, 600.0, HISTORY_ROW_HEIGHT);
        let l = history_row_layout(r, 50.0, 100.0);
        let a = l.action.unwrap();
        assert_eq!(a.right(), 600.0);
        assert_eq!(a.width, 100.0);
        assert_eq!(a.height, 36.0);
        assert_eq!(l.time.width, 90.0);
        assert_eq!(l.time.right() + 18.0, a.x);
        assert_eq!(l.cwd.width, 50.0);
        assert_eq!(l.cwd.right() + 18.0, l.time.x);
        assert_eq!(l.command.x, 0.0);
        assert_eq!(l.command.right() + 18.0, l.cwd.x);
    }

    #[test]
    fn history_row_without_action_slot() {
        let r = Rect::new(0.0, 0.0, 600.0, HISTORY_ROW_HEIGHT);
        let l = history_row_layout(r, 50.0, 0.0);
        assert!(l.action.is_none());
        assert_eq!(l.time.right(), 600.0);
    }

    #[test]
    fn a_long_cwd_never_runs_over_the_command() {
        // 1100 px window: the row is ~770 px, the cwd a deep worktree path.
        let r = Rect::new(288.0, 0.0, 770.0, HISTORY_ROW_HEIGHT);
        let l = history_row_layout(r, 600.0, 90.0);
        assert!(l.cwd.x >= l.command.right() + HISTORY_GAP - 0.01);
        assert!(l.command.width >= l.cwd.width, "the command keeps half");
        assert!(l.cwd.right() <= l.time.x);
        // A short cwd keeps its measured width.
        let short = history_row_layout(r, 60.0, 90.0);
        assert_eq!(short.cwd.width, 60.0);
    }

    #[test]
    fn elision_keeps_the_meaningful_end() {
        assert_eq!(elide_start("/home/me/dev/app", 9), "\u{2026}/dev/app");
        assert_eq!(elide_start("~/app", 9), "~/app");
        assert_eq!(elide_end("docker compose up -d", 10), "docker co\u{2026}");
        assert_eq!(elide_end("git pull", 10), "git pull");
        assert_eq!(elide_end("abc", 0), "");
    }

    #[test]
    fn history_hit_test() {
        let r = Rect::new(0.0, 0.0, 600.0, HISTORY_ROW_HEIGHT);
        let l = history_row_layout(r, 50.0, 100.0);
        let a = l.action.unwrap();
        assert_eq!(
            history_hit(r, 50.0, 100.0, a.x + 2.0, 26.0),
            Some(HistoryHit::Action)
        );
        assert_eq!(
            history_hit(r, 50.0, 100.0, 3.0, 26.0),
            Some(HistoryHit::Row)
        );
        assert_eq!(history_hit(r, 50.0, 100.0, 3.0, 60.0), None);
    }
}
