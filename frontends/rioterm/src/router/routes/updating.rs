//! Full-window screen shown while an update found at launch installs.

use crate::layout::ContextDimension;
use rio_backend::sugarloaf::text::DrawOpts;
use rio_backend::sugarloaf::Sugarloaf;

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
    version: &str,
    done: u64,
    total: u64,
) {
    let layout = sugarloaf.window_size();
    let scale = context_dimension.dimension.scale;
    let width = layout.width / scale;
    let height = layout.height / scale;

    sugarloaf.rect(
        None,
        0.0,
        0.0,
        width,
        height,
        [0.07, 0.07, 0.09, 1.0],
        0.0,
        0,
    );

    let bar_width = (width * 0.5).clamp(200.0, 420.0);
    let x = (width - bar_width) / 2.0;
    let y = height / 2.0;
    // Track, then the done part.
    sugarloaf.rect(None, x, y, bar_width, 6.0, [1.0, 1.0, 1.0, 0.12], 0.1, 1);
    sugarloaf.rect(
        None,
        x,
        y,
        bar_width * fraction(done, total),
        6.0,
        [0.23, 0.51, 0.96, 1.0],
        0.2,
        2,
    );

    let ui = sugarloaf.text_mut();
    ui.draw(
        x,
        y - 34.0,
        &format!("Updating Terminus to {version}"),
        &DrawOpts {
            font_size: 16.0,
            color: [255, 255, 255, 255],
            ..DrawOpts::default()
        },
    );
    ui.draw(
        x,
        y + 20.0,
        &format!(
            "{}  ·  Terminus restarts when it is done",
            progress_label(done, total)
        ),
        &DrawOpts {
            font_size: 12.0,
            color: [150, 150, 160, 255],
            ..DrawOpts::default()
        },
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
