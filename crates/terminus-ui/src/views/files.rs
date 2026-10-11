//! Files view: two SFTP panes side by side plus a transfer bar, laid out inside
//! a `content` rect handed in by the shell (never assuming where the header or
//! sidebar are).
//!
//! The state is the existing [`SftpPaneState`] (navigation, drag, rename,
//! mkdir, conflicts); this module only adds the J5 geometry and hit-testing,
//! and a few pure helpers (date column, pane titles).

use crate::geom::Rect;
use crate::sftp_pane::{SftpFocus, SftpHit, SftpPaneState};

/// Padding around the pane grid.
pub const GRID_PAD_X: f32 = 20.0;
pub const GRID_PAD_TOP: f32 = 16.0;
pub const GRID_PAD_BOTTOM: f32 = 12.0;
pub const PANE_GAP: f32 = 12.0;
pub const PANE_RADIUS: f32 = 14.0;
/// Pane header (title, path, icon buttons).
pub const HEADER_H: f32 = 52.0;
pub const HEADER_PAD_L: f32 = 16.0;
pub const HEADER_PAD_R: f32 = 12.0;
pub const HEADER_GAP: f32 = 10.0;
pub const ICON_BTN: f32 = 32.0;
pub const ICON_BTN_RADIUS: f32 = 8.0;
pub const ICON_BTN_ICON: f32 = 15.0;
/// Column captions row (Name / Size / Modified).
pub const SEARCH_H: f32 = 36.0;
pub const SEARCH_PAD_X: f32 = 12.0;
pub const CAPTION_H: f32 = 28.0;
/// Horizontal inset of rows inside a pane (so row text lands at the 16px pad).
pub const ROW_INSET: f32 = 6.0;
pub const LIST_PAD_BOTTOM: f32 = 6.0;
pub const ROW_H: f32 = crate::components::list::FILE_ROW_HEIGHT;
/// Transfer bar.
pub const BAR_H: f32 = 52.0;
pub const BAR_PAD_X: f32 = 28.0;
pub const BAR_GAP: f32 = 16.0;
pub const CANCEL_W: f32 = 68.0;
pub const CANCEL_H: f32 = 30.0;
pub const PROGRESS_CAPTION_W: f32 = 150.0;
/// Pixels scrolled per wheel line.
/// (One row: scrolling is quantized so rows never straddle the list's top.)
pub const SCROLL_STEP: f32 = ROW_H;

/// What the pointer is over inside the Files view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilesHit {
    /// Entry `index` of a pane's listing.
    Row(SftpFocus, usize),
    /// "Up one folder" icon button.
    Up(SftpFocus),
    /// "New folder" icon button.
    NewFolder(SftpFocus),
    /// Header title/path area (focuses the pane).
    Header(SftpFocus),
    /// Quick-find box.
    Search(SftpFocus),
    /// Captions strip or the empty part of a listing.
    Pane(SftpFocus),
    /// Transfer bar "Cancel".
    CancelTransfer,
    /// Transfer bar background.
    Bar,
    Miss,
}

impl FilesHit {
    /// The equivalent hit for the existing [`SftpPaneState`] actions.
    pub fn to_sftp_hit(self) -> SftpHit {
        match self {
            FilesHit::Row(SftpFocus::Left, i) => SftpHit::LeftRow(i),
            FilesHit::Row(SftpFocus::Right, i) => SftpHit::RightRow(i),
            FilesHit::Up(SftpFocus::Left) => SftpHit::LeftParent,
            FilesHit::Up(SftpFocus::Right) => SftpHit::RightParent,
            FilesHit::Header(_)
            | FilesHit::Search(_)
            | FilesHit::Pane(_)
            | FilesHit::Bar => SftpHit::Consume,
            FilesHit::NewFolder(_) | FilesHit::CancelTransfer => SftpHit::Consume,
            FilesHit::Miss => SftpHit::Miss,
        }
    }

