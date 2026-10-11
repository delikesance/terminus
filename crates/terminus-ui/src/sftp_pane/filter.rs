//! Quick-find box above a pane listing: `entries` is the visible subset of
//! `listing` whose name contains the typed text (case-insensitive).

use super::{SftpFocus, SftpPaneState, SftpRow, SftpSideState};

const MAX_FILTER_BYTES: usize = 255;

fn matches(row: &SftpRow, needle: &str) -> bool {
    row.name.to_lowercase().contains(needle)
}

impl SftpSideState {
    pub fn type_filter(&mut self, text: &str) {
        self.filter.insert(text, MAX_FILTER_BYTES, false);
        self.refilter();
    }

    pub fn filter_backspace(&mut self) {
        self.filter.backspace(false);
        self.refilter();
    }

    /// Empty the box and give up its focus. Returns whether anything changed.
    pub fn clear_filter(&mut self) -> bool {
        let changed = self.filter_focused || !self.filter.value.is_empty();
        self.filter.clear();
        self.filter_focused = false;
        self.refilter();
        changed
    }

    pub(super) fn reset_filter(&mut self) {
        self.filter.clear();
        self.filter_focused = false;
    }

    fn refilter(&mut self) {
        let needle = self.filter.value.to_lowercase();
        self.entries = self
            .listing
            .iter()
            .filter(|row| matches(row, &needle))
            .cloned()
            .collect();
        self.selected = None;
        self.scroll = 0.0;
    }
}

impl SftpPaneState {
    pub fn focus_filter(&mut self, side: SftpFocus) {
        self.blur_filters();
        self.focus = side;
        self.side_mut(side).filter_focused = true;
    }

    pub fn blur_filters(&mut self) {
        self.left.filter_focused = false;
        self.right.filter_focused = false;
    }

    /// The focused pane's quick-find box is capturing typing.
    pub fn filter_owns_keys(&self) -> bool {
        self.side(self.focus).filter_focused
    }
}
