// Copyright (c) 2026-present, Terminus Contributors.

use super::color::with_alpha;
use super::surfaces::paint_surface;
use super::{DEPTH_CONTENT, ORDER_CONNECTING};
use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::geom::Rect;
use terminus_ui::loading::breath_ring;
use terminus_ui::loading::orbit_dots;
use terminus_ui::theme::ChromeTheme;

/// Soft overlay over the terminal while a host session is starting.
#[allow(dead_code)]
pub(super) fn success_color() -> [f32; 4] {
    // Emerald-500 — matches the Termius mock's validated state.
    [16.0 / 255.0, 185.0 / 255.0, 129.0 / 255.0, 1.0]
}

/// Three chasing dots + a breathing ring around `(cx, cy)`.
pub(crate) fn draw_orbit_indicator(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    cx: f32,
    cy: f32,
    orbit_r: f32,
    dot_r: f32,
    phase: f32,
) {
    let ring = breath_ring(cx, cy, orbit_r + dot_r * 1.4, phase);
    let ring_color = with_alpha(theme.accent, ring.alpha);
    let diam = ring.radius * 2.0;
    paint_surface(
        sugarloaf,
        &Rect::new(ring.x - ring.radius, ring.y - ring.radius, diam, diam),
        ring_color,
        None,
        ring.radius,
        DEPTH_CONTENT + 0.05,
        ORDER_CONNECTING,
        false,
    );

    for dot in orbit_dots(cx, cy, orbit_r, dot_r, phase) {
        let color = with_alpha(theme.accent, dot.alpha);
        let d = dot.radius * 2.0;
        paint_surface(
            sugarloaf,
            &Rect::new(dot.x - dot.radius, dot.y - dot.radius, d, d),
            color,
            None,
            dot.radius,
            DEPTH_CONTENT + 0.06,
            ORDER_CONNECTING,
            false,
        );
    }
}
