use super::*;

/// Paint a rounded-rect outline via the shared chrome [`paint_surface_stroke`].
#[allow(clippy::too_many_arguments)]
pub(super) fn stroke_rounded_rect(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    stroke: f32,
    radius: f32,
    stroke_color: [f32; 4],
    fill_color: [f32; 4],
    depth: f32,
    order: u8,
) {
    crate::renderer::chrome::paint_surface_stroke(
        sugarloaf,
        &terminus_ui::Rect::new(x, y, width, height),
        fill_color,
        Some(stroke_color),
        radius,
        stroke,
        depth,
        order,
        false,
    );
}

/// Paint a "copy" icon (two overlapping rounded page outlines)
/// anchored at `(x, y)`. Drawn from rects only — no font glyph
/// dependency — so it renders consistently regardless of what the
/// user's font stack can produce for ⎘ / 📋 / similar.
///
/// `row_fill_color` is the background behind the icon (palette BG when
/// the row is idle, selection highlight when hovered/selected); it's
/// used to cut out the page interiors so the outlines read as a
/// proper border rather than two solid blobs. Back page painted
/// slightly below the front via depth so the front's cutout
/// correctly hides the overlapping portion of the back's stroke.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_copy_icon(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    y: f32,
    stroke_color: [f32; 4],
    row_fill_color: [f32; 4],
    depth: f32,
    order: u8,
) {
    // Back page (upper-left).
    stroke_rounded_rect(
        sugarloaf,
        x,
        y,
        COPY_ICON_PAGE_W,
        COPY_ICON_PAGE_H,
        COPY_ICON_STROKE,
        COPY_ICON_RADIUS,
        stroke_color,
        row_fill_color,
        depth,
        order,
    );
    // Front page (offset down-right), painted above the back so its
    // cutout hides the back's overlapping interior.
    stroke_rounded_rect(
        sugarloaf,
        x + COPY_ICON_OFFSET,
        y + COPY_ICON_OFFSET,
        COPY_ICON_PAGE_W,
        COPY_ICON_PAGE_H,
        COPY_ICON_STROKE,
        COPY_ICON_RADIUS,
        stroke_color,
        row_fill_color,
        depth + 0.01,
        order,
    );
}
