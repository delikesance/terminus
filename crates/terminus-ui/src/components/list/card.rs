use crate::geom::Rect;
use crate::theme::ChromeTheme;
use crate::tokens::{height, radius};

/// Vertical padding of a card.
pub const CARD_PAD_Y: f32 = 16.0;
/// Horizontal padding of a card.
pub const CARD_PAD_X: f32 = 20.0;
/// Gap between the flex items of a card (dot, text, meta, actions).
pub const CARD_GAP: f32 = 18.0;
/// Gap between two action buttons.
pub const CARD_ACTION_GAP: f32 = 10.0;
/// Status dot diameter.
pub const CARD_DOT: f32 = 8.0;
/// Title line (15 px) + 4 px gap + mono detail line (11 px).
pub const CARD_CONTENT_HEIGHT: f32 = 37.0;
/// Total card height.
pub const CARD_HEIGHT: f32 = 2.0 * CARD_PAD_Y + CARD_CONTENT_HEIGHT;
/// Card corner radius.
pub const CARD_RADIUS: f32 = radius::CARD;
/// Height of an action slot (a medium button).
pub const CARD_ACTION_HEIGHT: f32 = height::CONTROL_MD;

/// Card background state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardState {
    Default,
    Hover,
}

impl CardState {
    pub fn background(self, theme: &ChromeTheme) -> [f32; 4] {
        match self {
            CardState::Default => theme.frame,
            CardState::Hover => theme.surface,
        }
    }
}

/// Content-dependent inputs of a card layout (text widths are measured by
/// the painter and passed in, keeping this crate font-free).
#[derive(Debug, Clone, Copy)]
pub struct CardSpec<'a> {
    pub has_dot: bool,
    /// Measured width of the right meta text (0 when empty).
    pub meta_width: f32,
    /// Widths of 0-2 action slots, in reading order (leftmost first).
    pub action_widths: &'a [f32],
}

/// Resolved card rectangles.
#[derive(Debug, Clone, PartialEq)]
pub struct CardLayout {
    pub rect: Rect,
    pub dot: Option<Rect>,
    /// Title + detail block.
    pub text: Rect,
    pub meta: Rect,
    /// Action slots in reading order; `None` past the number of actions.
    pub actions: [Option<Rect>; 2],
}

pub fn card_layout(rect: Rect, spec: &CardSpec) -> CardLayout {
    let mid = rect.y + rect.height / 2.0;
    let mut right = rect.right() - CARD_PAD_X;
    let mut actions: [Option<Rect>; 2] = [None, None];
    let n = spec.action_widths.len().min(2);
    for i in (0..n).rev() {
        let w = spec.action_widths[i];
        actions[i] = Some(Rect::new(
            right - w,
            mid - CARD_ACTION_HEIGHT / 2.0,
            w,
            CARD_ACTION_HEIGHT,
        ));
        right -= w + CARD_ACTION_GAP;
    }
    if n > 0 {
        right = actions[0].map_or(right, |a| a.x);
    }
    let meta_right = if n > 0 { right - CARD_GAP } else { right };
    let meta = Rect::new(
        meta_right - spec.meta_width,
        rect.y + CARD_PAD_Y,
        spec.meta_width,
        CARD_CONTENT_HEIGHT,
    );
    let mut left = rect.x + CARD_PAD_X;
    let dot = if spec.has_dot {
        let d = Rect::new(left, mid - CARD_DOT / 2.0, CARD_DOT, CARD_DOT);
        left = d.right() + CARD_GAP;
        Some(d)
    } else {
        None
    };
    let text = Rect::new(
        left,
        rect.y + CARD_PAD_Y,
        (meta.x - CARD_GAP - left).max(0.0),
        CARD_CONTENT_HEIGHT,
    );
    CardLayout {
        rect,
        dot,
        text,
        meta,
        actions,
    }
}

/// What a point hit on a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardHit {
    /// Action slot index (reading order).
    Action(usize),
    Body,
}

pub fn card_hit(rect: Rect, spec: &CardSpec, x: f32, y: f32) -> Option<CardHit> {
    if !rect.contains(x, y) {
        return None;
    }
    let l = card_layout(rect, spec);
    for (i, a) in l.actions.iter().enumerate() {
        if a.is_some_and(|a| a.contains(x, y)) {
            return Some(CardHit::Action(i));
        }
    }
    Some(CardHit::Body)
}
