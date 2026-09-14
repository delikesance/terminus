//! Layout rectangles shared by the painters and the hit-tests.
//!
//! Every chrome box is computed here, in logical pixels, once: the
//! renderer walks the same functions the mouse does, so a button and
//! the pixels it was drawn on can never disagree.

/// A logical-pixel rectangle, origin at the window's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    pub fn right(&self) -> f32 {
        self.x + self.width
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.height
    }

    /// Half-open containment: the left/top edges belong to the rect,
    /// the right/bottom edges belong to the next one. Stacked rows
    /// therefore never both claim a boundary pixel.
    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }

    /// The vertical slice of `self` inside `[top, bottom)`, if any.
    ///
    /// Scrollable lists clip with this: a row scrolled half past the
    /// viewport's edge is painted truncated instead of overflowing
    /// onto the terminal.
    pub fn clip_rows(&self, top: f32, bottom: f32) -> Option<(f32, f32)> {
        let start = self.y.max(top);
        let end = self.bottom().min(bottom);
        if end <= start {
            None
        } else {
            Some((start, end))
        }
    }

    /// The whole rect shifted down by `dy`.
    pub fn shifted(&self, dy: f32) -> Self {
        Self {
            y: self.y + dy,
            ..*self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contains_is_half_open_on_the_trailing_edges() {
        let rect = Rect::new(10.0, 20.0, 100.0, 30.0);
        assert!(rect.contains(10.0, 20.0));
        assert!(rect.contains(109.9, 49.9));
        assert!(!rect.contains(110.0, 30.0));
        assert!(!rect.contains(9.9, 35.0));
    }

    #[test]
    fn stacked_rows_never_share_a_boundary() {
        let a = Rect::new(0.0, 0.0, 10.0, 20.0);
        let b = Rect::new(0.0, 20.0, 10.0, 20.0);
        assert!(!a.contains(5.0, 20.0));
        assert!(b.contains(5.0, 20.0));
    }

    #[test]
    fn clip_rows_trims_to_the_viewport() {
        let rect = Rect::new(0.0, 100.0, 10.0, 40.0);
        assert_eq!(rect.clip_rows(90.0, 200.0), Some((100.0, 140.0)));
        assert_eq!(rect.clip_rows(110.0, 130.0), Some((110.0, 130.0)));
        assert_eq!(rect.clip_rows(140.0, 200.0), None);
        assert_eq!(rect.clip_rows(0.0, 100.0), None);
    }
}
