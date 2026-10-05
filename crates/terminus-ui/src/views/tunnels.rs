//! Tunnels view: state, validation, geometry and hit-testing.
//!
//! Pure logic in logical pixels, no GPU and no I/O. Everything is computed
//! from a `content: Rect` handed in by the shell (the area under the machine
//! header), so the view never assumes where the header or sidebar are. The
//! painter (`frontends/rioterm/src/renderer/views/tunnels.rs`) walks the same
//! `*_layout` functions the pointer hit-tests, so a control cannot be drawn
//! where it cannot be clicked.
//!
//! Widths that depend on measured text (buttons, the segmented control) come
//! from [`Metrics`]; the painter measures once per frame and stores them in
//! [`TunnelsState::metrics`], the defaults are only good guesses for the first
//! frame and for tests.

use std::cell::Cell;

use crate::components::input::{
    field_layout, FieldKind, FieldLayout, FIELD_HEIGHT, HELPER_HEIGHT, LABEL_GAP,
    LABEL_HEIGHT,
};
use crate::components::list::CARD_HEIGHT;
use crate::components::list::{card_hit, card_layout, CardHit, CardLayout, CardSpec};
use crate::components::selection::{SegmentedLayout, SegmentedSize};
use crate::geom::Rect;
use crate::tokens::height;

/// Padding around the whole view.
pub const PAD: f32 = 28.0;
/// Vertical gap between cards.
pub const CARD_GAP: f32 = 10.0;
/// Header row (description + "New tunnel").
pub const HEADER_HEIGHT: f32 = height::CONTROL_MD;
/// Gap between the header row and the first card.
pub const HEADER_GAP: f32 = 16.0;
/// Height of the empty-state block.
pub const EMPTY_HEIGHT: f32 = 120.0;

pub const DIALOG_WIDTH: f32 = 520.0;
pub const DIALOG_PAD: f32 = 28.0;
pub const DIALOG_GAP: f32 = 18.0;
pub const TITLE_LINE: f32 = 30.0;
pub const BUTTON_HEIGHT: f32 = height::CONTROL_LG;
pub const BUTTON_GAP: f32 = 10.0;
const ARROW_WIDTH: f32 = 16.0;
const ROW_GAP: f32 = 12.0;
const LOCAL_WIDTH: f32 = 120.0;
const PORT_WIDTH: f32 = 100.0;
const ERROR_LINE: f32 = HELPER_HEIGHT;

// ---------------------------------------------------------------- model

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelKind {
    Local,
    Remote,
    Dynamic,
}

impl TunnelKind {
    pub const ALL: [TunnelKind; 3] =
        [TunnelKind::Local, TunnelKind::Remote, TunnelKind::Dynamic];

    pub fn label(self) -> &'static str {
        match self {
            TunnelKind::Local => "Local",
            TunnelKind::Remote => "Remote",
            TunnelKind::Dynamic => "Dynamic",
        }
    }

    /// Value stored in `PortForward::kind`.
    pub fn as_str(self) -> &'static str {
        match self {
            TunnelKind::Local => "local",
            TunnelKind::Remote => "remote",
            TunnelKind::Dynamic => "dynamic",
        }
    }

    /// Unknown stored values read as `Local`.
    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "remote" => TunnelKind::Remote,
            "dynamic" => TunnelKind::Dynamic,
            _ => TunnelKind::Local,
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TunnelStatus {
    Stopped,
    /// Process spawned, not yet known to be healthy.
    Starting,
    Running,
    /// Exited on its own; see [`TunnelItem::error`].
    Failed,
}

