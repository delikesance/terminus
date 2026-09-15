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
}

/// Layout + style inputs for one button.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonSpec {
    pub rect: Rect,
    pub kind: ButtonKind,
    pub radius: f32,
}

impl ButtonSpec {
    pub fn primary(rect: Rect) -> Self {
        Self {
            rect,
            kind: ButtonKind::Primary,
            radius: 8.0,
        }
    }

    pub fn secondary(rect: Rect) -> Self {
        Self {
            rect,
            kind: ButtonKind::Secondary,
            radius: 8.0,
        }
    }

    /// Fill color for the outer capsule, given theme accents.
    pub fn fill(self, accent: [f32; 4], surface: [f32; 4]) -> [f32; 4] {
        match self.kind {
            ButtonKind::Primary => accent,
            ButtonKind::Secondary => surface,
        }
    }

    /// Label color for the chosen kind.
    pub fn label_color(self, on_accent: [u8; 4], on_surface: [u8; 4]) -> [u8; 4] {
        match self.kind {
            ButtonKind::Primary => on_accent,
            ButtonKind::Secondary => on_surface,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centers_label_in_the_middle_of_the_button() {
        let rect = Rect::new(100.0, 50.0, 110.0, 28.0);
        let (x, y) = centered_label_origin(rect, 60.0, 12.0);
        assert!((x - 125.0).abs() < 0.01);
        assert!((y - 58.0).abs() < 0.01);
    }

    #[test]
    fn primary_uses_accent_fill() {
        let b = ButtonSpec::primary(Rect::new(0.0, 0.0, 10.0, 10.0));
        let accent = [0.2, 0.4, 1.0, 1.0];
        let surface = [0.1, 0.1, 0.1, 1.0];
        assert_eq!(b.fill(accent, surface), accent);
        assert!(!b.has_border());
    }
}
