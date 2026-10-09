// Copyright (c) 2026-present, Terminus Contributors.

use super::color::opaque_over;
use super::{BORDER_WIDTH, CARET_WIDTH, DEPTH_CONTENT, ORDER_CONTENT};
use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::geom::Rect;
use terminus_ui::theme::ChromeTheme;

/// How a dialog's 1px border is placed relative to the content rect.
pub(super) enum DialogBorderMode {
    /// Stroke occupies `dialog`; fill is inset (settings modal).
    Inset,
    /// Stroke expands outside `dialog`; fill is the content rect (add-host / connection).
    #[allow(dead_code)]
    Outward,
}

/// Full-window scrim + bordered dialog panel.
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_dialog_shell(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    window_width: f32,
    window_height: f32,
    dialog: &Rect,
    radius: f32,
    mode: DialogBorderMode,
    scrim_depth: f32,
    panel_depth: f32,
    order: u8,
    scrim_alpha_clamp: Option<(f32, f32)>,
) {
    let mut scrim = theme.scrim;
    if let Some((lo, hi)) = scrim_alpha_clamp {
        scrim[3] = scrim[3].max(lo).min(hi);
    }
    paint_scrim(
        sugarloaf,
        window_width,
        window_height,
        scrim,
        scrim_depth,
        order,
    );
    let shell = match mode {
        DialogBorderMode::Inset => *dialog,
        DialogBorderMode::Outward => Rect::new(
            dialog.x - BORDER_WIDTH,
            dialog.y - BORDER_WIDTH,
            dialog.width + 2.0 * BORDER_WIDTH,
            dialog.height + 2.0 * BORDER_WIDTH,
        ),
    };
    paint_surface(
        sugarloaf,
        &shell,
        theme.dialog_bg,
        Some(theme.dialog_border),
        radius,
        panel_depth,
        order,
        false,
    );
}

/// Borderless floating card: soft fill, optional thin luminous ring.
///
/// When a border is requested the outer shell is accent-colored; the fill
/// must be **opaque**. A translucent `accent_soft` over that shell reads as
/// a solid blue brick (the bug on selected hosts / sessions).
#[allow(dead_code)]
pub(super) fn paint_floating_surface(
    sugarloaf: &mut Sugarloaf,
    card: &Rect,
    bg: [f32; 4],
    border: Option<[f32; 4]>,
    radius: f32,
) {
    paint_surface(
        sugarloaf,
        card,
        bg,
        border,
        radius,
        DEPTH_CONTENT,
        ORDER_CONTENT,
        true,
    );
}

/// Shared bordered/plain rounded surface used by cards, chips, menus, badges.
///
/// `composite_alpha` matches [`paint_floating_surface`]: translucent fills are
/// composited over a dark card base so washes stay soft.
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_surface(
    sugarloaf: &mut Sugarloaf,
    card: &Rect,
    bg: [f32; 4],
    border: Option<[f32; 4]>,
    radius: f32,
    depth: f32,
    order: u8,
    composite_alpha: bool,
) {
    paint_surface_stroke(
        sugarloaf,
        card,
        bg,
        border,
        radius,
        BORDER_WIDTH,
        depth,
        order,
        composite_alpha,
    );
}

/// Like [`paint_surface`], with an explicit border stroke width.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_surface_stroke(
    sugarloaf: &mut Sugarloaf,
    card: &Rect,
    bg: [f32; 4],
    border: Option<[f32; 4]>,
    radius: f32,
    stroke: f32,
    depth: f32,
    order: u8,
    composite_alpha: bool,
) {
    let fill = if composite_alpha && bg[3] < 0.999 {
        opaque_over([0.133, 0.133, 0.149, 1.0], bg) // ≈ button_bg
    } else {
        bg
    };
    if let Some(border_color) = border {
        sugarloaf.rounded_rect(
            None,
            card.x,
            card.y,
            card.width,
            card.height,
            border_color,
            depth,
            radius,
            order,
        );
        sugarloaf.rounded_rect(
            None,
            card.x + stroke,
            card.y + stroke,
            (card.width - 2.0 * stroke).max(0.0),
            (card.height - 2.0 * stroke).max(0.0),
            fill,
            depth + 0.01,
            (radius - stroke).max(0.0),
            order,
        );
    } else {
        sugarloaf.rounded_rect(
            None,
            card.x,
            card.y,
            card.width,
            card.height,
            fill,
            depth,
            radius,
            order,
        );
    }
}

/// Axis-aligned opaque fill (panels, rails, scrims, footers, notice bands).
pub(crate) fn paint_flat(
    sugarloaf: &mut Sugarloaf,
    rect: &Rect,
    color: [f32; 4],
    depth: f32,
    order: u8,
) {
    sugarloaf.rect(
        None,
        rect.x,
        rect.y,
        rect.width,
        rect.height,
        color,
        depth,
        order,
    );
}

/// 1px horizontal separator.
pub(crate) fn paint_hairline_h(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    y: f32,
    width: f32,
    color: [f32; 4],
    depth: f32,
    order: u8,
) {
    paint_flat(
        sugarloaf,
        &Rect::new(x, y, width, BORDER_WIDTH),
        color,
        depth,
        order,
    );
}

/// 1px vertical separator.
#[allow(dead_code)]
pub(crate) fn paint_hairline_v(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    y: f32,
    height: f32,
    color: [f32; 4],
    depth: f32,
    order: u8,
) {
    paint_flat(
        sugarloaf,
        &Rect::new(x, y, BORDER_WIDTH, height),
        color,
        depth,
        order,
    );
}

/// Blinking text caret used by search, palette, settings, and rename fields.
pub(crate) fn paint_caret(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    y: f32,
    height: f32,
    color: [f32; 4],
    depth: f32,
    order: u8,
) {
    paint_flat(
        sugarloaf,
        &Rect::new(x, y, CARET_WIDTH, height),
        color,
        depth,
        order,
    );
}

/// Apple HIG title-bar strip + bottom hairline (island / context bar).
pub(crate) fn paint_title_strip(sugarloaf: &mut Sugarloaf, logical_w: f32, height: f32) {
    let theme = ChromeTheme::default();
    let strip = theme.frame;
    let strip_border = theme.divider;
    paint_flat(
        sugarloaf,
        &Rect::new(0.0, 0.0, logical_w, height),
        strip,
        0.04,
        0,
    );
    paint_hairline_h(
        sugarloaf,
        0.0,
        height - 1.0,
        logical_w,
        strip_border,
        0.041,
        0,
    );
}

/// Full-window modal/overlay scrim.
pub(crate) fn paint_scrim(
    sugarloaf: &mut Sugarloaf,
    window_width: f32,
    window_height: f32,
    color: [f32; 4],
    depth: f32,
    order: u8,
) {
    paint_flat(
        sugarloaf,
        &Rect::new(0.0, 0.0, window_width, window_height),
        color,
        depth,
        order,
    );
}

/// Straight stroke (e.g. reset-swatch slash).
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_line(
    sugarloaf: &mut Sugarloaf,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    stroke: f32,
    color: [f32; 4],
    depth: f32,
    order: u8,
) {
    sugarloaf.line(x0, y0, x1, y1, stroke, depth, color, order);
}
