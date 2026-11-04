//! Clipboard for the pane's two text sinks: the inline name editor (new
//! folder / rename) and the focused pane's quick-find box. The name editor
//! wins when both exist, as it does for typing.

use super::{SftpPaneState, SftpSideState};
use crate::components::input::TextDraft;

const MAX_NAME_BYTES: usize = 255;
const MAX_FILTER_BYTES: usize = 255;

impl SftpPaneState {
    /// A text sink is open, so Ctrl+C / X / V belong to it.
    pub fn text_sink_active(&self) -> bool {
        self.name_edit.is_some() || self.filter_owns_keys()
    }

    /// Paste into the active sink. Returns whether the text changed.
    pub fn paste_text(&mut self, text: &str) -> bool {
        if let Some(edit) = self.name_edit.as_mut() {
            return edit.draft.paste(text, MAX_NAME_BYTES, false);
        }
        if self.filter_owns_keys() {
            let focus = self.focus;
            return self.side_mut(focus).paste_filter(text);
        }
        false
    }

    /// The active sink's selection (Ctrl+C).
    pub fn copy_text(&self) -> Option<String> {
        self.active_draft()?.selected_text().map(str::to_string)
    }

    /// Remove and return the active sink's selection (Ctrl+X).
    pub fn cut_text(&mut self) -> Option<String> {
        if let Some(edit) = self.name_edit.as_mut() {
            return edit.draft.cut();
        }
        if self.filter_owns_keys() {
            let focus = self.focus;
            return self.side_mut(focus).cut_filter();
        }
        None
    }

    fn active_draft(&self) -> Option<&TextDraft> {
        if let Some(edit) = self.name_edit.as_ref() {
            return Some(&edit.draft);
        }
        self.filter_owns_keys()
            .then(|| &self.side(self.focus).filter)
    }
}

impl SftpSideState {
    pub(super) fn paste_filter(&mut self, text: &str) -> bool {
        let changed = self.filter.paste(text, MAX_FILTER_BYTES, false);
        if changed {
            self.refilter();
        }
        changed
    }

    pub(super) fn cut_filter(&mut self) -> Option<String> {
        let cut = self.filter.cut()?;
        self.refilter();
        Some(cut)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sftp_pane::SftpFocus;

    fn state() -> SftpPaneState {
        SftpPaneState::new_local_local("/a".to_string(), "/b".to_string())
    }

    #[test]
    fn no_sink_means_nothing_to_paste_into() {
        let mut s = state();
        assert!(!s.text_sink_active());
        assert!(!s.paste_text("x"));
        assert_eq!(s.copy_text(), None);
    }

    #[test]
    fn paste_goes_to_the_name_editor_and_flattens_newlines() {
        let mut s = state();
        s.begin_mkdir();
        assert!(s.text_sink_active());
        assert!(s.paste_text("my folder\r\n"));
        assert_eq!(s.name_edit.as_ref().unwrap().draft.value, "my folder");
    }

    #[test]
    fn copy_and_cut_take_the_selection() {
        let mut s = state();
        s.begin_mkdir();
        s.paste_text("abc");
        assert_eq!(s.copy_text(), None);
        s.name_edit.as_mut().unwrap().draft.select_all();
        assert_eq!(s.copy_text().as_deref(), Some("abc"));
        assert_eq!(s.cut_text().as_deref(), Some("abc"));
        assert_eq!(s.name_edit.as_ref().unwrap().draft.value, "");
    }

    #[test]
    fn paste_goes_to_the_focused_filter() {
        let mut s = state();
        s.focus_filter(SftpFocus::Right);
        assert!(s.paste_text("log\n"));
        assert_eq!(s.right.filter.value, "log");
        assert_eq!(s.left.filter.value, "");
    }
}
