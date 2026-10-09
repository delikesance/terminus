use super::*;

impl Island {
    /// Render the color picker dropdown below a tab
    pub(super) fn render_color_picker(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        tab_x: f32,
        tab_width: f32,
        selected_color: Option<[f32; 4]>,
    ) {
        let padding = PICKER_PADDING;
        let bg_y = ISLAND_HEIGHT;

        // Compute total swatches width to derive the consistent inner content width
        // N color swatches + 1 reset swatch
        let slot_count = PICKER_COLORS.len() + 1;
        let total_swatches_width = slot_count as f32 * PICKER_SWATCH_SIZE
            + (slot_count - 1) as f32 * PICKER_SWATCH_GAP;
        let inner_width = total_swatches_width;
        let bg_width = inner_width + padding * 2.0;
        let bg_x = tab_x + (tab_width - bg_width) / 2.0;
        let content_x = bg_x + padding;

        // Background
        crate::renderer::chrome::paint_surface_stroke(
            sugarloaf,
            &terminus_ui::Rect::new(bg_x, bg_y, bg_width, PICKER_HEIGHT),
            [0.15, 0.15, 0.15, 1.0],
            None,
            4.0,
            1.0,
            0.0,
            10,
            false,
        );

        // Swatches — aligned to content_x
        let swatch_y = bg_y + padding + PICKER_TOP_PADDING;
        for (i, color) in PICKER_COLORS.iter().enumerate() {
            let sx = content_x + i as f32 * (PICKER_SWATCH_SIZE + PICKER_SWATCH_GAP);
            let is_selected = selected_color == Some(*color);

            if is_selected {
                let border = 2.0;
                crate::renderer::chrome::paint_surface_stroke(
                    sugarloaf,
                    &terminus_ui::Rect::new(
                        sx - border,
                        swatch_y - border,
                        PICKER_SWATCH_SIZE + border * 2.0,
                        PICKER_SWATCH_SIZE + border * 2.0,
                    ),
                    *color,
                    Some([1.0, 1.0, 1.0, 1.0]),
                    4.0,
                    border,
                    0.0,
                    10,
                    false,
                );
            } else {
                crate::renderer::chrome::paint_surface_stroke(
                    sugarloaf,
                    &terminus_ui::Rect::new(
                        sx,
                        swatch_y,
                        PICKER_SWATCH_SIZE,
                        PICKER_SWATCH_SIZE,
                    ),
                    *color,
                    None,
                    3.0,
                    1.0,
                    0.0,
                    10,
                    false,
                );
            }
        }

        // Reset swatch — neutral box with a diagonal slash, selected when no color is set
        let reset_x = content_x
            + PICKER_COLORS.len() as f32 * (PICKER_SWATCH_SIZE + PICKER_SWATCH_GAP);
        let reset_selected = selected_color.is_none();
        let reset_fill = [0.22, 0.22, 0.22, 1.0];
        if reset_selected {
            let border = 2.0;
            crate::renderer::chrome::paint_surface_stroke(
                sugarloaf,
                &terminus_ui::Rect::new(
                    reset_x - border,
                    swatch_y - border,
                    PICKER_SWATCH_SIZE + border * 2.0,
                    PICKER_SWATCH_SIZE + border * 2.0,
                ),
                reset_fill,
                Some([1.0, 1.0, 1.0, 1.0]),
                4.0,
                border,
                0.0,
                10,
                false,
            );
        } else {
            crate::renderer::chrome::paint_surface_stroke(
                sugarloaf,
                &terminus_ui::Rect::new(
                    reset_x,
                    swatch_y,
                    PICKER_SWATCH_SIZE,
                    PICKER_SWATCH_SIZE,
                ),
                reset_fill,
                None,
                3.0,
                1.0,
                0.0,
                10,
                false,
            );
        }
        let slash_inset = 3.0;
        crate::renderer::chrome::paint_line(
            sugarloaf,
            reset_x + slash_inset,
            swatch_y + PICKER_SWATCH_SIZE - slash_inset,
            reset_x + PICKER_SWATCH_SIZE - slash_inset,
            swatch_y + slash_inset,
            1.5,
            [0.86, 0.26, 0.27, 1.0],
            0.0,
            10,
        );

        // Rename text input — same left/right edge as swatches
        let input_y = swatch_y + PICKER_SWATCH_SIZE + PICKER_INPUT_MARGIN_TOP;
        let input_x = content_x;
        let input_width = inner_width;

        // Input background
        crate::renderer::chrome::paint_surface_stroke(
            sugarloaf,
            &terminus_ui::Rect::new(input_x, input_y, input_width, PICKER_INPUT_HEIGHT),
            [0.10, 0.10, 0.10, 1.0],
            None,
            3.0,
            1.0,
            0.0,
            10,
            false,
        );

        let text_inset = 6.0;
        let text_x = input_x + text_inset;
        let max_text_width = input_width - text_inset * 2.0;
        let text_y = input_y + (PICKER_INPUT_HEIGHT - PICKER_INPUT_FONT_SIZE) / 2.0;

        let text_color = if self.rename_input.value.is_empty() {
            [0.45, 0.45, 0.45, 1.0]
        } else {
            [0.93, 0.93, 0.93, 1.0]
        };
        let rename_opts = DrawOpts {
            font_size: PICKER_INPUT_FONT_SIZE,
            color: color_u8(text_color),
            ..DrawOpts::default()
        };

        // Determine visible text: trim from the front if it overflows.
        let mut caret_prefix = String::new();
        let display_text: String = if self.rename_input.value.is_empty() {
            "Tab title...".to_string()
        } else {
            let input = self.rename_input.value.as_str();
            let chars: Vec<char> = input.chars().collect();
            let ui = sugarloaf.text_mut();
            let mut start = 0;
            let full_width = ui.measure(input, &rename_opts);
            if full_width > max_text_width {
                let mut lo = 0;
                let mut hi = chars.len();
                while lo < hi {
                    let mid = (lo + hi) / 2;
                    let substr: String = chars[mid..].iter().collect();
                    let w = ui.measure(&substr, &rename_opts);
                    if w > max_text_width {
                        lo = mid + 1;
                    } else {
                        hi = mid;
                    }
                }
                start = lo;
            }
            let caret = self.rename_input.caret.min(chars.len());
            caret_prefix = chars[start.min(caret)..caret].iter().collect();
            chars[start..].iter().collect()
        };

        sugarloaf
            .text_mut()
            .draw(text_x, text_y, &display_text, &rename_opts);
        // The caret follows the editing position, not the end of the text.
        let rendered_width = if caret_prefix.is_empty() {
            0.0
        } else {
            sugarloaf.text_mut().measure(&caret_prefix, &rename_opts)
        };

        // Blinking caret
        let elapsed = self.rename_caret_time.elapsed().as_millis();
        let show_caret = (elapsed / 500).is_multiple_of(2);
        if show_caret {
            let caret_x = text_x + rendered_width;
            if caret_x <= input_x + input_width {
                crate::renderer::chrome::paint_caret(
                    sugarloaf,
                    caret_x,
                    input_y + 4.0,
                    PICKER_INPUT_HEIGHT - 8.0,
                    [0.93, 0.93, 0.93, 1.0],
                    0.0,
                    10,
                );
            }
        }
    }
}
