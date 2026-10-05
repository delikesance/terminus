//! Selection controls: segmented control, toggle switch, checkbox, choice cards.
//!
//! Pure geometry, state and hit-testing (logical pixels). Text widths are
//! supplied by the caller because this crate does not measure fonts.

use crate::geom::Rect;

/// Interaction state shared by the selection controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ControlState {
    #[default]
    Default,
    Hover,
    Focus,
    Selected,
    Disabled,
}

impl ControlState {
    /// Opacity applied to the whole control (disabled = 40%).
    pub fn opacity(self) -> f32 {
        if self == ControlState::Disabled {
            0.4
        } else {
            1.0
        }
    }

    pub fn is_disabled(self) -> bool {
        self == ControlState::Disabled
    }
}

/// Focus ring: 2px canvas gap then 2px accent. Returns `(inner, outer)` rects
/// (inner = canvas-coloured, outer = accent-coloured), both around `r`.
pub fn focus_ring_rects(r: &Rect) -> (Rect, Rect) {
    (
        Rect::new(r.x - 2.0, r.y - 2.0, r.width + 4.0, r.height + 4.0),
        Rect::new(r.x - 4.0, r.y - 4.0, r.width + 8.0, r.height + 8.0),
    )
}

// ---------- segmented control ----------

pub const SEGMENT_TRACK_PAD: f32 = 3.0;
pub const SEGMENT_GAP: f32 = 2.0;
pub const SEGMENT_TRACK_RADIUS: f32 = 11.0;
pub const SEGMENT_RADIUS: f32 = 8.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SegmentedSize {
    #[default]
    Medium,
    Small,
}

impl SegmentedSize {
    pub fn segment_height(self) -> f32 {
        match self {
            Self::Medium => 34.0,
            Self::Small => 28.0,
        }
    }
    pub fn pad_x(self) -> f32 {
        match self {
            Self::Medium => 16.0,
            Self::Small => 12.0,
        }
    }
    pub fn font_size(self) -> f32 {
        match self {
            Self::Medium => 13.0,
            Self::Small => 12.0,
        }
    }
}

/// Selection state of a segmented control (2-4 options, exactly one selected).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segmented {
    pub count: usize,
    pub selected: usize,
}

impl Segmented {
    pub fn new(count: usize, selected: usize) -> Self {
        Self {
            count,
            selected: selected.min(count.saturating_sub(1)),
        }
    }

    /// Select `index`; returns true if the selection changed.
    pub fn select(&mut self, index: usize) -> bool {
        if index >= self.count || index == self.selected {
            return false;
        }
        self.selected = index;
        true
    }

