// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.

use rio_backend::sugarloaf::text::DrawOpts;
use rio_backend::sugarloaf::Sugarloaf;
use std::time::Instant;

/// Convert `[f32; 4]` colour to the `[u8; 4]` non-premul form the
/// `Text` draw path expects (the vertex shader premultiplies).
#[inline]
fn color_u8(c: [f32; 4]) -> [u8; 4] {
    [
        (c[0].clamp(0.0, 1.0) * 255.0) as u8,
        (c[1].clamp(0.0, 1.0) * 255.0) as u8,
        (c[2].clamp(0.0, 1.0) * 255.0) as u8,
        (c[3].clamp(0.0, 1.0) * 255.0) as u8,
    ]
}

// Layout
const OVERLAY_WIDTH: f32 = 320.0;
const OVERLAY_HEIGHT: f32 = 36.0;
const OVERLAY_CORNER_RADIUS: f32 = 12.0;
const OVERLAY_MARGIN_TOP: f32 = 8.0;
const OVERLAY_MARGIN_RIGHT: f32 = 8.0;
const OVERLAY_PADDING_X: f32 = 10.0;

const INPUT_FONT_SIZE: f32 = 13.0;
const BUTTON_FONT_SIZE: f32 = 14.0;

const BUTTON_SIZE: f32 = 24.0;
const BUTTON_CORNER_RADIUS: f32 = 4.0;
const BUTTON_GAP: f32 = 2.0;
const BUTTONS_AREA_WIDTH: f32 = BUTTON_SIZE * 3.0 + BUTTON_GAP * 2.0;

const CARET_BLINK_MS: u128 = 500;

// Colors — Apple HIG flat dark
const BG_COLOR: [f32; 4] = [
    0x22 as f32 / 255.0,
    0x22 as f32 / 255.0,
    0x26 as f32 / 255.0,
    1.0,
];
const INPUT_BG_COLOR: [f32; 4] = [
    0x1b as f32 / 255.0,
    0x1b as f32 / 255.0,
    0x1e as f32 / 255.0,
    1.0,
];
const TEXT_COLOR: [f32; 4] = [
    0xf1 as f32 / 255.0,
    0xf5 as f32 / 255.0,
    0xf9 as f32 / 255.0,
    1.0,
];
const DIM_TEXT_COLOR: [f32; 4] = [
    0x94 as f32 / 255.0,
    0xa3 as f32 / 255.0,
    0xb8 as f32 / 255.0,
    1.0,
];
const BUTTON_TEXT_COLOR: [f32; 4] = [
    0xcb as f32 / 255.0,
    0xd5 as f32 / 255.0,
    0xe1 as f32 / 255.0,
    1.0,
];
const BUTTON_HOVER_BG: [f32; 4] = [
    0x2a as f32 / 255.0,
    0x2a as f32 / 255.0,
    0x30 as f32 / 255.0,
    1.0,
];

// Depth / order
const DEPTH_BG: f32 = 0.1;
const DEPTH_ELEMENT: f32 = 0.2;
const ORDER: u8 = 20;

/// Actions triggered by clicking search overlay buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchOverlayAction {
    Previous,
    Next,
    Close,
}

pub struct SearchOverlay {
    active_search: Option<String>,
    caret_blink_start: Instant,
    hovered_button: Option<SearchOverlayAction>,
}

impl Default for SearchOverlay {
    fn default() -> Self {
        Self {
            active_search: None,
            caret_blink_start: Instant::now(),
            hovered_button: None,
        }
    }
}

impl SearchOverlay {
    #[inline]
    pub fn is_active(&self) -> bool {
        self.active_search.is_some()
    }

    #[inline]
    pub fn set_active_search(&mut self, active_search: Option<String>) {
        let was_active = self.active_search.is_some();
        self.active_search = active_search;
        if !was_active && self.active_search.is_some() {
            self.caret_blink_start = Instant::now();
        }
    }

    /// Returns (overlay_x, overlay_y, overlay_width, overlay_height) in logical coords.
    fn overlay_rect(&self, window_width: f32, scale_factor: f32) -> (f32, f32, f32, f32) {
        let logical_width = window_width / scale_factor;
        let x = logical_width - OVERLAY_WIDTH - OVERLAY_MARGIN_RIGHT;
        let y = OVERLAY_MARGIN_TOP;
        (x, y, OVERLAY_WIDTH, OVERLAY_HEIGHT)
    }

