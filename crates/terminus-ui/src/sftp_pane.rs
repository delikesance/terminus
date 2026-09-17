//! Dual-pane SFTP browser state, geometry, and hit-testing (paint-free).
//!
//! Layout lives inside the leaf grid `layout_rect`:
//! toolbar (Close + optional name edit) → Left | Right lists → status footer.
//!
//! Either side may be local FS or a remote host (`SftpBackend`).

use crate::geom::Rect;
use crate::settings::{field_input_in_card, FIELD_CARD_HEIGHT};
use crate::text_field::{FieldPaint, TextDraft};

/// Row height for file entries.
pub const ROW_HEIGHT: f32 = 30.0;
/// Breadcrumb / header strip height.
pub const HEADER_HEIGHT: f32 = 34.0;
/// Top toolbar (close / name edit) height.
pub const TOOLBAR_HEIGHT: f32 = 40.0;
/// Shared status footer height.
pub const FOOTER_HEIGHT: f32 = 28.0;
/// Gap between left and right panes.
pub const PANE_GAP: f32 = 6.0;
/// Horizontal padding inside a pane.
pub const PANE_PAD: f32 = 8.0;
/// Toolbar / header icon button size.
pub const BTN_SIZE: f32 = 28.0;
/// Gap between toolbar buttons.
pub const BTN_GAP: f32 = 6.0;
/// Pointer travel before a row press becomes a drag.
pub const SFTP_DRAG_THRESHOLD: f32 = 5.0;

/// Which side of the dual-pane has keyboard / selection focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SftpFocus {
    #[default]
    Left,
    Right,
}

/// What a pane side is browsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SftpBackend {
    Local,
    Remote { host_id: String, label: String },
}

impl SftpBackend {
    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }
}

/// One side of the dual-pane browser.
#[derive(Debug, Clone, PartialEq)]
pub struct SftpSideState {
    pub backend: SftpBackend,
    pub cwd: String,
    pub entries: Vec<SftpRow>,
    pub selected: Option<usize>,
    pub scroll: f32,
}

impl SftpSideState {
    pub fn local(cwd: impl Into<String>) -> Self {
        Self {
            backend: SftpBackend::Local,
            cwd: cwd.into(),
            entries: Vec::new(),
            selected: None,
            scroll: 0.0,
        }
    }

    pub fn remote(host_id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            backend: SftpBackend::Remote {
                host_id: host_id.into(),
                label: label.into(),
            },
            cwd: "/".into(),
            entries: Vec::new(),
            selected: None,
            scroll: 0.0,
        }
    }

    /// Pane header title: `"Local"` or the remote host label.
    pub fn title(&self) -> &str {
        match &self.backend {
            SftpBackend::Local => "Local",
            SftpBackend::Remote { label, .. } => label.as_str(),
        }
    }

    pub fn is_local(&self) -> bool {
        self.backend.is_local()
    }

    pub fn selected_row(&self) -> Option<&SftpRow> {
        self.selected.and_then(|i| self.entries.get(i))
    }

    pub fn set_listed(&mut self, path: String, entries: Vec<SftpRow>) {
        self.cwd = path;
        self.entries = entries;
        self.selected = None;
        self.scroll = 0.0;
    }
}

/// Active file drag between panes.
#[derive(Debug, Clone, PartialEq)]
pub struct SftpDrag {
    pub from: SftpFocus,
    pub row_index: usize,
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub pointer_x: f32,
    pub pointer_y: f32,
}

/// Inline name prompt kind (new folder or rename).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SftpNameKind {
    Mkdir,
    Rename,
}

/// Active inline name editor for mkdir / rename.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SftpNameEdit {
    pub kind: SftpNameKind,
    pub side: SftpFocus,
    pub draft: TextDraft,
    /// Absolute path being renamed (rename only).
    pub from_path: Option<String>,
}

impl SftpNameEdit {
    pub fn field_label(&self) -> &'static str {
        match self.kind {
            SftpNameKind::Mkdir => "Folder name",
            SftpNameKind::Rename => "New name",
        }
    }

    pub fn field_placeholder(&self) -> &'static str {
        match self.kind {
            SftpNameKind::Mkdir => "Folder name…",
            SftpNameKind::Rename => "New name…",
        }
    }

    /// Shared [`FieldPaint`] model (same path as Settings text fields).
    pub fn field_paint(&self) -> FieldPaint {
        FieldPaint::from_draft(&self.draft, self.field_placeholder(), true)
    }
}

