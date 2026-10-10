use super::form::{FormField, TunnelForm};
use super::model::{TunnelItem, TunnelKind, TunnelStatus};
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
pub(super) const ARROW_WIDTH: f32 = 16.0;
pub(super) const ROW_GAP: f32 = 12.0;
pub(super) const LOCAL_WIDTH: f32 = 120.0;
pub(super) const PORT_WIDTH: f32 = 100.0;
pub(super) const ERROR_LINE: f32 = HELPER_HEIGHT;

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
