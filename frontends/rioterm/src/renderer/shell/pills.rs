//! Session pills row: one Navigation session pill per open session of the
//! selected machine, the new-session "+", and the split buttons.

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::chrome::Chrome;
use terminus_ui::components::navigation::session_pill;
use terminus_ui::geom::Rect;
use terminus_ui::icons::{Icon, IconPlacement};
use terminus_ui::shell::pills::{PLUS_ICON, SPLIT_ICON, SPLIT_RADIUS};
use terminus_ui::shell::ShellHit;
use terminus_ui::theme::ChromeTheme;

use super::{fill, u8_to_f32};
use crate::renderer::chrome::draw_icon;
use crate::renderer::components::navigation::paint_session_pill;
use crate::renderer::ui_text::{measure_ui_text, UiWeight};

/// `label` cut with an ellipsis to fit `max_w`.
fn elide(sugarloaf: &mut Sugarloaf, label: &str, max_w: f32) -> String {
    let size = session_pill::SIZE;
    if measure_ui_text(sugarloaf, label, size, UiWeight::Regular) <= max_w + 0.5 {
        return label.to_string();
    }
    let chars: Vec<char> = label.chars().collect();
    let mut n = chars.len();
    while n > 0 {
        n -= 1;
        let s: String = chars[..n].iter().collect::<String>() + "…";
        if measure_ui_text(sugarloaf, &s, size, UiWeight::Regular) <= max_w {
            return s;
        }
    }
    "…".to_string()
}

fn icon_button(
    sugarloaf: &mut Sugarloaf,
    theme: &ChromeTheme,
    rect: &Rect,
    icon: Icon,
    size: f32,
    radius: f32,
    hovered: bool,
    device_scale: f32,
) {
    if hovered {
        fill(sugarloaf, rect, theme.surface, radius);
    }
    let fg = if hovered {
        theme.text
    } else {
        theme.text_muted
    };
    draw_icon(
        sugarloaf,
        icon,
        IconPlacement::new(
            rect.x + (rect.width - size) * 0.5,
            rect.y + (rect.height - size) * 0.5,
            size,
        ),
        u8_to_f32(fg),
        device_scale,
    );
}

pub(super) fn paint(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    device_scale: f32,
) {
    let shell = &chrome.shell;
    let Some(g) = shell.pills_geom() else {
        return;
    };
    let hovered = shell.hovered_pill();
    for (i, pill) in shell.pills.iter().enumerate() {
        let (Some(rect), Some(lw)) = (g.pills.get(i), g.label_w.get(i)) else {
            continue;
        };
        let label = elide(sugarloaf, &pill.label, *lw);
        let mut state = pill.state(hovered == Some(i));
        // Pinned sessions never show a close ×; draw them as their
        // close-less look when hovered.
        if !pill.closable
            && state == terminus_ui::components::navigation::PillState::Hover
        {
            state = terminus_ui::components::navigation::PillState::Default;
        }
        // The component fills with `radius::PILL` (999), which sugarloaf
        // does not clamp to half the height and so drops; lay the capsule
        // here first.
        if let Some(bg) = session_pill::style(theme, state).bg {
            fill(sugarloaf, rect, bg, rect.height * 0.5);
        }
        if !pill.closable
            && state == terminus_ui::components::navigation::PillState::Active
        {
            // The component's Active look always carries a ×; a pinned
            // session has none, so its label is drawn here.
            crate::renderer::ui_text::draw_ui_text(
                sugarloaf,
                session_pill::text_x(rect, false),
                rect.y + rect.height * 0.5 - session_pill::SIZE * 0.6,
                &label,
                session_pill::SIZE,
                theme.text,
                UiWeight::Regular,
            );
        } else {
            paint_session_pill(sugarloaf, theme, rect, &label, state, device_scale);
        }
    }
    let hover = shell.hover;
    icon_button(
        sugarloaf,
        theme,
        &g.plus,
        Icon::Plus,
        PLUS_ICON,
        g.plus.height * 0.5,
        hover == Some(ShellHit::NewSession),
        device_scale,
    );
    icon_button(
        sugarloaf,
        theme,
        &g.split_right,
        Icon::Columns2,
        SPLIT_ICON,
        SPLIT_RADIUS,
        hover == Some(ShellHit::SplitRight),
        device_scale,
    );
    icon_button(
        sugarloaf,
        theme,
        &g.split_down,
        Icon::Rows2,
        SPLIT_ICON,
        SPLIT_RADIUS,
        hover == Some(ShellHit::SplitDown),
        device_scale,
    );
}