impl TunnelStatus {
    /// A process exists (Stop is offered instead of Start).
    pub fn is_active(self) -> bool {
        matches!(self, TunnelStatus::Starting | TunnelStatus::Running)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TunnelItem {
    pub id: String,
    pub name: String,
    pub kind: TunnelKind,
    pub bind_host: String,
    /// Listening port: local for Local/Dynamic, remote for Remote.
    pub bind_port: u16,
    pub dest_host: String,
    pub dest_port: u16,
    pub status: TunnelStatus,
    pub error: Option<String>,
}

impl TunnelItem {
    /// Mono route line of the card.
    pub fn route(&self) -> String {
        route_text(self.kind, self.bind_port, &self.dest_host, self.dest_port)
    }
}

fn route_text(kind: TunnelKind, bind: u16, dest_host: &str, dest_port: u16) -> String {
    match kind {
        TunnelKind::Local => format!("localhost:{bind} \u{2192} {dest_host}:{dest_port}"),
        TunnelKind::Remote => format!("remote:{bind} \u{2192} {dest_host}:{dest_port}"),
        TunnelKind::Dynamic => format!("localhost:{bind} \u{2192} SOCKS5"),
    }
}

/// Number of running tunnels: the header badge of the Tunnels tab.
pub fn running_count(items: &[TunnelItem]) -> usize {
    items
        .iter()
        .filter(|t| t.status == TunnelStatus::Running)
        .count()
}

/// Parse a port field: 1-65535.
pub fn parse_port(s: &str) -> Result<u16, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("Enter a port".into());
    }
    match s.parse::<u32>() {
        Ok(p) if (1..=65535).contains(&p) => Ok(p as u16),
        Ok(_) => Err("Port must be 1\u{2013}65535".into()),
        Err(_) => Err("Port must be a number".into()),
    }
}

fn valid_host(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 253
        && s.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '[' | ']')
        })
}

// ---------------------------------------------------------------- form

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
    Backspace,
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
    pub name: String,
    pub local_port: String,
    pub dest_host: String,
    pub dest_port: String,
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
            name: String::new(),
            local_port: String::new(),
            dest_host: "localhost".into(),
            dest_port: String::new(),
            focus: FormField::Name,
            errors: Vec::new(),
            host_label: host_label.into(),
        }
    }

    pub fn editing(item: &TunnelItem, host_label: &str) -> Self {
        Self {
            id: Some(item.id.clone()),
            kind: item.kind,
            name: item.name.clone(),
            local_port: item.bind_port.to_string(),
            dest_host: item.dest_host.clone(),
            dest_port: if item.dest_port == 0 {
                String::new()
            } else {
                item.dest_port.to_string()
            },
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
        match field {
            FormField::Name => &self.name,
            FormField::LocalPort => &self.local_port,
            FormField::DestHost => &self.dest_host,
            FormField::DestPort => &self.dest_port,
        }
    }

    fn value_mut(&mut self, field: FormField) -> &mut String {
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

    pub fn type_char(&mut self, c: char) {
        let field = self.focus;
        let accept = match field {
            FormField::LocalPort | FormField::DestPort => {
                c.is_ascii_digit() && self.value(field).len() < 5
            }
            FormField::DestHost => {
                !c.is_whitespace() && !c.is_control() && self.value(field).len() < 253
            }
            FormField::Name => !c.is_control() && self.value(field).chars().count() < 60,
        };
        if accept {
            self.value_mut(field).push(c);
            self.errors.retain(|(f, _)| *f != field);
        }
    }

    pub fn backspace(&mut self) {
        let field = self.focus;
        self.value_mut(field).pop();
        self.errors.retain(|(f, _)| *f != field);
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
            FormKey::Backspace => self.backspace(),
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
    pub fn validate(
        &mut self,
        port_free: &dyn Fn(u16) -> bool,
    ) -> Result<TunnelDraft, ()> {
        self.errors.clear();
        let bind = parse_port(&self.local_port);
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
            let host = self.dest_host.trim().to_string();
            if valid_host(&host) {
                dest_host = host;
            } else {
                self.errors.push((
                    FormField::DestHost,
                    "Enter a host name or address without spaces".into(),
                ));
            }
            match parse_port(&self.dest_port) {
                Ok(p) => dest_port = p,
                Err(e) => self.errors.push((FormField::DestPort, e)),
            }
        }
        let bind_port = match (bind, self.errors.is_empty()) {
            (Ok(p), true) => p,
            _ => return Err(()),
        };
        let typed = self.name.trim();
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

// ---------------------------------------------------------------- metrics

/// Painter-measured widths (logical px), cached between frames.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub new_button: f32,
    pub start: f32,
    pub stop: f32,
    pub trash: f32,
    pub meta: f32,
    /// Measured label width of Local / Remote / Dynamic.
    pub kind_text: [f32; 3],
    pub cancel: f32,
    pub confirm: f32,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            new_button: 112.0,
            start: 66.0,
            stop: 64.0,
            trash: height::CONTROL_MD,
            meta: 48.0,
            kind_text: [36.0, 50.0, 52.0],
            cancel: 80.0,
            confirm: 118.0,
        }
    }
}

