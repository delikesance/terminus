//! Inputs: text field, search, select and textarea geometry.

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
pub const HOVER_BORDER: [f32; 4] = [
    0x46 as f32 / 255.0,
    0x3c as f32 / 255.0,
    0x5e as f32 / 255.0,
    1.0,
];
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
        FieldState::Hover => HOVER_BORDER,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ChromeTheme;

    fn lay(kind: FieldKind, label: bool, helper: bool) -> FieldLayout {
        field_layout((10.0, 20.0), 300.0, kind, label, helper)
    }

    #[test]
    fn box_is_46_tall_below_label() {
        let l = lay(FieldKind::Text, true, false);
        let label = l.label.unwrap();
        assert_eq!(label.height, LABEL_HEIGHT);
        assert_eq!(l.box_rect.y, 20.0 + LABEL_HEIGHT + LABEL_GAP);
        assert_eq!(l.box_rect.height, 46.0);
        assert_eq!(l.box_rect.width, 300.0);
        assert!(l.helper.is_none());
        assert_eq!(l.total.height, LABEL_HEIGHT + LABEL_GAP + 46.0);
    }

    #[test]
    fn without_label_box_starts_at_origin() {
        let l = lay(FieldKind::Text, false, false);
        assert!(l.label.is_none());
        assert_eq!(l.box_rect.y, 20.0);
    }

    #[test]
    fn helper_sits_below_box_with_gap() {
        let l = lay(FieldKind::Text, true, true);
        let h = l.helper.unwrap();
        assert_eq!(h.y, l.box_rect.bottom() + LABEL_GAP);
        assert_eq!(h.height, HELPER_HEIGHT);
        assert_eq!(l.total.bottom(), h.bottom());
    }

    #[test]
    fn textarea_is_92_tall_and_text_starts_12_down() {
        let l = lay(FieldKind::Textarea, true, false);
        assert_eq!(l.box_rect.height, 92.0);
        assert_eq!(l.text.y, l.box_rect.y + TEXTAREA_PAD_TOP);
    }

    #[test]
    fn single_line_text_is_vertically_centered() {
        let l = lay(FieldKind::Text, false, false);
        let mid = l.text.y + l.text.height / 2.0;
        assert!((mid - (l.box_rect.y + 23.0)).abs() < 0.01);
        assert_eq!(l.text.x, l.box_rect.x + PAD_LEFT);
    }

    #[test]
    fn plain_text_has_no_trailing_slot_and_text_fills_box() {
        let l = lay(FieldKind::Text, false, false);
        assert!(l.trailing.is_none());
        assert_eq!(l.text.right(), l.box_rect.right() - PAD_RIGHT);
    }

    #[test]
    fn password_and_select_have_trailing_slot_at_right_edge() {
        for kind in [FieldKind::Password, FieldKind::Select] {
            let l = lay(kind, false, false);
            let t = l.trailing.unwrap();
            assert_eq!(t.width, TRAILING_ICON);
            assert_eq!(t.right(), l.box_rect.right() - PAD_RIGHT);
            assert!((t.y + t.height / 2.0 - (l.box_rect.y + 23.0)).abs() < 0.01);
            assert_eq!(l.text.right(), t.x - INNER_GAP);
        }
    }

    #[test]
    fn trailing_icons() {
        assert_eq!(FieldKind::Password.trailing_icon(false), Some(Icon::Eye));
        assert_eq!(FieldKind::Password.trailing_icon(true), Some(Icon::EyeOff));
        assert_eq!(
            FieldKind::Select.trailing_icon(false),
            Some(Icon::ChevronDown)
        );
        assert_eq!(FieldKind::Text.trailing_icon(false), None);
        assert_eq!(FieldKind::Mono.trailing_icon(false), None);
    }

    #[test]
    fn value_fonts() {
        assert_eq!(FieldKind::Text.value_font(), 15.0);
        assert_eq!(FieldKind::Mono.value_font(), 14.0);
        assert_eq!(FieldKind::Textarea.value_font(), 14.0);
    }

    #[test]
    fn ring_expands_box_by_three() {
        let l = lay(FieldKind::Text, true, false);
        assert_eq!(l.ring.x, l.box_rect.x - 3.0);
        assert_eq!(l.ring.width, l.box_rect.width + 6.0);
        assert_eq!(l.ring.height, l.box_rect.height + 6.0);
    }

    #[test]
    fn caret_follows_measured_prefix_and_clamps() {
        let l = lay(FieldKind::Text, false, false);
        let c = caret_rect(&l, FieldKind::Text, 40.0);
        assert_eq!(c.x, l.text.x + 40.0);
        assert_eq!(c.width, CARET_WIDTH);
        assert_eq!(c.y, l.text.y);
        let far = caret_rect(&l, FieldKind::Text, 9999.0);
        assert!(far.right() <= l.text.right() + 0.01);
    }

    #[test]
    fn hit_test_box_and_trailing() {
        let l = lay(FieldKind::Password, true, false);
        let t = l.trailing.unwrap();
        assert_eq!(
            hit_test(&l, FieldState::Default, t.x + 1.0, t.y + 1.0),
            Some(FieldHit::Trailing)
        );
        assert_eq!(
            hit_test(
                &l,
                FieldState::Default,
                l.box_rect.x + 5.0,
                l.box_rect.y + 5.0
            ),
            Some(FieldHit::Box)
        );
        assert_eq!(
            hit_test(
                &l,
                FieldState::Default,
                l.label.unwrap().x + 1.0,
                l.label.unwrap().y + 1.0
            ),
            None
        );
        assert_eq!(hit_test(&l, FieldState::Default, -5.0, -5.0), None);
    }

    #[test]
    fn disabled_field_ignores_hits() {
        let l = lay(FieldKind::Text, false, false);
        assert_eq!(
            hit_test(
                &l,
                FieldState::Disabled,
                l.box_rect.x + 5.0,
                l.box_rect.y + 5.0
            ),
            None
        );
    }

    #[test]
    fn border_colours_per_state() {
        let th = ChromeTheme::default();
        assert_eq!(border_color(&th, FieldState::Default), th.line);
        assert_eq!(border_color(&th, FieldState::Filled), th.line);
        assert_eq!(border_color(&th, FieldState::Hover), HOVER_BORDER);
        assert_eq!(border_color(&th, FieldState::Focus), th.accent);
        assert_eq!(border_color(&th, FieldState::Error), th.danger_fill);
        assert_eq!(border_color(&th, FieldState::Disabled), th.divider);
    }

    #[test]
    fn mask_is_bullets() {
        assert_eq!(mask(3), "\u{2022}\u{2022}\u{2022}");
        assert_eq!(mask(0), "");
    }

    #[test]
    fn search_box_geometry() {
        let s = search_layout((0.0, 0.0), 300.0, SearchKind::Search, 0.0);
        assert_eq!(s.box_rect.height, 40.0);
        assert_eq!(s.icon.x, SEARCH_PAD);
        assert_eq!(s.icon.width, SEARCH_ICON);
        assert_eq!(s.text.x, SEARCH_PAD + SEARCH_ICON + INNER_GAP);
        assert!(s.hint.is_none());
        assert_eq!(s.text.right(), 300.0 - SEARCH_PAD);
    }

    #[test]
    fn command_bar_geometry_has_right_aligned_hint() {
        let s = search_layout((0.0, 0.0), 300.0, SearchKind::CommandBar, 40.0);
        assert_eq!(s.box_rect.height, 38.0);
        let h = s.hint.unwrap();
        assert_eq!(h.width, 40.0);
        assert_eq!(h.right(), 300.0 - COMMAND_PAD);
        assert_eq!(s.icon.x, COMMAND_PAD);
        assert_eq!(s.text.right(), h.x - INNER_GAP);
    }

    #[test]
    fn port_pair_widths_and_alignment() {
        let p = port_pair_layout((5.0, 7.0));
        assert_eq!(p.local.box_rect.width, 140.0);
        assert_eq!(p.dest.box_rect.width, 320.0);
        assert_eq!(p.port.box_rect.width, 110.0);
        assert_eq!(p.local.box_rect.x, 5.0);
        assert_eq!(p.arrow.x, p.local.box_rect.right() + PORT_GAP);
        assert_eq!(p.dest.box_rect.x, p.arrow.right() + PORT_GAP);
        assert_eq!(p.port.box_rect.x, p.dest.box_rect.right() + PORT_GAP);
        // arrow is vertically centered on the boxes
        assert_eq!(p.arrow.y, p.local.box_rect.y);
        assert_eq!(p.arrow.height, 46.0);
        assert_eq!(p.dest.box_rect.y, p.port.box_rect.y);
        assert_eq!(p.total.right(), p.port.box_rect.right());
    }

    #[test]
    fn grid_cells_three_columns() {
        let c0 = grid_cell((0.0, 0.0), 1000.0, 0, 100.0);
        let c1 = grid_cell((0.0, 0.0), 1000.0, 1, 100.0);
        let c3 = grid_cell((0.0, 0.0), 1000.0, 3, 100.0);
        let w = (1000.0 - 2.0 * GRID_COL_GAP) / 3.0;
        assert!((c0.width - w).abs() < 0.01);
        assert!((c1.x - (w + GRID_COL_GAP)).abs() < 0.01);
        assert_eq!(c3.x, 0.0);
        assert_eq!(c3.y, 100.0 + GRID_ROW_GAP);
    }
}
