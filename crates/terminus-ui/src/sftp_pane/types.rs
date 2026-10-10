use super::SftpRow;
use crate::components::input::{FieldPaint, TextDraft};
use crate::components::overlay::{DialogFocus, DialogKey, DialogKind};
use crate::confirm::ConfirmSpec;

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
    /// Why this pane's host could not be reached; cleared once it lists.
    pub connect_error: Option<String>,
}

impl SftpSideState {
    pub fn local(cwd: impl Into<String>) -> Self {
        Self {
            backend: SftpBackend::Local,
            cwd: cwd.into(),
            entries: Vec::new(),
            selected: None,
            scroll: 0.0,
            connect_error: None,
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
            connect_error: None,
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
        self.connect_error = None;
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

/// File vs directory conflict during a differential folder transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SftpConflictKind {
    File,
    Directory,
}

/// Modal prompt: replace or keep an existing destination path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SftpConflictPrompt {
    pub id: u64,
    pub kind: SftpConflictKind,
    pub relative_path: String,
    pub apply_to_all: bool,
    /// Button the keyboard acts on (Replace by default).
    pub focus: DialogFocus,
}

/// What a key on the conflict dialog asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SftpConflictKey {
    /// Focus moved: repaint.
    Changed,
    Overwrite,
    Keep,
}

impl SftpConflictPrompt {
    /// File or folder name, without its parent path.
    pub fn name(&self) -> &str {
        self.relative_path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(&self.relative_path)
    }

    pub fn title(&self) -> String {
        crate::confirm::elide_title(&format!("{} already exists", self.name()))
    }

    pub fn message(&self) -> String {
        let what = match self.kind {
            SftpConflictKind::File => "file",
            SftpConflictKind::Directory => "folder",
        };
        format!(
            "There is already a {what} with this name at \u{201c}{}\u{201d}. Replace it with the copy being transferred?",
            self.relative_path
        )
    }

    /// The With-option dialog: Replace / Keep existing + apply to all.
    pub fn spec(&self) -> ConfirmSpec {
        let mut spec = ConfirmSpec::new(
            DialogKind::WithOption,
            &format!("{} already exists", self.name()),
            self.message(),
            "Keep existing",
            "Replace",
        );
        spec.option = Some("Do this for every conflict".to_string());
        spec
    }

    /// Esc keeps the existing file; Enter runs the focused button.
    pub fn key(&mut self, key: DialogKey) -> SftpConflictKey {
        use crate::components::overlay::{dialog_key, DialogOutcome};
        match dialog_key(key, self.focus) {
            DialogOutcome::Cancel => SftpConflictKey::Keep,
            DialogOutcome::Confirm => SftpConflictKey::Overwrite,
            DialogOutcome::Focus(f) => {
                self.focus = f;
                SftpConflictKey::Changed
            }
        }
    }
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
