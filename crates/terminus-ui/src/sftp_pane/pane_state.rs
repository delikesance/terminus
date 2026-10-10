use super::*;
use crate::components::input::TextDraft;
use crate::components::overlay::DialogFocus;

/// Hit-test result inside the SFTP pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SftpHit {
    LeftRow(usize),
    RightRow(usize),
    LeftCrumb,
    RightCrumb,
    LeftParent,
    RightParent,
    /// Leave SFTP and restore the terminal pane.
    Close,
    /// Inline name field (mkdir / rename).
    NameField,
    NameConfirm,
    NameCancel,
    /// Differential transfer conflict prompt.
    ConflictOverwrite,
    ConflictKeep,
    ConflictApplyAll,
    ConflictCancel,
    Footer,
    /// Inside the pane chrome but not on a control.
    Consume,
    Miss,
}

/// Whether a hit should show a pointer cursor.
pub fn hit_is_clickable(hit: &SftpHit) -> bool {
    !matches!(
        hit,
        SftpHit::Miss | SftpHit::Footer | SftpHit::Consume | SftpHit::NameField
    )
}

/// Whether a hit should show a text (I-beam) cursor.
pub fn hit_is_text(hit: &SftpHit) -> bool {
    matches!(hit, SftpHit::NameField)
}

/// Result of routing a mouse press inside the SFTP pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SftpClickResult {
    Miss,
    Handled,
    Close,
}

/// Dual-pane browser state (either side local or remote).
#[derive(Debug, Clone, PartialEq)]
pub struct SftpPaneState {
    pub left: SftpSideState,
    pub right: SftpSideState,
    pub focus: SftpFocus,
    pub status: String,
    pub error: Option<String>,
    pub loading: bool,
    /// Last hovered control (mouse feedback).
    pub hover: Option<SftpHit>,
    /// Inline mkdir / rename editor.
    pub name_edit: Option<SftpNameEdit>,
    /// Differential transfer conflict waiting on the user.
    pub conflict: Option<SftpConflictPrompt>,
    /// Active drag ghost (file being dragged between panes).
    pub drag: Option<SftpDrag>,
    /// Transfer in flight, when any.
    pub transfer: Option<SftpTransfer>,
}

impl Default for SftpPaneState {
    fn default() -> Self {
        Self {
            left: SftpSideState::local(String::new()),
            right: SftpSideState::remote(String::new(), "Remote"),
            focus: SftpFocus::Left,
            status: "Connecting…".into(),
            error: None,
            loading: true,
            hover: None,
            name_edit: None,
            conflict: None,
            drag: None,
            transfer: None,
        }
    }
}

impl SftpPaneState {
    /// Left = local FS, right = remote host.
    pub fn new_local_remote(
        local_cwd: impl Into<String>,
        host_id: impl Into<String>,
        host_label: impl Into<String>,
    ) -> Self {
        Self {
            left: SftpSideState::local(local_cwd),
            right: SftpSideState::remote(host_id, host_label),
            ..Self::default()
        }
    }

    /// Both panes local (tests / dual-local harness).
    pub fn new_local_local(
        left_cwd: impl Into<String>,
        right_cwd: impl Into<String>,
    ) -> Self {
        Self {
            left: SftpSideState::local(left_cwd),
            right: SftpSideState::local(right_cwd),
            focus: SftpFocus::Left,
            status: "Ready".into(),
            error: None,
            loading: false,
            hover: None,
            name_edit: None,
            conflict: None,
            drag: None,
            transfer: None,
        }
    }

    /// Breadcrumb segments for `focus`: `(label, absolute_path)` from root to cwd.
    pub fn crumb_segments(&self, focus: SftpFocus) -> Vec<(String, String)> {
        let cwd = self.side(focus).cwd.as_str();
        crumb_segments_for_cwd(cwd, self.side(focus).is_local())
    }

    /// Path to navigate to when the user activates crumb `index` (0 = root).
    /// Returns `None` if the index is out of range.
    pub fn navigate_crumb_path(&self, focus: SftpFocus, index: usize) -> Option<String> {
        self.crumb_segments(focus)
            .into_iter()
            .nth(index)
            .map(|(_, path)| path)
    }

    pub fn side(&self, focus: SftpFocus) -> &SftpSideState {
        match focus {
            SftpFocus::Left => &self.left,
            SftpFocus::Right => &self.right,
        }
    }

