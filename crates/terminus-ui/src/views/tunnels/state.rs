use std::cell::Cell;

use super::form::{FormKey, FormOutcome, TunnelDraft, TunnelForm};
use super::layout::{dialog_hit, dialog_layout, list_hit, DialogHit, ListHit, Metrics};
use super::model::{running_count, TunnelItem, TunnelKind};
use crate::geom::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hover {
    None,
    New,
    Card(usize),
    Toggle(usize),
    Delete(usize),
    Cancel,
    Confirm,
}

/// What the app must do after an input event.
#[derive(Debug, Clone, PartialEq)]
pub enum TunnelAction {
    None,
    /// Only UI state changed: repaint (`Route::request_overlay_redraw`).
    Redraw,
    /// Persist (create or update) this tunnel.
    Save(TunnelDraft),
    /// Start it if stopped/failed, stop it if running.
    Toggle(String),
    Delete(String),
}

#[derive(Debug)]
pub struct TunnelsState {
    pub host_label: String,
    pub items: Vec<TunnelItem>,
    pub form: Option<TunnelForm>,
    pub hover: Hover,
    pub metrics: Cell<Metrics>,
}

impl TunnelsState {
    pub fn new(host_label: &str) -> Self {
        Self {
            host_label: host_label.into(),
            items: Vec::new(),
            form: None,
            hover: Hover::None,
            metrics: Cell::new(Metrics::default()),
        }
    }

    /// Badge for the Tunnels tab of the current machine.
    pub fn running_count(&self) -> usize {
        running_count(&self.items)
    }

    pub fn open_new(&mut self) {
        self.form = Some(TunnelForm::new(&self.host_label));
    }

    pub fn open_edit(&mut self, index: usize) {
        if let Some(item) = self.items.get(index) {
            self.form = Some(TunnelForm::editing(item, &self.host_label));
        }
    }

    fn hover_at(&self, content: Rect, x: f32, y: f32) -> Hover {
        let m = self.metrics.get();
        if let Some(form) = &self.form {
            let l = dialog_layout(content, form, &m);
            return match dialog_hit(&l, form, x, y) {
                DialogHit::Cancel => Hover::Cancel,
                DialogHit::Confirm => Hover::Confirm,
                _ => Hover::None,
            };
        }
        match list_hit(content, &self.items, &m, x, y) {
            Some(ListHit::New) => Hover::New,
            Some(ListHit::Toggle(i)) => Hover::Toggle(i),
            Some(ListHit::Delete(i)) => Hover::Delete(i),
            Some(ListHit::Card(i)) => Hover::Card(i),
            None => Hover::None,
        }
    }

    /// Pointer shape at `(x, y)`: a hand over buttons, segments and cards
    /// (a card press opens it for editing), an I-beam over form fields.
    pub fn cursor_at(
        &self,
        content: Rect,
        x: f32,
        y: f32,
    ) -> crate::chrome::ChromeCursor {
        use crate::chrome::ChromeCursor;
        let m = self.metrics.get();
        if let Some(form) = &self.form {
            let l = dialog_layout(content, form, &m);
            return match dialog_hit(&l, form, x, y) {
                DialogHit::Field(_) => ChromeCursor::Text,
                DialogHit::Kind(_) | DialogHit::Cancel | DialogHit::Confirm => {
                    ChromeCursor::Pointer
                }
                DialogHit::Inside | DialogHit::Outside => ChromeCursor::Default,
            };
        }
        match list_hit(content, &self.items, &m, x, y) {
            Some(_) => ChromeCursor::Pointer,
            None => ChromeCursor::Default,
        }
    }

    /// Pointer move; true when the hover changed (repaint).
    pub fn hover(&mut self, content: Rect, x: f32, y: f32) -> bool {
        let next = self.hover_at(content, x, y);
        let changed = next != self.hover;
        self.hover = next;
        changed
    }

    /// Pointer press at `(x, y)`.
    pub fn press(
        &mut self,
        content: Rect,
        x: f32,
        y: f32,
        port_free: &dyn Fn(u16) -> bool,
    ) -> TunnelAction {
        let m = self.metrics.get();
        if let Some(form) = self.form.as_mut() {
            let l = dialog_layout(content, form, &m);
            return match dialog_hit(&l, form, x, y) {
                DialogHit::Kind(i) => {
                    form.set_kind(TunnelKind::ALL[i.min(2)]);
                    TunnelAction::Redraw
                }
                DialogHit::Field(f) => {
                    form.focus = f;
                    TunnelAction::Redraw
                }
                DialogHit::Cancel => {
                    self.form = None;
                    TunnelAction::Redraw
                }
                DialogHit::Confirm => self.submit(port_free),
                DialogHit::Inside | DialogHit::Outside => TunnelAction::None,
            };
        }
        match list_hit(content, &self.items, &m, x, y) {
            Some(ListHit::New) => {
                self.open_new();
                TunnelAction::Redraw
            }
            Some(ListHit::Toggle(i)) => TunnelAction::Toggle(self.items[i].id.clone()),
            Some(ListHit::Delete(i)) => TunnelAction::Delete(self.items[i].id.clone()),
            Some(ListHit::Card(i)) => {
                self.open_edit(i);
                TunnelAction::Redraw
            }
            None => TunnelAction::None,
        }
    }

    fn submit(&mut self, port_free: &dyn Fn(u16) -> bool) -> TunnelAction {
        let Some(form) = self.form.as_mut() else {
            return TunnelAction::None;
        };
        match form.validate(port_free) {
            Ok(draft) => {
                self.form = None;
                TunnelAction::Save(draft)
            }
            Err(()) => TunnelAction::Redraw,
        }
    }

    /// Keyboard input while the dialog is open (ignored otherwise).
    pub fn key(&mut self, key: FormKey, port_free: &dyn Fn(u16) -> bool) -> TunnelAction {
        let Some(form) = self.form.as_mut() else {
            return TunnelAction::None;
        };
        match form.key(key) {
            FormOutcome::None => TunnelAction::Redraw,
            FormOutcome::Cancel => {
                self.form = None;
                TunnelAction::Redraw
            }
            FormOutcome::Submit => self.submit(port_free),
        }
    }

    /// Index of the segmented option for the open form's kind.
    pub fn form_kind_index(&self) -> usize {
        self.form.as_ref().map_or(0, |f| f.kind.index())
    }
}
