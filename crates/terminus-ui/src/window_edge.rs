//! Resize-edge hit-testing for frameless windows.
//!
//! A window without OS decorations has no resize handles, so the
//! frontend asks [`resize_edge_at`] which edge or corner the pointer is
//! on and starts a compositor-driven resize from the answer. Paint-free
//! and window-system-free: logical pixels in, an edge out.

/// Which side or corner of the window the pointer is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeEdge {
    North,
    South,
    East,
    West,
    NorthEast,
    NorthWest,
    SouthEast,
    SouthWest,
}

/// Thickness of the edge strips, in logical pixels.
pub const RESIZE_BORDER: f32 = 6.0;
/// How far a corner zone reaches along each edge, in logical pixels.
pub const RESIZE_CORNER: f32 = 14.0;

/// The edge or corner under `(x, y)` in a `(width, height)` window.
///
/// `border` is the strip thickness; `corner` is how far the diagonal
/// zone reaches along an edge from each corner (it must be at least
/// `border`). `locked` is true while the window is maximised or
/// fullscreen, where resizing by dragging an edge makes no sense.
/// Points outside the window return `None`. On a window narrower than
/// two corners the corner zones overlap; the first match wins in the
/// order NW, NE, SW, SE, so the answer stays deterministic.
pub fn resize_edge_at(
    window: (f32, f32),
    x: f32,
    y: f32,
    border: f32,
    corner: f32,
    locked: bool,
) -> Option<ResizeEdge> {
    let (w, h) = window;
    if locked || x < 0.0 || y < 0.0 || x >= w || y >= h {
        return None;
    }
    // Edge strips, and how far along the perpendicular axis each
    // corner zone reaches.
    let (left, right) = (x < border, x >= w - border);
    let (top, bottom) = (y < border, y >= h - border);
    let (left_c, right_c) = (x < corner, x >= w - corner);
    let (top_c, bottom_c) = (y < corner, y >= h - corner);

    if (left && top_c) || (top && left_c) {
        Some(ResizeEdge::NorthWest)
    } else if (right && top_c) || (top && right_c) {
        Some(ResizeEdge::NorthEast)
    } else if (left && bottom_c) || (bottom && left_c) {
        Some(ResizeEdge::SouthWest)
    } else if (right && bottom_c) || (bottom && right_c) {
        Some(ResizeEdge::SouthEast)
    } else if top {
        Some(ResizeEdge::North)
    } else if bottom {
        Some(ResizeEdge::South)
    } else if left {
        Some(ResizeEdge::West)
    } else if right {
        Some(ResizeEdge::East)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ResizeEdge::*;

    const W: (f32, f32) = (800.0, 600.0);
    const B: f32 = 6.0;
    const C: f32 = 14.0;

    fn at(x: f32, y: f32) -> Option<ResizeEdge> {
        resize_edge_at(W, x, y, B, C, false)
    }

    #[test]
    fn the_interior_is_not_an_edge() {
        assert_eq!(at(400.0, 300.0), None);
        assert_eq!(
            at(B, 300.0),
            None,
            "strip is half-open: x == border is inside"
        );
        assert_eq!(at(400.0, B), None);
        assert_eq!(at(W.0 - B - 0.5, 300.0), None);
    }

    #[test]
    fn the_four_sides() {
        assert_eq!(at(400.0, 0.0), Some(North));
        assert_eq!(at(400.0, 5.9), Some(North));
        assert_eq!(at(400.0, 599.0), Some(South));
        assert_eq!(at(0.0, 300.0), Some(West));
        assert_eq!(at(799.0, 300.0), Some(East));
    }

    #[test]
    fn the_four_corners() {
        assert_eq!(at(1.0, 1.0), Some(NorthWest));
        assert_eq!(at(799.0, 1.0), Some(NorthEast));
        assert_eq!(at(1.0, 599.0), Some(SouthWest));
        assert_eq!(at(799.0, 599.0), Some(SouthEast));
    }

    #[test]
    fn a_corner_reaches_along_each_edge() {
        // On the top strip, within `corner` of the left edge: diagonal.
        assert_eq!(at(C - 0.5, 1.0), Some(NorthWest));
        assert_eq!(at(C + 0.5, 1.0), Some(North));
        // On the left strip, within `corner` of the top edge: diagonal.
        assert_eq!(at(1.0, C - 0.5), Some(NorthWest));
        assert_eq!(at(1.0, C + 0.5), Some(West));
        // Same on the far corner.
        assert_eq!(at(W.0 - C + 0.5, W.1 - 1.0), Some(SouthEast));
        assert_eq!(at(W.0 - 1.0, W.1 - C - 0.5), Some(East));
    }

    #[test]
    fn a_wider_border_widens_the_strip() {
        assert_eq!(resize_edge_at(W, 400.0, 8.0, 10.0, C, false), Some(North));
        assert_eq!(resize_edge_at(W, 400.0, 8.0, 6.0, C, false), None);
    }

    #[test]
    fn maximised_or_fullscreen_windows_have_no_edges() {
        assert_eq!(resize_edge_at(W, 400.0, 0.0, B, C, true), None);
        assert_eq!(resize_edge_at(W, 1.0, 1.0, B, C, true), None);
    }

    #[test]
    fn points_outside_the_window_are_not_edges() {
        assert_eq!(at(-1.0, 300.0), None);
        assert_eq!(at(400.0, -0.5), None);
        assert_eq!(at(W.0, 300.0), None);
        assert_eq!(at(400.0, W.1), None);
    }

    #[test]
    fn overlapping_corner_zones_resolve_deterministically() {
        // 20 px wide: the NW and NE zones both cover x = 10.
        let tiny = (20.0, 20.0);
        assert_eq!(
            resize_edge_at(tiny, 10.0, 1.0, B, C, false),
            Some(NorthWest)
        );
        assert_eq!(
            resize_edge_at(tiny, 10.0, 19.0, B, C, false),
            Some(SouthWest)
        );
    }
}
