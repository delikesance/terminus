//! Workspace header: Identity title (+ mono address), Navigation view tabs
//! on the bottom border, and the window controls (Windows).

use rio_backend::sugarloaf::Sugarloaf;
use terminus_ui::chrome::Chrome;
use terminus_ui::components::identity::{
    HEADER_ADDRESS_SIZE, HEADER_TITLE_INK_DY, HEADER_TITLE_SIZE,
};
use terminus_ui::components::navigation::TabState;
use terminus_ui::icons::{Icon, IconPlacement};
use terminus_ui::shell::{ShellHit, WindowButton};
use terminus_ui::theme::ChromeTheme;

use super::{fill, u8_to_f32};
use crate::renderer::chrome::draw_icon;
use crate::renderer::components::navigation::paint_view_tab;
use crate::renderer::ui_text::{draw_mono_text, draw_ui_text, UiWeight};

/// Close-button hover fill (Windows caption convention).
const CLOSE_HOVER: [f32; 4] = [0.91, 0.18, 0.22, 1.0];
const CONTROL_ICON: f32 = 13.0;

pub(super) fn paint(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    device_scale: f32,
) {
    let shell = &chrome.shell;
    let g = shell.header_geom();
    let (title, address) = shell.title();

    fill(sugarloaf, &g.ident.border, theme.divider, 0.0);
    draw_ui_text(
        sugarloaf,
        g.ident.title.x,
        g.ident.title.y + HEADER_TITLE_INK_DY,
        &title,
        HEADER_TITLE_SIZE,
        theme.text,
        UiWeight::SemiBold,
    );
    if let (Some(a), Some(r)) = (address.as_deref(), g.ident.address) {
        draw_mono_text(
            sugarloaf,
            r.x,
            r.y,
            a,
            HEADER_ADDRESS_SIZE,
            theme.text_faint,
            UiWeight::Regular,
        );
    }

    let view = shell.view();
    for (rect, tab) in g.tabs.iter().zip(shell.header_tabs()) {
        let state = if tab.view == view {
            TabState::Active
        } else if shell.hover == Some(ShellHit::Tab(tab.view)) {
            TabState::Hover
        } else {
            TabState::Default
        };
        paint_view_tab(sugarloaf, theme, rect, tab.label, &tab.badge, state);
    }

    if shell.window_controls {
        let buttons = [
            (WindowButton::Minimize, Icon::Minus),
            (WindowButton::Maximize, Icon::Square),
            (WindowButton::Close, Icon::X),
        ];
        for (rect, (button, icon)) in shell.layout().window_controls().iter().zip(buttons)
        {
            let hovered = shell.hover == Some(ShellHit::Control(button));
            let mut fg = u8_to_f32(theme.text_faint);
            if hovered {
                if button == WindowButton::Close {
                    fill(sugarloaf, rect, CLOSE_HOVER, 8.0);
                    fg = [1.0, 1.0, 1.0, 1.0];
                } else {
                    fill(sugarloaf, rect, theme.surface, 8.0);
                    fg = u8_to_f32(theme.text);
                }
            }
            let x = rect.x + (rect.width - CONTROL_ICON) * 0.5;
            let y = rect.y + (rect.height - CONTROL_ICON) * 0.5;
            draw_icon(
                sugarloaf,
                icon,
                IconPlacement::new(x, y, CONTROL_ICON),
                fg,
                device_scale,
            );
        }
    }
}