    /// Pane a hit belongs to, if any.
    pub fn pane(self) -> Option<SftpFocus> {
        match self {
            FilesHit::Row(f, _)
            | FilesHit::Up(f)
            | FilesHit::NewFolder(f)
            | FilesHit::Header(f)
            | FilesHit::Search(f)
            | FilesHit::Pane(f) => Some(f),
            _ => None,
        }
    }

    pub fn is_clickable(self) -> bool {
        !matches!(self, FilesHit::Miss | FilesHit::Bar | FilesHit::Pane(_))
    }
}

/// Geometry of one pane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PaneRects {
    /// The rounded frame.
    pub frame: Rect,
    pub header: Rect,
    /// Title + path area (left of the icon buttons).
    pub title_area: Rect,
    pub up: Rect,
    pub new_folder: Rect,
    /// Quick-find box strip under the header.
    pub search: Rect,
    /// Name / Size / Modified captions.
    pub captions: Rect,
    /// Scrollable listing.
    pub list: Rect,
}

/// Geometry of the transfer bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarRects {
    pub bar: Rect,
    pub label: Rect,
    pub track: Rect,
    pub caption: Rect,
    pub cancel: Rect,
}

/// Everything the Files view lays out for one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FilesLayout {
    pub content: Rect,
    pub left: PaneRects,
    pub right: PaneRects,
    pub bar: Option<BarRects>,
}

impl FilesLayout {
    /// Lay out inside `content`; the transfer bar is present only while
    /// `show_bar` (a transfer is in flight).
    pub fn new(content: Rect, show_bar: bool) -> Self {
        let bar_h = if show_bar { BAR_H } else { 0.0 };
        let grid = Rect::new(
            content.x + GRID_PAD_X,
            content.y + GRID_PAD_TOP,
            (content.width - 2.0 * GRID_PAD_X).max(0.0),
            (content.height - GRID_PAD_TOP - GRID_PAD_BOTTOM - bar_h).max(0.0),
        );
        let pane_w = ((grid.width - PANE_GAP) * 0.5).max(0.0);
        let left = pane_rects(Rect::new(grid.x, grid.y, pane_w, grid.height));
        let right = pane_rects(Rect::new(
            grid.x + pane_w + PANE_GAP,
            grid.y,
            pane_w,
            grid.height,
        ));
        let bar = show_bar.then(|| bar_rects(content));
        Self {
            content,
            left,
            right,
            bar,
        }
    }

    /// Layout for `state` (bar shown while a transfer is in flight).
    pub fn for_state(content: Rect, state: &SftpPaneState) -> Self {
        Self::new(content, state.transfer.is_some())
    }

    pub fn pane(&self, focus: SftpFocus) -> &PaneRects {
        match focus {
            SftpFocus::Left => &self.left,
            SftpFocus::Right => &self.right,
        }
    }

    /// Row rect for entry `index` (unclipped; may sit above/below the list).
    /// `offset_rows` shifts rows down, e.g. for the inline new-folder row.
    pub fn row_rect(
        &self,
        focus: SftpFocus,
        index: usize,
        scroll: f32,
        offset_rows: usize,
    ) -> Rect {
        let list = self.pane(focus).list;
        Rect::new(
            list.x + ROW_INSET,
            list.y + (index + offset_rows) as f32 * ROW_H - scroll,
            (list.width - 2.0 * ROW_INSET).max(0.0),
            ROW_H,
        )
    }

    /// The pane under `(x, y)` (frame included): drag-drop target.
    pub fn focus_at(&self, x: f32, y: f32) -> Option<SftpFocus> {
        if self.left.frame.contains(x, y) {
            Some(SftpFocus::Left)
        } else if self.right.frame.contains(x, y) {
            Some(SftpFocus::Right)
        } else {
            None
        }
    }

