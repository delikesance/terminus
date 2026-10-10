use crate::geom::Rect;
use crate::icons::Icon;

pub const FIELD_HEIGHT: f32 = 46.0;
pub const TEXTAREA_HEIGHT: f32 = 92.0;
pub const SEARCH_HEIGHT: f32 = 40.0;
pub const COMMAND_HEIGHT: f32 = 38.0;
pub const LABEL_FONT: f32 = 13.0;
pub const LABEL_HEIGHT: f32 = 16.0;
pub const HELPER_FONT: f32 = 12.0;
pub const HELPER_HEIGHT: f32 = 16.0;
pub const LABEL_GAP: f32 = 8.0;
pub const PAD_LEFT: f32 = 14.0;
pub const PAD_RIGHT: f32 = 12.0;
pub const INNER_GAP: f32 = 10.0;
pub const TEXTAREA_PAD_TOP: f32 = 12.0;
pub const TRAILING_ICON: f32 = 16.0;
pub const FOCUS_RING: f32 = 3.0;
pub const FOCUS_RING_ALPHA: f32 = 0.18;
pub const DISABLED_OPACITY: f32 = 0.5;
pub const CARET_WIDTH: f32 = 1.5;

pub const SANS_VALUE_FONT: f32 = 15.0;
pub const MONO_VALUE_FONT: f32 = 14.0;

pub const SEARCH_PAD: f32 = 12.0;
pub const SEARCH_ICON: f32 = 15.0;
pub const SEARCH_FONT: f32 = 14.0;
pub const COMMAND_PAD: f32 = 10.0;
pub const COMMAND_FONT: f32 = 13.0;
pub const COMMAND_HINT_FONT: f32 = 10.0;
pub const COMMAND_HINT: &str = "Ctrl K";
pub const COMMAND_PLACEHOLDER: &str = "Search or run\u{2026}";

pub const PORT_LOCAL_WIDTH: f32 = 140.0;
pub const PORT_ARROW_WIDTH: f32 = 16.0;
pub const PORT_DEST_WIDTH: f32 = 320.0;
pub const PORT_PORT_WIDTH: f32 = 110.0;
pub const PORT_GAP: f32 = 12.0;

