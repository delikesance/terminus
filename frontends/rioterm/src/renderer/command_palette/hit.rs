use super::*;

impl CommandPalette {
    /// Query to offer "Add server" for: nothing matched but something was typed.
    pub fn add_server_query(&self) -> Option<String> {
        let q = self.query.value.trim();
        // A `>` query is an operation, never a server name to add.
        if self.op_query() != terminus_ui::palette_query::Query::Plain {
            return None;
        }
        (!q.is_empty() && self.filtered_rows().is_empty()).then(|| q.to_string())
    }

    /// Hit-test a click against the last painted frame (logical pixels).
    /// `Err(())`: outside the panel (close). `Ok(Some(i))`: result row `i`.
    pub fn hit_test(
        &self,
        mouse_x: f32,
        mouse_y: f32,
        _window_width: f32,
        _scale_factor: f32,
    ) -> Result<Option<usize>, ()> {
        use terminus_ui::components::overlay::PaletteHit;
        let Some(layout) = self.last_layout.as_ref() else {
            return Ok(None);
        };
        match layout.hit_test(mouse_x, mouse_y) {
            PaletteHit::Outside => Err(()),
            PaletteHit::Item(rel) => {
                let index = self.scroll_offset + rel;
                Ok((index < self.filtered_rows().len()).then_some(index))
            }
            PaletteHit::AddServer | PaletteHit::Inside => Ok(None),
        }
    }

    /// Whether the click landed on the empty-state "Add server" line.
    pub fn add_server_hit(&self, mouse_x: f32, mouse_y: f32) -> bool {
        use terminus_ui::components::overlay::PaletteHit;
        self.add_server_query().is_some()
            && self
                .last_layout
                .as_ref()
                .is_some_and(|l| l.hit_test(mouse_x, mouse_y) == PaletteHit::AddServer)
    }

    /// Update selection based on mouse position. Returns true if selection changed.
    pub fn hover(
        &mut self,
        mouse_x: f32,
        mouse_y: f32,
        window_width: f32,
        scale_factor: f32,
    ) -> bool {
        if let Ok(Some(index)) =
            self.hit_test(mouse_x, mouse_y, window_width, scale_factor)
        {
            if self.selected_index != index {
                self.selected_index = index;
                return true;
            }
        }
        false
    }

    pub fn render(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        theme: &terminus_ui::theme::ChromeTheme,
        dimensions: (f32, f32, f32),
    ) {
        use crate::renderer::components::overlay::paint_palette;
        use crate::renderer::ui_text::{measure_ui_text, UiWeight};
        use terminus_ui::components::overlay as ov;

        if !self.enabled {
            // Immediate mode: not drawing == not visible.
            self.last_layout = None;
            return;
        }

        let (window_width, window_height, scale_factor) = dimensions;
        let window = (window_width / scale_factor, window_height / scale_factor);
        let palette = self.view();
        let layout = palette.layout(window);

        sugarloaf.begin_overlay();
        crate::renderer::chrome::paint_flat(
            sugarloaf,
            &terminus_ui::Rect::new(0.0, 0.0, window.0, window.1),
            ov::SCRIM,
            0.08,
            30,
        );
        paint_palette(sugarloaf, theme, &palette, &layout);

        let elapsed_ms = self.caret_blink_start.elapsed().as_millis();
        if (elapsed_ms / CARET_BLINK_MS).is_multiple_of(2) {
            let q = &layout.query;
            let text_x = q.x + ov::PALETTE_QUERY_PAD_X + ov::PALETTE_QUERY_ICON + 12.0;
            let prefix = self.query.prefix();
            let w = if prefix.is_empty() {
                0.0
            } else {
                measure_ui_text(sugarloaf, &prefix, 19.0, UiWeight::Regular)
            };
            let h = (19.0_f32 * 1.25).round();
            crate::renderer::chrome::paint_flat(
                sugarloaf,
                &terminus_ui::Rect::new(text_x + w, q.y + (q.height - h) / 2.0, 1.5, h),
                theme.accent,
                0.16,
                30,
            );
        }
        sugarloaf.end_overlay();
        self.last_layout = Some(layout);
    }
}