    /// Returns the rect for each button: (prev, next, close).
    fn button_rects(
        &self,
        overlay_x: f32,
        overlay_y: f32,
        overlay_width: f32,
        overlay_height: f32,
    ) -> [(f32, f32, f32, f32); 3] {
        let buttons_x =
            overlay_x + overlay_width - OVERLAY_PADDING_X - BUTTONS_AREA_WIDTH;
        let button_y = overlay_y + (overlay_height - BUTTON_SIZE) / 2.0;

        let prev_x = buttons_x;
        let next_x = buttons_x + BUTTON_SIZE + BUTTON_GAP;
        let close_x = buttons_x + (BUTTON_SIZE + BUTTON_GAP) * 2.0;

        [
            (prev_x, button_y, BUTTON_SIZE, BUTTON_SIZE),
            (next_x, button_y, BUTTON_SIZE, BUTTON_SIZE),
            (close_x, button_y, BUTTON_SIZE, BUTTON_SIZE),
        ]
    }

    /// Hit-test a mouse click. Returns Some(action) if a button was clicked.
    /// Returns Err(()) if clicked outside the overlay entirely.
    pub fn hit_test(
        &self,
        mouse_x: f32,
        mouse_y: f32,
        window_width: f32,
        scale_factor: f32,
    ) -> Result<Option<SearchOverlayAction>, ()> {
        if !self.is_active() {
            return Err(());
        }

        let (ox, oy, ow, oh) = self.overlay_rect(window_width, scale_factor);

        if mouse_x < ox || mouse_x > ox + ow || mouse_y < oy || mouse_y > oy + oh {
            return Err(());
        }

        let buttons = self.button_rects(ox, oy, ow, oh);
        let actions = [
            SearchOverlayAction::Previous,
            SearchOverlayAction::Next,
            SearchOverlayAction::Close,
        ];

        for (i, (bx, by, bw, bh)) in buttons.iter().enumerate() {
            if mouse_x >= *bx
                && mouse_x <= bx + bw
                && mouse_y >= *by
                && mouse_y <= by + bh
            {
                return Ok(Some(actions[i]));
            }
        }

        Ok(None) // Clicked on input area
    }

    /// Update hover state based on mouse position. Returns true if changed.
    pub fn hover(
        &mut self,
        mouse_x: f32,
        mouse_y: f32,
        window_width: f32,
        scale_factor: f32,
    ) -> bool {
        if !self.is_active() {
            return false;
        }

        let (ox, oy, ow, oh) = self.overlay_rect(window_width, scale_factor);
        let buttons = self.button_rects(ox, oy, ow, oh);
        let actions = [
            SearchOverlayAction::Previous,
            SearchOverlayAction::Next,
            SearchOverlayAction::Close,
        ];

        let mut new_hover = None;
        for (i, (bx, by, bw, bh)) in buttons.iter().enumerate() {
            if mouse_x >= *bx
                && mouse_x <= bx + bw
                && mouse_y >= *by
                && mouse_y <= by + bh
            {
                new_hover = Some(actions[i]);
                break;
            }
        }

        if new_hover != self.hovered_button {
            self.hovered_button = new_hover;
            return true;
        }
        false
    }

