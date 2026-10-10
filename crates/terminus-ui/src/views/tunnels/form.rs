use super::model::{parse_port, route_text, valid_host, TunnelItem, TunnelKind};
use crate::components::input::{TextDraft, TextEdit};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormField {
    Name,
    LocalPort,
    DestHost,
    DestPort,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormKey {
    Char(char),
    /// Shared caret / selection / delete editing ([`TextDraft::apply`]).
    Edit(TextEdit),
    Tab,
    BackTab,
    Enter,
    Escape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormOutcome {
    None,
    Submit,
    Cancel,
}

/// A validated tunnel, ready to persist.
#[derive(Debug, Clone, PartialEq)]
pub struct TunnelDraft {
    /// `Some` when editing an existing tunnel.
    pub id: Option<String>,
    pub kind: TunnelKind,
    pub name: String,
    pub bind_host: String,
    pub bind_port: u16,
    /// Empty for Dynamic.
    pub dest_host: String,
    /// 0 for Dynamic.
    pub dest_port: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TunnelForm {
    pub id: Option<String>,
    pub kind: TunnelKind,
    pub name: TextDraft,
    pub local_port: TextDraft,
    pub dest_host: TextDraft,
    pub dest_port: TextDraft,
    pub focus: FormField,
    pub errors: Vec<(FormField, String)>,
    /// Machine name shown in "Destination on <host>".
    pub host_label: String,
}

impl TunnelForm {
    pub fn new(host_label: &str) -> Self {
        Self {
            id: None,
            kind: TunnelKind::Local,
            name: TextDraft::default(),
            local_port: TextDraft::default(),
            dest_host: TextDraft::new("localhost"),
            dest_port: TextDraft::default(),
            focus: FormField::Name,
            errors: Vec::new(),
            host_label: host_label.into(),
        }
    }

    pub fn editing(item: &TunnelItem, host_label: &str) -> Self {
        Self {
            id: Some(item.id.clone()),
            kind: item.kind,
            name: TextDraft::new(item.name.clone()),
            local_port: TextDraft::new(item.bind_port.to_string()),
            dest_host: TextDraft::new(item.dest_host.clone()),
            dest_port: TextDraft::new(if item.dest_port == 0 {
                String::new()
            } else {
                item.dest_port.to_string()
            }),
            focus: FormField::Name,
            errors: Vec::new(),
            host_label: host_label.into(),
        }
    }

    pub fn title(&self) -> &'static str {
        if self.id.is_some() {
            "Edit tunnel"
        } else {
            "New tunnel"
        }
    }

    /// Label of the first port field.
    pub fn local_label(&self) -> &'static str {
        match self.kind {
            TunnelKind::Remote => "Remote port",
            _ => "Local port",
        }
    }

    /// Label of the destination field.
    pub fn dest_label(&self) -> String {
        match self.kind {
            TunnelKind::Remote => "Destination on this computer".into(),
            _ => format!("Destination on {}", self.host_label),
        }
    }

    /// Fields shown for the current kind, in tab order.
    pub fn fields(&self) -> Vec<FormField> {
        match self.kind {
            TunnelKind::Dynamic => vec![FormField::Name, FormField::LocalPort],
            _ => vec![
                FormField::Name,
                FormField::LocalPort,
                FormField::DestHost,
                FormField::DestPort,
            ],
        }
    }

    pub fn set_kind(&mut self, kind: TunnelKind) {
        self.kind = kind;
        self.errors.clear();
        if !self.fields().contains(&self.focus) {
            self.focus = FormField::Name;
        }
    }

    pub fn value(&self, field: FormField) -> &str {
        &self.draft(field).value
    }

    pub fn draft(&self, field: FormField) -> &TextDraft {
        match field {
            FormField::Name => &self.name,
            FormField::LocalPort => &self.local_port,
            FormField::DestHost => &self.dest_host,
            FormField::DestPort => &self.dest_port,
        }
    }

    fn draft_mut(&mut self, field: FormField) -> &mut TextDraft {
        match field {
            FormField::Name => &mut self.name,
            FormField::LocalPort => &mut self.local_port,
            FormField::DestHost => &mut self.dest_host,
            FormField::DestPort => &mut self.dest_port,
        }
    }

    pub fn error_for(&self, field: FormField) -> Option<&str> {
        self.errors
            .iter()
            .find(|(f, _)| *f == field)
            .map(|(_, m)| m.as_str())
    }

    /// First error in tab order, for the single error line.
    pub fn first_error(&self) -> Option<&str> {
        self.fields().into_iter().find_map(|f| self.error_for(f))
    }

    /// Insert typed or pasted text into the focused field, keeping only
    /// what the field accepts (digits for ports, no spaces for hosts).
    pub fn insert_text(&mut self, text: &str) {
        let field = self.focus;
        let (keep, max): (fn(char) -> bool, usize) = match field {
            FormField::LocalPort | FormField::DestPort => (|c| c.is_ascii_digit(), 5),
            FormField::DestHost => (|c| !c.is_whitespace() && !c.is_control(), 253),
            FormField::Name => (|c| !c.is_control(), 60),
        };
        let draft = self.draft_mut(field);
        let room = max.saturating_sub(draft.value.chars().count());
        let text: String = text.chars().filter(|c| keep(*c)).take(room).collect();
        if draft.insert(&text, usize::MAX, false) {
            self.errors.retain(|(f, _)| *f != field);
        }
    }

    pub fn type_char(&mut self, c: char) {
        self.insert_text(c.encode_utf8(&mut [0; 4]));
    }

    pub fn edit(&mut self, edit: TextEdit) {
        let field = self.focus;
        if self.draft_mut(field).apply(edit) {
            self.errors.retain(|(f, _)| *f != field);
        }
    }

    pub fn backspace(&mut self) {
        self.edit(TextEdit::Backspace { by_word: false });
    }

    fn move_focus(&mut self, delta: i32) {
        let fields = self.fields();
        let at = fields.iter().position(|f| *f == self.focus).unwrap_or(0) as i32;
        let n = fields.len() as i32;
        self.focus = fields[(at + delta).rem_euclid(n) as usize];
    }

    pub fn key(&mut self, key: FormKey) -> FormOutcome {
        match key {
            FormKey::Char(c) => self.type_char(c),
            FormKey::Edit(e) => self.edit(e),
            FormKey::Tab => self.move_focus(1),
            FormKey::BackTab => self.move_focus(-1),
            FormKey::Enter => return FormOutcome::Submit,
            FormKey::Escape => return FormOutcome::Cancel,
        }
        FormOutcome::None
    }

    /// Validate every visible field. `port_free(p)` says whether local port
    /// `p` can be bound (only asked for Local and Dynamic tunnels). On
    /// failure `errors` names each bad field.
    #[allow(clippy::result_unit_err)]
    pub fn validate(
        &mut self,
        port_free: &dyn Fn(u16) -> bool,
    ) -> Result<TunnelDraft, ()> {
        self.errors.clear();
        let bind = parse_port(&self.local_port.value);
        match &bind {
            Err(e) => self.errors.push((FormField::LocalPort, e.clone())),
            Ok(p) if self.kind != TunnelKind::Remote && !port_free(*p) => {
                self.errors
                    .push((FormField::LocalPort, format!("Port {p} is already in use")));
            }
            Ok(_) => {}
        }
        let (mut dest_host, mut dest_port) = (String::new(), 0u16);
        if self.kind != TunnelKind::Dynamic {
            let host = self.dest_host.value.trim().to_string();
            if valid_host(&host) {
                dest_host = host;
            } else {
                self.errors.push((
                    FormField::DestHost,
                    "Enter a host name or address without spaces".into(),
                ));
            }
            match parse_port(&self.dest_port.value) {
                Ok(p) => dest_port = p,
                Err(e) => self.errors.push((FormField::DestPort, e)),
            }
        }
        let bind_port = match (bind, self.errors.is_empty()) {
            (Ok(p), true) => p,
            _ => return Err(()),
        };
        let typed = self.name.value.trim();
        let name = if typed.is_empty() {
            route_text(self.kind, bind_port, &dest_host, dest_port)
        } else {
            typed.to_string()
        };
        Ok(TunnelDraft {
            id: self.id.clone(),
            kind: self.kind,
            name,
            bind_host: "127.0.0.1".into(),
            bind_port,
            dest_host,
            dest_port,
        })
    }
}
