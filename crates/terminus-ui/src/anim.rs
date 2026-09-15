//! Lightweight animation primitives for Terminus chrome.
//!
//! Paint-free and GPU-free: tweens produce values the frontend samples each
//! frame. Keep this module small — new motion should reuse [`Ease`] /
//! [`Tween`] / [`RectTween`] rather than inventing per-feature maths.

use crate::geom::Rect;

/// Sampling curve for a unit progress `t ∈ [0, 1]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Ease {
    Linear,
    #[default]
    OutCubic,
    InOutCubic,
    /// Soft overshoot then settle — good for "snap into place".
    OutBack,
}

impl Ease {
    /// Map linear `t` (clamped to 0..1) through this curve.
    pub fn sample(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Self::Linear => t,
            Self::OutCubic => 1.0 - (1.0 - t).powi(3),
            Self::InOutCubic => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Self::OutBack => {
                const C1: f32 = 1.70158;
                const C3: f32 = C1 + 1.0;
                1.0 + C3 * (t - 1.0).powi(3) + C1 * (t - 1.0).powi(2)
            }
        }
    }
}

/// Linear interpolation.
#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Interpolate two rects component-wise.
#[inline]
pub fn lerp_rect(a: Rect, b: Rect, t: f32) -> Rect {
    Rect::new(
        lerp(a.x, b.x, t),
        lerp(a.y, b.y, t),
        lerp(a.width, b.width, t),
        lerp(a.height, b.height, t),
    )
}

/// Scalar tween driven by elapsed seconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tween {
    pub from: f32,
    pub to: f32,
    pub duration: f32,
    pub elapsed: f32,
    pub ease: Ease,
}

impl Tween {
    pub fn new(from: f32, to: f32, duration: f32, ease: Ease) -> Self {
        Self {
            from,
            to,
            duration: duration.max(0.001),
            elapsed: 0.0,
            ease,
        }
    }

    pub fn progress(&self) -> f32 {
        (self.elapsed / self.duration).clamp(0.0, 1.0)
    }

    pub fn finished(&self) -> bool {
        self.elapsed >= self.duration
    }

    pub fn value(&self) -> f32 {
        lerp(self.from, self.to, self.ease.sample(self.progress()))
    }

    /// Advance by `dt` seconds; returns the new value.
    pub fn tick(&mut self, dt: f32) -> f32 {
        self.elapsed = (self.elapsed + dt.max(0.0)).min(self.duration);
        self.value()
    }
}

/// Rectangle tween (position + size).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RectTween {
    pub from: Rect,
    pub to: Rect,
    pub duration: f32,
    pub elapsed: f32,
    pub ease: Ease,
}

impl RectTween {
    pub fn new(from: Rect, to: Rect, duration: f32, ease: Ease) -> Self {
        Self {
            from,
            to,
            duration: duration.max(0.001),
            elapsed: 0.0,
            ease,
        }
    }

    pub fn progress(&self) -> f32 {
        (self.elapsed / self.duration).clamp(0.0, 1.0)
    }

    pub fn finished(&self) -> bool {
        self.elapsed >= self.duration
    }

    pub fn value(&self) -> Rect {
        lerp_rect(self.from, self.to, self.ease.sample(self.progress()))
    }

    pub fn tick(&mut self, dt: f32) -> Rect {
        self.elapsed = (self.elapsed + dt.max(0.0)).min(self.duration);
        self.value()
    }
}

/// Default snap duration for host→group drops (seconds).
pub const SNAP_DURATION: f32 = 0.28;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ease_endpoints_are_stable() {
        for ease in [Ease::Linear, Ease::OutCubic, Ease::InOutCubic, Ease::OutBack] {
            assert!((ease.sample(0.0) - 0.0).abs() < 0.001, "{ease:?}");
            // OutBack overshoots near the end but settles at 1.
            assert!((ease.sample(1.0) - 1.0).abs() < 0.001, "{ease:?}");
        }
    }

    #[test]
    fn tween_reaches_target() {
        let mut t = Tween::new(0.0, 100.0, 0.5, Ease::OutCubic);
        assert!((t.value() - 0.0).abs() < 0.01);
        t.tick(0.25);
        assert!(t.value() > 40.0 && t.value() < 100.0);
        t.tick(1.0);
        assert!(t.finished());
        assert!((t.value() - 100.0).abs() < 0.01);
    }

    #[test]
    fn rect_tween_interpolates_all_edges() {
        let from = Rect::new(0.0, 0.0, 100.0, 40.0);
        let to = Rect::new(50.0, 80.0, 200.0, 60.0);
        let mut tw = RectTween::new(from, to, 1.0, Ease::Linear);
        tw.tick(0.5);
        let mid = tw.value();
        assert!((mid.x - 25.0).abs() < 0.01);
        assert!((mid.y - 40.0).abs() < 0.01);
        assert!((mid.width - 150.0).abs() < 0.01);
        assert!((mid.height - 50.0).abs() < 0.01);
    }
}
