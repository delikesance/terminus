//! Selecting rows: one (click, arrows), several (Ctrl-click toggles, Shift
//! extends from the anchor, Ctrl+A takes all) and keeping the cursor on
//! screen.

use super::{SftpPaneState, SftpRow, SftpSideState};

impl SftpSideState {
    /// The rows an action applies to: the marked ones in list order, or
    /// just the cursor row when nothing is marked.
    pub fn selection(&self) -> Vec<&SftpRow> {
        if self.marked.is_empty() {
            return self.selected_row().into_iter().collect();
        }
        self.entries
            .iter()
            .filter(|r| self.marked.contains(&r.path))
            .collect()
    }

    pub fn selection_count(&self) -> usize {
        if self.marked.is_empty() {
            usize::from(self.selected_row().is_some())
        } else {
            self.marked.len()
        }
    }

    /// Whether row `i` is part of the selection (painted highlighted).
    pub fn is_selected(&self, i: usize) -> bool {
        if self.marked.is_empty() {
            return self.selected == Some(i);
        }
        self.entries
            .get(i)
            .is_some_and(|r| self.marked.contains(&r.path))
    }

    /// Plain click: this row only.
    pub fn select_only(&mut self, i: usize) {
        if i >= self.entries.len() {
            return;
        }
        self.marked.clear();
        self.selected = Some(i);
        self.anchor = Some(i);
        self.reveal_selected = true;
    }

    /// Ctrl+click: add or remove this row.
    pub fn toggle_mark(&mut self, i: usize) {
        let Some(path) = self.entries.get(i).map(|r| r.path.clone()) else {
            return;
        };
        if self.marked.is_empty() {
            if let Some(cursor) = self.selected_row().map(|r| r.path.clone()) {
                self.marked.insert(cursor);
            }
        }
        if !self.marked.remove(&path) {
            self.marked.insert(path);
        }
        self.selected = Some(i);
        self.anchor = Some(i);
        self.reveal_selected = true;
    }

    /// Shift+click / Shift+arrows: every row from the anchor to `i`.
    pub fn select_range_to(&mut self, i: usize) {
        if i >= self.entries.len() {
            return;
        }
        let anchor = self.anchor.or(self.selected).unwrap_or(i);
        let anchor = anchor.min(self.entries.len() - 1);
        let (lo, hi) = (anchor.min(i), anchor.max(i));
        self.marked = self.entries[lo..=hi]
            .iter()
            .map(|r| r.path.clone())
            .collect();
        self.selected = Some(i);
        self.anchor = Some(anchor);
        self.reveal_selected = true;
    }

    /// Ctrl+A.
    pub fn select_all_rows(&mut self) -> bool {
        if self.entries.is_empty() {
            return false;
        }
        self.marked = self.entries.iter().map(|r| r.path.clone()).collect();
        self.selected.get_or_insert(0);
        true
    }

    /// Drop the selection. Returns whether there was one.
    pub fn clear_selection(&mut self) -> bool {
        let had = self.selected.is_some() || !self.marked.is_empty();
        self.selected = None;
        self.anchor = None;
        self.marked.clear();
        had
    }

    /// Move the cursor by `delta` rows; with `extend`, grow the selection.
    pub fn move_cursor(&mut self, delta: isize, extend: bool) {
        let len = self.entries.len();
        if len == 0 {
            return;
        }
        let cur = self.selected.unwrap_or(0) as isize;
        let next = (cur + delta).clamp(0, len as isize - 1) as usize;
        if extend {
            if self.anchor.is_none() {
                self.anchor = self.selected.or(Some(next));
            }
            self.select_range_to(next);
        } else {
            self.select_only(next);
        }
    }

    /// Scroll (in whole rows) so the cursor row is inside the list.
    /// `offset_rows` is how far the listing is pushed down (new-folder row).
    pub fn ensure_visible(
        &mut self,
        visible_rows: usize,
        row_h: f32,
        offset_rows: usize,
    ) {
        self.reveal_selected = false;
        let Some(i) = self.selected else {
            return;
        };
        if visible_rows == 0 {
            return;
        }
        let slot = i + offset_rows;
        let first = (self.scroll / row_h).round() as usize;
        if slot < first {
            self.scroll = slot as f32 * row_h;
        } else if slot >= first + visible_rows {
            self.scroll = (slot + 1 - visible_rows) as f32 * row_h;
        }
    }
}