// ---------------------------------------------------------------- list geometry

#[derive(Debug, Clone, PartialEq)]
pub struct ListLayout {
    pub description: Rect,
    pub new_button: Rect,
    pub cards: Vec<Rect>,
    /// Shown instead of cards when there are none.
    pub empty: Option<Rect>,
}

pub fn list_layout(content: Rect, count: usize, m: &Metrics) -> ListLayout {
    let x = content.x + PAD;
    let w = (content.width - 2.0 * PAD).max(0.0);
    let y = content.y + PAD;
    let new_button = Rect::new(
        content.right() - PAD - m.new_button,
        y,
        m.new_button,
        HEADER_HEIGHT,
    );
    let description =
        Rect::new(x, y, (new_button.x - x - ROW_GAP).max(0.0), HEADER_HEIGHT);
    let mut cy = y + HEADER_HEIGHT + HEADER_GAP;
    let cards: Vec<Rect> = (0..count)
        .map(|_| {
            let r = Rect::new(x, cy, w, CARD_HEIGHT);
            cy += CARD_HEIGHT + CARD_GAP;
            r
        })
        .collect();
    let empty = (count == 0).then(|| Rect::new(x, cy, w, EMPTY_HEIGHT));
    ListLayout {
        description,
        new_button,
        cards,
        empty,
    }
}

/// Widths of a card's two action slots: Start/Stop then delete.
pub fn card_action_widths(m: &Metrics, status: TunnelStatus) -> [f32; 2] {
    [if status.is_active() { m.stop } else { m.start }, m.trash]
}

pub fn card_spec_widths<'a>(m: &Metrics, widths: &'a [f32; 2]) -> CardSpec<'a> {
    CardSpec {
        has_dot: true,
        meta_width: m.meta,
        action_widths: widths,
    }
}

