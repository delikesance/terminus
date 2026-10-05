//! UI text helpers: Sora (sans) and Martian Mono for chrome text.
//!
//! The faces are bundled in sugarloaf and registered lazily in the
//! window's font library (`FontLibrary::ui_fonts`). Terminal grid text
//! never goes through here.
//!
//! ```ignore
//! draw_ui_text(sugarloaf, x, y, "Hosts", 15.0, theme.text, UiWeight::SemiBold);
//! let w = measure_ui_text(sugarloaf, "Hosts", 15.0, UiWeight::SemiBold);
//! draw_mono_text(sugarloaf, x, y, "10.0.0.4", 12.0, theme.muted_u8, UiWeight::Regular);
//! ```

use std::cell::Cell;

use rio_backend::sugarloaf::font::UiFonts;
use rio_backend::sugarloaf::text::DrawOpts;
use rio_backend::sugarloaf::Sugarloaf;

/// Weight of a UI face. Mono only ships Regular and Medium, so
/// `SemiBold` maps to Medium there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiWeight {
    Regular,
    Medium,
    SemiBold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UiFamily {
    Sans,
    Mono,
}

/// Pure mapping from (family, weight) to the registered font id.
pub(crate) fn pick_font_id(fonts: &UiFonts, family: UiFamily, weight: UiWeight) -> usize {
    match (family, weight) {
        (UiFamily::Sans, UiWeight::Regular) => fonts.sora_regular,
        (UiFamily::Sans, UiWeight::Medium) => fonts.sora_medium,
        (UiFamily::Sans, UiWeight::SemiBold) => fonts.sora_semibold,
        (UiFamily::Mono, UiWeight::Regular) => fonts.mono_regular,
        (UiFamily::Mono, UiWeight::Medium | UiWeight::SemiBold) => fonts.mono_medium,
    }
}

thread_local! {
    static UI_FONTS: Cell<Option<UiFonts>> = const { Cell::new(None) };
}

/// Make sure the UI faces are registered and cache their ids for this
/// thread. Called at the start of each chrome/gallery frame; the
/// helpers below call it too, so they work standalone.
pub(crate) fn sync_ui_fonts(sugarloaf: &Sugarloaf) -> UiFonts {
    let fonts = sugarloaf.font_library().ui_fonts();
    UI_FONTS.with(|c| c.set(Some(fonts)));
    fonts
}

/// Draw options for UI text; `None` font id (terminal font) only before
/// the first [`sync_ui_fonts`].
pub(crate) fn ui_opts(
    family: UiFamily,
    size: f32,
    color: [u8; 4],
    weight: UiWeight,
) -> DrawOpts {
    let font_id = UI_FONTS
        .with(|c| c.get())
        .map(|f| pick_font_id(&f, family, weight));
    DrawOpts {
        font_size: size,
        color,
        font_id,
        ..DrawOpts::default()
    }
}

/// Draw `text` in Sora at logical `(x, y)`; returns the drawn width.
pub fn draw_ui_text(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    y: f32,
    text: &str,
    size: f32,
    color: [u8; 4],
    weight: UiWeight,
) -> f32 {
    draw(sugarloaf, UiFamily::Sans, x, y, text, size, color, weight)
}

/// Draw `text` in Martian Mono at logical `(x, y)`; returns the drawn width.
pub fn draw_mono_text(
    sugarloaf: &mut Sugarloaf,
    x: f32,
    y: f32,
    text: &str,
    size: f32,
    color: [u8; 4],
    weight: UiWeight,
) -> f32 {
    draw(sugarloaf, UiFamily::Mono, x, y, text, size, color, weight)
}

/// Logical width of `text` in Sora.
pub fn measure_ui_text(
    sugarloaf: &mut Sugarloaf,
    text: &str,
    size: f32,
    weight: UiWeight,
) -> f32 {
    measure(sugarloaf, UiFamily::Sans, text, size, weight)
}

/// Logical width of `text` in Martian Mono.
pub fn measure_mono_text(
    sugarloaf: &mut Sugarloaf,
    text: &str,
    size: f32,
    weight: UiWeight,
) -> f32 {
    measure(sugarloaf, UiFamily::Mono, text, size, weight)
}

#[allow(clippy::too_many_arguments)]
fn draw(
    sugarloaf: &mut Sugarloaf,
    family: UiFamily,
    x: f32,
    y: f32,
    text: &str,
    size: f32,
    color: [u8; 4],
    weight: UiWeight,
) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    sync_ui_fonts(sugarloaf);
    let opts = ui_opts(family, size, color, weight);
    sugarloaf.text_mut().draw(x, y, text, &opts)
}

fn measure(
    sugarloaf: &mut Sugarloaf,
    family: UiFamily,
    text: &str,
    size: f32,
    weight: UiWeight,
) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    sync_ui_fonts(sugarloaf);
    let opts = ui_opts(family, size, [0; 4], weight);
    sugarloaf.text_mut().measure(text, &opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fonts() -> UiFonts {
        UiFonts {
            sora_regular: 10,
            sora_medium: 11,
            sora_semibold: 12,
            mono_regular: 13,
            mono_medium: 14,
        }
    }

    #[test]
    fn sans_weights_map_to_distinct_ids() {
        let f = fonts();
        assert_eq!(pick_font_id(&f, UiFamily::Sans, UiWeight::Regular), 10);
        assert_eq!(pick_font_id(&f, UiFamily::Sans, UiWeight::Medium), 11);
        assert_eq!(pick_font_id(&f, UiFamily::Sans, UiWeight::SemiBold), 12);
    }

    #[test]
    fn mono_semibold_falls_back_to_medium() {
        let f = fonts();
        assert_eq!(pick_font_id(&f, UiFamily::Mono, UiWeight::Regular), 13);
        assert_eq!(pick_font_id(&f, UiFamily::Mono, UiWeight::Medium), 14);
        assert_eq!(pick_font_id(&f, UiFamily::Mono, UiWeight::SemiBold), 14);
    }

    #[test]
    fn opts_carry_the_synced_font_id() {
        UI_FONTS.with(|c| c.set(Some(fonts())));
        let o = ui_opts(UiFamily::Sans, 15.0, [1, 2, 3, 255], UiWeight::Medium);
        assert_eq!(o.font_id, Some(11));
        assert_eq!(o.font_size, 15.0);
        assert!(!o.bold, "weight comes from the face, not synthetic bold");
        UI_FONTS.with(|c| c.set(None));
        assert_eq!(
            ui_opts(UiFamily::Sans, 15.0, [0; 4], UiWeight::Medium).font_id,
            None
        );
    }
}