    pub fn hit_test(&self, state: &SftpPaneState, x: f32, y: f32) -> FilesHit {
        if let Some(bar) = &self.bar {
            if bar.cancel.contains(x, y) {
                return FilesHit::CancelTransfer;
            }
            if bar.bar.contains(x, y) {
                return FilesHit::Bar;
            }
        }
        for focus in [SftpFocus::Left, SftpFocus::Right] {
            let p = self.pane(focus);
            if !p.frame.contains(x, y) {
                continue;
            }
            if p.up.contains(x, y) {
                return FilesHit::Up(focus);
            }
            if p.new_folder.contains(x, y) {
                return FilesHit::NewFolder(focus);
            }
            if p.header.contains(x, y) {
                return FilesHit::Header(focus);
            }
            if p.search.contains(x, y) {
                return FilesHit::Search(focus);
            }
            if p.list.contains(x, y) {
                let side = state.side(focus);
                let offset = row_offset(state, focus);
                let rel = y - p.list.y + side.scroll;
                if rel >= 0.0 {
                    let slot = (rel / ROW_H) as usize;
                    // Rows cut off at the bottom are not painted, so not hit.
                    let fits =
                        (slot + 1) as f32 * ROW_H - side.scroll <= p.list.height + 0.5;
                    if fits && slot >= offset && slot - offset < side.entries.len() {
                        return FilesHit::Row(focus, slot - offset);
                    }
                }
            }
            return FilesHit::Pane(focus);
        }
        FilesHit::Miss
    }

    /// Largest scroll offset (a whole number of rows) for a pane with `rows`
    /// entries, so the last row can reach the bottom of the list.
    pub fn max_scroll(&self, focus: SftpFocus, rows: usize) -> f32 {
        let h = self.pane(focus).list.height;
        let over = (rows as f32 * ROW_H - h).max(0.0);
        (over / ROW_H).ceil() * ROW_H
    }

    /// Fully visible rows of a pane list.
    pub fn visible_rows(&self, focus: SftpFocus) -> usize {
        (self.pane(focus).list.height / ROW_H).floor() as usize
    }
}

/// Where releasing the active drag at `(x, y)` would drop: the destination
/// pane (never the source) and, when the pointer is over a folder row of that
/// pane, that folder's entry index.
pub fn drop_target(
    layout: &FilesLayout,
    state: &SftpPaneState,
    x: f32,
    y: f32,
) -> Option<(SftpFocus, Option<usize>)> {
    let drag = state.drag.as_ref()?;
    let to = layout.focus_at(x, y)?;
    if to == drag.from {
        return None;
    }
    let into = match layout.hit_test(state, x, y) {
        FilesHit::Row(f, i)
            if f == to && state.side(f).entries.get(i).is_some_and(|r| r.is_dir) =>
        {
            Some(i)
        }
        _ => None,
    };
    Some((to, into))
}

/// Rows the listing is pushed down by (the inline new-folder row).
pub fn row_offset(state: &SftpPaneState, focus: SftpFocus) -> usize {
    match &state.name_edit {
        Some(e) if e.side == focus && e.kind == crate::sftp_pane::SftpNameKind::Mkdir => {
            1
        }
        _ => 0,
    }
}

fn pane_rects(frame: Rect) -> PaneRects {
    let header = Rect::new(frame.x, frame.y, frame.width, HEADER_H);
    let by = header.y + (HEADER_H - ICON_BTN) * 0.5;
    let new_folder = Rect::new(
        header.right() - HEADER_PAD_R - ICON_BTN,
        by,
        ICON_BTN,
        ICON_BTN,
    );
    let up = Rect::new(new_folder.x - HEADER_GAP - ICON_BTN, by, ICON_BTN, ICON_BTN);
    let title_area = Rect::new(
        header.x + HEADER_PAD_L,
        header.y,
        (up.x - HEADER_GAP - header.x - HEADER_PAD_L).max(0.0),
        HEADER_H,
    );
    let search = Rect::new(frame.x, header.bottom(), frame.width, SEARCH_H);
    let captions = Rect::new(frame.x, search.bottom(), frame.width, CAPTION_H);
    let list_top = captions.bottom();
    let list = Rect::new(
        frame.x,
        list_top,
        frame.width,
        (frame.bottom() - LIST_PAD_BOTTOM - list_top).max(0.0),
    );
    PaneRects {
        frame,
        header,
        title_area,
        up,
        new_folder,
        search,
        captions,
        list,
    }
}

