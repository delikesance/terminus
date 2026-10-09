//! Scrollbar: vertical thumb geometry, hit-testing and drag mapping.
//!
//! Pure logical-pixel math shared by the sidebar list and the terminal
//! scrollback; the painter in
//! `frontends/rioterm/src/renderer/components/scroll.rs` only fills the
//! thumb rect computed here.

use crate::geom::Rect;

/// Scroll ranges at or below this many units hide the thumb.
const MIN_SCROLLABLE: f32 = 0.5;

/// Where a press landed on a scrollbar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollHit {
    /// On the thumb, `grab` pixels below its top edge.
    Thumb { grab: f32 },
    /// On the track outside the thumb.
    Track,
}

/// A vertical scrollbar over `track`, for `content` units of which
/// `viewport` are visible. Offsets count from the top of the content.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VScroll {
    pub track: Rect,
    pub content: f32,
    pub viewport: f32,
    pub min_thumb: f32,
}

impl VScroll {
    pub fn max_offset(&self) -> f32 {
        (self.content - self.viewport).max(0.0)
    }

    pub fn thumb_len(&self) -> f32 {
        let track = self.track.height;
        (track * self.viewport / self.content.max(1.0))
            .clamp(self.min_thumb.min(track), track)
    }

    /// The thumb at `offset`; `None` while everything fits.
    pub fn thumb(&self, offset: f32) -> Option<Rect> {
        let max = self.max_offset();
        if max <= MIN_SCROLLABLE {
            return None;
        }
        let len = self.thumb_len();
        let progress = (offset / max).clamp(0.0, 1.0);
        Some(Rect::new(
            self.track.x,
            self.track.y + (self.track.height - len) * progress,
            self.track.width,
            len,
        ))
    }

    /// Hit-test the track widened to `hit_width` around its centre line.
    pub fn hit(&self, x: f32, y: f32, offset: f32, hit_width: f32) -> Option<ScrollHit> {
        let thumb = self.thumb(offset)?;
        let pad = (hit_width - self.track.width) / 2.0;
        let inside = x >= self.track.x - pad
            && x <= self.track.x - pad + hit_width
            && y >= self.track.y
            && y <= self.track.bottom();
        if !inside {
            return None;
        }
        if y >= thumb.y && y <= thumb.bottom() {
            Some(ScrollHit::Thumb { grab: y - thumb.y })
        } else {
            Some(ScrollHit::Track)
        }
    }

    /// Offset for a pointer at `y` holding the thumb `grab` pixels below
    /// its top edge, clamped to `0..=max_offset`.
    pub fn offset_at(&self, y: f32, grab: f32) -> f32 {
        let free = self.track.height - self.thumb_len();
        if free <= 0.0 {
            return 0.0;
        }
        let top = (y - grab - self.track.y).clamp(0.0, free);
        top / free * self.max_offset()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar() -> VScroll {
        VScroll {
            track: Rect::new(100.0, 10.0, 6.0, 200.0),
            content: 1000.0,
            viewport: 200.0,
            min_thumb: 20.0,
        }
    }

    #[test]
    fn thumb_hidden_while_content_fits() {
        let fits = VScroll {
            content: 200.0,
            ..bar()
        };
        assert_eq!(fits.thumb(0.0), None);
        assert_eq!(fits.hit(103.0, 50.0, 0.0, 14.0), None);
    }

    #[test]
    fn thumb_length_follows_the_viewport_share() {
        assert_eq!(bar().thumb_len(), 40.0);
    }

    #[test]
    fn thumb_length_respects_the_minimum_and_the_track() {
        let long = VScroll {
            content: 1_000_000.0,
            ..bar()
        };
        assert_eq!(long.thumb_len(), 20.0);
        let tiny = VScroll {
            track: Rect::new(0.0, 0.0, 6.0, 10.0),
            content: 1_000_000.0,
            ..bar()
        };
        assert_eq!(tiny.thumb_len(), 10.0);
    }

    #[test]
    fn thumb_spans_the_track_from_top_to_bottom() {
        let top = bar().thumb(0.0).unwrap();
        assert_eq!((top.x, top.y, top.width), (100.0, 10.0, 6.0));
        let bottom = bar().thumb(800.0).unwrap();
        assert_eq!(bottom.bottom(), 210.0);
    }

    #[test]
    fn offsets_outside_the_range_are_clamped() {
        assert_eq!(bar().thumb(-50.0), bar().thumb(0.0));
        assert_eq!(bar().thumb(5000.0), bar().thumb(800.0));
    }

    #[test]
    fn hit_distinguishes_thumb_track_and_miss() {
        let bar = bar();
        assert_eq!(
            bar.hit(103.0, 20.0, 0.0, 14.0),
            Some(ScrollHit::Thumb { grab: 10.0 })
        );
        assert_eq!(bar.hit(103.0, 100.0, 0.0, 14.0), Some(ScrollHit::Track));
        assert_eq!(bar.hit(103.0, 5.0, 0.0, 14.0), None);
        assert_eq!(bar.hit(103.0, 215.0, 0.0, 14.0), None);
    }

    #[test]
    fn hit_width_widens_the_grab_area_symmetrically() {
        let bar = bar();
        assert!(bar.hit(96.0, 20.0, 0.0, 14.0).is_some());
        assert!(bar.hit(110.0, 20.0, 0.0, 14.0).is_some());
        assert!(bar.hit(95.0, 20.0, 0.0, 14.0).is_none());
        assert!(bar.hit(111.0, 20.0, 0.0, 14.0).is_none());
    }

    #[test]
    fn dragging_maps_the_thumb_back_to_an_offset() {
        let bar = bar();
        assert_eq!(bar.offset_at(0.0, 0.0), 0.0);
        assert_eq!(bar.offset_at(10.0, 0.0), 0.0);
        assert_eq!(bar.offset_at(1000.0, 0.0), 800.0);
        // Thumb top at 90 = half of the 160px free travel.
        assert_eq!(bar.offset_at(100.0, 10.0), 400.0);
    }

    #[test]
    fn drag_round_trips_through_the_thumb_position() {
        let bar = bar();
        let thumb = bar.thumb(300.0).unwrap();
        let offset = bar.offset_at(thumb.y + 7.0, 7.0);
        assert!((offset - 300.0).abs() < 0.01);
    }
}