pub const GRID_COLUMNS: usize = 3;
pub const GRID_COL_GAP: f32 = 24.0;
pub const GRID_ROW_GAP: f32 = 28.0;
pub const CAPTION_HEIGHT: f32 = 16.0;
pub const CAPTION_GAP: f32 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldState {
    Default,
    Hover,
    Focus,
    Filled,
    Error,
    Disabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Password,
    Mono,
    Select,
    Textarea,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    Search,
    CommandBar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldHit {
    Box,
    Trailing,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldLayout {
    pub total: Rect,
    pub label: Option<Rect>,
    pub box_rect: Rect,
    pub ring: Rect,
    pub text: Rect,
    pub trailing: Option<Rect>,
    pub helper: Option<Rect>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SearchLayout {
    pub box_rect: Rect,
    pub icon: Rect,
    pub text: Rect,
    pub hint: Option<Rect>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PortPairLayout {
    pub local: FieldLayout,
    pub arrow: Rect,
    pub dest: FieldLayout,
    pub port: FieldLayout,
    pub total: Rect,
}

impl FieldKind {
    pub fn is_mono(self) -> bool {
        matches!(self, FieldKind::Mono)
    }

    pub fn value_font(self) -> f32 {
        if self.is_mono() || self == FieldKind::Textarea {
            MONO_VALUE_FONT
        } else {
            SANS_VALUE_FONT
        }
    }

    /// Icon in the trailing slot, if the kind has one.
    pub fn trailing_icon(self, revealed: bool) -> Option<Icon> {
        match self {
            FieldKind::Password if revealed => Some(Icon::EyeOff),
            FieldKind::Password => Some(Icon::Eye),
            FieldKind::Select => Some(Icon::ChevronDown),
            _ => None,
        }
    }

    fn has_trailing(self) -> bool {
        matches!(self, FieldKind::Password | FieldKind::Select)
    }
}

/// Height of the input box itself.
pub fn field_height(kind: FieldKind) -> f32 {
    if kind == FieldKind::Textarea {
        TEXTAREA_HEIGHT
    } else {
        FIELD_HEIGHT
    }
}

/// Line box height used for value text of `font` size.
fn line_height(font: f32) -> f32 {
    (font * 1.25).round()
}

/// Label above, box, optional helper below, all stacked in `width`.
pub fn field_layout(
    origin: (f32, f32),
    width: f32,
    kind: FieldKind,
    has_label: bool,
    has_helper: bool,
) -> FieldLayout {
    let (x, mut y) = origin;
    let label = has_label.then(|| {
        let r = Rect::new(x, y, width, LABEL_HEIGHT);
        y += LABEL_HEIGHT + LABEL_GAP;
        r
    });
    let h = field_height(kind);
    let box_rect = Rect::new(x, y, width, h);
    y += h;
    let helper = has_helper.then(|| {
        y += LABEL_GAP;
        let r = Rect::new(x, y, width, HELPER_HEIGHT);
        y += HELPER_HEIGHT;
        r
    });
    let ring = Rect::new(
        box_rect.x - FOCUS_RING,
        box_rect.y - FOCUS_RING,
        box_rect.width + 2.0 * FOCUS_RING,
        box_rect.height + 2.0 * FOCUS_RING,
    );
    let inner_right = box_rect.right() - PAD_RIGHT;
    let trailing = kind.has_trailing().then(|| {
        Rect::new(
            inner_right - TRAILING_ICON,
            box_rect.y + (FIELD_HEIGHT - TRAILING_ICON) / 2.0,
            TRAILING_ICON,
            TRAILING_ICON,
        )
    });
    let text_right = trailing.map_or(inner_right, |t| t.x - INNER_GAP);
    let lh = line_height(kind.value_font());
    let text_y = if kind == FieldKind::Textarea {
        box_rect.y + TEXTAREA_PAD_TOP
    } else {
        box_rect.y + (h - lh) / 2.0
    };
    let text_x = box_rect.x + PAD_LEFT;
    let text = Rect::new(text_x, text_y, (text_right - text_x).max(0.0), lh);
    let total = Rect::new(x, origin.1, width, y - origin.1);
    FieldLayout {
        total,
        label,
        box_rect,
        ring,
        text,
        trailing,
        helper,
    }
}

/// Layout of a bare text field already placed at `rect`; `trailing_slot`
/// keeps the value clear of an adornment the caller draws itself.
pub fn bare_field_layout(rect: &Rect, trailing_slot: f32) -> FieldLayout {
    let mut layout =
        field_layout((rect.x, rect.y), rect.width, FieldKind::Text, false, false);
    layout.text.width = (layout.text.width - trailing_slot).max(0.0);
    layout
}

/// Caret for a measured `prefix_width` (text from line start to the caret).
pub fn caret_rect(layout: &FieldLayout, _kind: FieldKind, prefix_width: f32) -> Rect {
    let t = layout.text;
    let x = (t.x + prefix_width.max(0.0)).min((t.right() - CARET_WIDTH).max(t.x));
    Rect::new(x, t.y, CARET_WIDTH, t.height)
}

/// What a click at `(x, y)` lands on. Disabled fields are inert.
pub fn hit_test(
    layout: &FieldLayout,
    state: FieldState,
    x: f32,
    y: f32,
) -> Option<FieldHit> {
    if state == FieldState::Disabled {
        return None;
    }
    if layout.trailing.is_some_and(|t| t.contains(x, y)) {
        return Some(FieldHit::Trailing);
    }
    layout.box_rect.contains(x, y).then_some(FieldHit::Box)
}

/// Bullets shown for a password of `chars` characters.
pub fn mask(chars: usize) -> String {
    "\u{2022}".repeat(chars)
}

/// Box border colour for a state (the focus ring is separate).
pub fn border_color(theme: &crate::theme::ChromeTheme, state: FieldState) -> [f32; 4] {
    match state {
        FieldState::Default | FieldState::Filled => theme.line,
        FieldState::Hover => theme.hover_border,
        FieldState::Focus => theme.accent,
        FieldState::Error => theme.danger_fill,
        FieldState::Disabled => theme.divider,
    }
}

/// Search box (40px) or command bar (38px). `hint_width` is the measured
/// width of the command-bar hint; ignored for plain search.
pub fn search_layout(
    origin: (f32, f32),
    width: f32,
    kind: SearchKind,
    hint_width: f32,
) -> SearchLayout {
    let (h, pad) = match kind {
        SearchKind::Search => (SEARCH_HEIGHT, SEARCH_PAD),
        SearchKind::CommandBar => (COMMAND_HEIGHT, COMMAND_PAD),
    };
    let box_rect = Rect::new(origin.0, origin.1, width, h);
    let icon = Rect::new(
        box_rect.x + pad,
        box_rect.y + (h - SEARCH_ICON) / 2.0,
        SEARCH_ICON,
        SEARCH_ICON,
    );
    let right = box_rect.right() - pad;
    let hint = (kind == SearchKind::CommandBar).then(|| {
        let hh = line_height(COMMAND_HINT_FONT);
        Rect::new(
            right - hint_width,
            box_rect.y + (h - hh) / 2.0,
            hint_width,
            hh,
        )
    });
    let text_right = hint.map_or(right, |r| r.x - INNER_GAP);
    let font = if kind == SearchKind::Search {
        SEARCH_FONT
    } else {
        COMMAND_FONT
    };
    let lh = line_height(font);
    let tx = icon.right() + INNER_GAP;
    let text = Rect::new(
        tx,
        box_rect.y + (h - lh) / 2.0,
        (text_right - tx).max(0.0),
        lh,
    );
    SearchLayout {
        box_rect,
        icon,
        text,
        hint,
    }
}

/// Local port -> destination -> port, bottom-aligned like the design.
pub fn port_pair_layout(origin: (f32, f32)) -> PortPairLayout {
    let (x, y) = origin;
    let local = field_layout((x, y), PORT_LOCAL_WIDTH, FieldKind::Mono, true, false);
    let ax = local.box_rect.right() + PORT_GAP;
    let arrow = Rect::new(ax, local.box_rect.y, PORT_ARROW_WIDTH, FIELD_HEIGHT);
    let dest = field_layout(
        (arrow.right() + PORT_GAP, y),
        PORT_DEST_WIDTH,
        FieldKind::Mono,
        true,
        false,
    );
    let port = field_layout(
        (dest.box_rect.right() + PORT_GAP, y),
        PORT_PORT_WIDTH,
        FieldKind::Mono,
        true,
        false,
    );
    let total = Rect::new(x, y, port.box_rect.right() - x, local.total.height);
    PortPairLayout {
        local,
        arrow,
        dest,
        port,
        total,
    }
}

/// Cell `index` of the 3-column gallery grid.
pub fn grid_cell(origin: (f32, f32), width: f32, index: usize, row_height: f32) -> Rect {
    let w = (width - GRID_COL_GAP * (GRID_COLUMNS as f32 - 1.0)) / GRID_COLUMNS as f32;
    let (col, row) = (index % GRID_COLUMNS, index / GRID_COLUMNS);
    Rect::new(
        origin.0 + col as f32 * (w + GRID_COL_GAP),
        origin.1 + row as f32 * (row_height + GRID_ROW_GAP),
        w,
        row_height,
    )
}