fn bar_rects(content: Rect) -> BarRects {
    let bar = Rect::new(content.x, content.bottom() - BAR_H, content.width, BAR_H);
    let cy = bar.y + BAR_H * 0.5;
    let cancel = Rect::new(
        bar.right() - BAR_PAD_X - CANCEL_W,
        cy - CANCEL_H * 0.5,
        CANCEL_W,
        CANCEL_H,
    );
    let caption = Rect::new(
        cancel.x - BAR_GAP - PROGRESS_CAPTION_W,
        bar.y,
        PROGRESS_CAPTION_W,
        BAR_H,
    );
    let label_w = (content.width * 0.22).clamp(120.0, 280.0);
    let label = Rect::new(bar.x + BAR_PAD_X, bar.y, label_w, BAR_H);
    let track_x = label.right() + BAR_GAP;
    let track = Rect::new(
        track_x,
        cy - crate::components::feedback::PROGRESS_TRANSFER_H * 0.5,
        (caption.x - BAR_GAP - track_x).max(0.0),
        crate::components::feedback::PROGRESS_TRANSFER_H,
    );
    BarRects {
        bar,
        label,
        track,
        caption,
        cancel,
    }
}

/// Pane title: "This computer" for the local side, else the host name.
pub fn pane_title(state: &SftpPaneState, focus: SftpFocus) -> &str {
    let side = state.side(focus);
    if side.is_local() {
        "This computer"
    } else {
        side.title()
    }
}

/// Path shown in the pane header: `$HOME` shortened to `~` on the local side.
pub fn pane_path(state: &SftpPaneState, focus: SftpFocus, home: Option<&str>) -> String {
    let side = state.side(focus);
    let cwd = side.cwd.as_str();
    if cwd.is_empty() {
        return if side.is_local() {
            ".".into()
        } else {
            "/".into()
        };
    }
    if side.is_local() {
        if let Some(home) = home
            .map(|h| h.trim_end_matches(['/', '\\']))
            .filter(|h| !h.is_empty())
        {
            if cwd == home {
                return "~".into();
            }
            if let Some(rest) = cwd.strip_prefix(home) {
                if rest.starts_with('/') || rest.starts_with('\\') {
                    return format!("~{rest}");
                }
            }
        }
    }
    cwd.to_string()
}

/// Size column: `"—"` for folders, else `"18 MB"`.
pub fn size_label(is_dir: bool, size: u64) -> String {
    if is_dir {
        "\u{2014}".into()
    } else {
        crate::sftp_pane::format_bytes(size)
    }
}

