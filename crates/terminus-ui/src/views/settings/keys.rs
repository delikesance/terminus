//! SSH keys tab: intro line, Import / Generate key, an inline draft card,
//! one list card per managed key (Copy public key, trash with a destructive
//! confirm).

use super::{
    button_spec, column, confirm_hit, confirm_layout, edit_draft, icon_button_spec,
    Confirm, ConfirmHit, ConfirmLayout, Key, Measure, SettingsAction, ROW_GAP,
};
use crate::components::button::{ButtonKind, ButtonSize};
use crate::components::input::{
    field_layout, FieldKind, FieldLayout, HELPER_HEIGHT, LABEL_GAP,
};
use crate::components::list::{
    card_hit, card_layout, CardHit, CardLayout, CardSpec, CARD_HEIGHT,
};
use crate::components::overlay::{dialog_key, DialogFocus, DialogKey, DialogOutcome};
use crate::geom::Rect;
use crate::settings::{SshKeyItem, KEY_LABEL_MAX_BYTES, KEY_PEM_MAX_BYTES};
use crate::text_field::TextDraft;

pub const INTRO: &str =
    "Keys Terminus manages for you. Copy a public key into a server's authorized_keys.";
pub const HEADER_HEIGHT: f32 = 40.0;
pub const HEADER_GAP: f32 = 18.0;
pub const DRAFT_PAD: f32 = 20.0;
pub const DRAFT_TITLE_HEIGHT: f32 = 22.0;
pub const DRAFT_FIELD_GAP: f32 = 14.0;
pub const DRAFT_MAX_FIELD_WIDTH: f32 = 520.0;
pub const PASSPHRASE_MAX_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftMode {
    Generate,
    Import,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftField {
    Name,
    Pem,
    Passphrase,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeyDraft {
    pub mode: DraftMode,
    pub name: TextDraft,
    pub pem: TextDraft,
    pub passphrase: TextDraft,
    pub focus: DraftField,
    pub error: Option<String>,
}

impl KeyDraft {
    pub fn new(mode: DraftMode) -> Self {
        Self {
            mode,
            name: TextDraft::default(),
            pem: TextDraft::default(),
            passphrase: TextDraft::default(),
            focus: DraftField::Name,
            error: None,
        }
    }

    pub fn fields(&self) -> &'static [DraftField] {
        match self.mode {
            DraftMode::Generate => &[DraftField::Name],
            DraftMode::Import => {
                &[DraftField::Name, DraftField::Pem, DraftField::Passphrase]
            }
        }
    }

    pub fn title(&self) -> &'static str {
        match self.mode {
            DraftMode::Generate => "Generate an SSH key",
            DraftMode::Import => "Import an SSH key",
        }
    }

    pub fn submit_label(&self) -> &'static str {
        match self.mode {
            DraftMode::Generate => "Generate",
            DraftMode::Import => "Import",
        }
    }

    fn active(&mut self) -> (&mut TextDraft, usize, bool) {
        match self.focus {
            DraftField::Name => (&mut self.name, KEY_LABEL_MAX_BYTES, false),
            DraftField::Pem => (&mut self.pem, KEY_PEM_MAX_BYTES, true),
            DraftField::Passphrase => (&mut self.passphrase, PASSPHRASE_MAX_BYTES, false),
        }
    }

    fn cycle(&mut self, delta: i32) {
        let fields = self.fields();
        let at = fields.iter().position(|f| *f == self.focus).unwrap_or(0) as i32;
        let n = fields.len() as i32;
        self.focus = fields[((at + delta).rem_euclid(n)) as usize];
    }

    /// Validate and build the worker request; sets the inline error on failure.
    pub fn submit(&mut self) -> Option<SettingsAction> {
        let name = self.name.value.trim().to_string();
        if name.is_empty() {
            self.error = Some("Enter a name for the key".into());
            return None;
        }
        let pem = self.pem.value.trim().to_string();
        let pem = match self.mode {
            DraftMode::Generate => None,
            DraftMode::Import if pem.is_empty() => {
                self.error = Some("Paste the private key to import".into());
                return None;
            }
            DraftMode::Import => Some(self.pem.value.clone()),
        };
        let passphrase = (!self.passphrase.value.is_empty()
            && self.mode == DraftMode::Import)
            .then(|| self.passphrase.value.clone());
        self.error = None;
        Some(SettingsAction::SubmitKey {
            name,
            pem,
            passphrase,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeysTarget {
    Import,
    Generate,
    Copy(usize),
    Delete(usize),
    Field(DraftField),
    Submit,
    Cancel,
}

#[derive(Debug, Clone, PartialEq)]
struct PendingDelete {
    id: String,
    dialog: Confirm,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct KeysState {
    pub keys: Vec<SshKeyItem>,
    pub draft: Option<KeyDraft>,
    /// Confirmation under the list ("Public key copied").
    pub notice: Option<String>,
    pub hover: Option<KeysTarget>,
    pending_delete: Option<PendingDelete>,
}

/// Geometry of the draft card.
#[derive(Debug, Clone, PartialEq)]
pub struct DraftLayout {
    pub card: Rect,
    pub title: Rect,
    /// `(field, layout)` in tab order.
    pub fields: Vec<(DraftField, FieldLayout)>,
    pub error: Option<Rect>,
    pub cancel: Rect,
    pub submit: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RowLayout {
    pub card: CardLayout,
    pub copy_width: f32,
    pub trash_width: f32,
    pub meta_width: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct KeysLayout {
    pub intro: Rect,
    pub import: Rect,
    pub generate: Rect,
    pub draft: Option<DraftLayout>,
    pub rows: Vec<RowLayout>,
    /// "No keys yet" line when the list and draft are empty.
    pub empty: Option<Rect>,
    pub notice: Option<Rect>,
    pub confirm: Option<ConfirmLayout>,
}

impl KeysState {
    pub fn set_keys(&mut self, keys: Vec<SshKeyItem>) {
        self.keys = keys;
        if let Some(p) = &self.pending_delete {
            if !self.keys.iter().any(|k| k.id == p.id) {
                self.pending_delete = None;
            }
        }
    }

    pub fn open_draft(&mut self, mode: DraftMode) {
        self.draft = Some(KeyDraft::new(mode));
        self.notice = None;
    }

    pub fn close_draft(&mut self) {
        self.draft = None;
    }

    /// Worker failure to show under the draft.
    pub fn set_draft_error(&mut self, message: impl Into<String>) {
        if let Some(d) = &mut self.draft {
            d.error = Some(message.into());
        }
    }

    pub fn blur(&mut self) {
        self.pending_delete = None;
        self.hover = None;
    }

    pub fn delete_dialog(&self) -> Option<&Confirm> {
        self.pending_delete.as_ref().map(|p| &p.dialog)
    }

    pub fn captures_keyboard(&self) -> bool {
        self.draft.is_some() || self.pending_delete.is_some()
    }

    pub fn layout(&self, content: Rect, m: Measure) -> KeysLayout {
        let col = column(content);
        let mut y = col.y;

        let generate_spec = button_spec(
            m,
            (0.0, 0.0),
            ButtonKind::Primary,
            ButtonSize::Medium,
            "Generate key",
        );
        let import_spec = button_spec(
            m,
            (0.0, 0.0),
            ButtonKind::Secondary,
            ButtonSize::Medium,
            "Import",
        );
        let btn_y = y + (HEADER_HEIGHT - ButtonSize::Medium.height()) / 2.0;
        let generate = Rect::new(
            col.right() - generate_spec.width(),
            btn_y,
            generate_spec.width(),
            ButtonSize::Medium.height(),
        );
        let import = Rect::new(
            generate.x - 10.0 - import_spec.width(),
            btn_y,
            import_spec.width(),
            ButtonSize::Medium.height(),
        );
        let intro =
            Rect::new(col.x, y, (import.x - 16.0 - col.x).max(0.0), HEADER_HEIGHT);
        y += HEADER_HEIGHT + HEADER_GAP;

        let draft = self.draft.as_ref().map(|d| {
            let l = draft_layout(d, Rect::new(col.x, y, col.width, 0.0), m);
            y = l.card.bottom() + ROW_GAP;
            l
        });

        let mut rows = Vec::with_capacity(self.keys.len());
        for key in &self.keys {
            let rect = Rect::new(col.x, y, col.width, CARD_HEIGHT);
            let copy_w = button_spec(
                m,
                (0.0, 0.0),
                ButtonKind::Secondary,
                ButtonSize::Medium,
                "Copy public key",
            )
            .width();
            let trash_w =
                icon_button_spec((0.0, 0.0), ButtonKind::Quiet, ButtonSize::Medium)
                    .width();
            let meta_w = m(&key.created, 12.0, false);
            let card = card_layout(
                rect,
                &CardSpec {
                    has_dot: false,
                    meta_width: meta_w,
                    action_widths: &[copy_w, trash_w],
                },
            );
            rows.push(RowLayout {
                card,
                copy_width: copy_w,
                trash_width: trash_w,
                meta_width: meta_w,
            });
            y += CARD_HEIGHT + ROW_GAP;
        }

        let empty = (self.keys.is_empty() && self.draft.is_none())
            .then(|| Rect::new(col.x, y, col.width, 22.0));
        let notice = self
            .notice
            .as_ref()
            .map(|_| Rect::new(col.x, y, col.width, HELPER_HEIGHT));
        let confirm = self
            .pending_delete
            .as_ref()
            .map(|p| confirm_layout(content, m, &p.dialog));

        KeysLayout {
            intro,
            import,
            generate,
            draft,
            rows,
            empty,
            notice,
            confirm,
        }
    }

    pub fn hit(&self, layout: &KeysLayout, x: f32, y: f32) -> Option<KeysTarget> {
        if let Some(d) = &layout.draft {
            if d.submit.contains(x, y) {
                return Some(KeysTarget::Submit);
            }
            if d.cancel.contains(x, y) {
                return Some(KeysTarget::Cancel);
            }
            for (field, fl) in &d.fields {
                if fl.box_rect.contains(x, y) {
                    return Some(KeysTarget::Field(*field));
                }
            }
        }
        if layout.generate.contains(x, y) {
            return Some(KeysTarget::Generate);
        }
        if layout.import.contains(x, y) {
            return Some(KeysTarget::Import);
        }
        for (i, row) in layout.rows.iter().enumerate() {
            let spec = CardSpec {
                has_dot: false,
                meta_width: row.meta_width,
                action_widths: &[row.copy_width, row.trash_width],
            };
            match card_hit(row.card.rect, &spec, x, y) {
                Some(CardHit::Action(0)) => return Some(KeysTarget::Copy(i)),
                Some(CardHit::Action(1)) => return Some(KeysTarget::Delete(i)),
                _ => {}
            }
        }
        None
    }

    pub fn hover(&mut self, content: Rect, m: Measure, x: f32, y: f32) -> bool {
        if self.pending_delete.is_some() {
            return false;
        }
        let layout = self.layout(content, m);
        let target = self.hit(&layout, x, y);
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
        let layout = self.layout(content, m);
        if let Some(cl) = &layout.confirm {
            return match confirm_hit(cl, x, y) {
                ConfirmHit::Confirm => self.finish_delete(true),
                ConfirmHit::Cancel => self.finish_delete(false),
                ConfirmHit::Inside => None,
            };
        }
        let target = self.hit(&layout, x, y);
        match target {
            Some(KeysTarget::Import) => {
                self.open_draft(DraftMode::Import);
                None
            }
            Some(KeysTarget::Generate) => {
                self.open_draft(DraftMode::Generate);
                None
            }
            Some(KeysTarget::Copy(i)) => {
                let key = self.keys.get(i)?;
                self.notice = Some(format!("Copied the public key of {}", key.name));
                Some(SettingsAction::CopyPublicKey { id: key.id.clone() })
            }
            Some(KeysTarget::Delete(i)) => {
                let key = self.keys.get(i)?;
                self.pending_delete = Some(PendingDelete {
                    id: key.id.clone(),
                    dialog: Confirm::destructive(
                        format!("Delete {}?", key.name),
                        "Servers that sign in with this key will ask for another way in. \
                         You can't undo this.",
                        "Delete",
                    ),
                });
                None
            }
            Some(KeysTarget::Field(f)) => {
                if let Some(d) = &mut self.draft {
                    d.focus = f;
                }
                None
            }
            Some(KeysTarget::Submit) => self.draft.as_mut().and_then(KeyDraft::submit),
            Some(KeysTarget::Cancel) => {
                self.draft = None;
                None
            }
            None => None,
        }
    }

    fn finish_delete(&mut self, confirmed: bool) -> Option<SettingsAction> {
        let p = self.pending_delete.take()?;
        if confirmed {
            // "Copied the public key of …" may name the key going away.
            self.notice = None;
        }
        confirmed.then_some(SettingsAction::DeleteKey { id: p.id })
    }

    pub fn key(&mut self, key: Key) -> Option<SettingsAction> {
        if let Some(p) = &mut self.pending_delete {
            let k = match key {
                Key::Escape => DialogKey::Escape,
                Key::Enter => DialogKey::Enter,
                Key::Tab | Key::ShiftTab => DialogKey::Tab,
                _ => return None,
            };
            return match dialog_key(k, p.dialog.focus) {
                DialogOutcome::Cancel => self.finish_delete(false),
                DialogOutcome::Confirm => self.finish_delete(true),
                DialogOutcome::Focus(f) => {
                    p.dialog.focus = f;
                    None
                }
            };
        }
        let draft = self.draft.as_mut()?;
        match key {
            Key::Escape => {
                self.draft = None;
                None
            }
            Key::Tab => {
                draft.cycle(1);
                None
            }
            Key::ShiftTab => {
                draft.cycle(-1);
                None
            }
            Key::Enter if draft.focus == DraftField::Pem => {
                draft.error = None;
                let (d, max, nl) = draft.active();
                d.insert("\n", max, nl);
                None
            }
            Key::Enter => draft.submit(),
            other => {
                draft.error = None;
                edit_draft(draft.active().0, other);
                None
            }
        }
    }

    pub fn insert_text(&mut self, text: &str) -> bool {
        if self.pending_delete.is_some() {
            return false;
        }
        let Some(draft) = self.draft.as_mut() else {
            return false;
        };
        draft.error = None;
        let (d, max, nl) = draft.active();
        d.insert(text, max, nl)
    }

    /// Dialog focus, for the painter.
    pub fn delete_focus(&self) -> Option<DialogFocus> {
        self.pending_delete.as_ref().map(|p| p.dialog.focus)
    }
}

fn draft_layout(d: &KeyDraft, area: Rect, m: Measure) -> DraftLayout {
    let _ = m;
    let x = area.x + DRAFT_PAD;
    let w = (area.width - 2.0 * DRAFT_PAD).clamp(0.0, DRAFT_MAX_FIELD_WIDTH);
    let mut y = area.y + DRAFT_PAD;
    let title = Rect::new(x, y, area.width - 2.0 * DRAFT_PAD, DRAFT_TITLE_HEIGHT);
    y += DRAFT_TITLE_HEIGHT + DRAFT_FIELD_GAP;
    let mut fields = Vec::new();
    for f in d.fields() {
        let kind = match f {
            DraftField::Name => FieldKind::Text,
            DraftField::Pem => FieldKind::Textarea,
            DraftField::Passphrase => FieldKind::Text, // masked by the painter, no eye toggle
        };
        let l = field_layout((x, y), w, kind, true, false);
        y = l.total.bottom() + DRAFT_FIELD_GAP;
        fields.push((*f, l));
    }
    let error = d.error.as_ref().map(|_| {
        let r = Rect::new(
            x,
            y - DRAFT_FIELD_GAP + LABEL_GAP,
            w.max(300.0),
            HELPER_HEIGHT,
        );
        y = r.bottom() + DRAFT_FIELD_GAP;
        r
    });
    let bh = ButtonSize::Medium.height();
    let submit_w = button_spec(
        m,
        (0.0, 0.0),
        ButtonKind::Primary,
        ButtonSize::Medium,
        d.submit_label(),
    )
    .width();
    let cancel_w = button_spec(
        m,
        (0.0, 0.0),
        ButtonKind::Secondary,
        ButtonSize::Medium,
        "Cancel",
    )
    .width();
    let right = area.right() - DRAFT_PAD;
    let submit = Rect::new(right - submit_w, y, submit_w, bh);
    let cancel = Rect::new(submit.x - 10.0 - cancel_w, y, cancel_w, bh);
    let card = Rect::new(
        area.x,
        area.y,
        area.width,
        submit.bottom() + DRAFT_PAD - area.y,
    );
    DraftLayout {
        card,
        title,
        fields,
        error,
        cancel,
        submit,
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_measure;
    use super::*;

    const CONTENT: Rect = Rect::new(260.0, 96.0, 1180.0, 804.0);

    fn item(id: &str, name: &str) -> SshKeyItem {
        SshKeyItem {
            id: id.into(),
            name: name.into(),
            fingerprint: format!("SHA256:{id}"),
            created: "2026-01-02".into(),
            public_key: format!("ssh-ed25519 AAAA{id}"),
        }
    }

    fn state() -> KeysState {
        let mut s = KeysState::default();
        s.set_keys(vec![item("k1", "id_ed25519"), item("k2", "work")]);
        s
    }

    fn center(r: Rect) -> (f32, f32) {
        (r.x + r.width / 2.0, r.y + r.height / 2.0)
    }

    #[test]
    fn header_buttons_sit_right_aligned_inside_padding() {
        let s = state();
        let l = s.layout(CONTENT, &mut test_measure);
        assert!((l.generate.right() - (CONTENT.right() - 28.0)).abs() < 0.01);
        assert!(l.import.right() < l.generate.x);
        assert!(l.intro.right() < l.import.x);
        assert_eq!(l.rows.len(), 2);
        assert!(l.rows[1].card.rect.y > l.rows[0].card.rect.y);
    }

    #[test]
    fn generate_opens_draft_and_name_has_focus() {
        let mut s = state();
        let l = s.layout(CONTENT, &mut test_measure);
        let (x, y) = center(l.generate);
        assert_eq!(s.press(CONTENT, &mut test_measure, x, y), None);
        let d = s.draft.as_ref().unwrap();
        assert_eq!(d.mode, DraftMode::Generate);
        assert_eq!(d.focus, DraftField::Name);
        // List moves below the draft card.
        let l2 = s.layout(CONTENT, &mut test_measure);
        assert!(l2.rows[0].card.rect.y >= l2.draft.as_ref().unwrap().card.bottom());
    }

    #[test]
    fn empty_name_is_rejected_inline() {
        let mut s = state();
        s.open_draft(DraftMode::Generate);
        assert_eq!(s.key(Key::Enter), None);
        assert_eq!(
            s.draft.as_ref().unwrap().error.as_deref(),
            Some("Enter a name for the key")
        );
    }

    #[test]
    fn generate_submits_name_without_pem() {
        let mut s = state();
        s.open_draft(DraftMode::Generate);
        assert!(s.insert_text("laptop"));
        assert_eq!(
            s.key(Key::Enter),
            Some(SettingsAction::SubmitKey {
                name: "laptop".into(),
                pem: None,
                passphrase: None
            })
        );
    }

    #[test]
    fn import_requires_pem_and_carries_passphrase() {
        let mut s = state();
        s.open_draft(DraftMode::Import);
        s.insert_text("old");
        assert_eq!(s.key(Key::Enter), None);
        assert_eq!(
            s.draft.as_ref().unwrap().error.as_deref(),
            Some("Paste the private key to import")
        );
        s.key(Key::Tab);
        assert_eq!(s.draft.as_ref().unwrap().focus, DraftField::Pem);
        s.insert_text("-----BEGIN OPENSSH PRIVATE KEY-----\nabc");
        s.key(Key::Tab);
        s.insert_text("secret");
        let action = s.key(Key::Enter).unwrap();
        match action {
            SettingsAction::SubmitKey {
                name,
                pem,
                passphrase,
            } => {
                assert_eq!(name, "old");
                assert!(pem.unwrap().contains("abc"));
                assert_eq!(passphrase.as_deref(), Some("secret"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn tab_cycles_and_escape_closes() {
        let mut s = state();
        s.open_draft(DraftMode::Import);
        s.key(Key::ShiftTab);
        assert_eq!(s.draft.as_ref().unwrap().focus, DraftField::Passphrase);
        s.key(Key::Tab);
        assert_eq!(s.draft.as_ref().unwrap().focus, DraftField::Name);
        s.key(Key::Escape);
        assert!(s.draft.is_none());
    }

    #[test]
    fn copy_returns_the_key_id_and_sets_notice() {
        let mut s = state();
        let l = s.layout(CONTENT, &mut test_measure);
        let r = l.rows[1].card.actions[0].unwrap();
        let (x, y) = center(r);
        assert_eq!(
            s.press(CONTENT, &mut test_measure, x, y),
            Some(SettingsAction::CopyPublicKey { id: "k2".into() })
        );
        assert!(s.notice.as_deref().unwrap().contains("work"));
    }

    #[test]
    fn deleting_a_key_drops_the_copy_notice_about_it() {
        let mut s = state();
        let l = s.layout(CONTENT, &mut test_measure);
        let (x, y) = center(l.rows[1].card.actions[0].unwrap());
        s.press(CONTENT, &mut test_measure, x, y);
        assert!(s.notice.is_some());
        let (x, y) = center(l.rows[1].card.actions[1].unwrap());
        s.press(CONTENT, &mut test_measure, x, y);
        s.key(Key::Tab);
        assert_eq!(
            s.key(Key::Enter),
            Some(SettingsAction::DeleteKey { id: "k2".into() })
        );
        assert_eq!(s.notice, None);
    }

    #[test]
    fn delete_needs_confirmation_and_cancel_is_default() {
        let mut s = state();
        let l = s.layout(CONTENT, &mut test_measure);
        let (x, y) = center(l.rows[0].card.actions[1].unwrap());
        assert_eq!(s.press(CONTENT, &mut test_measure, x, y), None);
        assert!(s.delete_dialog().is_some());
        assert_eq!(s.delete_focus(), Some(DialogFocus::Cancel));
        // Enter on the default (Cancel) keeps the key.
        assert_eq!(s.key(Key::Enter), None);
        assert!(s.delete_dialog().is_none());

        // Open again, confirm with the button.
        s.press(CONTENT, &mut test_measure, x, y);
        let l = s.layout(CONTENT, &mut test_measure);
        let c = l.confirm.unwrap().dialog.confirm;
        let (cx, cy) = center(c);
        assert_eq!(
            s.press(CONTENT, &mut test_measure, cx, cy),
            Some(SettingsAction::DeleteKey { id: "k1".into() })
        );
        assert!(s.delete_dialog().is_none());
    }

    #[test]
    fn scrim_click_and_escape_dismiss_without_deleting() {
        let mut s = state();
        let l = s.layout(CONTENT, &mut test_measure);
        let (x, y) = center(l.rows[0].card.actions[1].unwrap());
        s.press(CONTENT, &mut test_measure, x, y);
        assert_eq!(
            s.press(CONTENT, &mut test_measure, CONTENT.x + 3.0, CONTENT.y + 3.0),
            None
        );
        assert!(s.delete_dialog().is_none());
        s.press(CONTENT, &mut test_measure, x, y);
        assert_eq!(s.key(Key::Escape), None);
        assert!(s.delete_dialog().is_none());
    }

    #[test]
    fn hover_reports_changes_only() {
        let mut s = state();
        let l = s.layout(CONTENT, &mut test_measure);
        let (x, y) = center(l.import);
        assert!(s.hover(CONTENT, &mut test_measure, x, y));
        assert!(!s.hover(CONTENT, &mut test_measure, x, y));
        assert_eq!(s.hover, Some(KeysTarget::Import));
    }

    #[test]
    fn worker_error_shows_under_draft_and_clears_on_typing() {
        let mut s = state();
        s.open_draft(DraftMode::Generate);
        s.set_draft_error("A key with that name exists");
        assert!(s
            .layout(CONTENT, &mut test_measure)
            .draft
            .unwrap()
            .error
            .is_some());
        s.insert_text("x");
        assert!(s.draft.as_ref().unwrap().error.is_none());
    }

    #[test]
    fn keys_removed_by_the_worker_drop_a_stale_confirm() {
        let mut s = state();
        let l = s.layout(CONTENT, &mut test_measure);
        let (x, y) = center(l.rows[0].card.actions[1].unwrap());
        s.press(CONTENT, &mut test_measure, x, y);
        s.set_keys(vec![item("k2", "work")]);
        assert!(s.delete_dialog().is_none());
    }
}
