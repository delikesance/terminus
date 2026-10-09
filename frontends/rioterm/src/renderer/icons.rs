// Copyright (c) 2026-present, Terminus Contributors.

use super::chrome::color::as_u8;
use rio_backend::sugarloaf::text::CoverageMask;
use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::icons::Cmd;
use terminus_ui::icons::Icon;
use terminus_ui::icons::IconPlacement;
use terminus_ui::icons::LUCIDE_STROKE;
use terminus_ui::os_icons::OsGlyph;

/// Draw a Lucide icon.
///
/// The outline is rasterized at device resolution and handed to `sugarloaf`
/// as a coverage mask, which the glyph shader then reads texel for texel.
/// That is the whole point of doing it this way: painting the same outline
/// out of `line`/`rect` primitives can only put straight edges on whole
/// pixels, so a 24-unit grid scaled to 20px gives a 2px stroke as two hard
/// columns with nothing in between — a staircase, however the endpoints are
/// snapped. A mask keeps the rasterizer's own per-pixel coverage, so the
/// same outline comes out antialiased at any size.
pub(crate) fn draw_icon(
    sugarloaf: &mut Sugarloaf,
    icon: Icon,
    placement: IconPlacement,
    color: [f32; 4],
    device_scale: f32,
) {
    let size = placement.device_size(device_scale);
    // Icons travel with the UI text (same shader, same atlas), which is
    // the one part of the painter that works in bytes rather than floats.
    sugarloaf.text_mut().draw_mask(
        placement.x,
        placement.y,
        icon.id(),
        size,
        as_u8(color),
        |size| rasterize_icon(icon, size),
    );
}

/// Draw a filled brand OS mark (Nix/Ubuntu/Windows/Linux).
pub(crate) fn draw_os_glyph(
    sugarloaf: &mut Sugarloaf,
    glyph: OsGlyph,
    placement: IconPlacement,
    color: [f32; 4],
    device_scale: f32,
) {
    if !glyph.has_mark() {
        return;
    }
    let size = placement.device_size(device_scale);
    sugarloaf.text_mut().draw_mask(
        placement.x,
        placement.y,
        glyph.mask_id(),
        size,
        as_u8(color),
        |size| rasterize_os_glyph(glyph, size),
    );
}

/// Rasterize a Lucide outline into an 8-bit coverage mask.
///
/// The artwork is scaled so its 24-unit grid fills the `size`-pixel mask
/// exactly, then stroked the way a browser strokes the SVG: Lucide's own
/// 2/24 width, round caps and round joins. `None` when the path is
/// degenerate.
fn rasterize_icon(icon: Icon, size: u16) -> Option<CoverageMask> {
    let unit = IconPlacement::unit(size);
    let mut builder = tiny_skia::PathBuilder::new();
    let (mut from, mut start) = ((0.0f32, 0.0f32), (0.0f32, 0.0f32));
    // Lucide spells a dot as a line with no length (`M6 6L6.01 6` in
    // `server`) and leans on the round cap to turn it into a disc. A
    // stroker has no direction to work with on a segment that short, so
    // those are filled as circles instead of being stroked.
    let mut dots = Vec::new();

    for cmd in icon.path() {
        let at = |x: f32, y: f32| (x * unit, y * unit);
        match cmd {
            Cmd::MoveTo { x, y } => {
                let point = at(x, y);
                builder.move_to(point.0, point.1);
                from = point;
                start = point;
            }
            Cmd::LineTo { x, y } => {
                let point = at(x, y);
                if (point.0 - from.0).abs() < unit * 0.05
                    && (point.1 - from.1).abs() < unit * 0.05
                {
                    dots.push(point);
                } else {
                    builder.line_to(point.0, point.1);
                }
                from = point;
            }
            Cmd::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let (c1, c2, point) = (at(x1, y1), at(x2, y2), at(x, y));
                builder.cubic_to(c1.0, c1.1, c2.0, c2.1, point.0, point.1);
                from = point;
            }
            Cmd::Close => {
                builder.close();
                from = start;
            }
        }
    }

    let path = builder.finish()?;
    let mut pixmap = tiny_skia::Pixmap::new(size as u32, size as u32)?;

    let stroke_width = LUCIDE_STROKE * unit;
    let mut paint = tiny_skia::Paint {
        anti_alias: true,
        ..Default::default()
    };
    paint.set_color_rgba8(255, 255, 255, 255);
    let stroke = tiny_skia::Stroke {
        width: stroke_width,
        line_cap: tiny_skia::LineCap::Round,
        line_join: tiny_skia::LineJoin::Round,
        ..tiny_skia::Stroke::default()
    };
    pixmap.stroke_path(
        &path,
        &paint,
        &stroke,
        tiny_skia::Transform::identity(),
        None,
    );

    for (x, y) in dots {
        if let Some(dot) = tiny_skia::PathBuilder::from_circle(x, y, stroke_width / 2.0) {
            pixmap.fill_path(
                &dot,
                &paint,
                tiny_skia::FillRule::Winding,
                tiny_skia::Transform::identity(),
                None,
            );
        }
    }

    // The mask is the alpha channel: the paint is opaque white, so alpha
    // *is* the coverage, and the tint comes from the instance color.
    let bytes = pixmap.pixels().iter().map(|pixel| pixel.alpha()).collect();
    CoverageMask::new(size, bytes)
}

/// Rasterize a filled brand mark into an 8-bit coverage mask.
fn rasterize_os_glyph(glyph: OsGlyph, size: u16) -> Option<CoverageMask> {
    let unit = IconPlacement::unit(size);
    let mut builder = tiny_skia::PathBuilder::new();
    let mut start = (0.0f32, 0.0f32);

    for cmd in glyph.path() {
        let at = |x: f32, y: f32| (x * unit, y * unit);
        match cmd {
            Cmd::MoveTo { x, y } => {
                let point = at(x, y);
                builder.move_to(point.0, point.1);
                start = point;
            }
            Cmd::LineTo { x, y } => {
                let point = at(x, y);
                builder.line_to(point.0, point.1);
            }
            Cmd::CubicTo {
                x1,
                y1,
                x2,
                y2,
                x,
                y,
            } => {
                let (c1, c2, point) = (at(x1, y1), at(x2, y2), at(x, y));
                builder.cubic_to(c1.0, c1.1, c2.0, c2.1, point.0, point.1);
            }
            Cmd::Close => {
                builder.close();
                let _ = start;
            }
        }
    }

    let path = builder.finish()?;
    let mut pixmap = tiny_skia::Pixmap::new(size as u32, size as u32)?;
    let mut paint = tiny_skia::Paint {
        anti_alias: true,
        ..Default::default()
    };
    paint.set_color_rgba8(255, 255, 255, 255);
    pixmap.fill_path(
        &path,
        &paint,
        tiny_skia::FillRule::Winding,
        tiny_skia::Transform::identity(),
        None,
    );

    let bytes = pixmap.pixels().iter().map(|pixel| pixel.alpha()).collect();
    CoverageMask::new(size, bytes)
}
