//! Component galleries: visual-test painters for each component family.
//!
//! Each submodule owns one file and exposes
//! `paint_gallery(sugarloaf, theme, origin, width) -> height_used`.
//! Run with `TERMINUS_COMPONENT_GALLERY=<section|all>` (see DEVELOPMENT.md).

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::theme::ChromeTheme;

pub mod button;
pub mod input;
pub mod selection;
pub mod navigation;
pub mod list;
pub mod feedback;
pub mod overlay;
pub mod identity;

/// Where a component paints when it is embedded in another surface (a
/// dialog, a menu) instead of the gallery wells: the sugarloaf `order`
/// (layer group), the base `depth`, and the opaque colour behind it (used
/// for focus-ring gaps and disabled fading).
#[derive(Debug, Clone, Copy)]
pub struct Layer {
    pub order: u8,
    pub depth: f32,
    pub backdrop: [f32; 4],
}

/// Section names accepted by `TERMINUS_COMPONENT_GALLERY`.
pub const SECTIONS: [&str; 8] = [
    "button",
    "input",
    "selection",
    "navigation",
    "list",
    "feedback",
    "overlay",
    "identity",
];

type GalleryFn = fn(&mut Sugarloaf, &ChromeTheme, (f32, f32), f32) -> f32;

fn section_fn(name: &str) -> Option<GalleryFn> {
    Some(match name {
        "button" => button::paint_gallery,
        "input" => input::paint_gallery,
        "selection" => selection::paint_gallery,
        "navigation" => navigation::paint_gallery,
        "list" => list::paint_gallery,
        "feedback" => feedback::paint_gallery,
        "overlay" => overlay::paint_gallery,
        "identity" => identity::paint_gallery,
        _ => return None,
    })
}

/// Names selected by `selector`: `"all"` (or empty) means every section,
/// otherwise a comma-separated list of section names. Unknown names are
/// dropped.
pub fn selected_sections(selector: &str) -> Vec<&'static str> {
    let selector = selector.trim();
    if selector.is_empty() || selector.eq_ignore_ascii_case("all") {
        return SECTIONS.to_vec();
    }
    selector
        .split(',')
        .filter_map(|part| {
            let part = part.trim().to_ascii_lowercase();
            SECTIONS.iter().copied().find(|s| *s == part)
        })
        .collect()
}

/// Section selector from `TERMINUS_COMPONENT_GALLERY`, read once.
/// `None` means normal UI; `Some("all")` / `Some("button")` / `Some("button,list")`
/// switches the window to the gallery.
pub fn gallery_selector() -> Option<&'static str> {
    static SELECTOR: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    SELECTOR
        .get_or_init(|| {
            std::env::var("TERMINUS_COMPONENT_GALLERY")
                .ok()
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        })
        .as_deref()
}

/// Outer padding of the gallery page.
pub const PAGE_PAD: f32 = 32.0;

/// Paint one full-window gallery frame on the `frame` background.
pub fn paint_gallery_frame(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    selector: &str,
) {
    use crate::renderer::chrome::paint_flat;
    use terminus_ui::geom::Rect;
    let scale = sugarloaf.scale_factor();
    let size = sugarloaf.window_size();
    let (w, h) = (size.width / scale, size.height / scale);
    crate::renderer::ui_text::sync_ui_fonts(sugarloaf);
    paint_flat(sugarloaf, &Rect::new(0.0, 0.0, w, h), theme.frame, 0.0, 0);
    paint_component_gallery(
        sugarloaf,
        theme,
        (PAGE_PAD, PAGE_PAD),
        (w - 2.0 * PAGE_PAD).max(0.0),
        selector,
    );
}

/// Gap between stacked sections.
pub const SECTION_GAP: f32 = 28.0;

/// Paint the selected sections stacked vertically; returns the total
/// height used.
pub fn paint_component_gallery(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    origin: (f32, f32),
    width: f32,
    selector: &str,
) -> f32 {
    let mut y = origin.1;
    for name in selected_sections(selector) {
        if let Some(paint) = section_fn(name) {
            y += paint(sugarloaf, theme, (origin.0, y), width) + SECTION_GAP;
        }
    }
    y - origin.1
}

/// Paints a section heading (Sora semibold) and returns its height. Shared
/// by the stubs; component agents replace their `paint_gallery` body and
/// can keep calling this for the title.
pub fn paint_section_title(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    origin: (f32, f32),
    title: &str,
) -> f32 {
    use crate::renderer::ui_text::{draw_mono_text, draw_ui_text, UiWeight};
    use terminus_ui::tokens::{font_size, space};
    let w = draw_ui_text(
        sugarloaf,
        origin.0,
        origin.1,
        title,
        font_size::TITLE,
        theme.text,
        UiWeight::SemiBold,
    );
    // Section id in Martian Mono: the selector value for this section.
    draw_mono_text(
        sugarloaf,
        origin.0 + w + space::MD,
        origin.1 + (font_size::TITLE - font_size::MONO) * 0.6,
        &title.to_ascii_lowercase(),
        font_size::MONO,
        theme.text_faint,
        UiWeight::Regular,
    );
    terminus_ui::tokens::font_size::TITLE + terminus_ui::tokens::space::MD
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_selects_every_section() {
        assert_eq!(selected_sections("all").len(), 8);
        assert_eq!(selected_sections("").len(), 8);
    }

    #[test]
    fn selector_picks_named_sections_and_drops_unknown() {
        assert_eq!(selected_sections("button"), vec!["button"]);
        assert_eq!(selected_sections("Button, list,nope"), vec!["button", "list"]);
        assert!(selected_sections("nope").is_empty());
    }

    #[test]
    fn every_section_has_a_painter() {
        for name in SECTIONS {
            assert!(section_fn(name).is_some(), "{name}");
        }
    }
}
