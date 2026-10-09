use super::*;

impl Island {
    /// Render tabs using equal-width layout
    #[inline]
    pub fn render(
        &mut self,
        sugarloaf: &mut Sugarloaf,
        dimensions: (f32, f32, f32),
        context_manager: &ContextManager<EventProxy>,
        _bg_color: [f32; 4],
        #[cfg_attr(not(target_os = "windows"), allow(unused_variables))]
        window_maximized: bool,
    ) {
        let (window_width, _window_height, scale_factor) = dimensions;
        let num_tabs = context_manager.len();
        let current_tab_index = context_manager.current_index();
        let logical_w = window_width / scale_factor;

        // Apple HIG title bar strip (#111113) + bottom hairline (#2f2f35).
        // Strip under the pills (order 0). Pills / close / dots sit above
        // it — painting pills at 0 left them invisible under this rect,
        // so hover lift and × never showed (only the floating drag tab
        // at order 11 did).
        let strip = [
            0x11 as f32 / 255.0,
            0x11 as f32 / 255.0,
            0x13 as f32 / 255.0,
            1.0,
        ];
        crate::renderer::chrome::paint_title_strip(sugarloaf, logical_w, ISLAND_HEIGHT);

        // Immediate-mode: no cached ids to hide. If we early-return
        // without drawing, the tabs just don't appear this frame.

        // A lone tab cannot be reordered. A drag can only start with two
        // or more tabs, but one can outlive the second tab (its shell
        // exits mid-drag) — drop the drag so we don't float a phantom.
        if num_tabs == 1 {
            self.drag = None;
            self.slide_springs.clear();
        }

        // A reorder that didn't come from this drag (tab closed via
        // shell exit, keyboard move) breaks the drag.tab_index ==
        // current_index invariant — drop the drag instead of floating
        // a phantom tab over the wrong slot.
        if self
            .drag
            .as_ref()
            .is_some_and(|d| d.tab_index != current_tab_index)
        {
            self.drag = None;
        }

        // Advance the slide springs (drag-reorder animation) by this
        // frame's dt; settled springs drop out of the map.
        let now = Instant::now();
        let dt = now
            .duration_since(self.last_anim_frame)
            .as_secs_f32()
            .min(DRAG_MAX_DT);
        self.last_anim_frame = now;
        self.slide_springs
            .retain(|_, s| s.update(dt, DRAG_ANIMATION_LENGTH));

        // Measure each tab's content, then hug — left-aligned pills.
        let measure_opts = DrawOpts {
            font_size: TITLE_FONT_SIZE,
            ..DrawOpts::default()
        };
        let mut natural: SmallVec<[f32; 12]> = SmallVec::with_capacity(num_tabs);
        for tab_index in 0..num_tabs {
            let raw_title = self.get_title_for_tab(context_manager, tab_index);
            let text_w = if raw_title.is_empty() {
                0.0
            } else {
                sugarloaf.text_mut().measure(&raw_title, &measure_opts)
            };
            let has_icon = terminus_ui::OsGlyph::from_hint(
                context_manager.tab_os_id(tab_index),
                &raw_title,
            )
            .has_mark();
            let closable = !context_manager.is_pinned(tab_index);
            natural.push(tab_slot_width_for_content(text_w, has_icon, closable));
        }
        let layout = tab_strip_layout_from_widths(
            window_width,
            scale_factor,
            self.max_tab_width,
            &natural,
        );
        self.layout_cache = layout.clone();
        let left_margin = layout.left_margin;

        // Starting from left edge (with margin on macOS for traffic lights)
        let mut x_position = left_margin;

        // Active drag: the dragged tab is skipped in the slot loop and
        // drawn floating (after the loop, on a higher layer) instead.
        let drag_index = self.drag_index();
        let floating_left = self.drag_floating_left(&layout);

        // Adaptive island fills from Apple HIG strip (not terminal bg).
        let fills = island_fills(strip);

        // Render each tab
        for tab_index in 0..num_tabs {
            let tab_width = layout.width_at(tab_index);
            // The dragged tab floats — drawn after the loop instead.
            if Some(tab_index) == drag_index {
                x_position += tab_width;
                continue;
            }

            let is_active = tab_index == current_tab_index;

            // Slot position plus any slide-spring offset (tab still
            // animating into its slot after a reorder).
            let tab_x = x_position
                + self
                    .slide_springs
                    .get(&tab_index)
                    .map_or(0.0, |s| s.position);

            // Get title for this tab, then truncate with a trailing
            // ellipsis so overflowing titles can't bleed into the next
            // tab or past the left edge (issue #1508).
            let raw_title = self.get_title_for_tab(context_manager, tab_index);
            if raw_title.is_empty() {
                x_position += tab_width;
                continue;
            }

            let closable = !context_manager.is_pinned(tab_index);
            let close_budget = if closable { CLOSE_RESERVE } else { 0.0 };
            let glyph = terminus_ui::OsGlyph::from_hint(
                context_manager.tab_os_id(tab_index),
                &raw_title,
            );
            let icon_slot = if glyph.has_mark() {
                TITLEBAR_ICON + 4.0
            } else {
                0.0
            };
            let max_text_width = (tab_width
                - TAB_GAP
                - TAB_PADDING_X * 2.0
                - close_budget
                - STATUS_DOT
                - STATUS_GAP
                - icon_slot)
                .max(0.0);
            let title = fit_title_to_width(sugarloaf, &raw_title, max_text_width);

            let text_color = if is_active {
                self.active_text_color
            } else {
                self.inactive_text_color
            };

            let title_opts = DrawOpts {
                font_size: TITLE_FONT_SIZE,
                color: color_u8(text_color),
                ..DrawOpts::default()
            };

            // UI text always paints in a final pass above every rect,
            // so the floating tab's opaque background can't occlude
            // titles passing underneath it — skip a title once the
            // floating tab intrudes past the slot's text padding.
            let drag_w = drag_index.map(|i| layout.width_at(i)).unwrap_or(tab_width);
            let hidden_by_drag = floating_left.is_some_and(|fl| {
                let overlap = (tab_x + tab_width).min(fl + drag_w) - tab_x.max(fl);
                overlap > TAB_PADDING_X
            });

            // Pill fill first (above strip), then chrome / label on top.
            let (ix, iy, iw, ih, radius) = island_rect(tab_x, tab_width);
            let fill = match context_manager.custom_color(tab_index) {
                Some(mut custom) => {
                    if !is_active {
                        custom[3] *= INACTIVE_CUSTOM_MUTE;
                    }
                    if self.hovered_tab == Some(tab_index) {
                        custom[0] = (custom[0] + 0.05).min(1.0);
                        custom[1] = (custom[1] + 0.05).min(1.0);
                        custom[2] = (custom[2] + 0.05).min(1.0);
                    }
                    custom
                }
                None => {
                    let hovered = self.hovered_tab == Some(tab_index);
                    if is_active {
                        if hovered {
                            [
                                (fills.active[0] + 0.05).min(1.0),
                                (fills.active[1] + 0.05).min(1.0),
                                (fills.active[2] + 0.05).min(1.0),
                                fills.active[3],
                            ]
                        } else {
                            fills.active
                        }
                    } else if hovered {
                        [
                            (fills.inactive[0] + 0.08).min(1.0),
                            (fills.inactive[1] + 0.08).min(1.0),
                            (fills.inactive[2] + 0.08).min(1.0),
                            fills.inactive[3],
                        ]
                    } else {
                        fills.inactive
                    }
                }
            };
            // Above the strip (0); below chrome rail (4+) and floating drag (11).
            draw_island(
                sugarloaf,
                ix,
                iy,
                iw,
                ih,
                radius,
                fill,
                fills.outline,
                Some(strip),
                2,
            );

            // Close × only on hover of a closable (non-pinned) tab.
            let show_close = self.hovered_tab == Some(tab_index)
                && !context_manager.is_pinned(tab_index);
            if show_close {
                if let Some(cx) = close_button_center(ix, iw) {
                    if self.close_hover {
                        crate::renderer::chrome::paint_surface_stroke(
                            sugarloaf,
                            &terminus_ui::Rect::new(
                                cx - CLOSE_HOVER_HALF,
                                ISLAND_HEIGHT / 2.0 - CLOSE_HOVER_HALF,
                                CLOSE_HOVER_HALF * 2.0,
                                CLOSE_HOVER_HALF * 2.0,
                            ),
                            fills.close_hover,
                            None,
                            CLOSE_HOVER_CORNER_RADIUS,
                            1.0,
                            0.05,
                            3,
                            false,
                        );
                    }
                    draw_close_button(
                        sugarloaf,
                        cx,
                        if is_active {
                            self.active_text_color
                        } else {
                            self.inactive_text_color
                        },
                        self.close_hover,
                        scale_factor,
                    );
                }
            }

            if !hidden_by_drag {
                let group_x = tab_x + TAB_GAP / 2.0 + TAB_PADDING_X;
                let text_y = (ISLAND_HEIGHT / 2.0) - (TITLE_FONT_SIZE / 2.);
                let dot_y = (ISLAND_HEIGHT - STATUS_DOT) / 2.0;
                crate::renderer::chrome::paint_surface_stroke(
                    sugarloaf,
                    &terminus_ui::Rect::new(group_x, dot_y, STATUS_DOT, STATUS_DOT),
                    [0.20, 0.83, 0.60, 1.0],
                    None,
                    STATUS_DOT / 2.0,
                    1.0,
                    0.06,
                    5,
                    false,
                );
                let icon_x = group_x + STATUS_DOT + STATUS_GAP;
                let text_x = icon_x + icon_slot;
                if glyph.has_mark() {
                    use crate::renderer::chrome;
                    use terminus_ui::icons::IconPlacement;
                    let iy = (ISLAND_HEIGHT - TITLEBAR_ICON) / 2.0;
                    chrome::draw_os_glyph(
                        sugarloaf,
                        glyph,
                        IconPlacement::new(icon_x, iy, TITLEBAR_ICON),
                        glyph.color(),
                        scale_factor,
                    );
                }
                sugarloaf
                    .text_mut()
                    .draw(text_x, text_y, &title, &title_opts);
            }

            x_position += tab_width;
        }

        // Draw the floating (dragged) tab above the slot tabs.
        if let (Some(drag_idx), Some(floating_x)) = (drag_index, floating_left) {
            let tab_width = layout.width_at(drag_idx);
            let (ix, iy, iw, ih, radius) = island_rect(floating_x, tab_width);

            // Soft elevation: a slightly inflated dark halo behind the
            // lifted island so it reads as floating over the strip.
            crate::renderer::chrome::paint_surface_stroke(
                sugarloaf,
                &terminus_ui::Rect::new(ix - 2.0, iy - 1.0, iw + 4.0, ih + 3.0),
                [0.0, 0.0, 0.0, 0.18],
                None,
                radius + 2.0,
                1.0,
                0.05,
                11,
                false,
            );

            let fill = match context_manager.custom_color(drag_idx) {
                Some(mut custom) => {
                    custom[3] = 1.0;
                    custom
                }
                None => fills.active,
            };
            draw_island(
                sugarloaf,
                ix,
                iy,
                iw,
                ih,
                radius,
                fill,
                fills.outline,
                None,
                11,
            );

            if let Some(cx) = close_button_center(ix, iw) {
                draw_close_button(
                    sugarloaf,
                    cx,
                    self.active_text_color,
                    false,
                    scale_factor,
                );
            }

            let raw_title = self.get_title_for_tab(context_manager, drag_idx);
            if !raw_title.is_empty() {
                let max_text_width = (tab_width
                    - TAB_GAP
                    - TAB_PADDING_X
                    - CLOSE_MARGIN_RIGHT
                    - CLOSE_HIT_HALF_WIDTH)
                    .max(0.0);
                let title = fit_title_to_width(sugarloaf, &raw_title, max_text_width);
                let title_opts = DrawOpts {
                    font_size: TITLE_FONT_SIZE,
                    color: color_u8(self.active_text_color),
                    ..DrawOpts::default()
                };
                let ui = sugarloaf.text_mut();
                let text_y = (ISLAND_HEIGHT / 2.0) - (TITLE_FONT_SIZE / 2.);
                let text_x = floating_x + TAB_GAP / 2.0 + TAB_PADDING_X;
                ui.draw(text_x, text_y, &title, &title_opts);
            }
        }

        // Render color picker if open
        if let Some(picker_tab) = self.color_picker_tab {
            if picker_tab < num_tabs {
                let picker_tab_x = layout.slot_x(picker_tab);
                let tab_width = layout.width_at(picker_tab);
                let selected = context_manager.custom_color(picker_tab);
                self.render_color_picker(sugarloaf, picker_tab_x, tab_width, selected);
            }
        }

        // Render the progress bar below the island
        self.render_progress_bar(sugarloaf, window_width, scale_factor, ISLAND_HEIGHT);

        let logical_w = window_width / scale_factor;
        render_title_bar_chrome(
            sugarloaf,
            logical_w,
            scale_factor,
            self.active_text_color,
        );

        #[cfg(target_os = "windows")]
        {
            crate::renderer::window_controls::render(
                sugarloaf,
                logical_w,
                scale_factor,
                window_maximized,
                self.window_control_hover,
                self.active_text_color,
            );
        }
    }
}