    pub fn render(&mut self, sugarloaf: &mut Sugarloaf, dimensions: (f32, f32, f32)) {
        if !self.is_active() {
            // Immediate mode: not drawing == not visible. No cleanup
            // needed — previous frame's instances were cleared at
            // render.
            return;
        }

        let (window_width, _window_height, scale_factor) = dimensions;

        let (ox, oy, ow, oh) = self.overlay_rect(window_width, scale_factor);

        // Background
        crate::renderer::chrome::paint_surface_stroke(
            sugarloaf,
            &terminus_ui::Rect::new(ox, oy, ow, oh),
            BG_COLOR,
            None,
            OVERLAY_CORNER_RADIUS,
            1.0,
            DEPTH_BG,
            ORDER,
            false,
        );

        // Input area background
        let input_x = ox + OVERLAY_PADDING_X;
        let input_width = ow - OVERLAY_PADDING_X * 2.0 - BUTTONS_AREA_WIDTH - 8.0;
        let input_y = oy + 6.0;
        let input_height = oh - 12.0;

        crate::renderer::chrome::paint_surface_stroke(
            sugarloaf,
            &terminus_ui::Rect::new(input_x, input_y, input_width, input_height),
            INPUT_BG_COLOR,
            None,
            4.0,
            1.0,
            DEPTH_ELEMENT,
            ORDER,
            false,
        );

        // Input text
        let active_search = self.active_search.clone().unwrap_or_default();
        let text_x = input_x + 6.0;
        let text_y = input_y + (input_height - INPUT_FONT_SIZE) / 2.0;
        let max_text_width = input_width - 12.0;

        let text_color = if active_search.is_empty() {
            DIM_TEXT_COLOR
        } else {
            TEXT_COLOR
        };

        let input_opts = DrawOpts {
            font_size: INPUT_FONT_SIZE,
            color: color_u8(text_color),
            ..DrawOpts::default()
        };

        // Determine visible text: trim from the front if it overflows.
        let display_text: String = if active_search.is_empty() {
            "Search...".to_string()
        } else {
            let chars: Vec<char> = active_search.chars().collect();
            let ui = sugarloaf.text_mut();
            let mut start = 0;
            let full_width = ui.measure(&active_search, &input_opts);
            if full_width > max_text_width {
                // Binary search for the right start index.
                let mut lo = 0;
                let mut hi = chars.len();
                while lo < hi {
                    let mid = (lo + hi) / 2;
                    let substr: String = chars[mid..].iter().collect();
                    let w = ui.measure(&substr, &input_opts);
                    if w > max_text_width {
                        lo = mid + 1;
                    } else {
                        hi = mid;
                    }
                }
                start = lo;
            }
            chars[start..].iter().collect()
        };

        let rendered_width =
            sugarloaf
                .text_mut()
                .draw(text_x, text_y, &display_text, &input_opts);

        // Caret
        let elapsed_ms = self.caret_blink_start.elapsed().as_millis();
        let caret_visible = (elapsed_ms / CARET_BLINK_MS).is_multiple_of(2);

        if caret_visible {
            let caret_x = if active_search.is_empty() {
                text_x
            } else {
                text_x + rendered_width
            };
            let caret_y = input_y + (input_height - INPUT_FONT_SIZE) / 2.0 + 1.0;

            crate::renderer::chrome::paint_caret(
                sugarloaf,
                caret_x,
                caret_y,
                INPUT_FONT_SIZE,
                TEXT_COLOR,
                DEPTH_ELEMENT,
                ORDER,
            );
        }

        // Buttons: prev (↑), next (↓), close (✕)
        let button_rects = self.button_rects(ox, oy, ow, oh);
        let labels = ["\u{2191}", "\u{2193}", "\u{2022}"];
        let actions = [
            SearchOverlayAction::Previous,
            SearchOverlayAction::Next,
            SearchOverlayAction::Close,
        ];

        let btn_opts = DrawOpts {
            font_size: BUTTON_FONT_SIZE,
            color: color_u8(BUTTON_TEXT_COLOR),
            ..DrawOpts::default()
        };

        for (i, (bx, by, bw, bh)) in button_rects.iter().enumerate() {
            let is_hovered = self.hovered_button == Some(actions[i]);

            if is_hovered {
                crate::renderer::chrome::paint_surface_stroke(
                    sugarloaf,
                    &terminus_ui::Rect::new(*bx, *by, *bw, *bh),
                    BUTTON_HOVER_BG,
                    None,
                    BUTTON_CORNER_RADIUS,
                    1.0,
                    DEPTH_ELEMENT,
                    ORDER,
                    false,
                );
            }

            // Measure first so we can centre horizontally — the
            // labels are single glyphs of varying width (arrows vs
            // bullet).
            let ui = sugarloaf.text_mut();
            let label_w = ui.measure(labels[i], &btn_opts);
            let label_x = bx + (bw - label_w) / 2.0;
            let label_y = by + (bh - BUTTON_FONT_SIZE) / 2.0;
            ui.draw(label_x, label_y, labels[i], &btn_opts);
        }
    }
}