    pub fn side_mut(&mut self, focus: SftpFocus) -> &mut SftpSideState {
        match focus {
            SftpFocus::Left => &mut self.left,
            SftpFocus::Right => &mut self.right,
        }
    }

    pub fn other_focus(&self) -> SftpFocus {
        match self.focus {
            SftpFocus::Left => SftpFocus::Right,
            SftpFocus::Right => SftpFocus::Left,
        }
    }

    /// The host on `focus` could not be reached.
    pub fn set_connect_error(&mut self, focus: SftpFocus, message: String) {
        self.side_mut(focus).connect_error = Some(message);
        self.loading = false;
    }

    /// Footer line: the latest error, else a pane that failed to connect,
    /// else the status.
    pub fn footer_text(&self) -> &str {
        self.error
            .as_deref()
            .or(self.left.connect_error.as_deref())
            .or(self.right.connect_error.as_deref())
            .unwrap_or(self.status.as_str())
    }

    pub fn set_listed(&mut self, focus: SftpFocus, path: String, entries: Vec<SftpRow>) {
        self.side_mut(focus).set_listed(path, entries);
        self.loading = false;
        self.error = None;
    }

    pub fn selected(&self, focus: SftpFocus) -> Option<&SftpRow> {
        self.side(focus).selected_row()
    }

    pub fn focused_entries(&self) -> &[SftpRow] {
        &self.side(self.focus).entries
    }

    pub fn move_selection(&mut self, delta: isize) {
        let len = self.focused_entries().len();
        if len == 0 {
            return;
        }
        let slot = &mut self.side_mut(self.focus).selected;
        let cur = slot.unwrap_or(0) as isize;
        let next = (cur + delta).clamp(0, (len as isize) - 1) as usize;
        *slot = Some(next);
    }

    /// Update hover from a pointer position. Returns whether it changed.
    pub fn set_hover(&mut self, hit: Option<SftpHit>) -> bool {
        if self.hover == hit {
            return false;
        }
        self.hover = hit;
        true
    }

    pub fn begin_mkdir(&mut self) {
        self.name_edit = Some(SftpNameEdit {
            kind: SftpNameKind::Mkdir,
            side: self.focus,
            draft: TextDraft::new(""),
            from_path: None,
        });
        self.status = "Name the new folder, then Enter or ✓".into();
        self.error = None;
    }

    pub fn begin_rename(&mut self) -> bool {
        let Some(row) = self.selected(self.focus).cloned() else {
            let side = match self.focus {
                SftpFocus::Left => "left",
                SftpFocus::Right => "right",
            };
            self.error = Some(format!(
                "Select a file or folder on the {side} pane to rename"
            ));
            return false;
        };
        let mut draft = TextDraft::new(row.name);
        draft.select_all();
        self.name_edit = Some(SftpNameEdit {
            kind: SftpNameKind::Rename,
            side: self.focus,
            draft,
            from_path: Some(row.path),
        });
        self.status = "Edit the name, then Enter or ✓".into();
        self.error = None;
        true
    }

    pub fn cancel_name_edit(&mut self) {
        self.name_edit = None;
        self.status = "Ready".into();
    }

    pub fn begin_conflict(
        &mut self,
        id: u64,
        kind: SftpConflictKind,
        relative_path: impl Into<String>,
    ) {
        self.conflict = Some(SftpConflictPrompt {
            id,
            kind,
            relative_path: relative_path.into(),
            apply_to_all: false,
            focus: DialogFocus::Confirm,
        });
        self.status = "Resolve the conflict to continue…".into();
        self.error = None;
    }

    /// Record worker progress for the transfer bar.
    pub fn set_transfer(&mut self, label: impl Into<String>, done: u64, total: u64) {
        self.transfer = Some(SftpTransfer {
            label: label.into(),
            done,
            total,
        });
    }

    pub fn clear_transfer(&mut self) {
        self.transfer = None;
    }

    pub fn clear_conflict(&mut self) {
        self.conflict = None;
    }

    pub fn toggle_conflict_apply_all(&mut self) -> bool {
        let Some(c) = self.conflict.as_mut() else {
            return false;
        };
        c.apply_to_all = !c.apply_to_all;
        true
    }
}