const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// `(year, month 1-12, day 1-31)` of a day count since 1970-01-01.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Modified column in UTC: `Today`, `Yesterday`, a weekday within the last
/// week (`Mon`), `14 Jul` within the year, else `14 Jul 2023`. `—` when unknown.
pub fn modified_label(modified: Option<i64>, now: i64) -> String {
    let Some(t) = modified else {
        return "\u{2014}".into();
    };
    let day = t.div_euclid(86_400);
    let today = now.div_euclid(86_400);
    let age = today - day;
    match age {
        0 => "Today".into(),
        1 => "Yesterday".into(),
        2..=6 => {
            // 1970-01-01 was a Thursday (index 3 with Monday = 0).
            WEEKDAYS[(day + 3).rem_euclid(7) as usize].into()
        }
        _ => {
            let (y, m, d) = civil_from_days(day);
            let (ny, _, _) = civil_from_days(today);
            let mon = MONTHS[(m - 1) as usize];
            if y == ny {
                format!("{d} {mon}")
            } else {
                format!("{d} {mon} {y}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sftp_pane::{SftpPaneState, SftpRow};

    fn content() -> Rect {
        Rect::new(260.0, 96.0, 1180.0, 804.0)
    }

    fn rows(n: usize) -> Vec<SftpRow> {
        (0..n)
            .map(|i| SftpRow {
                name: format!("f{i}"),
                path: format!("/f{i}"),
                is_dir: i == 0,
                size: 10,
                modified: None,
            })
            .collect()
    }

    fn state(n: usize) -> SftpPaneState {
        let mut st = SftpPaneState::new_local_local("/home/u", "/srv");
        st.set_listed(SftpFocus::Left, "/home/u".into(), rows(n));
        st.set_listed(SftpFocus::Right, "/srv".into(), rows(n));
        st
    }

    #[test]
    fn panes_split_content_with_gap_and_padding() {
        let l = FilesLayout::new(content(), false);
        assert_eq!(l.left.frame.x, 280.0);
        assert_eq!(l.left.frame.y, 112.0);
        assert!((l.right.frame.x - l.left.frame.right() - PANE_GAP).abs() < 1e-3);
        assert!((l.right.frame.right() - (content().right() - GRID_PAD_X)).abs() < 1e-3);
        assert!(
            (l.left.frame.bottom() - (content().bottom() - GRID_PAD_BOTTOM)).abs() < 1e-3
        );
        assert!(l.bar.is_none());
    }

    #[test]
    fn bar_takes_space_from_the_panes() {
        let without = FilesLayout::new(content(), false);
        let with = FilesLayout::new(content(), true);
        assert!(
            (without.left.frame.height - with.left.frame.height - BAR_H).abs() < 1e-3
        );
        let bar = with.bar.unwrap();
        assert_eq!(bar.bar.bottom(), content().bottom());
        assert!(bar.label.right() < bar.track.x);
        assert!(bar.track.right() < bar.caption.x);
        assert!(bar.caption.right() < bar.cancel.x);
        assert_eq!(bar.track.height, 4.0);
    }

    #[test]
    fn header_controls_sit_right_and_never_overlap_title() {
        let l = FilesLayout::new(content(), false);
        let p = l.left;
        assert!(p.up.right() < p.new_folder.x);
        assert!(p.title_area.right() <= p.up.x);
        assert_eq!(p.up.width, ICON_BTN);
        assert_eq!(p.header.height, HEADER_H);
        assert_eq!(p.search.y, p.header.bottom());
        assert_eq!(p.search.height, SEARCH_H);
        assert_eq!(p.captions.y, p.search.bottom());
        assert_eq!(p.list.y, p.captions.bottom());
    }

    #[test]
    fn hit_test_search_strip_per_pane() {
        let st = state(5);
        let l = FilesLayout::new(content(), false);
        let s = l.right.search;
        assert_eq!(
            l.hit_test(&st, s.x + 4.0, s.y + 4.0),
            FilesHit::Search(SftpFocus::Right)
        );
        assert!(FilesHit::Search(SftpFocus::Right).is_clickable());
    }

    #[test]
    fn hit_test_rows_buttons_and_misses() {
        let st = state(5);
        let l = FilesLayout::new(content(), true);
        let r = l.row_rect(SftpFocus::Left, 2, 0.0, 0);
        assert_eq!(
            l.hit_test(&st, r.x + 20.0, r.y + 5.0),
            FilesHit::Row(SftpFocus::Left, 2)
        );
        let rr = l.row_rect(SftpFocus::Right, 0, 0.0, 0);
        assert_eq!(
            l.hit_test(&st, rr.x + 20.0, rr.y + 5.0),
            FilesHit::Row(SftpFocus::Right, 0)
        );
        let up = l.right.up;
        assert_eq!(
            l.hit_test(&st, up.x + 3.0, up.y + 3.0),
            FilesHit::Up(SftpFocus::Right)
        );
        let nf = l.left.new_folder;
        assert_eq!(
            l.hit_test(&st, nf.x + 3.0, nf.y + 3.0),
            FilesHit::NewFolder(SftpFocus::Left)
        );
        let t = l.left.title_area;
        assert_eq!(
            l.hit_test(&st, t.x + 3.0, t.y + 3.0),
            FilesHit::Header(SftpFocus::Left)
        );
        // Below the 5 rows: empty list space.
        let below = l.row_rect(SftpFocus::Left, 7, 0.0, 0);
        assert_eq!(
            l.hit_test(&st, below.x + 5.0, below.y + 1.0),
            FilesHit::Pane(SftpFocus::Left)
        );
        let c = l.bar.unwrap().cancel;
        assert_eq!(
            l.hit_test(&st, c.x + 2.0, c.y + 2.0),
            FilesHit::CancelTransfer
        );
        assert_eq!(l.hit_test(&st, 5.0, 5.0), FilesHit::Miss);
        // The gap between panes belongs to neither.
        let gap_x = l.left.frame.right() + PANE_GAP * 0.5;
        assert_eq!(l.hit_test(&st, gap_x, 300.0), FilesHit::Miss);
    }

    #[test]
    fn scroll_shifts_row_hits() {
        let mut st = state(30);
        st.left.scroll = 2.0 * ROW_H;
        let l = FilesLayout::new(content(), false);
        let list = l.left.list;
        assert_eq!(
            l.hit_test(&st, list.x + 30.0, list.y + 3.0),
            FilesHit::Row(SftpFocus::Left, 2)
        );
        assert!(l.max_scroll(SftpFocus::Left, 30) > 0.0);
        assert_eq!(l.max_scroll(SftpFocus::Left, 2), 0.0);
        let max = l.max_scroll(SftpFocus::Left, 30);
        assert_eq!(max % ROW_H, 0.0);
        assert!(30.0 * ROW_H - max <= l.left.list.height);
    }

    #[test]
    fn clipped_bottom_row_is_not_hit() {
        let st = state(40);
        let l = FilesLayout::new(content(), false);
        let list = l.left.list;
        let full = l.visible_rows(SftpFocus::Left);
        let partial_y = list.y + full as f32 * ROW_H + 1.0;
        if partial_y < list.bottom() {
            assert_eq!(
                l.hit_test(&st, list.x + 30.0, partial_y),
                FilesHit::Pane(SftpFocus::Left)
            );
        }
        let last_full = list.y + (full - 1) as f32 * ROW_H + 1.0;
        assert_eq!(
            l.hit_test(&st, list.x + 30.0, last_full),
            FilesHit::Row(SftpFocus::Left, full - 1)
        );
    }

    #[test]
    fn drop_target_is_the_other_pane_or_a_folder_in_it() {
        use crate::sftp_pane::SftpDrag;
        let mut st = state(4);
        let l = FilesLayout::new(content(), false);
        st.drag = Some(SftpDrag {
            from: SftpFocus::Left,
            row_index: 1,
            name: "f1".into(),
            path: "/f1".into(),
            is_dir: false,
            pointer_x: 0.0,
            pointer_y: 0.0,
        });
        // Over the source pane: nowhere to drop.
        let src = l.row_rect(SftpFocus::Left, 0, 0.0, 0);
        assert_eq!(drop_target(&l, &st, src.x + 20.0, src.y + 5.0), None);
        // Row 0 of the right pane is a folder (is_dir for i == 0).
        let dir = l.row_rect(SftpFocus::Right, 0, 0.0, 0);
        assert_eq!(
            drop_target(&l, &st, dir.x + 20.0, dir.y + 5.0),
            Some((SftpFocus::Right, Some(0)))
        );
        // A file row, or empty space, drops into the pane's cwd.
        let file = l.row_rect(SftpFocus::Right, 2, 0.0, 0);
        assert_eq!(
            drop_target(&l, &st, file.x + 20.0, file.y + 5.0),
            Some((SftpFocus::Right, None))
        );
        // No drag, no target.
        st.drag = None;
        assert_eq!(drop_target(&l, &st, dir.x + 20.0, dir.y + 5.0), None);
    }

    #[test]
    fn mkdir_row_pushes_entries_down() {
        let mut st = state(3);
        st.focus = SftpFocus::Left;
        st.begin_mkdir();
        let l = FilesLayout::new(content(), false);
        let list = l.left.list;
        // Slot 0 is the new-folder editor, not an entry.
        assert_eq!(
            l.hit_test(&st, list.x + 30.0, list.y + 3.0),
            FilesHit::Pane(SftpFocus::Left)
        );
        assert_eq!(
            l.hit_test(&st, list.x + 30.0, list.y + ROW_H + 3.0),
            FilesHit::Row(SftpFocus::Left, 0)
        );
        // The other pane is unaffected.
        let rl = l.right.list;
        assert_eq!(
            l.hit_test(&st, rl.x + 30.0, rl.y + 3.0),
            FilesHit::Row(SftpFocus::Right, 0)
        );
    }

    #[test]
    fn focus_at_finds_drop_pane() {
        let l = FilesLayout::new(content(), false);
        assert_eq!(
            l.focus_at(l.left.list.x + 5.0, l.left.list.y + 5.0),
            Some(SftpFocus::Left)
        );
        assert_eq!(
            l.focus_at(l.right.frame.x + 5.0, l.right.header.y + 5.0),
            Some(SftpFocus::Right)
        );
        assert_eq!(l.focus_at(2.0, 2.0), None);
    }

    #[test]
    fn hits_map_to_existing_sftp_hits() {
        assert_eq!(
            FilesHit::Row(SftpFocus::Right, 3).to_sftp_hit(),
            SftpHit::RightRow(3)
        );
        assert_eq!(
            FilesHit::Up(SftpFocus::Left).to_sftp_hit(),
            SftpHit::LeftParent
        );
        assert_eq!(FilesHit::Miss.to_sftp_hit(), SftpHit::Miss);
        assert_eq!(
            FilesHit::Header(SftpFocus::Left).pane(),
            Some(SftpFocus::Left)
        );
        assert!(!FilesHit::Pane(SftpFocus::Left).is_clickable());
    }

    #[test]
    fn titles_and_paths() {
        let mut st =
            SftpPaneState::new_local_remote("/home/u/projects", "h", "jerem prod");
        st.right.cwd = "/home/ubuntu/app".into();
        assert_eq!(pane_title(&st, SftpFocus::Left), "This computer");
        assert_eq!(pane_title(&st, SftpFocus::Right), "jerem prod");
        assert_eq!(
            pane_path(&st, SftpFocus::Left, Some("/home/u")),
            "~/projects"
        );
        assert_eq!(pane_path(&st, SftpFocus::Left, None), "/home/u/projects");
        assert_eq!(
            pane_path(&st, SftpFocus::Right, Some("/home/ubuntu")),
            "/home/ubuntu/app"
        );
        st.left.cwd = "/home/u".into();
        assert_eq!(pane_path(&st, SftpFocus::Left, Some("/home/u/")), "~");
        st.left.cwd = "/home/user2".into();
        assert_eq!(
            pane_path(&st, SftpFocus::Left, Some("/home/user")),
            "/home/user2"
        );
    }

    #[test]
    fn size_column() {
        assert_eq!(size_label(true, 4096), "\u{2014}");
        assert_eq!(size_label(false, 4096), "4 KB");
    }

    #[test]
    fn modified_column() {
        // 2024-07-17 (Wednesday) 12:00 UTC.
        let now = 1_721_217_600;
        let day = 86_400;
        assert_eq!(modified_label(Some(now - 3600), now), "Today");
        assert_eq!(modified_label(Some(now - day), now), "Yesterday");
        assert_eq!(modified_label(Some(now - 2 * day), now), "Mon");
        assert_eq!(modified_label(Some(now - 3 * day), now), "Sun");
        assert_eq!(modified_label(Some(now - 10 * day), now), "7 Jul");
        assert_eq!(modified_label(Some(now - 400 * day), now), "13 Jun 2023");
        assert_eq!(modified_label(None, now), "\u{2014}");
    }
}
