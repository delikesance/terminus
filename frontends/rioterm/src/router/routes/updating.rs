//! Full-window screen shown while an update found at launch installs.

use crate::layout::ContextDimension;
use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::theme::ChromeTheme;

/// "4.2 of 19.4 MB" (or just the received size while the total is unknown).
pub fn progress_label(done: u64, total: u64) -> String {
    let mb = |bytes: u64| bytes as f64 / (1024.0 * 1024.0);
    if total == 0 {
        format!("{:.1} MB", mb(done))
    } else {
        format!("{:.1} of {:.1} MB", mb(done), mb(total))
    }
}

/// Share of the download done, clamped to 0..=1.
pub fn fraction(done: u64, total: u64) -> f32 {
    if total == 0 {
        0.0
    } else {
        (done as f64 / total as f64).clamp(0.0, 1.0) as f32
    }
}

pub fn screen(
    sugarloaf: &mut Sugarloaf,
    context_dimension: &ContextDimension,
    theme: &ChromeTheme,
    version: &str,
    done: u64,
    total: u64,
) {
    use crate::renderer::ui_text::{draw_ui_text, sync_ui_fonts, UiWeight};
    use terminus_ui::tokens::{font_size, radius};

    sync_ui_fonts(sugarloaf);
    let layout = sugarloaf.window_size();
    let scale = context_dimension.dimension.scale;
    let width = layout.width / scale;
    let height = layout.height / scale;

    sugarloaf.rect(None, 0.0, 0.0, width, height, theme.canvas, 0.0, 0);

    // Card: title, 6px progress bar, caption (the "Updating" dialog).
    let card_w = 400.0_f32.min(width - 32.0);
    let pad = 30.0;
    let card_h = pad * 2.0 + 26.0 + 18.0 + 6.0 + 18.0 + 18.0 + 4.0 + 18.0;
    let x = ((width - card_w) / 2.0).round();
    let y = ((height - card_h) / 2.0).round();
    sugarloaf.rounded_rect(
        None,
        x - 1.0,
        y - 1.0,
        card_w + 2.0,
        card_h + 2.0,
        theme.dialog_line,
        0.05,
        radius::DIALOG + 1.0,
        1,
    );
    sugarloaf.rounded_rect(
        None,
        x,
        y,
        card_w,
        card_h,
        theme.dialog,
        0.06,
        radius::DIALOG,
        1,
    );
    let (ix, iw) = (x + pad, card_w - 2.0 * pad);
    let bar_y = y + pad + 26.0 + 18.0;
    sugarloaf.rounded_rect(None, ix, bar_y, iw, 6.0, theme.raised, 0.1, 3.0, 2);
    let fill = (iw * fraction(done, total)).max(if done > 0 { 6.0 } else { 0.0 });
    if fill > 0.0 {
        sugarloaf.rounded_rect(
            None,
            ix,
            bar_y,
            fill.min(iw),
            6.0,
            theme.accent,
            0.2,
            3.0,
            3,
        );
    }

    draw_ui_text(
        sugarloaf,
        ix,
        y + pad,
        &format!("Updating Terminus to {version}"),
        font_size::TITLE,
        theme.text,
        UiWeight::SemiBold,
    );
    draw_ui_text(
        sugarloaf,
        ix,
        bar_y + 6.0 + 18.0,
        &progress_label(done, total),
        font_size::LABEL,
        theme.text,
        UiWeight::Regular,
    );
    draw_ui_text(
        sugarloaf,
        ix,
        bar_y + 6.0 + 18.0 + 22.0,
        "Terminus restarts on its own when it's done.",
        font_size::LABEL,
        theme.text_muted,
        UiWeight::Regular,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_reads_in_megabytes() {
        assert_eq!(progress_label(0, 0), "0.0 MB");
        assert_eq!(
            progress_label(4 * 1024 * 1024 + 200 * 1024, 19 * 1024 * 1024),
            "4.2 of 19.0 MB"
        );
        assert_eq!(fraction(5, 10), 0.5);
        assert_eq!(fraction(20, 10), 1.0);
        assert_eq!(fraction(3, 0), 0.0);
    }
}
