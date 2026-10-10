// Copyright (c) 2026-present, Terminus Contributors.
//! Painter for the Terminus chrome.
//!
//! Every rectangle comes from `terminus_ui` (`activity_bar`, `sidebar`,
//! `add_host`), which is also what the mouse hit-tests against, so the
//! pixels and the click targets cannot drift apart. This module only
//! turns those rectangles into `sugarloaf` primitives and picks the
//! colors.
//!
//! Draw order notes for `sugarloaf`: `rect`/`line` take an `order` and
//! the quad batches are painted in ascending order (`arc` does not — it
//! always lands in the order-0 batch, *under* the terminal grid). The
//! grid uses order 3, so the chrome lives at 4 and up, and the overlays
//! (palette, search) at 20 sit above it. Icons do not go through the
//! quad batches at all: they are rasterized into coverage masks and
//! drawn with the UI text — see `draw_icon`.

pub(super) mod color;
mod drag;
mod fields;
mod forms;
mod indicator;
mod masks;
#[cfg(test)]
mod mod_tests;
mod notices;
mod settings;
mod settings_keys;
mod settings_sql;
mod surfaces;
mod text;

use rio_backend::sugarloaf::Sugarloaf;

use terminus_ui::chrome::Chrome;
use terminus_ui::theme::ChromeTheme;

use drag::{paint_host_drag_ghost, paint_host_drag_insertion_bar};
use settings::render_settings_modal;

pub(crate) use super::icons::{draw_icon, draw_os_glyph};
pub(crate) use fields::paint_rename_text;
pub(crate) use forms::paint_new_group_form;
pub(crate) use indicator::draw_orbit_indicator;
pub(crate) use notices::{paint_search_field, render_empty_hint, render_notice};
pub(crate) use surfaces::{
    paint_caret, paint_flat, paint_line, paint_surface_stroke, paint_title_strip,
};
pub(crate) use text::wrap_lines;

/// Chrome paint orders. The grid is 3 and the overlays are 20.
const ORDER_CONTENT: u8 = 7;
const ORDER_CONNECTING: u8 = 8;
const ORDER_DIALOG: u8 = 30;
/// Popovers inside the settings dialog (engine dropdown, …) — above dialog cards.
const ORDER_DIALOG_POPOVER: u8 = 31;
/// Host-drag phantom — above dialogs, rail, sticky header, everything.
const ORDER_GHOST: u8 = 50;

#[allow(dead_code)]
const DEPTH_BG: f32 = 0.05;
const DEPTH_CONTENT: f32 = 0.06;
/// Sticky drawer header (title + search) painted after the scrollable list
/// so host cards slide underneath instead of over it.
#[allow(dead_code)]
const DEPTH_STICKY: f32 = 0.085;
const DEPTH_DIALOG: f32 = 0.1;
const DEPTH_DIALOG_BG: f32 = 0.2;
const DEPTH_GHOST: f32 = 0.35;

const ADD_ICON_SIZE: f32 = 15.0;

// Whole-pixel sizes on purpose: the atlas rasterises each size bucket
// separately and the glyph quads are whole pixels wide, so an integer
// size plus an integer origin samples 1:1. A half-pixel size (10.5,
// 12.5) only ever renders as a blurrier 10 or 13.
const TITLE_SIZE: f32 = 12.0;
const ROW_TITLE_SIZE: f32 = 13.0;
/// Secondary labels / endpoints — one step smaller than the title.
const ROW_SUB_SIZE: f32 = 11.0;
#[allow(dead_code)]
const DIALOG_TITLE_SIZE: f32 = 14.0;
#[allow(dead_code)]
const CAPTION_SIZE: f32 = 11.0;
/// Mock inputs are `text-xs` (12px).
#[allow(dead_code)]
const INPUT_SIZE: f32 = 12.0;
const HINT_SIZE: f32 = 11.0;

const BORDER_WIDTH: f32 = 1.0;
#[allow(dead_code)]
const INPUT_PAD_X: f32 = 10.0;
const CARET_WIDTH: f32 = 1.5;

/// Paint the whole chrome for one frame.
///
/// `window_width` / `window_height` are logical pixels (physical size
/// divided by the scale factor), matching `terminus_ui`'s geometry;
/// `device_scale` is that same factor, needed to rasterize icons into
/// masks that land 1:1 on the device grid.
pub fn render(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    window_width: f32,
    window_height: f32,
    device_scale: f32,
) {
    // Register Sora / Martian Mono (once per library) so every `opts()`
    // below resolves to the UI faces.
    super::ui_text::sync_ui_fonts(sugarloaf);
    // The sidebar itself is painted by `renderer::shell` (under the grid
    // decorations); here only what floats above it.
    paint_modal_stack(
        sugarloaf,
        chrome,
        theme,
        window_width,
        window_height,
        device_scale,
    );

    if let Some(menu) = chrome.context_menu.as_ref() {
        sugarloaf.begin_overlay();
        super::components::overlay::paint_menu(sugarloaf, theme, menu);
        sugarloaf.end_overlay();
    }

    // Insertion bar while dragging, then ghost at max z-order.
    paint_host_drag_insertion_bar(sugarloaf, chrome, theme);
    paint_host_drag_ghost(sugarloaf, chrome, theme, device_scale);
}

/// Single Sugarloaf overlay pass for every open dialog, back → front.
///
/// Only the front-most layer emits text/icons. Lower layers paint scrim +
/// panel shells so a translucent top scrim still dimly shows the form
/// underneath — without lower-modal glyphs floating above it.
fn paint_modal_stack(
    sugarloaf: &mut Sugarloaf,
    chrome: &Chrome,
    theme: &ChromeTheme,
    window_width: f32,
    window_height: f32,
    device_scale: f32,
) {
    let stack = chrome.modal_paint_stack();
    if stack.is_empty() {
        return;
    }
    let top = stack.last().copied();
    sugarloaf.begin_overlay();
    for layer in stack {
        let paint_glyphs = top == Some(layer);
        match layer {
            terminus_ui::ModalPaintLayer::HostEditor => {
                crate::renderer::screens::add_server::render(
                    sugarloaf,
                    chrome,
                    theme,
                    window_width,
                    window_height,
                    device_scale,
                    paint_glyphs,
                );
            }
            terminus_ui::ModalPaintLayer::AddSnippet => {
                super::dialogs::snippet::paint_add_snippet(
                    sugarloaf,
                    chrome,
                    theme,
                    (window_width, window_height),
                    paint_glyphs,
                );
            }
            terminus_ui::ModalPaintLayer::Settings => {
                render_settings_modal(
                    sugarloaf,
                    chrome,
                    theme,
                    window_width,
                    window_height,
                    device_scale,
                    paint_glyphs,
                );
            }
            terminus_ui::ModalPaintLayer::Confirm => {
                super::dialogs::confirm::paint_chrome_confirm(
                    sugarloaf,
                    chrome,
                    theme,
                    (window_width, window_height),
                    paint_glyphs,
                );
            }
            terminus_ui::ModalPaintLayer::VaultUnlock => {
                super::dialogs::vault::paint_vault_unlock(
                    sugarloaf,
                    chrome,
                    theme,
                    (window_width, window_height),
                    paint_glyphs,
                );
            }
        }
    }
    sugarloaf.end_overlay();
}
