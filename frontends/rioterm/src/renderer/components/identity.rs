//! Identity component gallery (stub: section title only).

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::theme::ChromeTheme;

/// Paint the `identity` gallery at `origin` within `width`; returns the height used.
pub fn paint_gallery(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    origin: (f32, f32),
    width: f32,
) -> f32 {
    let _ = width;
    super::paint_section_title(sugarloaf, theme, origin, "Identity")
}
