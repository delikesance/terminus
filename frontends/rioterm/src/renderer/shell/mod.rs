//! Painter of the app shell: window frame, sidebar, workspace header,
//! session pills and the non-terminal view area.
//!
//! Geometry and hit-testing live in `terminus_ui::shell`; every rect drawn
//! here comes from there, so a control cannot be painted where it cannot be
//! clicked. [`measure`] feeds real text widths back into that state before
//! the frame is laid out.

mod header;
mod pills;
mod sidebar;

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::chrome::Chrome;
use terminus_ui::geom::Rect;
use terminus_ui::shell::layout::MAIN_RADIUS;
use terminus_ui::shell::{ShellLayout, TextKind};
use terminus_ui::theme::ChromeTheme;

use crate::renderer::ui_text::{measure_mono_text, measure_ui_text, UiWeight};

/// Shell quads sit on the lowest order: under every component painter
/// (which paint at 0..=2 in insertion order after these) and under dialogs.
pub(crate) const ORDER: u8 = 0;
pub(crate) const DEPTH: f32 = 0.0;

/// Top y that vertically centres a line of `size` text on `cy`.
pub(crate) fn text_top(cy: f32, size: f32) -> f32 {
    cy - size * 0.6
}

pub(crate) fn u8_to_f32(c: [u8; 4]) -> [f32; 4] {
    c.map(|v| v as f32 / 255.0)
}

pub(crate) fn fill(sugarloaf: &mut Sugarloaf, r: &Rect, color: [f32; 4], radius: f32) {
    if radius > 0.0 {
        sugarloaf.rounded_rect(
            None, r.x, r.y, r.width, r.height, color, DEPTH, radius, ORDER,
        );
    } else {
        sugarloaf.rect(None, r.x, r.y, r.width, r.height, color, DEPTH, ORDER);
    }
}

/// Record the width of every text the shell lays out this frame, so the
/// header tabs and pills sit exactly where they are drawn.
pub fn measure(sugarloaf: &mut Sugarloaf, chrome: &mut Chrome) {
    // Widths are cached: measure with the UI faces, never the fallback.
    crate::renderer::ui_text::sync_ui_fonts(sugarloaf);
    for (kind, text) in chrome.shell.texts() {
        if chrome.shell.has_width(kind, &text) {
            continue;
        }
        let w = match kind {
            TextKind::Title => measure_ui_text(
                sugarloaf,
                &text,
                terminus_ui::components::identity::HEADER_TITLE_SIZE,
                UiWeight::SemiBold,
            ),
            TextKind::Address => measure_mono_text(
                sugarloaf,
                &text,
                terminus_ui::components::identity::HEADER_ADDRESS_SIZE,
                UiWeight::Regular,
            ),
            TextKind::Tab => {
                crate::renderer::components::navigation::measure_tab(sugarloaf, &text, "")
                    .label_w
            }
            TextKind::Badge => measure_ui_text(
                sugarloaf,
                &text,
                terminus_ui::components::navigation::view_tabs::SIZE,
                UiWeight::Regular,
            ),
            TextKind::Pill => measure_ui_text(
                sugarloaf,
                &text,
                terminus_ui::components::navigation::session_pill::SIZE,
                UiWeight::Regular,
            ),
        };
        chrome.shell.record_width(kind, &text, w);
    }
    crate::renderer::screens::measure(sugarloaf, chrome);
}

/// Frame colour around the main card: the sidebar and the 8px gutters,
/// with the card's corners rounded against it.
fn paint_frame(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    l: &ShellLayout,
    card: [f32; 4],
) {
    let f = theme.frame;
    let (w, h) = (l.window.width, l.window.height);
    let m = l.main;
    fill(sugarloaf, &l.sidebar, f, 0.0);
    fill(sugarloaf, &Rect::new(m.x, 0.0, w - m.x, m.y), f, 0.0);
    fill(
        sugarloaf,
        &Rect::new(m.right(), 0.0, w - m.right(), h),
        f,
        0.0,
    );
    fill(
        sugarloaf,
        &Rect::new(m.x, m.bottom(), m.width, h - m.bottom()),
        f,
        0.0,
    );
    // Rounded card corners: frame square, then a card-coloured quarter
    // (one rounded corner) over it.
    let r = MAIN_RADIUS;
    let corners = [
        (m.x, m.y, [r, 0.0, 0.0, 0.0]),
        (m.right() - r, m.y, [0.0, r, 0.0, 0.0]),
        (m.right() - r, m.bottom() - r, [0.0, 0.0, r, 0.0]),
        (m.x, m.bottom() - r, [0.0, 0.0, 0.0, r]),
    ];
    for (x, y, radii) in corners {
        fill(sugarloaf, &Rect::new(x, y, r, r), f, 0.0);
        sugarloaf.quad(None, x, y, r, r, card, radii, DEPTH, ORDER);
    }
}

/// Paint the whole shell for one frame. `card` is the main card colour —
/// the terminal's background, so the card and the grid read as one surface.
pub fn paint(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    device_scale: f32,
    card: [f32; 4],
    connecting_phase: Option<f32>,
    views: &crate::renderer::screens::ViewStates,
) {
    crate::renderer::ui_text::sync_ui_fonts(sugarloaf);
    let shell = &chrome.shell;
    let l = shell.layout();
    paint_frame(sugarloaf, theme, &l, card);
    sidebar::paint(sugarloaf, chrome, theme, device_scale, connecting_phase);

    let view = shell.view();
    if !view.shows_terminal() {
        // Cover the (still running, still sized) terminal under the view.
        let content = shell.content_rect();
        let r = MAIN_RADIUS;
        sugarloaf.quad(
            None,
            content.x,
            content.y,
            content.width,
            content.height,
            card,
            [0.0, 0.0, r, r],
            DEPTH,
            ORDER,
        );
        crate::renderer::screens::paint(
            sugarloaf,
            chrome,
            views,
            theme,
            content,
            device_scale,
        );
    }
    if view.shows_terminal() && chrome.connection.is_some() {
        // Connection progress replaces the session's terminal until it
        // speaks (the mock's "connecting" state of the Terminal view).
        crate::renderer::dialogs::connection::paint_connection_content(
            sugarloaf,
            chrome,
            theme,
            card,
            connecting_phase.unwrap_or(0.0),
        );
    }
    if let (Some(area), Some(lost)) = (chrome.lost_area(), chrome.lost.as_ref()) {
        // A session whose link dropped: dim its terminal and offer
        // Reconnect over it, laid out where the chrome hit-tests it.
        crate::renderer::dialogs::confirm::paint_confirm(
            sugarloaf,
            theme,
            &lost.spec,
            &lost.layout_in(area),
            crate::renderer::dialogs::confirm::ConfirmView {
                focus: Some(lost.focus),
                hover: lost.hover,
                option_checked: false,
            },
            true,
        );
    }
    header::paint(sugarloaf, chrome, theme, device_scale);
    pills::paint(sugarloaf, chrome, theme, device_scale);
}