/// One row in either pane list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SftpRow {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
}

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
    /// Active drag ghost (file being dragged between panes).
    pub drag: Option<SftpDrag>,
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
            drag: None,
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
            drag: None,
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
            self.error = Some(format!("Select a file or folder on the {side} pane to rename"));
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
}

/// Computed geometry for one paint / hit-test pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SftpPaneLayout {
    pub bounds: Rect,
    pub toolbar: Rect,
    pub btn_close: Rect,
    pub name_field: Rect,
    pub name_confirm: Rect,
    pub name_cancel: Rect,
    pub left: Rect,
    pub right: Rect,
    pub left_header: Rect,
    pub right_header: Rect,
    pub left_parent_btn: Rect,
    pub right_parent_btn: Rect,
    pub left_list: Rect,
    pub right_list: Rect,
    pub footer: Rect,
}

impl SftpPaneLayout {
    /// Build layout inside the leaf grid's `layout_rect` (logical pixels).
    pub fn from_bounds(bounds: Rect) -> Self {
        Self::with_name_edit(bounds, false)
    }

    /// Layout for an active SFTP session (expands toolbar while naming).
    pub fn from_state(bounds: Rect, state: &SftpPaneState) -> Self {
        Self::with_name_edit(bounds, state.name_edit.is_some())
    }

    /// Build layout; when `name_editing`, toolbar grows for the shared field card.
    pub fn with_name_edit(bounds: Rect, name_editing: bool) -> Self {
        const TOOLBAR_PAD: f32 = 4.0;
        let toolbar_h = if name_editing {
            FIELD_CARD_HEIGHT + 2.0 * TOOLBAR_PAD
        } else {
            TOOLBAR_HEIGHT
        };
        let toolbar = Rect::new(bounds.x, bounds.y, bounds.width, toolbar_h);
        let footer = Rect::new(
            bounds.x,
            bounds.bottom() - FOOTER_HEIGHT,
            bounds.width,
            FOOTER_HEIGHT,
        );

        let btn_y = toolbar.y + (toolbar_h - BTN_SIZE) * 0.5;
        let close_w = 96.0;
        let btn_close = Rect::new(
            toolbar.right() - PANE_PAD - close_w,
            btn_y,
            close_w,
            BTN_SIZE,
        );

        // Name editor spans left of Close (shared field card).
        let name_actions_w = BTN_SIZE * 2.0 + BTN_GAP * 2.0;
        let name_left = toolbar.x + PANE_PAD;
        let name_right = btn_close.x - BTN_GAP - name_actions_w;
        let name_field = if name_editing {
            Rect::new(
                name_left,
                toolbar.y + TOOLBAR_PAD,
                (name_right - name_left).max(120.0),
                FIELD_CARD_HEIGHT,
            )
        } else {
            Rect::new(name_left, btn_y, 0.0, BTN_SIZE)
        };
        let name_btn_y = if name_editing {
            let input = field_input_in_card(name_field);
            input.y + (input.height - BTN_SIZE) * 0.5
        } else {
            btn_y
        };
        let name_cancel = Rect::new(
            btn_close.x - BTN_GAP - BTN_SIZE,
            name_btn_y,
            BTN_SIZE,
            BTN_SIZE,
        );
        let name_confirm = Rect::new(
            name_cancel.x - BTN_GAP - BTN_SIZE,
            name_btn_y,
            BTN_SIZE,
            BTN_SIZE,
        );

        let body_top = bounds.y + toolbar_h;
        let body_h = (footer.y - body_top).max(0.0);
        let pane_w = ((bounds.width - PANE_GAP) * 0.5).max(0.0);
        let left = Rect::new(bounds.x, body_top, pane_w, body_h);
        let right = Rect::new(bounds.x + pane_w + PANE_GAP, body_top, pane_w, body_h);
        let left_header = Rect::new(left.x, left.y, left.width, HEADER_HEIGHT);
        let right_header = Rect::new(right.x, right.y, right.width, HEADER_HEIGHT);
        let parent_pad = (HEADER_HEIGHT - BTN_SIZE) * 0.5;
        let left_parent_btn = Rect::new(
            left_header.x + PANE_PAD,
            left_header.y + parent_pad,
            BTN_SIZE,
            BTN_SIZE,
        );
        let right_parent_btn = Rect::new(
            right_header.x + PANE_PAD,
            right_header.y + parent_pad,
            BTN_SIZE,
            BTN_SIZE,
        );
        let list_h = (body_h - HEADER_HEIGHT).max(0.0);
        let left_list = Rect::new(left.x, left.y + HEADER_HEIGHT, left.width, list_h);
        let right_list = Rect::new(right.x, right.y + HEADER_HEIGHT, right.width, list_h);
        Self {
            bounds,
            toolbar,
            btn_close,
            name_field,
            name_confirm,
            name_cancel,
            left,
            right,
            left_header,
            right_header,
            left_parent_btn,
            right_parent_btn,
            left_list,
            right_list,
            footer,
        }
    }

