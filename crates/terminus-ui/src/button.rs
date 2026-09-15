//! Shared chrome button geometry helpers.
//!
//! Paint lives in the frontend; this module owns the math so every
//! accent / secondary control centers its label the same way.

use crate::geom::Rect;

/// Visual variant for a chrome button.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ButtonKind {
    /// Filled accent CTA.
    Primary,
    /// Bordered / surface secondary action.
    Secondary,
    /// Flat surface fill without a border shell.
    Ghost,
}

/// Layout + style inputs for one button.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonSpec {
    pub rect: Rect,
    pub kind: ButtonKind,
    pub radius: f32,
    /// When set on a secondary button, the label uses the muted text color.
    pub muted_label: bool,
}

impl ButtonSpec {
    pub fn primary(rect: Rect) -> Self {
        Self {
            rect,
            kind: ButtonKind::Primary,
            radius: 8.0,
            muted_label: false,
        }
    }

    pub fn secondary(rect: Rect) -> Self {
        Self {
            rect,
            kind: ButtonKind::Secondary,
            radius: 8.0,
            muted_label: false,
        }
    }

    /// Flat surface button (no border ring).
    pub fn ghost(rect: Rect) -> Self {
        Self {
            rect,
            kind: ButtonKind::Ghost,
            radius: 8.0,
            muted_label: false,
        }
    }

    /// Override corner radius (e.g. add-host inputs use 12px).
    pub fn with_radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    /// Secondary label uses the theme muted color.
    pub fn muted(mut self) -> Self {
        self.muted_label = true;
        self
    }

    /// Fill color for the outer capsule, given theme accents.
    pub fn fill(self, accent: [f32; 4], surface: [f32; 4]) -> [f32; 4] {
        match self.kind {
            ButtonKind::Primary => accent,
            ButtonKind::Secondary | ButtonKind::Ghost => surface,
        }
    }

    /// Label color for the chosen kind.
    pub fn label_color(
        self,
        on_accent: [u8; 4],
        on_surface: [u8; 4],
        on_muted: [u8; 4],
    ) -> [u8; 4] {
        match self.kind {
            ButtonKind::Primary => on_accent,
            ButtonKind::Secondary | ButtonKind::Ghost if self.muted_label => on_muted,
            ButtonKind::Secondary | ButtonKind::Ghost => on_surface,
        }
    }

    /// Whether a secondary button draws a 1px border shell.
    pub fn has_border(self) -> bool {
        matches!(self.kind, ButtonKind::Secondary)
    }
}

/// Mathematically center a label of measured `text_width` and `font_size`
/// inside `rect`.
///
/// X is exact midpoint of the glyph advance. Y places the text top so the
/// em-box is vertically centered in the button (`(h - font_size) / 2`).
pub fn centered_label_origin(rect: Rect, text_width: f32, font_size: f32) -> (f32, f32) {
    let x = rect.x + (rect.width - text_width) * 0.5;
    let y = rect.y + (rect.height - font_size) * 0.5;
    (x, y)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centers_label_in_the_middle_of_the_button() {
        let rect = Rect::new(10.0, 20.0, 100.0, 30.0);
        let (x, y) = centered_label_origin(rect, 40.0, 12.0);
        assert!((x - 40.0).abs() < 0.01);
        assert!((y - 29.0).abs() < 0.01);
    }

    #[test]
    fn primary_fill_is_accent() {
        let b = ButtonSpec::primary(Rect::new(0.0, 0.0, 10.0, 10.0));
        assert_eq!(b.fill([1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 0.0, 1.0])[0], 1.0);
    }

    #[test]
    fn muted_secondary_uses_muted_label() {
        let b = ButtonSpec::secondary(Rect::new(0.0, 0.0, 10.0, 10.0)).muted();
        assert_eq!(
            b.label_color([1, 1, 1, 255], [2, 2, 2, 255], [3, 3, 3, 255]),
            [3, 3, 3, 255]
        );
    }

    #[test]
    fn dashed_cta_badge_is_vertically_centered() {
        let cta = Rect::new(0.0, 0.0, 200.0, 56.0);
        let badge = dashed_cta_badge(cta, 32.0, 12.0);
        assert!((badge.x - 12.0).abs() < 0.01);
        assert!((badge.y - 12.0).abs() < 0.01);
        assert!((badge.width - 32.0).abs() < 0.01);
    }
}
