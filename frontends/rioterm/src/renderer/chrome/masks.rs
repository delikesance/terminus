// Copyright (c) 2026-present, Terminus Contributors.

#[allow(dead_code)]
pub(super) const CTX_MENU_BG_KIND: u32 = 0x01;
#[allow(dead_code)]
pub(super) const CTX_MENU_BORDER_KIND: u32 = 0x02;
#[allow(dead_code)]
pub(super) const CTX_MENU_HOVER_KIND: u32 = 0x10;

/// Atlas key for a rounded-rect mask.
///
/// `GlyphKey::glyph_id` is a **u32** (`artwork_id as u32` in sugarloaf), so
/// every bit that distinguishes shapes must fit in 32 bits. Packing height in
/// the high half of a u64 was truncated away — host (h=104) and group (h=72)
/// menus then shared one atlas slot (`side` is max(w,h) and usually the width).
#[allow(dead_code)]
pub(super) fn ctx_menu_mask_id(kind: u32, content_w: f32, content_h: f32) -> u64 {
    let w = content_w.round().clamp(1.0, 0xFFF as f32) as u32;
    let h = content_h.round().clamp(1.0, 0xFFF as f32) as u32;
    // [kind:8][w:12][h:12]
    let id = (kind & 0xFF) | ((w & 0xFFF) << 8) | ((h & 0xFFF) << 20);
    id as u64
}

#[allow(dead_code)]
pub(super) fn rasterize_rounded_rect_mask(
    size: u16,
    content_w: f32,
    content_h: f32,
    radius: f32,
    stroke_only: bool,
) -> Option<rio_backend::sugarloaf::text::CoverageMask> {
    let side = size as u32;
    let mut pixmap = tiny_skia::Pixmap::new(side, side)?;
    let w = content_w.min(size as f32).max(1.0);
    let h = content_h.min(size as f32).max(1.0);
    let r = radius.min(w * 0.5).min(h * 0.5).max(0.0);
    let path = rounded_rect_path(0.0, 0.0, w, h, r)?;
    let mut paint = tiny_skia::Paint {
        anti_alias: true,
        ..Default::default()
    };
    paint.set_color_rgba8(255, 255, 255, 255);
    if stroke_only {
        let stroke = tiny_skia::Stroke {
            width: 1.0,
            ..tiny_skia::Stroke::default()
        };
        pixmap.stroke_path(
            &path,
            &paint,
            &stroke,
            tiny_skia::Transform::identity(),
            None,
        );
    } else {
        pixmap.fill_path(
            &path,
            &paint,
            tiny_skia::FillRule::Winding,
            tiny_skia::Transform::identity(),
            None,
        );
    }
    let bytes: Vec<u8> = pixmap.pixels().iter().map(|p| p.alpha()).collect();
    rio_backend::sugarloaf::text::CoverageMask::new(size, bytes)
}

#[allow(dead_code)]
pub(super) fn rounded_rect_path(
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    r: f32,
) -> Option<tiny_skia::Path> {
    let mut b = tiny_skia::PathBuilder::new();
    if r <= 0.5 {
        b.move_to(x, y);
        b.line_to(x + w, y);
        b.line_to(x + w, y + h);
        b.line_to(x, y + h);
        b.close();
        return b.finish();
    }
    b.move_to(x + r, y);
    b.line_to(x + w - r, y);
    b.cubic_to(x + w, y, x + w, y, x + w, y + r);
    b.line_to(x + w, y + h - r);
    b.cubic_to(x + w, y + h, x + w, y + h, x + w - r, y + h);
    b.line_to(x + r, y + h);
    b.cubic_to(x, y + h, x, y + h, x, y + h - r);
    b.line_to(x, y + r);
    b.cubic_to(x, y, x, y, x + r, y);
    b.close();
    b.finish()
}