    /// Left/Right key: move by `delta`, clamped. Returns true if changed.
    pub fn step(&mut self, delta: i32) -> bool {
        let next = self.selected as i64 + delta as i64;
        if next < 0 {
            return false;
        }
        self.select(next as usize)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SegmentedLayout {
    pub track: Rect,
    pub segments: Vec<Rect>,
}

impl SegmentedLayout {
    /// `text_widths`: measured label width of each option.
    pub fn new(x: f32, y: f32, text_widths: &[f32], size: SegmentedSize) -> Self {
        let h = size.segment_height();
        let mut cx = x + SEGMENT_TRACK_PAD;
        let mut segments = Vec::with_capacity(text_widths.len());
        for (i, w) in text_widths.iter().enumerate() {
            if i > 0 {
                cx += SEGMENT_GAP;
            }
            let sw = w + 2.0 * size.pad_x();
            segments.push(Rect::new(cx, y + SEGMENT_TRACK_PAD, sw, h));
            cx += sw;
        }
        let track = Rect::new(x, y, cx - x + SEGMENT_TRACK_PAD, h + 2.0 * SEGMENT_TRACK_PAD);
        Self { track, segments }
    }

    pub fn hit_test(&self, px: f32, py: f32) -> Option<usize> {
        self.segments.iter().position(|r| r.contains(px, py))
    }
}

// ---------- toggle ----------

pub const TOGGLE_WIDTH: f32 = 44.0;
pub const TOGGLE_HEIGHT: f32 = 26.0;
pub const TOGGLE_KNOB: f32 = 20.0;
pub const TOGGLE_PAD: f32 = 3.0;

pub fn toggle_rect(x: f32, y: f32) -> Rect {
    Rect::new(x, y, TOGGLE_WIDTH, TOGGLE_HEIGHT)
}

pub fn toggle_knob_rect(track: &Rect, on: bool) -> Rect {
    let kx = if on {
        track.right() - TOGGLE_PAD - TOGGLE_KNOB
    } else {
        track.x + TOGGLE_PAD
    };
    Rect::new(kx, track.y + TOGGLE_PAD, TOGGLE_KNOB, TOGGLE_KNOB)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Toggle {
    pub on: bool,
    pub state: ControlState,
}

impl Toggle {
    pub fn new(on: bool) -> Self {
        Self {
            on,
            state: ControlState::Default,
        }
    }

    /// Flip unless disabled; returns true if flipped.
    pub fn toggle(&mut self) -> bool {
        if self.state.is_disabled() {
            return false;
        }
        self.on = !self.on;
        true
    }
}

// ---------- checkbox ----------

pub const CHECKBOX_SIZE: f32 = 20.0;
pub const CHECKBOX_RADIUS: f32 = 6.0;
pub const CHECKBOX_BORDER: f32 = 1.5;
pub const CHECKBOX_LABEL_GAP: f32 = 10.0;
pub const CHECKBOX_MARK: f32 = 13.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CheckboxLayout {
    pub box_rect: Rect,
    /// Left edge of the label text.
    pub label_x: f32,
    /// Clickable row (box + label).
    pub row: Rect,
}

impl CheckboxLayout {
    pub fn new(x: f32, y: f32, label_width: f32) -> Self {
        let box_rect = Rect::new(x, y, CHECKBOX_SIZE, CHECKBOX_SIZE);
        let label_x = box_rect.right() + CHECKBOX_LABEL_GAP;
        Self {
            box_rect,
            label_x,
            row: Rect::new(x, y, label_x - x + label_width, CHECKBOX_SIZE),
        }
    }

    /// Rect of the check mark glyph, centred in the box.
    pub fn mark_rect(&self) -> Rect {
        let o = (CHECKBOX_SIZE - CHECKBOX_MARK) * 0.5;
        Rect::new(self.box_rect.x + o, self.box_rect.y + o, CHECKBOX_MARK, CHECKBOX_MARK)
    }

    pub fn hit_test(&self, px: f32, py: f32) -> bool {
        self.row.contains(px, py)
    }
}

// ---------- choice cards ----------

pub const CHOICE_HEIGHT: f32 = 68.0;
pub const CHOICE_RADIUS: f32 = 12.0;
pub const CHOICE_PAD: f32 = 14.0;
pub const CHOICE_TITLE_SIZE: f32 = 14.0;
pub const CHOICE_SUB_SIZE: f32 = 12.0;

/// Equal-width cards in a row, `gap` apart.
pub fn choice_row(x: f32, y: f32, width: f32, gap: f32, count: usize) -> Vec<Rect> {
    if count == 0 {
        return Vec::new();
    }
    let w = ((width - gap * (count as f32 - 1.0)) / count as f32).max(0.0);
    (0..count)
        .map(|i| Rect::new(x + i as f32 * (w + gap), y, w, CHOICE_HEIGHT))
        .collect()
}

/// Index of the enabled card under the pointer.
pub fn choice_hit_test(
    rects: &[Rect],
    states: &[ControlState],
    px: f32,
    py: f32,
) -> Option<usize> {
    rects.iter().enumerate().position(|(i, r)| {
        r.contains(px, py) && !states.get(i).is_some_and(|s| s.is_disabled())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: [f32; 3] = [40.0, 50.0, 60.0];

    #[test]
    fn segmented_layout_sizes() {
        let l = SegmentedLayout::new(10.0, 20.0, &W, SegmentedSize::Medium);
        assert_eq!(l.track.height, 40.0);
        assert_eq!(l.segments[0].height, 34.0);
        assert_eq!(l.segments[0].x, 13.0);
        assert_eq!(l.segments[0].width, 40.0 + 32.0);
        assert_eq!(l.segments[1].x, l.segments[0].right() + 2.0);
        assert_eq!(l.track.right(), l.segments[2].right() + 3.0);
        let s = SegmentedLayout::new(0.0, 0.0, &W, SegmentedSize::Small);
        assert_eq!(s.track.height, 34.0);
        assert_eq!(s.segments[0].width, 40.0 + 24.0);
    }

    #[test]
    fn segmented_hit_and_keys() {
        let l = SegmentedLayout::new(0.0, 0.0, &W, SegmentedSize::Medium);
        let c = l.segments[1];
        assert_eq!(l.hit_test(c.x + 1.0, c.y + 1.0), Some(1));
        assert_eq!(l.hit_test(1.0, 1.0), None); // track padding
        assert_eq!(l.hit_test(-5.0, 10.0), None);
        let mut s = Segmented::new(3, 0);
        assert!(s.step(1));
        assert!(s.step(1));
        assert!(!s.step(1));
        assert_eq!(s.selected, 2);
        assert!(s.step(-1));
        assert!(!Segmented::new(3, 0).step(-1));
        assert!(!s.select(9));
        assert!(s.select(0));
        assert_eq!(s.selected, 0);
    }

    #[test]
    fn toggle_geometry() {
        let r = toggle_rect(5.0, 6.0);
        assert_eq!((r.width, r.height), (44.0, 26.0));
        let on = toggle_knob_rect(&r, true);
        let off = toggle_knob_rect(&r, false);
        assert_eq!((on.width, on.height), (20.0, 20.0));
        assert_eq!(off.x, r.x + 3.0);
        assert_eq!(on.right(), r.right() - 3.0);
        assert_eq!(on.y, r.y + 3.0);
    }

    #[test]
    fn toggle_state_flips_unless_disabled() {
        let mut t = Toggle::new(false);
        assert!(t.toggle());
        assert!(t.on);
        t.state = ControlState::Disabled;
        assert!(!t.toggle());
        assert!(t.on);
        assert_eq!(ControlState::Disabled.opacity(), 0.4);
        assert_eq!(ControlState::Default.opacity(), 1.0);
    }

    #[test]
    fn checkbox_geometry_and_hit() {
        let l = CheckboxLayout::new(0.0, 0.0, 100.0);
        assert_eq!((l.box_rect.width, l.box_rect.height), (20.0, 20.0));
        assert_eq!(l.label_x, 30.0);
        assert_eq!(l.row.width, 130.0);
        assert!(l.hit_test(120.0, 10.0)); // on the label
        assert!(!l.hit_test(131.0, 10.0));
        assert!(!l.hit_test(5.0, 25.0));
    }

    #[test]
    fn choice_row_equal_widths_and_hit() {
        let rects = choice_row(0.0, 0.0, 480.0, 24.0, 4);
        assert_eq!(rects.len(), 4);
        assert!(rects.iter().all(|r| (r.width - 102.0).abs() < 1e-4));
        assert!((rects[1].x - 126.0).abs() < 1e-4);
        let states = [
            ControlState::Selected,
            ControlState::Default,
            ControlState::Default,
            ControlState::Disabled,
        ];
        assert_eq!(choice_hit_test(&rects, &states, 130.0, 5.0), Some(1));
        let d = rects[3];
        assert_eq!(choice_hit_test(&rects, &states, d.x + 1.0, 5.0), None);
        assert_eq!(choice_hit_test(&rects, &states, 110.0, 5.0), None); // gap
        assert!(choice_row(0.0, 0.0, 100.0, 8.0, 0).is_empty());
    }

    #[test]
    fn focus_ring_is_two_plus_two() {
        let r = toggle_rect(10.0, 10.0);
        let (inner, outer) = focus_ring_rects(&r);
        assert_eq!(inner.x, 8.0);
        assert_eq!(outer.x, 6.0);
        assert_eq!(outer.width, r.width + 8.0);
    }
}
