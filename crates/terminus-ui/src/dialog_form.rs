//! A generic dialog form component extracted from `add_host.rs`.
//! Supports dynamic text fields while strictly retaining the exact visual layout
//! (paddings, corner radii, and button placements) of the original Host modal.

use crate::geom::Rect;
use crate::text_field::{TextDraft, TextMoveKind};

pub const PAD: f32 = 24.0;
pub const TITLE_HEIGHT: f32 = 28.0;
pub const FIELD_HEIGHT: f32 = 52.0;
pub const FIELD_GAP: f32 = 12.0;
/// Corner radius (`rounded-2xl`).
pub const DIALOG_RADIUS: f32 = 16.0;
pub const BUTTON_HEIGHT: f32 = 32.0;
pub const BUTTON_GAP: f32 = 8.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormFieldData {
    pub key: String,
    pub label: String,
    pub draft: TextDraft,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicFormState {
    pub title: String,
    pub fields: Vec<FormFieldData>,
    pub focused_index: usize,
    pub error: Option<String>,
    pub closing: bool,
    pub save_label: String,
}

impl DynamicFormState {
    pub fn new(title: impl Into<String>, save_label: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            fields: Vec::new(),
            focused_index: 0,
            error: None,
            closing: false,
            save_label: save_label.into(),
        }
    }

    pub fn with_field(mut self, key: &str, label: &str, initial_value: &str) -> Self {
        let mut draft = TextDraft::new(initial_value.to_string());
        draft.caret = initial_value.chars().count();
        self.fields.push(FormFieldData {
            key: key.to_string(),
            label: label.to_string(),
            draft,
        });
        self
    }

    pub fn is_open(&self) -> bool {
        !self.closing
    }

    pub fn set_error(&mut self, err: String) {
        self.error = Some(err);
    }
    
    pub fn get_value(&self, key: &str) -> Option<&str> {
        self.fields.iter().find(|f| f.key == key).map(|f| f.draft.value.as_str())
    }

    pub fn cycle_focus(&mut self, reverse: bool) {
        if self.fields.is_empty() { return; }
        if reverse {
            self.focused_index = (self.focused_index + self.fields.len() - 1) % self.fields.len();
        } else {
            self.focused_index = (self.focused_index + 1) % self.fields.len();
        }
    }

    pub fn focused_draft_mut(&mut self) -> Option<&mut TextDraft> {
        let i = self.focused_index;
        self.fields.get_mut(i).map(|f| &mut f.draft)
    }

    pub fn handle_key(&mut self, key: &str) {
        if let Some(draft) = self.focused_draft_mut() {
            draft.insert(key, usize::MAX, false);
            self.error = None;
        }
    }

    pub fn handle_backspace(&mut self) {
        if let Some(draft) = self.focused_draft_mut() {
            draft.backspace(false);
            self.error = None;
        }
    }
    
    pub fn handle_delete(&mut self) {
        if let Some(draft) = self.focused_draft_mut() {
            draft.delete_forward(false);
            self.error = None;
        }
    }
    
    pub fn move_caret(&mut self, offset: isize) {
        use crate::text_field::TextMoveKind;
        if let Some(draft) = self.focused_draft_mut() {
            if offset < 0 {
                for _ in 0..offset.abs() {
                    draft.move_left(TextMoveKind::Collapse, false);
                }
            } else {
                for _ in 0..offset.abs() {
                    draft.move_right(TextMoveKind::Collapse, false);
                }
            }
        }
    }

}

pub struct DialogFormLayout {
    pub dialog: Rect,
    pub title: Rect,
    pub fields: Vec<Rect>,
    pub error_line: Option<Rect>,
    pub cancel_btn: Rect,
    pub save_btn: Rect,
}

impl DialogFormLayout {
    pub fn compute(
        form: &DynamicFormState,
        window_width: f32,
        window_height: f32,
    ) -> Self {
        let width = 460.0;
        let mut height = PAD + TITLE_HEIGHT;
        let n: f32 = form.fields.len() as f32;
        
        if n > 0.0 {
            height += n * FIELD_HEIGHT + (n - 1.0) * FIELD_GAP;
        }

        let error_rect = if form.error.is_some() {
            height += PAD; 
            height += 24.0; // error text height space
            Some(Rect::new(0.0,0.0,0.0,0.0)) // assigned later
        } else {
            None
        };

        height += PAD + BUTTON_HEIGHT + PAD;

        let x = (window_width - width).max(0.0) / 2.0;
        let y = (window_height - height).max(0.0) / 2.0;
        let dialog = Rect::new(x, y, width, height);

        let title = Rect::new(x + PAD, y + PAD, width - 2.0 * PAD, TITLE_HEIGHT);
        
        let mut fields = Vec::with_capacity(form.fields.len());
        for i in 0..form.fields.len() {
            let ry = y + PAD + TITLE_HEIGHT + i as f32 * (FIELD_HEIGHT + FIELD_GAP);
            fields.push(Rect::new(x + PAD, ry, width - 2.0 * PAD, FIELD_HEIGHT));
        }

        let mut bottom_y = if form.fields.is_empty() {
            title.bottom() + PAD
        } else {
            fields.last().unwrap().bottom() + PAD
        };

        let mut final_error_rect = None;
        if form.error.is_some() {
            final_error_rect = Some(Rect::new(x + PAD, bottom_y, width - 2.0 * PAD, 24.0));
            bottom_y += 24.0 + PAD;
        }

        // buttons are aligned to the right like AddHostForm
        // wait, AddHostForm has CANCEL_BUTTON_WIDTH and CONNECT_BUTTON_WIDTH
        // Let's use 84.0 and 72.0
        let btn_w = 84.0;
        let cancel_w = 72.0;
        
        // Right alignment
        let save_x = x + width - PAD - btn_w;
        let cancel_x = save_x - BUTTON_GAP - cancel_w;
        
        let save_btn = Rect::new(save_x, bottom_y, btn_w, BUTTON_HEIGHT);
        let cancel_btn = Rect::new(cancel_x, bottom_y, cancel_w, BUTTON_HEIGHT);

        Self {
            dialog,
            title,
            fields,
            error_line: final_error_rect,
            cancel_btn,
            save_btn,
        }
    }

    pub fn hit_test(&self, x: f32, y: f32) -> Option<DynamicFormHit> {
        if !self.dialog.contains(x, y) {
            return Some(DynamicFormHit::Background);
        }
        for (i, r) in self.fields.iter().enumerate() {
            if r.contains(x, y) {
                return Some(DynamicFormHit::Field(i));
            }
        }
        if self.save_btn.contains(x, y) {
            return Some(DynamicFormHit::Save);
        }
        if self.cancel_btn.contains(x, y) {
            return Some(DynamicFormHit::Cancel);
        }
        Some(DynamicFormHit::Background)
    }
}

pub enum DynamicFormHit {
    Field(usize),
    Save,
    Cancel,
    Background,
}
