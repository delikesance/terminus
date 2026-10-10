use super::*;
use crate::geom::Rect;

/// What a button shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ButtonContent {
    /// Label (measured width) with an optional leading icon.
    Label { label_width: f32, icon: bool },
    /// Square icon-only button; the accessible name is the tooltip.
    IconOnly,
}

/// Geometry of one button.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonSpec {
    pub origin: (f32, f32),
    pub kind: ButtonKind,
    pub size: ButtonSize,
    pub content: ButtonContent,
}

impl ButtonSpec {
    pub fn label(
        origin: (f32, f32),
        kind: ButtonKind,
        size: ButtonSize,
        label_width: f32,
        icon: bool,
    ) -> Self {
        Self {
            origin,
            kind,
            size,
            content: ButtonContent::Label { label_width, icon },
        }
    }

    pub fn icon_only(origin: (f32, f32), kind: ButtonKind, size: ButtonSize) -> Self {
        Self {
            origin,
            kind,
            size,
            content: ButtonContent::IconOnly,
        }
    }

    /// Label button filling `rect` horizontally and centred vertically.
    pub fn in_rect(rect: Rect, kind: ButtonKind, size: ButtonSize) -> Self {
        let label_width = rect.width - 2.0 * size.padding_x();
        let y = rect.y + (rect.height - size.height()) * 0.5;
        Self::label((rect.x, y), kind, size, label_width, false)
    }

    pub fn width(&self) -> f32 {
        match self.content {
            ButtonContent::IconOnly => self.size.height(),
            ButtonContent::Label { label_width, icon } => {
                let lead = if icon {
                    self.size.lead_icon() + ICON_GAP
                } else {
                    0.0
                };
                2.0 * self.size.padding_x() + lead + label_width
            }
        }
    }

    pub fn rect(&self) -> Rect {
        Rect::new(
            self.origin.0,
            self.origin.1,
            self.width(),
            self.size.height(),
        )
    }

    pub fn radius(&self) -> f32 {
        self.size.radius()
    }

    /// Icon box (leading icon, or the centred icon of an icon-only button).
    pub fn icon_rect(&self) -> Option<Rect> {
        let r = self.rect();
        match self.content {
            ButtonContent::IconOnly => {
                let s = self.size.solo_icon();
                Some(Rect::new(
                    r.x + (r.width - s) * 0.5,
                    r.y + (r.height - s) * 0.5,
                    s,
                    s,
                ))
            }
            ButtonContent::Label { icon: true, .. } => {
                let s = self.size.lead_icon();
                Some(Rect::new(
                    r.x + self.size.padding_x(),
                    r.y + (r.height - s) * 0.5,
                    s,
                    s,
                ))
            }
            ButtonContent::Label { icon: false, .. } => None,
        }
    }

    /// Top-left of the label text box (font-size tall, vertically centred).
    pub fn label_origin(&self) -> Option<(f32, f32)> {
        let ButtonContent::Label { icon, .. } = self.content else {
            return None;
        };
        let r = self.rect();
        let lead = if icon {
            self.size.lead_icon() + ICON_GAP
        } else {
            0.0
        };
        Some((
            r.x + self.size.padding_x() + lead,
            r.y + (r.height - self.size.font_size()) * 0.5,
        ))
    }

    /// Canvas-coloured gap ring drawn under the accent ring.
    pub fn focus_gap_rect(&self) -> Rect {
        grow(self.rect(), FOCUS_GAP)
    }

    /// Outer edge of the accent focus ring.
    pub fn focus_ring_rect(&self) -> Rect {
        grow(self.rect(), FOCUS_GAP + FOCUS_RING)
    }

    pub fn focus_gap_radius(&self) -> f32 {
        self.radius() + FOCUS_GAP
    }

    pub fn focus_ring_radius(&self) -> f32 {
        self.radius() + FOCUS_GAP + FOCUS_RING
    }

    /// Pointer hit test; a disabled button is never hit.
    pub fn hit_test(&self, x: f32, y: f32, disabled: bool) -> bool {
        !disabled && self.rect().contains(x, y)
    }

    /// Tooltip text: the name of an icon-only button, nothing otherwise.
    pub fn tooltip<'a>(&self, name: &'a str) -> Option<&'a str> {
        matches!(self.content, ButtonContent::IconOnly).then_some(name)
    }
}

/// Badge tile inset inside a dashed CTA row.
pub fn dashed_cta_badge(cta: Rect, badge_size: f32, pad: f32) -> Rect {
    Rect::new(
        cta.x + pad,
        cta.y + (cta.height - badge_size) * 0.5,
        badge_size,
        badge_size,
    )
}

fn grow(r: Rect, by: f32) -> Rect {
    Rect::new(r.x - by, r.y - by, r.width + 2.0 * by, r.height + 2.0 * by)
}
