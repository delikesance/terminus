//! Uploads tab: the remote directory a dropped or pasted file is uploaded
//! to, one text field per remote family, bound to `[uploads] dir` and
//! `windows-dir`. A field persists on Enter or when it loses focus.

use super::sync::{INTRO_HEIGHT, MAX_WIDTH, SECTION_GAP};
use super::{button_spec, column, edit_draft, Key, Measure, SettingsAction};
use crate::components::button::{ButtonKind, ButtonSize};
use crate::components::input::{field_layout, FieldKind, FieldLayout, TextDraft};
use crate::geom::Rect;

pub const INTRO: &str =
    "Where files you drop or paste on an SSH tab are uploaded on the remote host.";
pub const RESET_LABEL: &str = "Reset";
const PATH_MAX_BYTES: usize = 1024;
const RESET_LIFT: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadsField {
    Linux,
    Windows,
}

impl UploadsField {
    pub const ALL: [UploadsField; 2] = [UploadsField::Linux, UploadsField::Windows];

    pub fn label(self) -> &'static str {
        match self {
            UploadsField::Linux => "Linux / macOS hosts",
            UploadsField::Windows => "Windows hosts",
        }
    }

    pub fn placeholder(self) -> &'static str {
        match self {
            UploadsField::Linux => "/tmp",
            UploadsField::Windows => "Temp folder (%TEMP%)",
        }
    }

    pub fn config_key(self) -> &'static str {
        match self {
            UploadsField::Linux => "dir",
            UploadsField::Windows => "windows-dir",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UploadsTarget {
    Field(UploadsField),
    Reset(UploadsField),
}

#[derive(Debug, Clone, PartialEq, Default)]
struct Entry {
    draft: TextDraft,
    saved: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct UploadsState {
    entries: [Entry; 2],
    pub focus: Option<UploadsField>,
    pub hover: Option<UploadsTarget>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UploadsRow {
    pub field: FieldLayout,
    pub reset: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UploadsLayout {
    pub intro: Rect,
    pub rows: [UploadsRow; 2],
}

impl UploadsState {
    /// Seed both fields from the loaded config.
    pub fn load(&mut self, dir: &str, windows_dir: &str) {
        for (field, value) in [
            (UploadsField::Linux, dir),
            (UploadsField::Windows, windows_dir),
        ] {
            let entry = &mut self.entries[field as usize];
            entry.draft = TextDraft::new(value);
            entry.saved = value.to_string();
        }
    }

    pub fn draft(&self, field: UploadsField) -> &TextDraft {
        &self.entries[field as usize].draft
    }

    pub fn layout(&self, content: Rect, m: Measure) -> UploadsLayout {
        let col = column(content);
        let w = col.width.min(MAX_WIDTH);
        let reset = button_spec(
            m,
            (0.0, 0.0),
            ButtonKind::Text,
            ButtonSize::Small,
            RESET_LABEL,
        );
        let intro = Rect::new(col.x, col.y, w, INTRO_HEIGHT);
        let mut y = intro.bottom() + SECTION_GAP;
        let rows = UploadsField::ALL.map(|_| {
            let field = field_layout((col.x, y), w, FieldKind::Mono, true, false);
            y = field.total.bottom() + SECTION_GAP;
            let height = ButtonSize::Small.height();
            let reset = Rect::new(
                field.box_rect.right() - reset.width(),
                field.box_rect.y - height - RESET_LIFT,
                reset.width(),
                height,
            );
            UploadsRow { field, reset }
        });
        UploadsLayout { intro, rows }
    }

    pub fn hit(&self, l: &UploadsLayout, x: f32, y: f32) -> Option<UploadsTarget> {
        UploadsField::ALL.into_iter().find_map(|f| {
            let row = &l.rows[f as usize];
            if row.reset.contains(x, y) {
                return Some(UploadsTarget::Reset(f));
            }
            row.field
                .box_rect
                .contains(x, y)
                .then_some(UploadsTarget::Field(f))
        })
    }

    pub fn hover(&mut self, content: Rect, m: Measure, x: f32, y: f32) -> bool {
        let l = self.layout(content, m);
        let target = self.hit(&l, x, y);
        let changed = target != self.hover;
        self.hover = target;
        changed
    }

    pub fn press(
        &mut self,
        content: Rect,
        m: Measure,
        x: f32,
        y: f32,
    ) -> Option<SettingsAction> {
        let l = self.layout(content, m);
        let target = self.hit(&l, x, y);
        let focus = match target {
            Some(UploadsTarget::Field(f)) => Some(f),
            _ => None,
        };
        let reset_field = match target {
            Some(UploadsTarget::Reset(f)) => Some(f),
            _ => None,
        };
        let blurred = self.focus.filter(|_| focus != self.focus);
        let pending = blurred
            .filter(|f| Some(*f) != reset_field)
            .and_then(|f| self.commit(f));
        self.focus = focus;
        match reset_field {
            Some(f) => merge(pending, self.reset(f)),
            None => pending,
        }
    }

    /// Drop focus, persisting a pending edit.
    pub fn blur(&mut self) -> Option<SettingsAction> {
        self.focus.take().and_then(|f| self.commit(f))
    }

    pub fn key(&mut self, key: Key) -> Option<SettingsAction> {
        let field = self.focus?;
        match key {
            Key::Enter => self.blur(),
            Key::Escape => {
                self.focus = None;
                let entry = &mut self.entries[field as usize];
                entry.draft = TextDraft::new(entry.saved.clone());
                None
            }
            Key::Tab | Key::ShiftTab => {
                let action = self.commit(field);
                self.focus = match (key, field) {
                    (Key::Tab, UploadsField::Linux) => Some(UploadsField::Windows),
                    (Key::ShiftTab, UploadsField::Windows) => Some(UploadsField::Linux),
                    _ => None,
                };
                action
            }
            other => {
                edit_draft(&mut self.entries[field as usize].draft, other);
                None
            }
        }
    }

    pub fn insert_text(&mut self, text: &str) -> bool {
        self.focus.is_some_and(|f| {
            self.entries[f as usize]
                .draft
                .insert(text, PATH_MAX_BYTES, false)
        })
    }

    pub fn captures_keyboard(&self) -> bool {
        self.focus.is_some()
    }

    fn reset(&mut self, field: UploadsField) -> Option<SettingsAction> {
        self.entries[field as usize].draft = TextDraft::new("");
        self.commit(field)
    }

    fn commit(&mut self, field: UploadsField) -> Option<SettingsAction> {
        let entry = &mut self.entries[field as usize];
        let value = entry.draft.value.trim().to_string();
        if value == entry.saved {
            return None;
        }
        entry.saved.clone_from(&value);
        Some(SettingsAction::SetUploadDir { field, value })
    }
}

fn merge(
    first: Option<SettingsAction>,
    second: Option<SettingsAction>,
) -> Option<SettingsAction> {
    match (first, second) {
        (Some(a), Some(b)) => Some(SettingsAction::Batch(vec![a, b])),
        (a, b) => a.or(b),
    }
}

#[cfg(test)]
#[path = "uploads_tests.rs"]
mod tests;