impl SftpPaneState {
    /// Cursor movement for the focused pane.
    pub fn move_selection_by(&mut self, delta: isize, extend: bool) {
        let focus = self.focus;
        self.side_mut(focus).move_cursor(delta, extend);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn side(n: usize) -> SftpSideState {
        let mut s = SftpSideState::local("/d");
        let rows = (0..n)
            .map(|i| SftpRow {
                name: format!("f{i}"),
                path: format!("/d/f{i}"),
                is_dir: false,
                size: 1,
                modified: None,
            })
            .collect();
        s.set_listed("/d".into(), rows);
        s
    }

    #[test]
    fn plain_click_selects_one_and_drops_marks() {
        let mut s = side(5);
        s.toggle_mark(1);
        s.toggle_mark(3);
        assert_eq!(s.selection_count(), 2);
        s.select_only(2);
        assert_eq!(s.selection_count(), 1);
        assert_eq!(s.selection()[0].name, "f2");
    }

    #[test]
    fn ctrl_click_toggles_and_keeps_the_cursor_row() {
        let mut s = side(5);
        s.select_only(0);
        s.toggle_mark(2);
        let names: Vec<_> = s.selection().iter().map(|r| r.name.clone()).collect();
        assert_eq!(names, ["f0", "f2"]);
        assert!(s.is_selected(0) && s.is_selected(2) && !s.is_selected(1));
        s.toggle_mark(0);
        let names: Vec<_> = s.selection().iter().map(|r| r.name.clone()).collect();
        assert_eq!(names, ["f2"]);
    }

    #[test]
    fn shift_selects_the_range_from_the_anchor() {
        let mut s = side(6);
        s.select_only(1);
        s.select_range_to(4);
        let names: Vec<_> = s.selection().iter().map(|r| r.name.clone()).collect();
        assert_eq!(names, ["f1", "f2", "f3", "f4"]);
        s.select_range_to(0);
        let names: Vec<_> = s.selection().iter().map(|r| r.name.clone()).collect();
        assert_eq!(names, ["f0", "f1"]);
    }

    #[test]
    fn shift_arrows_extend_and_plain_arrows_reset() {
        let mut s = side(5);
        s.select_only(1);
        s.move_cursor(1, true);
        s.move_cursor(1, true);
        assert_eq!(s.selection_count(), 3);
        assert_eq!(s.selected, Some(3));
        s.move_cursor(1, false);
        assert_eq!(s.selection_count(), 1);
        assert_eq!(s.selected, Some(4));
    }

    #[test]
    fn select_all_and_clear() {
        let mut s = side(3);
        assert!(s.select_all_rows());
        assert_eq!(s.selection_count(), 3);
        assert!(s.clear_selection());
        assert_eq!(s.selection_count(), 0);
        assert!(!s.clear_selection());
    }

    #[test]
    fn moving_past_the_viewport_scrolls_it() {
        let mut s = side(20);
        s.select_only(0);
        s.move_cursor(6, false);
        s.ensure_visible(5, 40.0, 0);
        assert_eq!(s.scroll, 2.0 * 40.0, "row 6 is the last visible row");
        s.move_cursor(-6, false);
        s.ensure_visible(5, 40.0, 0);
        assert_eq!(s.scroll, 0.0);
        assert!(!s.reveal_selected);
    }

    #[test]
    fn relisting_the_same_folder_keeps_scroll_and_selection() {
        let mut s = side(20);
        s.select_only(7);
        s.scroll = 3.0 * 40.0;
        let rows = s.listing.clone();
        s.set_listed("/d".into(), rows);
        assert_eq!(s.selected, Some(7));
        assert_eq!(s.scroll, 120.0);
    }

    #[test]
    fn a_deleted_selection_falls_to_its_neighbour() {
        let mut s = side(5);
        s.select_only(2);
        let mut rows = s.listing.clone();
        rows.remove(2);
        s.set_listed("/d".into(), rows);
        assert_eq!(s.selected_row().map(|r| r.name.as_str()), Some("f3"));
        let mut rows = s.listing.clone();
        rows.truncate(3);
        s.selected = Some(3);
        s.set_listed("/d".into(), rows);
        assert_eq!(s.selected, Some(2), "clamped to the last row");
    }

    #[test]
    fn marks_survive_a_relist_only_for_rows_that_remain() {
        let mut s = side(5);
        s.select_only(0);
        s.toggle_mark(1);
        s.toggle_mark(4);
        let mut rows = s.listing.clone();
        rows.remove(1);
        s.set_listed("/d".into(), rows);
        let names: Vec<_> = s.selection().iter().map(|r| r.name.clone()).collect();
        assert_eq!(names, ["f0", "f4"]);
    }

    #[test]
    fn a_new_folder_starts_clean_and_pending_names_get_selected() {
        let mut s = side(5);
        s.select_only(3);
        s.scroll = 80.0;
        s.set_listed("/other".into(), s.listing.clone());
        assert_eq!((s.selected, s.scroll), (None, 0.0));
        s.pending_select = Some("f2".into());
        s.set_listed("/other".into(), s.listing.clone());
        assert_eq!(s.selected, Some(2));
        assert!(s.reveal_selected);
        s.pending_select = Some("gone".into());
        s.set_listed("/other".into(), s.listing.clone());
        assert_eq!(s.selected, Some(2), "an unknown name keeps the old cursor");
    }

    #[test]
    fn filtering_resets_the_selection() {
        let mut s = side(5);
        s.select_only(1);
        s.toggle_mark(2);
        s.type_filter("f3");
        assert_eq!((s.selected, s.selection_count()), (None, 0));
    }
}