/// Layout of one card (dot, text, kind meta, [Start/Stop, delete]).
pub fn card_geometry(rect: Rect, m: &Metrics, status: TunnelStatus) -> CardLayout {
    let widths = card_action_widths(m, status);
    card_layout(rect, &card_spec_widths(m, &widths))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListHit {
    New,
    Toggle(usize),
    Delete(usize),
    /// Anywhere else on a card: edit it.
    Card(usize),
}

pub fn list_hit(
    content: Rect,
    items: &[TunnelItem],
    m: &Metrics,
    x: f32,
    y: f32,
) -> Option<ListHit> {
    let l = list_layout(content, items.len(), m);
    if l.new_button.contains(x, y) {
        return Some(ListHit::New);
    }
    for (i, (rect, item)) in l.cards.iter().zip(items).enumerate() {
        let widths = card_action_widths(m, item.status);
        match card_hit(*rect, &card_spec_widths(m, &widths), x, y) {
            Some(CardHit::Action(0)) => return Some(ListHit::Toggle(i)),
            Some(CardHit::Action(_)) => return Some(ListHit::Delete(i)),
            Some(CardHit::Body) => return Some(ListHit::Card(i)),
            None => {}
        }
    }
    None
}

// ---------------------------------------------------------------- dialog geometry

#[derive(Debug, Clone, PartialEq)]
pub struct DialogLayout {
    pub dialog: Rect,
    pub title: Rect,
    pub segments: SegmentedLayout,
    pub name: FieldLayout,
    pub local: FieldLayout,
    pub arrow: Option<Rect>,
    pub dest: Option<FieldLayout>,
    pub port: Option<FieldLayout>,
    pub error_line: Option<Rect>,
    pub cancel: Rect,
    pub confirm: Rect,
}

pub fn dialog_layout(content: Rect, form: &TunnelForm, m: &Metrics) -> DialogLayout {
    let inner = DIALOG_WIDTH - 2.0 * DIALOG_PAD;
    let field_block = LABEL_HEIGHT + LABEL_GAP + FIELD_HEIGHT;
    let seg_h = SegmentedSize::Medium.segment_height() + 6.0;
    let error_h = if form.first_error().is_some() {
        DIALOG_GAP + ERROR_LINE
    } else {
        0.0
    };
    let h = DIALOG_PAD
        + TITLE_LINE
        + DIALOG_GAP
        + seg_h
        + DIALOG_GAP
        + field_block
        + DIALOG_GAP
        + field_block
        + error_h
        + DIALOG_GAP
        + 6.0
        + BUTTON_HEIGHT
        + DIALOG_PAD;
    let dx = content.x + ((content.width - DIALOG_WIDTH) / 2.0).max(0.0);
    let dy = content.y + ((content.height - h) / 2.0).max(0.0);
    let dialog = Rect::new(dx, dy, DIALOG_WIDTH, h);
    let (ix, mut y) = (dx + DIALOG_PAD, dy + DIALOG_PAD);
    let title = Rect::new(ix, y, inner, TITLE_LINE);
    y += TITLE_LINE + DIALOG_GAP;
    let segments = SegmentedLayout::new(ix, y, &m.kind_text, SegmentedSize::Medium);
    y += seg_h + DIALOG_GAP;
    let name = field_layout((ix, y), inner, FieldKind::Text, true, false);
    y += field_block + DIALOG_GAP;
    let local = field_layout((ix, y), LOCAL_WIDTH, FieldKind::Mono, true, false);
    let (mut arrow, mut dest, mut port) = (None, None, None);
    if form.kind != TunnelKind::Dynamic {
        let ax = ix + LOCAL_WIDTH + ROW_GAP;
        arrow = Some(Rect::new(
            ax,
            y + LABEL_HEIGHT + LABEL_GAP,
            ARROW_WIDTH,
            FIELD_HEIGHT,
        ));
        let dx0 = ax + ARROW_WIDTH + ROW_GAP;
        let dest_w = inner - LOCAL_WIDTH - PORT_WIDTH - ARROW_WIDTH - 3.0 * ROW_GAP;
        dest = Some(field_layout((dx0, y), dest_w, FieldKind::Mono, true, false));
        port = Some(field_layout(
            (dx0 + dest_w + ROW_GAP, y),
            PORT_WIDTH,
            FieldKind::Mono,
            true,
            false,
        ));
    }
    y += field_block;
    let error_line = (error_h > 0.0).then(|| {
        let r = Rect::new(ix, y + DIALOG_GAP, inner, ERROR_LINE);
        y += error_h;
        r
    });
    y += DIALOG_GAP + 6.0;
    let confirm = Rect::new(
        dialog.right() - DIALOG_PAD - m.confirm,
        y,
        m.confirm,
        BUTTON_HEIGHT,
    );
    let cancel = Rect::new(
        confirm.x - BUTTON_GAP - m.cancel,
        y,
        m.cancel,
        BUTTON_HEIGHT,
    );
    DialogLayout {
        dialog,
        title,
        segments,
        name,
        local,
        arrow,
        dest,
        port,
        error_line,
        cancel,
        confirm,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogHit {
    Kind(usize),
    Field(FormField),
    Cancel,
    Confirm,
    /// Inside the dialog, on nothing interactive.
    Inside,
    /// On the scrim.
    Outside,
}

pub fn dialog_hit(l: &DialogLayout, form: &TunnelForm, x: f32, y: f32) -> DialogHit {
    if !l.dialog.contains(x, y) {
        return DialogHit::Outside;
    }
    if let Some(i) = l.segments.hit_test(x, y) {
        return DialogHit::Kind(i);
    }
    if l.name.box_rect.contains(x, y) || l.name.label.is_some_and(|r| r.contains(x, y)) {
        return DialogHit::Field(FormField::Name);
    }
    let fields = form.fields();
    let pairs = [
        (Some(l.local), FormField::LocalPort),
        (l.dest, FormField::DestHost),
        (l.port, FormField::DestPort),
    ];
    for (layout, field) in pairs {
        if let Some(fl) = layout {
            if fields.contains(&field) && fl.box_rect.contains(x, y) {
                return DialogHit::Field(field);
            }
        }
    }
    if l.cancel.contains(x, y) {
        return DialogHit::Cancel;
    }
    if l.confirm.contains(x, y) {
        return DialogHit::Confirm;
    }
    DialogHit::Inside
}

// ---------------------------------------------------------------- view state

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;

    fn content() -> Rect {
        Rect::new(260.0, 96.0, 1180.0, 804.0)
    }

    fn item(id: &str, kind: TunnelKind, status: TunnelStatus) -> TunnelItem {
        TunnelItem {
            id: id.into(),
            name: format!("tunnel {id}"),
            kind,
            bind_host: "127.0.0.1".into(),
            bind_port: 5432,
            dest_host: "localhost".into(),
            dest_port: 5432,
            status,
            error: None,
        }
    }

    fn free(_: u16) -> bool {
        true
    }

    #[test]
    fn list_and_dialog_controls_show_a_hand_fields_an_i_beam() {
        use crate::chrome::ChromeCursor;
        let mut s = TunnelsState::new("jerem prod");
        s.items = vec![item("a", TunnelKind::Local, TunnelStatus::Running)];
        let m = s.metrics.get();
        let mid = |r: Rect| (r.x + r.width / 2.0, r.y + r.height / 2.0);
        let l = list_layout(content(), 1, &m);
        let (x, y) = mid(l.new_button);
        assert_eq!(s.cursor_at(content(), x, y), ChromeCursor::Pointer);
        let (x, y) = mid(l.cards[0]);
        assert_eq!(s.cursor_at(content(), x, y), ChromeCursor::Pointer);
        assert_eq!(
            s.cursor_at(content(), content().x + 2.0, content().bottom() - 2.0),
            ChromeCursor::Default
        );

        s.open_new();
        let form = s.form.clone().unwrap();
        let d = dialog_layout(content(), &form, &m);
        let (x, y) = mid(d.confirm);
        assert_eq!(s.cursor_at(content(), x, y), ChromeCursor::Pointer);
        let (x, y) = mid(d.cancel);
        assert_eq!(s.cursor_at(content(), x, y), ChromeCursor::Pointer);
        let (x, y) = mid(d.local.box_rect);
        assert_eq!(s.cursor_at(content(), x, y), ChromeCursor::Text);
        let (x, y) = mid(d.title);
        assert_eq!(s.cursor_at(content(), x, y), ChromeCursor::Default);
    }

    #[test]
    fn ports_are_1_to_65535() {
        assert_eq!(parse_port("5432"), Ok(5432));
        assert_eq!(parse_port(" 80 "), Ok(80));
        assert!(parse_port("").is_err());
        assert!(parse_port("0").is_err());
        assert!(parse_port("65536").is_err());
        assert!(parse_port("12a").is_err());
        assert_eq!(parse_port("65535"), Ok(65535));
    }

    #[test]
    fn routes_read_left_to_right_per_kind() {
        let mut t = item("a", TunnelKind::Local, TunnelStatus::Stopped);
        t.dest_host = "db.internal".into();
        t.dest_port = 5433;
        assert_eq!(t.route(), "localhost:5432 \u{2192} db.internal:5433");
        t.kind = TunnelKind::Remote;
        assert_eq!(t.route(), "remote:5432 \u{2192} db.internal:5433");
        t.kind = TunnelKind::Dynamic;
        assert_eq!(t.route(), "localhost:5432 \u{2192} SOCKS5");
    }

    #[test]
    fn kind_round_trips_through_its_stored_name() {
        for k in TunnelKind::ALL {
            assert_eq!(TunnelKind::parse(k.as_str()), k);
        }
        assert_eq!(TunnelKind::parse("???"), TunnelKind::Local);
    }

    #[test]
    fn the_badge_counts_running_tunnels_only() {
        let items = vec![
            item("a", TunnelKind::Local, TunnelStatus::Running),
            item("b", TunnelKind::Local, TunnelStatus::Stopped),
            item("c", TunnelKind::Remote, TunnelStatus::Starting),
            item("d", TunnelKind::Local, TunnelStatus::Running),
            item("e", TunnelKind::Local, TunnelStatus::Failed),
        ];
        assert_eq!(running_count(&items), 2);
        assert_eq!(running_count(&[]), 0);
    }

    #[test]
    fn a_valid_local_form_yields_a_draft() {
        let mut f = TunnelForm::new("jerem prod");
        f.name = "Database".into();
        f.local_port = "5432".into();
        f.dest_host = "localhost".into();
        f.dest_port = "5432".into();
        let d = f.validate(&free).expect("valid");
        assert_eq!(d.kind, TunnelKind::Local);
        assert_eq!(d.name, "Database");
        assert_eq!((d.bind_port, d.dest_port), (5432, 5432));
        assert_eq!(d.bind_host, "127.0.0.1");
        assert!(d.id.is_none());
    }

    #[test]
    fn an_empty_name_falls_back_to_the_route() {
        let mut f = TunnelForm::new("h");
        f.local_port = "8080".into();
        f.dest_host = "localhost".into();
        f.dest_port = "80".into();
        assert_eq!(
            f.validate(&free).unwrap().name,
            "localhost:8080 \u{2192} localhost:80"
        );
    }

    #[test]
    fn bad_ports_and_hosts_flag_their_fields() {
        let mut f = TunnelForm::new("h");
        f.local_port = "0".into();
        f.dest_host = "bad host".into();
        f.dest_port = "70000".into();
        assert!(f.validate(&free).is_err());
        assert!(f.error_for(FormField::LocalPort).is_some());
        assert!(f.error_for(FormField::DestHost).is_some());
        assert!(f.error_for(FormField::DestPort).is_some());
        assert!(f.error_for(FormField::Name).is_none());
    }

    #[test]
    fn a_busy_local_port_is_refused_but_not_for_remote_forwards() {
        let busy = |p: u16| p != 5432;
        let mut f = TunnelForm::new("h");
        f.local_port = "5432".into();
        f.dest_host = "localhost".into();
        f.dest_port = "5432".into();
        assert!(f.validate(&busy).is_err());
        assert!(f
            .error_for(FormField::LocalPort)
            .unwrap()
            .contains("in use"));
        f.set_kind(TunnelKind::Remote);
        assert!(
            f.validate(&busy).is_ok(),
            "the remote port cannot be probed locally"
        );
    }

    #[test]
    fn dynamic_needs_only_a_local_port() {
        let mut f = TunnelForm::new("h");
        f.set_kind(TunnelKind::Dynamic);
        assert_eq!(f.fields(), vec![FormField::Name, FormField::LocalPort]);
        f.local_port = "1080".into();
        let d = f.validate(&free).expect("valid without a destination");
        assert_eq!(d.kind, TunnelKind::Dynamic);
        assert_eq!(d.dest_host, "");
        assert_eq!(d.dest_port, 0);
    }

    #[test]
    fn editing_keeps_the_id_and_fields() {
        let mut t = item("abc", TunnelKind::Remote, TunnelStatus::Stopped);
        t.dest_host = "10.0.0.5".into();
        let mut f = TunnelForm::editing(&t, "h");
        assert_eq!(f.kind, TunnelKind::Remote);
        assert_eq!(f.dest_host, "10.0.0.5");
        let d = f.validate(&free).unwrap();
        assert_eq!(d.id.as_deref(), Some("abc"));
    }

    #[test]
    fn typing_is_filtered_per_field() {
        let mut f = TunnelForm::new("h");
        f.focus = FormField::LocalPort;
        for c in "12ab345678".chars() {
            f.type_char(c);
        }
        assert_eq!(f.local_port, "12345", "digits only, 5 max");
        f.focus = FormField::DestHost;
        f.type_char(' ');
        f.type_char('a');
        assert_eq!(f.dest_host, "localhosta");
        f.focus = FormField::Name;
        f.type_char('x');
        f.backspace();
        f.backspace();
        assert_eq!(f.name, "");
    }

    #[test]
    fn tab_cycles_the_visible_fields() {
        let mut f = TunnelForm::new("h");
        assert_eq!(f.focus, FormField::Name);
        f.key(FormKey::Tab);
        assert_eq!(f.focus, FormField::LocalPort);
        f.key(FormKey::Tab);
        f.key(FormKey::Tab);
        f.key(FormKey::Tab);
        assert_eq!(f.focus, FormField::Name, "wraps");
        f.key(FormKey::BackTab);
        assert_eq!(f.focus, FormField::DestPort);
        f.set_kind(TunnelKind::Dynamic);
        assert_eq!(f.focus, FormField::Name, "a hidden field loses focus");
    }

    #[test]
    fn enter_submits_and_escape_cancels() {
        let mut f = TunnelForm::new("h");
        assert_eq!(f.key(FormKey::Enter), FormOutcome::Submit);
        assert_eq!(f.key(FormKey::Escape), FormOutcome::Cancel);
        assert_eq!(f.key(FormKey::Char('a')), FormOutcome::None);
    }

    #[test]
    fn the_description_and_new_button_share_the_header_row() {
        let l = list_layout(content(), 0, &Metrics::default());
        assert!(l.new_button.right() <= content().right() - PAD + 0.01);
        assert!(l.description.right() <= l.new_button.x);
        assert!((l.new_button.y - l.description.y).abs() < 20.0);
        assert!(l.cards.is_empty());
        assert!(l.empty.is_some(), "empty state");
    }

    #[test]
    fn cards_stack_without_overlap_inside_the_content() {
        let l = list_layout(content(), 3, &Metrics::default());
        assert_eq!(l.cards.len(), 3);
        assert!(l.empty.is_none());
        for w in l.cards.windows(2) {
            assert!((w[1].y - w[0].bottom() - CARD_GAP).abs() < 0.01);
        }
        for c in &l.cards {
            assert!(c.x >= content().x && c.right() <= content().right());
        }
        let mut all: Vec<(&'static str, Rect)> =
            l.cards.iter().map(|c| ("card", *c)).collect();
        all.push(("new", l.new_button));
        crate::overlap::assert_no_overlaps(&all, "tunnels list");
    }

    #[test]
    fn hit_testing_finds_new_toggle_delete_and_card() {
        let items = vec![
            item("a", TunnelKind::Local, TunnelStatus::Running),
            item("b", TunnelKind::Local, TunnelStatus::Stopped),
        ];
        let m = Metrics::default();
        let l = list_layout(content(), 2, &m);
        let c = l.new_button;
        assert_eq!(
            list_hit(content(), &items, &m, c.x + 2.0, c.y + 2.0),
            Some(ListHit::New)
        );
        let cl = card_geometry(l.cards[1], &m, items[1].status);
        let a = cl.actions[0].unwrap();
        assert_eq!(
            list_hit(content(), &items, &m, a.x + 2.0, a.y + 2.0),
            Some(ListHit::Toggle(1))
        );
        let t = cl.actions[1].unwrap();
        assert_eq!(
            list_hit(content(), &items, &m, t.x + 2.0, t.y + 2.0),
            Some(ListHit::Delete(1))
        );
        let body = l.cards[0];
        assert_eq!(
            list_hit(content(), &items, &m, body.x + 30.0, body.y + 5.0),
            Some(ListHit::Card(0))
        );
        assert_eq!(list_hit(content(), &items, &m, 1.0, 1.0), None);
    }

    #[test]
    fn the_dialog_sits_inside_the_content_and_dynamic_hides_the_destination() {
        let mut f = TunnelForm::new("h");
        let d = dialog_layout(content(), &f, &Metrics::default());
        assert!(d.dialog.x >= content().x && d.dialog.right() <= content().right());
        assert!(d.dialog.y >= content().y && d.dialog.bottom() <= content().bottom());
        assert!(d.dest.is_some() && d.port.is_some());
        f.set_kind(TunnelKind::Dynamic);
        let d = dialog_layout(content(), &f, &Metrics::default());
        assert!(d.dest.is_none() && d.port.is_none());
        assert!(d.confirm.right() <= d.dialog.right());
        assert!(d.cancel.right() <= d.confirm.x);
    }

    #[test]
    fn dialog_hits_cover_kind_fields_and_buttons() {
        let f = TunnelForm::new("h");
        let m = Metrics::default();
        let d = dialog_layout(content(), &f, &m);
        let at = |r: Rect| (r.x + 3.0, r.y + 3.0);
        let (x, y) = at(d.segments.segments[2]);
        assert_eq!(dialog_hit(&d, &f, x, y), DialogHit::Kind(2));
        let (x, y) = at(d.name.box_rect);
        assert_eq!(dialog_hit(&d, &f, x, y), DialogHit::Field(FormField::Name));
        let (x, y) = at(d.local.box_rect);
        assert_eq!(
            dialog_hit(&d, &f, x, y),
            DialogHit::Field(FormField::LocalPort)
        );
        let (x, y) = at(d.cancel);
        assert_eq!(dialog_hit(&d, &f, x, y), DialogHit::Cancel);
        let (x, y) = at(d.confirm);
        assert_eq!(dialog_hit(&d, &f, x, y), DialogHit::Confirm);
        assert_eq!(dialog_hit(&d, &f, 1.0, 1.0), DialogHit::Outside);
    }

    #[test]
    fn state_press_flow_opens_validates_and_saves() {
        let mut s = TunnelsState::new("jerem prod");
        let l = list_layout(content(), 0, &s.metrics.get());
        let n = l.new_button;
        assert_eq!(
            s.press(content(), n.x + 3.0, n.y + 3.0, &free),
            TunnelAction::Redraw
        );
        assert!(s.form.is_some());
        {
            let f = s.form.as_mut().unwrap();
            f.local_port = "5432".into();
            f.dest_host = "localhost".into();
            f.dest_port = "5432".into();
        }
        let d = dialog_layout(content(), s.form.as_ref().unwrap(), &s.metrics.get());
        let c = d.confirm;
        match s.press(content(), c.x + 3.0, c.y + 3.0, &free) {
            TunnelAction::Save(draft) => assert_eq!(draft.bind_port, 5432),
            other => panic!("{other:?}"),
        }
        assert!(s.form.is_none(), "the dialog closes on a valid save");
    }

    #[test]
    fn an_invalid_save_keeps_the_dialog_open() {
        let mut s = TunnelsState::new("h");
        s.form = Some(TunnelForm::new("h"));
        let d = dialog_layout(content(), s.form.as_ref().unwrap(), &s.metrics.get());
        let c = d.confirm;
        assert_eq!(
            s.press(content(), c.x + 3.0, c.y + 3.0, &free),
            TunnelAction::Redraw
        );
        assert!(s.form.is_some());
        assert!(s
            .form
            .as_ref()
            .unwrap()
            .error_for(FormField::LocalPort)
            .is_some());
    }

    #[test]
    fn cancel_and_escape_close_the_dialog() {
        let mut s = TunnelsState::new("h");
        s.form = Some(TunnelForm::new("h"));
        assert_eq!(s.key(FormKey::Escape, &free), TunnelAction::Redraw);
        assert!(s.form.is_none());
        s.form = Some(TunnelForm::new("h"));
        let d = dialog_layout(content(), s.form.as_ref().unwrap(), &s.metrics.get());
        s.press(content(), d.cancel.x + 3.0, d.cancel.y + 3.0, &free);
        assert!(s.form.is_none());
    }

    #[test]
    fn toggle_delete_and_edit_become_actions() {
        let mut s = TunnelsState::new("h");
        s.items = vec![item("a", TunnelKind::Local, TunnelStatus::Stopped)];
        let m = s.metrics.get();
        let l = list_layout(content(), 1, &m);
        let cl = card_geometry(l.cards[0], &m, TunnelStatus::Stopped);
        let a = cl.actions[0].unwrap();
        assert_eq!(
            s.press(content(), a.x + 3.0, a.y + 3.0, &free),
            TunnelAction::Toggle("a".into())
        );
        let t = cl.actions[1].unwrap();
        assert_eq!(
            s.press(content(), t.x + 3.0, t.y + 3.0, &free),
            TunnelAction::Delete("a".into())
        );
        let b = l.cards[0];
        assert_eq!(
            s.press(content(), b.x + 30.0, b.y + 5.0, &free),
            TunnelAction::Redraw
        );
        assert_eq!(s.form.as_ref().and_then(|f| f.id.clone()), Some("a".into()));
    }

    #[test]
    fn hover_reports_whether_it_changed() {
        let mut s = TunnelsState::new("h");
        s.items = vec![item("a", TunnelKind::Local, TunnelStatus::Stopped)];
        let l = list_layout(content(), 1, &s.metrics.get());
        let b = l.cards[0];
        assert!(s.hover(content(), b.x + 30.0, b.y + 5.0));
        assert!(!s.hover(content(), b.x + 31.0, b.y + 5.0));
        assert!(s.hover(content(), 1.0, 1.0));
    }
}
