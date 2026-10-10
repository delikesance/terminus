// Copyright (c) 2026-present, Terminus Contributors.

pub(crate) fn color_from_f32(c: [f32; 4]) -> [u8; 4] {
    [
        (c[0] * 255.0).round() as u8,
        (c[1] * 255.0).round() as u8,
        (c[2] * 255.0).round() as u8,
        (c[3] * 255.0).round() as u8,
    ]
}

pub(crate) fn with_alpha(color: [f32; 4], alpha: f32) -> [f32; 4] {
    [
        color[0],
        color[1],
        color[2],
        (color[3] * alpha).clamp(0.0, 1.0),
    ]
}

/// Composite `fg` (possibly translucent) over opaque `bg` → opaque result.
pub(crate) fn opaque_over(bg: [f32; 4], fg: [f32; 4]) -> [f32; 4] {
    let a = fg[3].clamp(0.0, 1.0);
    let inv = 1.0 - a;
    [
        fg[0] * a + bg[0] * inv,
        fg[1] * a + bg[1] * inv,
        fg[2] * a + bg[2] * inv,
        1.0,
    ]
}

pub(crate) fn as_u8(color: [f32; 4]) -> [u8; 4] {
    color.map(|channel| (channel * 255.0).round().clamp(0.0, 255.0) as u8)
}

pub(crate) fn as_f32(color: [u8; 4]) -> [f32; 4] {
    [
        color[0] as f32 / 255.0,
        color[1] as f32 / 255.0,
        color[2] as f32 / 255.0,
        color[3] as f32 / 255.0,
    ]
}
