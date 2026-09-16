//! Pure geometry for the session-connecting animation.
//!
//! The chrome painter turns these dots into sugarloaf rects; this module
//! stays GPU-free so the orbit maths can be unit-tested without a window.

use std::f32::consts::TAU;

/// One filled circle of the orbit indicator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OrbitDot {
    pub x: f32,
    pub y: f32,
    /// Drawn radius in logical pixels.
    pub radius: f32,
    /// Coverage multiplier in `0.0..=1.0` (opacity before theme tint).
    pub alpha: f32,
}

/// Soft ring that breathes behind the orbit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BreathRing {
    pub x: f32,
    pub y: f32,
    pub radius: f32,
    pub alpha: f32,
}

/// How many dots chase each other around the centre.
pub const ORBIT_COUNT: usize = 3;

/// Full orbit period, in seconds — fast enough to feel alive, slow
/// enough to read as deliberate rather than frantic.
pub const ORBIT_PERIOD_SECS: f32 = 1.05;

/// Convert elapsed seconds into a looping phase in `0.0..1.0`.
#[inline]
pub fn phase(elapsed_secs: f32) -> f32 {
    let t = elapsed_secs / ORBIT_PERIOD_SECS;
    t - t.floor()
}

/// Three dots orbiting `(cx, cy)`.
///
/// `orbit_r` is the path radius; `dot_r` the base disc size. Leading
/// dots are brighter and slightly larger so the chase reads as motion
/// instead of a static triangle.
pub fn orbit_dots(
    cx: f32,
    cy: f32,
    orbit_r: f32,
    dot_r: f32,
    phase: f32,
) -> [OrbitDot; ORBIT_COUNT] {
    let mut out = [OrbitDot {
        x: cx,
        y: cy,
        radius: dot_r,
        alpha: 1.0,
    }; ORBIT_COUNT];
    for (i, slot) in out.iter_mut().enumerate() {
        let frac = i as f32 / ORBIT_COUNT as f32;
        let angle = (phase + frac) * TAU;
        // Trail fade: the lead (i == 0 relative to phase) is brightest.
        // Offset so index 0 is the "head" of the chase.
        let trail = 1.0 - frac;
        let alpha = 0.35 + 0.65 * trail;
        let radius = dot_r * (0.72 + 0.28 * trail);
        slot.x = cx + angle.cos() * orbit_r;
        slot.y = cy + angle.sin() * orbit_r;
        slot.radius = radius;
        slot.alpha = alpha;
    }
    out
}

/// A ring whose radius and alpha pulse once per orbit.
pub fn breath_ring(cx: f32, cy: f32, base_r: f32, phase: f32) -> BreathRing {
    // Smooth in-out pulse (two beats per orbit feels restless; one is calm).
    let pulse = (phase * TAU).sin() * 0.5 + 0.5;
    BreathRing {
        x: cx,
        y: cy,
        radius: base_r * (0.85 + 0.25 * pulse),
        alpha: 0.12 + 0.18 * pulse,
    }
}

/// Horizontal shimmer segment for the sidebar row — a soft bar that
/// sweeps under the subtitle while the session is coming up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShimmerBar {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub alpha: f32,
}

pub fn shimmer_bar(x: f32, y: f32, track_w: f32, height: f32, phase: f32) -> ShimmerBar {
    let bar_w = (track_w * 0.38).max(12.0).min(track_w);
    let travel = (track_w - bar_w).max(0.0);
    // Ease both ends so the sweep doesn't hard-bounce.
    let t = phase;
    let eased = t * t * (3.0 - 2.0 * t);
    ShimmerBar {
        x: x + eased * travel,
        y,
        width: bar_w,
        height,
        alpha: 0.55,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_wraps_unit_interval() {
        assert!((phase(0.0) - 0.0).abs() < 1e-5);
        assert!(phase(ORBIT_PERIOD_SECS * 2.5) >= 0.0);
        assert!(phase(ORBIT_PERIOD_SECS * 2.5) < 1.0);
    }

    #[test]
    fn orbit_dots_sit_on_the_circle() {
        let dots = orbit_dots(10.0, 20.0, 8.0, 2.0, 0.0);
        for d in dots {
            let dx = d.x - 10.0;
            let dy = d.y - 20.0;
            let r = (dx * dx + dy * dy).sqrt();
            assert!((r - 8.0).abs() < 1e-4, "dot left the orbit (r={r})");
            assert!(d.alpha > 0.0 && d.alpha <= 1.0);
            assert!(d.radius > 0.0);
        }
    }

    #[test]
    fn orbit_head_is_brighter_than_tail() {
        let dots = orbit_dots(0.0, 0.0, 6.0, 2.0, 0.25);
        assert!(dots[0].alpha >= dots[1].alpha);
        assert!(dots[1].alpha >= dots[2].alpha);
    }

    #[test]
    fn shimmer_stays_inside_the_track() {
        let bar = shimmer_bar(100.0, 50.0, 80.0, 2.0, 0.0);
        assert_eq!(bar.x, 100.0);
        let end = shimmer_bar(100.0, 50.0, 80.0, 2.0, 1.0);
        assert!((end.x + end.width) <= 100.0 + 80.0 + 1e-3);
    }

    #[test]
    fn breath_ring_stays_near_base_radius() {
        let a = breath_ring(0.0, 0.0, 10.0, 0.0);
        let b = breath_ring(0.0, 0.0, 10.0, 0.25);
        assert!(a.radius >= 8.0 && a.radius <= 12.0);
        assert!(b.radius >= 8.0 && b.radius <= 12.0);
        assert!(a.alpha > 0.0 && a.alpha < 0.5);
    }
}