    /// Row rect for `index` in the left list (unclipped; may sit above/below).
    pub fn left_row_rect(&self, index: usize, scroll: f32) -> Rect {
        row_rect(self.left_list, index, scroll)
    }

    pub fn right_row_rect(&self, index: usize, scroll: f32) -> Rect {
        row_rect(self.right_list, index, scroll)
    }

    pub fn hit_test(&self, state: &SftpPaneState, x: f32, y: f32) -> SftpHit {
        if !self.bounds.contains(x, y) {
            return SftpHit::Miss;
        }
        if state.name_edit.is_some() {
            if self.name_confirm.contains(x, y) {
                return SftpHit::NameConfirm;
            }
            if self.name_cancel.contains(x, y) {
                return SftpHit::NameCancel;
            }
            if self.name_field.contains(x, y) {
                return SftpHit::NameField;
            }
            if self.btn_close.contains(x, y) {
                return SftpHit::Close;
            }
            if self.toolbar.contains(x, y) {
                return SftpHit::Consume;
            }
        }
        if self.btn_close.contains(x, y) {
            return SftpHit::Close;
        }
        if self.toolbar.contains(x, y) {
            return SftpHit::Consume;
        }
        if self.footer.contains(x, y) {
            return SftpHit::Footer;
        }
        if self.left_parent_btn.contains(x, y) {
            return SftpHit::LeftParent;
        }
        if self.right_parent_btn.contains(x, y) {
            return SftpHit::RightParent;
        }
        if self.left_header.contains(x, y) {
            return SftpHit::LeftCrumb;
        }
        if self.right_header.contains(x, y) {
            return SftpHit::RightCrumb;
        }
        if self.left_list.contains(x, y) {
            if let Some(i) = row_index_at(self.left_list, y, state.left.scroll) {
                if i < state.left.entries.len() {
                    return SftpHit::LeftRow(i);
                }
            }
            return SftpHit::Consume;
        }
        if self.right_list.contains(x, y) {
            if let Some(i) = row_index_at(self.right_list, y, state.right.scroll) {
                if i < state.right.entries.len() {
                    return SftpHit::RightRow(i);
                }
            }
            return SftpHit::Consume;
        }
        SftpHit::Consume
    }

    /// Which list (if any) contains `(x, y)` — used for drag-drop targets.
    pub fn focus_at_list(&self, x: f32, y: f32) -> Option<SftpFocus> {
        if self.left_list.contains(x, y) || self.left.contains(x, y) {
            Some(SftpFocus::Left)
        } else if self.right_list.contains(x, y) || self.right.contains(x, y) {
            Some(SftpFocus::Right)
        } else {
            None
        }
    }
}

fn row_rect(list: Rect, index: usize, scroll: f32) -> Rect {
    Rect::new(
        list.x,
        list.y + index as f32 * ROW_HEIGHT - scroll,
        list.width,
        ROW_HEIGHT,
    )
}

fn row_index_at(list: Rect, y: f32, scroll: f32) -> Option<usize> {
    if y < list.y || y >= list.bottom() {
        return None;
    }
    let rel = y - list.y + scroll;
    if rel < 0.0 {
        return None;
    }
    Some((rel / ROW_HEIGHT) as usize)
}

/// Parent directory for a Unix-style remote path.
pub fn parent_path(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() || trimmed == "/" {
        return "/".into();
    }
    match trimmed.rsplit_once('/') {
        Some(("", _)) => "/".into(),
        Some((parent, _)) if !parent.is_empty() => parent.to_string(),
        _ => "/".into(),
    }
}

/// Join `name` onto a remote directory path.
pub fn join_remote(cwd: &str, name: &str) -> String {
    if cwd == "/" {
        format!("/{name}")
    } else {
        format!("{cwd}/{name}")
    }
}

/// Split `cwd` into breadcrumb `(label, path)` pairs.
pub fn crumb_segments_for_cwd(cwd: &str, is_local: bool) -> Vec<(String, String)> {
    let cwd = if cwd.is_empty() {
        if is_local {
            return vec![(".".into(), ".".into())];
        }
        "/"
    } else {
        cwd
    };
    if !is_local && (cwd == "/" || cwd.is_empty()) {
        return vec![("/".into(), "/".into())];
    }
    let mut out = Vec::new();
    if is_local {
        #[cfg(windows)]
        {
            // Keep drive letter as first segment when present.
            let path = std::path::Path::new(cwd);
            let mut acc = std::path::PathBuf::new();
            for (i, comp) in path.components().enumerate() {
                acc.push(comp.as_os_str());
                let label = if i == 0 {
                    acc.to_string_lossy().into_owned()
                } else {
                    comp.as_os_str().to_string_lossy().into_owned()
                };
                out.push((label, acc.to_string_lossy().into_owned()));
            }
            if out.is_empty() {
                out.push((cwd.to_string(), cwd.to_string()));
            }
            return out;
        }
        #[cfg(not(windows))]
        {
            if cwd == "/" {
                return vec![("/".into(), "/".into())];
            }
            out.push(("/".into(), "/".into()));
            let mut acc = String::new();
            for part in cwd.trim_start_matches('/').split('/').filter(|p| !p.is_empty()) {
                acc.push('/');
                acc.push_str(part);
                out.push((part.to_string(), acc.clone()));
            }
            return out;
        }
    }
    // Remote POSIX-style
    if cwd == "/" {
        return vec![("/".into(), "/".into())];
    }
    out.push(("/".into(), "/".into()));
    let mut acc = String::new();
    for part in cwd.trim_start_matches('/').split('/').filter(|p| !p.is_empty()) {
        acc.push('/');
        acc.push_str(part);
        out.push((part.to_string(), acc.clone()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_state() -> SftpPaneState {
        let mut state = SftpPaneState::new_local_remote("/home/user", "h1", "demo");
        state.set_listed(
            SftpFocus::Left,
            "/home/user".into(),
            vec![
                SftpRow {
                    name: "docs".into(),
                    path: "/home/user/docs".into(),
                    is_dir: true,
                    size: 0,
                },
                SftpRow {
                    name: "a.txt".into(),
                    path: "/home/user/a.txt".into(),
                    is_dir: false,
                    size: 12,
                },
            ],
        );
        state.set_listed(
            SftpFocus::Right,
            "/var".into(),
            vec![SftpRow {
                name: "log".into(),
                path: "/var/log".into(),
                is_dir: true,
                size: 0,
            }],
        );
        state
    }

    #[test]
    fn layout_name_edit_uses_field_card_height() {
        let layout = SftpPaneLayout::with_name_edit(Rect::new(0.0, 0.0, 640.0, 400.0), true);
        assert!(
            (layout.name_field.height - FIELD_CARD_HEIGHT).abs() < 0.01,
            "name field should be FIELD_CARD_HEIGHT, got {}",
            layout.name_field.height
        );
        assert!(layout.toolbar.height > TOOLBAR_HEIGHT);
        assert!(layout.left.y >= layout.toolbar.bottom() - 0.01);
        assert!(layout.name_field.x < layout.btn_close.x);
    }

    #[test]
    fn name_edit_field_paint_uses_shared_model() {
        let mut state = sample_state();
        state.begin_mkdir();
        let edit = state.name_edit.as_ref().unwrap();
        let paint = edit.field_paint();
        assert_eq!(edit.field_label(), "Folder name");
        assert!(paint.placeholder || paint.text.is_empty());
        assert!(paint.show_caret);
        assert_eq!(edit.side, SftpFocus::Left);
    }

    #[test]
    fn hit_test_close_only_in_toolbar() {
        let state = sample_state();
        let layout = SftpPaneLayout::from_bounds(Rect::new(0.0, 0.0, 500.0, 360.0));
        assert_eq!(
            layout.hit_test(
                &state,
                layout.btn_close.x + 4.0,
                layout.btn_close.y + 4.0
            ),
            SftpHit::Close
        );
        // Slim toolbar: no mkdir/upload/download action hits.
        assert_eq!(
            layout.hit_test(&state, layout.toolbar.x + 20.0, layout.toolbar.y + 10.0),
            SftpHit::Consume
        );
    }

    #[test]
    fn hit_test_rows_and_parent() {
        let state = sample_state();
        let layout = SftpPaneLayout::from_bounds(Rect::new(0.0, 0.0, 400.0, 300.0));

        let row0 = layout.left_row_rect(0, 0.0);
        assert_eq!(
            layout.hit_test(&state, row0.x + 4.0, row0.y + 4.0),
            SftpHit::LeftRow(0)
        );

        assert_eq!(
            layout.hit_test(
                &state,
                layout.left_parent_btn.x + 4.0,
                layout.left_parent_btn.y + 4.0
            ),
            SftpHit::LeftParent
        );
        assert_eq!(layout.hit_test(&state, -10.0, -10.0), SftpHit::Miss);
    }

    #[test]
    fn side_titles() {
        let state = sample_state();
        assert_eq!(state.left.title(), "Local");
        assert_eq!(state.right.title(), "demo");
        assert!(state.left.is_local());
        assert!(!state.right.is_local());
    }

    #[test]
    fn parent_and_join_paths() {
        assert_eq!(parent_path("/home/alice/docs"), "/home/alice");
        assert_eq!(parent_path("/"), "/");
        assert_eq!(join_remote("/", "tmp"), "/tmp");
        assert_eq!(join_remote("/home", "alice"), "/home/alice");
    }

    #[test]
    fn move_selection_clamps() {
        let mut state = sample_state();
        state.focus = SftpFocus::Left;
        state.move_selection(1);
        assert_eq!(state.left.selected, Some(1));
        state.move_selection(10);
        assert_eq!(state.left.selected, Some(1));
        state.move_selection(-10);
        assert_eq!(state.left.selected, Some(0));
    }

    #[test]
    fn hit_test_covers_all_primary_targets() {
        let mut state = sample_state();
        state.begin_mkdir();
        let layout = SftpPaneLayout::from_state(Rect::new(0.0, 0.0, 640.0, 400.0), &state);

        assert_eq!(
            layout.hit_test(&state, layout.left_header.x + 4.0, layout.left_header.y + 4.0),
            SftpHit::LeftCrumb
        );
        assert_eq!(
            layout.hit_test(&state, layout.right_header.x + 4.0, layout.right_header.y + 4.0),
            SftpHit::RightCrumb
        );
        assert_eq!(
            layout.hit_test(
                &state,
                layout.right_parent_btn.x + 2.0,
                layout.right_parent_btn.y + 2.0
            ),
            SftpHit::RightParent
        );
        assert_eq!(
            layout.hit_test(&state, layout.name_field.x + 4.0, layout.name_field.y + 4.0),
            SftpHit::NameField
        );
        assert_eq!(
            layout.hit_test(
                &state,
                layout.name_confirm.x + 2.0,
                layout.name_confirm.y + 2.0
            ),
            SftpHit::NameConfirm
        );
        assert_eq!(
            layout.hit_test(
                &state,
                layout.name_cancel.x + 2.0,
                layout.name_cancel.y + 2.0
            ),
            SftpHit::NameCancel
        );
        assert_eq!(
            layout.hit_test(&state, layout.footer.x + 8.0, layout.footer.y + 4.0),
            SftpHit::Footer
        );
        let right_row = layout.right_row_rect(0, 0.0);
        assert_eq!(
            layout.hit_test(&state, right_row.x + 4.0, right_row.y + 4.0),
            SftpHit::RightRow(0)
        );
    }

    #[test]
    fn crumb_segments_and_navigate() {
        let state = sample_state();
        let segs = state.crumb_segments(SftpFocus::Left);
        assert!(
            segs.len() >= 2,
            "expected root + home/user segments, got {segs:?}"
        );
        assert_eq!(segs.last().map(|(_, p)| p.as_str()), Some("/home/user"));
        let root = state.navigate_crumb_path(SftpFocus::Left, 0).unwrap();
        assert_eq!(root, "/");
        let mid = state.navigate_crumb_path(SftpFocus::Left, segs.len() - 1);
        assert_eq!(mid.as_deref(), Some("/home/user"));
    }

    #[test]
    fn name_edit_rename_label() {
        let mut state = sample_state();
        state.focus = SftpFocus::Left;
        state.left.selected = Some(1);
        assert!(state.begin_rename());
        let edit = state.name_edit.as_ref().unwrap();
        assert_eq!(edit.kind, SftpNameKind::Rename);
        assert_eq!(edit.field_label(), "New name");
        assert_eq!(edit.draft.value, "a.txt");
    }

    #[test]
    fn local_local_constructor() {
        let state = SftpPaneState::new_local_local("/tmp/a", "/tmp/b");
        assert!(state.left.is_local());
        assert!(state.right.is_local());
        assert_eq!(state.left.cwd, "/tmp/a");
        assert_eq!(state.right.cwd, "/tmp/b");
    }

    #[test]
    fn drag_struct_carries_dir_flag() {
        let drag = SftpDrag {
            from: SftpFocus::Left,
            row_index: 0,
            name: "docs".into(),
            path: "/home/user/docs".into(),
            is_dir: true,
            pointer_x: 10.0,
            pointer_y: 20.0,
        };
        assert!(drag.is_dir);
    }
}
