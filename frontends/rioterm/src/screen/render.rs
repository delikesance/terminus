//! `Screen` render surface, split out of `screen/mod.rs`.

use super::Screen;
use crate::context;
use crate::context::renderable::Cursor;
use crate::crosswords::pos::Pos;
use crate::layout::ContextDimension;
use crate::renderer::Renderer;
use crate::screen::hint::HintMatches;
use crate::selection::Selection;
use rio_backend::crosswords::pos::Line;

impl Screen<'_> {
    pub(crate) fn render(&mut self) -> Option<crate::context::renderable::WindowUpdate> {
        // Host-list answers from the worker thread land here, at the top
        // of the frame, so the painter below always sees this frame's
        // list rather than the previous one.
        if self.pump_chrome() {
            self.mark_dirty();
        }

        self.update_close_button_hover(self.mouse.x, self.mouse.y);

        let is_search_active = self.search_active();
        if is_search_active {
            if let Some(history_index) = self.search_state.history_index {
                self.renderer.set_active_search(
                    self.search_state.history.get(history_index).cloned(),
                );
            }
        } else {
            self.renderer.set_active_search(None);
        }

        if is_search_active {
            // Update search hints in renderable content
            let terminal = self.context_manager.current().terminal.lock();
            let hints = self
                .search_state
                .dfas_mut()
                .map(|dfas| HintMatches::visible_regex_matches(&terminal, dfas));
            drop(terminal);

            self.context_manager
                .current_mut()
                .renderable_content
                .hint_matches = hints.map(|h| h.iter().cloned().collect());

            // Force invalidation for search with full damage
            {
                let current = self.context_manager.current_mut();
                current
                    .renderable_content
                    .pending_update
                    .set_terminal_damage(rio_backend::event::TerminalDamage::Full);
            }
        }

        self.tick_session_connecting();
        let host_drag_action = self.tick_host_drag_animation();
        if let Some(action) = host_drag_action {
            self.apply_host_drag_action(action);
        }
        let connecting_phase = self.connecting_phase();

        let sftp_paint = self.sftp.as_ref().and_then(|session| {
            self.sftp_bounds().map(|bounds| (&session.state, bounds))
        });

        let (window_update, any_panel_dirty) = self.renderer.run(
            &mut self.sugarloaf,
            &mut self.context_manager,
            &self.chrome,
            connecting_phase,
            self.window_maximized,
            sftp_paint,
        );

        if self.renderer.custom_mouse_cursor {
            let scale = self.sugarloaf.scale_factor();
            crate::renderer::custom_cursor::draw(
                &mut self.sugarloaf,
                self.mouse.x as f32,
                self.mouse.y as f32,
                scale,
            );
        }

        if self.renderer.trail_cursor_enabled {
            let current_grid = self.context_manager.current_grid();
            let scaled_margin = current_grid.get_scaled_margin();

            if let Some(current_item) = current_grid.current_item() {
                let layout = current_item.val.dimension;
                // Canonical integer stride — same value the GPU
                // shader uses; line_height is already baked in.
                let cell_width = layout.cell.cell_width as f32;
                let cell_height = layout.cell.cell_height as f32;
                let scale_factor = self.sugarloaf.scale_factor();

                let panel_rect = current_item.layout_rect;
                let origin_x = panel_rect[0] + scaled_margin.left;
                let origin_y = panel_rect[1] + scaled_margin.top;

                let current = self.context_manager.current();
                let cursor = &current.renderable_content.cursor;
                // Vi mode reports the cursor in scroll-adjusted viewport
                // rows; today's clamps keep it non-negative, but a
                // negative Line wrapping through `as usize` would fling
                // the trail target off by ~10^18 px, so clamp first.
                let cursor_row = cursor.state.pos.row.0.max(0) as usize;
                let cursor_col = cursor.state.pos.col.0;

                // Cursor position in physical pixels.
                let cursor_px_x = origin_x + cursor_col as f32 * cell_width;
                let cursor_px_y = origin_y + cursor_row as f32 * cell_height;

                self.renderer.trail_cursor.update(
                    cursor_px_x,
                    cursor_px_y,
                    cell_width,
                    cell_height,
                    cursor.state.content,
                    cursor.state.is_visible(),
                    current.route_id,
                );

                let cursor_color = self.renderer.named_colors.cursor;
                self.renderer.trail_cursor.draw(
                    &mut self.sugarloaf,
                    scale_factor,
                    cursor_color,
                );
            }
        }

        // Animation state is read after the trail advanced: a cursor
        // movement can start animating in this same frame, and reading
        // it earlier would fail to schedule the continuation frame,
        // freezing the trail mid-flight until unrelated damage arrives.
        let has_animation = self.renderer.needs_redraw()
            || self.chrome.connection.is_some()
            || self.chrome.panel.connecting_id.is_some()
            || self.chrome.needs_animation_frames();
        let should_present = any_panel_dirty || has_animation;

        // Phase 2.2/2.3: per-panel CellBg + CellText emission with
        // per-row dirty gating. Iterates every panel in the active
        // grid. For each:
        // - `damage == Noop | CursorOnly` + grid not forcing full:
        // skip `write_row` entirely. Cursor state is carried
        // by `GridUniforms`, so a pure blink/move doesn't
        // touch the cell buffers.
        // - `damage == Full` | first-frame | resize:
        // rebuild every visible row.
        // - `damage == Partial(lines)`:
        // rebuild only those rows.
        // Unchanged rows keep their CellBg + CellText resident in
        // the grid's CPU state, which is re-uploaded verbatim.
        {
            struct PanelFrame {
                route_id: usize,
                layout_rect: [f32; 4],
                cols: u32,
                rows: u32,
                cell_w: f32,
                cell_h: f32,
                font_px: f32,
                visible_rows: Vec<
                    rio_backend::crosswords::grid::row::Row<
                        rio_backend::crosswords::square::Square,
                    >,
                >,
                row_styles: Vec<Vec<rio_backend::crosswords::style::Style>>,
                /// Snapshot of the grid's extras table — needed to hash
                /// per-cell zero-width combining codepoints into the run
                /// shape key so cells with the same base codepoint but
                /// different combining marks don't alias in the cache.
                extras:
                    rustc_hash::FxHashMap<u16, rio_backend::crosswords::square::Extras>,
                term_colors: rio_backend::config::colors::term::TermColors,
                cursor_col: u16,
                cursor_row: u16,
                cursor_visible: bool,
                /// Terminal-side cursor shape (block / underline /
                /// beam / hidden). Driven by DECSCUSR + the
                /// configured default. Mapped to a render style
                /// inside the rebuild loop.
                cursor_shape: rio_backend::ansi::CursorShape,
                /// `true` when the terminal has cursor blink
                /// enabled (DECTCEM blink mode or SGR cursor blink).
                cursor_blinking: bool,
                /// `true` for the visible half of the blink cycle.
                /// Always `true` when blink isn't enabled. Driven
                /// by `Renderer::run`'s blink toggler.
                cursor_blink_visible: bool,
                /// `true` while an IME pre-edit string is active —
                /// forces a block cursor regardless of the
                /// configured shape so the user can tell IME is
                /// taking input.
                cursor_preedit: bool,
                /// Resolved cursor color: OSC 12 wins, then config /
                /// theme `cursor`.
                /// `state.colors.cursor → config.cursor_color`
                /// resolution. Per-panel
                /// because each terminal can issue its own OSC 12.
                cursor_color: rio_backend::config::colors::ColorArray,
                is_active: bool,
                damage: rio_backend::event::TerminalDamage,
                /// Selection is per-context (`renderable_content`), not
                /// per-terminal. Grabbed alongside the grid snapshot so
                /// `build_row_bg`/`build_row_fg` can tint selected cells.
                selection: Option<rio_backend::selection::SelectionRange>,
                /// `i - display_offset = absolute Line` for the
                /// per-row selection interval check. Snapshotted at
                /// the same lock as `visible_rows` to stay consistent.
                display_offset: i32,
                /// Search-hint matches for this panel. `None` when
                /// search is inactive. Consumed alongside `selection`
                /// inside `build_row_bg` / `build_row_fg` to apply
                /// `search_match_background` / `_foreground`.
                hint_matches: Option<Vec<rio_backend::crosswords::search::Match>>,
                /// Currently-focused search match (↑/↓ navigation).
                /// Rendered with `search_focused_match_background` /
                /// `_foreground` — `.search_selected`
                /// highlight tag.
                focused_match: Option<rio_backend::crosswords::search::Match>,
                /// (start, end) of the currently-hovered hyperlink /
                /// regex hint. Only populated for the active panel.
                /// Triggers the forced underline in `emit_underlines`;
                /// no bg / fg color change.
                hovered_hyperlink: Option<(
                    rio_backend::crosswords::pos::Pos,
                    rio_backend::crosswords::pos::Pos,
                )>,
                hint_labels: Option<Vec<crate::context::renderable::HintLabel>>,
                /// Active IME composition, laid out on the cursor row.
                /// Only ever `Some` for the active panel with an
                /// unscrolled viewport: the composition belongs to the
                /// focused context, and a scrolled viewport has no
                /// on-screen cursor row to anchor it to.
                preedit_line: Option<rio_grid::preedit::PreeditLine>,
            }

            let (active_key, scaled_margin) = {
                let grid = self.context_manager.current_grid();
                (grid.current, grid.scaled_margin)
            };
            // Snapshot the window's focused search match before the
            // per-context borrow below. `search_state` lives on
            // `Screen`, so we can't reach for it from inside the
            // `contexts_mut` iteration.
            let search_focused_match = self.search_state.focused_match.clone();
            let mut panels: Vec<PanelFrame> = Vec::new();
            for (key, item) in self
                .context_manager
                .current_grid_mut()
                .contexts_mut()
                .iter_mut()
            {
                let ctx = &mut item.val;
                let dim = ctx.dimension;
                // Canonical integer cell stride — single source of
                // truth for paint, layout, and mouse hit-test. The
                // bg fragment shader does
                // `floor((pixel - padding) / cell_size)` and the text
                // vertex multiplies `grid_pos * cell_size`, so both
                // sides must agree on the same integer stride or
                // adjacent columns drift to 7 vs 8 px wide and seams
                // show up.
                let cell_w = dim.cell.cell_width as f32;
                let cell_h = dim.cell.cell_height as f32;
                // Per-panel font size lives on `ContextDimension` since
                // the panel-state migration; sugarloaf is no longer
                // consulted. Per-panel zoom mutates
                // `dim.scaled_font_size` directly.
                let font_px = if dim.scaled_font_size > 0.0 {
                    dim.scaled_font_size
                } else {
                    let s = self.sugarloaf.style();
                    s.font_size * s.scale_factor
                };
                // The viewport snapshot was already taken by
                // `Renderer::run` for this context: visible rows,
                // per-cell styles, extras table, term colors, and
                // display offset all live on `ctx.renderable_content`.
                // No second terminal lock and no second materialize —
                // we take ownership of the buffers via `mem::take`
                // and put them back at the end of the render pass so
                // the next frame's `Renderer::run` resumes the same
                // allocations.
                let visible_rows =
                    std::mem::take(&mut ctx.renderable_content.visible_rows);
                let row_styles = std::mem::take(&mut ctx.renderable_content.row_styles);
                let extras = std::mem::take(&mut ctx.renderable_content.extras);
                let term_colors = ctx.renderable_content.term_colors;
                let display_offset = ctx.renderable_content.display_offset as i32;
                let selection = ctx.renderable_content.selection_range;
                let cursor = &ctx.renderable_content.cursor;
                // Take + reset so next frame sees fresh damage only
                // from this frame's `Renderer::run`.
                let damage = std::mem::replace(
                    &mut ctx.renderable_content.frame_damage,
                    rio_backend::event::TerminalDamage::Noop,
                );
                let hint_matches = ctx.renderable_content.hint_matches.clone();
                let is_active = *key == active_key;
                // `focused_match` lives on `Screen::search_state` — it's
                // a per-window state tied to whichever panel has search
                // focus, which is the active one. Don't paint a focused
                // highlight on non-active panels even if they happen to
                // carry hint_matches.
                let focused_match = if is_active {
                    search_focused_match.clone()
                } else {
                    None
                };
                // Only the active panel can be under the mouse, so
                // hyperlink-hover state only makes sense there. Same
                // reasoning as `focused_match` above.
                let hovered_hyperlink = if is_active {
                    ctx.renderable_content
                        .highlighted_hint
                        .as_ref()
                        .map(|h| (h.start, h.end))
                } else {
                    None
                };
                let hint_labels = if is_active {
                    std::mem::take(&mut ctx.renderable_content.hint_labels)
                } else {
                    None
                };
                let cursor_shape = cursor.state.content;
                let cursor_blinking = ctx.renderable_content.has_blinking_enabled;
                let cursor_blink_visible =
                    !cursor_blinking || ctx.renderable_content.is_blinking_cursor_visible;
                // IME state is window-level (`self.ime`); it renders
                // on the active panel only, and never over scrollback
                // (the cursor row is off-viewport there — an anchor
                // computed from it would paint on history).
                let preedit_line = if is_active && display_offset == 0 {
                    self.ime.preedit().and_then(|preedit| {
                        rio_grid::preedit::PreeditLine::new(
                            &preedit.text,
                            preedit.cursor,
                            (cursor.state.pos.row.0.max(0) as usize)
                                .min(ctx.renderable_content.screen_lines.max(1) - 1),
                            cursor.state.pos.col.0,
                            ctx.renderable_content.columns.max(1),
                        )
                    })
                } else {
                    None
                };
                let cursor_preedit = preedit_line.is_some();
                // OSC 12 wins; otherwise fall back to the named-color
                // theme value. `Renderer::color`'s fallback (the
                // indexed-color List) is not populated for the Cursor
                // slot — `List::fill_named` skips it — so we read
                // `named_colors.cursor` directly.
                let cursor_color = term_colors
                    [rio_backend::config::colors::NamedColor::Cursor as usize]
                    .unwrap_or(self.renderer.named_colors.cursor);
                panels.push(PanelFrame {
                    route_id: ctx.route_id,
                    layout_rect: item.layout_rect,
                    cols: ctx.renderable_content.columns.max(1) as u32,
                    rows: ctx.renderable_content.screen_lines.max(1) as u32,
                    cell_w,
                    cell_h,
                    font_px,
                    visible_rows,
                    row_styles,
                    extras,
                    term_colors,
                    cursor_col: cursor.state.pos.col.0 as u16,
                    cursor_row: cursor.state.pos.row.0 as u16,
                    cursor_visible: cursor.state.is_visible(),
                    cursor_shape,
                    cursor_blinking,
                    cursor_blink_visible,
                    cursor_preedit,
                    cursor_color,
                    is_active,
                    damage,
                    selection,
                    display_offset,
                    hint_matches,
                    focused_match,
                    hovered_hyperlink,
                    hint_labels,
                    preedit_line,
                });
            }

            // --- ensure every panel has a matching GridRenderer ---
            for p in &panels {
                self.ensure_grid(p.route_id, p.cols, p.rows);
            }

            // --- emit cells + build uniforms per panel ---
            let window_size = self.sugarloaf.window_size();
            let font_library = self.sugarloaf.font_library().clone();
            let bg_col = self.renderer.named_colors.background.0;
            // Same `input_colorspace` value the Metal quad pipeline
            // feeds into `Globals` — the grid shader applies the
            // matching sRGB → DisplayP3 transform so cell bg, window
            // fill, and UI overlays produce identical framebuffer
            // colors. single `load_color` path.
            let input_colorspace = self.sugarloaf.input_colorspace();

            let mut frame_grids: Vec<(
                &mut rio_backend::sugarloaf::grid::GridRenderer,
                rio_backend::sugarloaf::grid::GridUniforms,
            )> = Vec::with_capacity(panels.len());

            let rasterizer = &mut self.grid_rasterizer;
            let renderer_ref = &self.renderer;
            for (route_id, grid) in self.grids.iter_mut() {
                let Some(p) = panels.iter_mut().find(|p| p.route_id == *route_id) else {
                    continue;
                };

                // Decide which rows to rebuild.
                //
                // `force_full` short-circuits damage to "rebuild all":
                // - grid was just created or resized (CPU buffers
                // are zeroed, so whatever damage says we have to
                // do a full fill).
                // - damage == Full (the terminal explicitly asked).
                //
                // `Noop` / `CursorOnly` → no row rebuilds, uniforms
                // alone carry the frame's state change.
                //
                // `Partial(lines)` → rebuild only those row indices.
                let force_full = grid.needs_full_rebuild()
                    || matches!(p.damage, rio_backend::event::TerminalDamage::Full);

                enum RowsToRebuild {
                    None,
                    All,
                    /// Per-row decision: walk `visible_rows` and
                    /// rebuild rows whose `dirty` bit is set.
                    Dirty,
                }
                let rows_to_rebuild = if force_full {
                    RowsToRebuild::All
                } else {
                    match p.damage {
                        rio_backend::event::TerminalDamage::Full => RowsToRebuild::All,
                        rio_backend::event::TerminalDamage::Partial => {
                            RowsToRebuild::Dirty
                        }
                        rio_backend::event::TerminalDamage::CursorOnly
                        | rio_backend::event::TerminalDamage::Noop => RowsToRebuild::None,
                    }
                };

                let cols = p.cols as usize;
                let mut bg_scratch: Vec<rio_backend::sugarloaf::grid::CellBg> =
                    Vec::with_capacity(cols);
                let mut fg_scratch: Vec<rio_backend::sugarloaf::grid::CellText> =
                    Vec::with_capacity(cols);
                let mut hint_scratch: Vec<rio_grid::RowHint> = Vec::new();

                let label_styles_pair = rio_grid::hint_label_styles(
                    self.renderer.named_colors.hint_foreground,
                    self.renderer.named_colors.hint_background,
                );
                let hint_labels_converted: Option<Vec<rio_grid::HintLabel>> = p
                    .hint_labels
                    .as_deref()
                    .filter(|labels| !labels.is_empty())
                    .map(|labels| {
                        labels
                            .iter()
                            .map(|l| rio_grid::HintLabel {
                                position: l.position,
                                label: l.label,
                                is_first: l.is_first,
                            })
                            .collect()
                    });

                // Small helper: rebuild one row into the grid's
                // buffers. Closure-style to avoid duplicating the
                // body between the `All` and `Only` branches.
                //
                // Two passes now: `build_row_bg` emits `CellBg` per
                // cell (unconditional), `build_row_fg` does run-level
                // shaping + glyph emission (macOS only). The bg pass
                // never needs shaping so it runs on all platforms;
                // the fg path is macOS-specific pending the
                // wgpu+swash port.
                let mut rebuild_row =
                    |p: &PanelFrame,
                     y: usize,
                     grid: &mut rio_backend::sugarloaf::grid::GridRenderer,
                     rasterizer: &mut rio_grid::GridGlyphRasterizer| {
                        let Some(row) = p.visible_rows.get(y) else {
                            return;
                        };
                        let row_styles =
                            p.row_styles.get(y).map(Vec::as_slice).unwrap_or(&[]);
                        let row_sel = rio_grid::row_selection_for(
                            p.selection,
                            y,
                            cols,
                            p.display_offset,
                        );
                        rio_grid::row_hints_for(
                            p.hint_matches.as_deref(),
                            p.focused_match.as_ref(),
                            p.hovered_hyperlink,
                            y,
                            cols,
                            p.display_offset,
                            &mut hint_scratch,
                        );
                        let label_row;
                        let label_styles;
                        let (row, row_styles) = match hint_labels_converted.as_deref() {
                            Some(labels) => {
                                match rio_grid::overlay_hint_labels(
                                    row,
                                    row_styles,
                                    labels,
                                    y,
                                    p.display_offset,
                                    label_styles_pair,
                                    &mut hint_scratch,
                                ) {
                                    Some((r, styles)) => {
                                        label_row = r;
                                        label_styles = styles;
                                        (&label_row, label_styles.as_slice())
                                    }
                                    None => (row, row_styles),
                                }
                            }
                            None => (row, row_styles),
                        };
                        // Thread the composition only into its own
                        // row: everything else renders untouched.
                        let preedit_row =
                            p.preedit_line.as_ref().filter(|line| line.row == y).map(
                                |line| rio_grid::PreeditRow {
                                    line,
                                    block_bg: rio_grid::normalized_to_u8(p.cursor_color),
                                },
                            );
                        rio_grid::build_row_bg(
                            row,
                            cols,
                            row_styles,
                            renderer_ref,
                            &p.term_colors,
                            row_sel,
                            &hint_scratch,
                            preedit_row.as_ref(),
                            &mut bg_scratch,
                        );
                        let cursor_col_for_row = if p.cursor_visible
                            && (y as u16) == p.cursor_row
                            && p.cursor_shape != rio_backend::ansi::CursorShape::Hidden
                        {
                            Some(p.cursor_col)
                        } else {
                            None
                        };
                        rio_grid::build_row_fg(
                            row,
                            cols,
                            y as u16,
                            row_styles,
                            &p.extras,
                            renderer_ref,
                            &p.term_colors,
                            rasterizer,
                            grid,
                            p.font_px,
                            p.cell_w,
                            p.cell_h,
                            row_sel,
                            &hint_scratch,
                            preedit_row.as_ref(),
                            &font_library,
                            p.route_id,
                            cursor_col_for_row,
                            &mut fg_scratch,
                        );
                        grid.write_row(y as u32, &bg_scratch, &fg_scratch);
                    };

                match rows_to_rebuild {
                    RowsToRebuild::None => {
                        // Nothing to rebuild — previous frame's
                        // CellBg/CellText stay resident. The GPU
                        // pass below still runs so updated uniforms
                        // (cursor_pos moved, etc.) take effect.
                    }
                    RowsToRebuild::All => {
                        // Clear the flag before rebuilding so an
                        // atlas-full clear inside `rebuild_row` can
                        // re-set it for the recovery pass below.
                        grid.mark_full_rebuild_done();
                        #[allow(clippy::needless_range_loop)]
                        for y in 0..p.visible_rows.len() {
                            rebuild_row(p, y, grid, rasterizer);
                        }
                    }
                    RowsToRebuild::Dirty => {
                        // Walk the snapshot rows; rebuild + clear the
                        // per-row dirty bit. Set by `snapshot_visible`
                        // for rows it copied this frame; cleared here
                        // so next frame starts clean.
                        #[allow(clippy::needless_range_loop)]
                        for y in 0..p.visible_rows.len() {
                            if !p.visible_rows[y].dirty {
                                continue;
                            }
                            rebuild_row(p, y, grid, rasterizer);
                            p.visible_rows[y].dirty = false;
                        }
                    }
                }

                // The composition is painted into the row's CPU
                // cells, so its row must rebuild whenever the overlay
                // exists, moved, or just disappeared — even when
                // terminal damage says nothing changed (the text under
                // it didn't; the overlay did). Cheap: at most two rows.
                if p.is_active {
                    let current = p.preedit_line.as_ref().map(|line| line.row);
                    for row in [
                        self.last_preedit_row
                            .filter(|_| self.last_preedit_row != current),
                        current,
                    ]
                    .into_iter()
                    .flatten()
                    {
                        if row < p.visible_rows.len() {
                            rebuild_row(p, row, grid, rasterizer);
                            p.visible_rows[row].dirty = false;
                        }
                    }
                    self.last_preedit_row = current;
                }

                // Atlas-full recovery: the backend cleared the atlas
                // during the rebuild above, so rows written before the
                // clear reference stale slots. Re-emit everything.
                if grid.needs_full_rebuild() {
                    grid.mark_full_rebuild_done();
                    for y in 0..p.visible_rows.len() {
                        rebuild_row(p, y, grid, rasterizer);
                    }
                }

                // Cursor pipeline (`addCursor` /
                // `cursor.style()`):
                // 1. Decide render style with strict priority:
                // preedit > visible > focused > blink > shape.
                // 2. Some(style): build the sprite for the block
                // slot (drawn under text; the bg-tint uniforms
                // below make the bg fragment paint the block +
                // the text shader invert the underlying glyph)
                // or the tail slot (bar/underline, drawn over
                // text). None: both stay empty + zero uniforms.
                // 3. One `grid.set_cursor(block, tail)` call
                // replaces both slots. It diffs against last
                // frame and only dirties cursor buffers on
                // change — do NOT clear the slots beforehand,
                // that would dirty them every frame.
                let render_style =
                    rio_grid::cursor_render_style(rio_grid::CursorRenderInputs {
                        visible: p.cursor_visible,
                        focused: p.is_active && self.renderer.is_window_focused,
                        blink_visible: p.cursor_blink_visible,
                        blinking: p.cursor_blinking,
                        preedit: p.cursor_preedit,
                        shape: p.cursor_shape,
                    });
                let mut block_cursor: Option<rio_backend::sugarloaf::grid::CellText> =
                    None;
                let mut tail_cursor: Option<rio_backend::sugarloaf::grid::CellText> =
                    None;
                if let Some(style) = render_style {
                    let cell_w = p.cell_w.round().clamp(1.0, u32::MAX as f32) as u32;
                    let cell_h = p.cell_h.round().clamp(1.0, u32::MAX as f32) as u32;
                    let cursor_color = [
                        (p.cursor_color[0].clamp(0.0, 1.0) * 255.0) as u8,
                        (p.cursor_color[1].clamp(0.0, 1.0) * 255.0) as u8,
                        (p.cursor_color[2].clamp(0.0, 1.0) * 255.0) as u8,
                        255,
                    ];
                    if let Some((is_block, cell)) = rio_grid::cursor_sprite_cell(
                        grid,
                        style,
                        p.cursor_col,
                        p.cursor_row,
                        cursor_color,
                        cell_w,
                        cell_h,
                    ) {
                        if is_block {
                            block_cursor = Some(cell);
                        } else {
                            tail_cursor = Some(cell);
                        }
                    }
                }
                grid.set_cursor(block_cursor.as_slice(), tail_cursor.as_slice());

                // Panel's grid origin in drawable-pixel space =
                // window scaled_margin + the panel's layout rect
                // offset inside the root container. Snap to integer
                // pixels so `cell_size * grid_pos + grid_padding`
                // always lands on pixel boundaries. Without this, a
                // fractional margin (e.g. Taffy layout computing
                // 10.5px offsets) shifts the whole grid half a pixel
                // and the bg fragment's
                // `floor((pixel - padding) / cell_size)` disagrees
                // with the text vertex's `cell_size * grid_pos`
                // about where cell boundaries are → visible seams.
                let panel_left = (scaled_margin.left + p.layout_rect[0]).round();
                let panel_top = (scaled_margin.top + p.layout_rect[1]).round();

                // Bg-tint uniforms fire ONLY for the active block
                // style — the bg shader paints the cursor cell in
                // `cursor_bg_color` and the text shader swaps glyph
                // fg to `cursor_color` (so the character inverts on
                // top of the block). All other styles (bar /
                // underline / hollow) draw via the sprite emitted
                // above; their bg/text stays untouched. Same gate as
                // .
                let (cursor_pos, cursor_col_u, cursor_bg_u) =
                    if matches!(render_style, Some(rio_grid::CursorRenderStyle::Block)) {
                        (
                            [p.cursor_col as u32, p.cursor_row as u32],
                            [bg_col[0], bg_col[1], bg_col[2], bg_col[3]],
                            [
                                p.cursor_color[0],
                                p.cursor_color[1],
                                p.cursor_color[2],
                                1.0,
                            ],
                        )
                    } else {
                        ([u32::MAX; 2], [0.0; 4], [0.0; 4])
                    };

                let uniforms = rio_backend::sugarloaf::grid::GridUniforms {
                    projection:
                        rio_backend::sugarloaf::components::core::orthographic_projection(
                            window_size.width,
                            window_size.height,
                        ),
                    // grid_padding = (top, right, bottom, left). The
                    // bg shader only reads `.w` (left) + `.x` (top)
                    // to anchor the grid, so right/bottom can stay
                    // 0. padding_extend is 0 too — each panel's
                    // grid must stay bounded to its own rect so
                    // sibling panels / the window margin aren't
                    // painted by this grid. The full-window bg fill
                    // (re-enabled in sugarloaf's render_metal) now
                    // handles the space outside all panels.
                    grid_padding: [panel_top, 0.0, 0.0, panel_left],
                    cursor_color: cursor_col_u,
                    cursor_bg_color: cursor_bg_u,
                    cell_size: [p.cell_w, p.cell_h],
                    grid_size: [p.cols, p.rows],
                    cursor_pos,
                    _pad_cursor: [0; 2],
                    min_contrast: 0.0,
                    flags: 0,
                    padding_extend: 0,
                    input_colorspace,
                };

                frame_grids.push((grid, uniforms));
            }

            if should_present {
                if frame_grids.is_empty() {
                    self.sugarloaf.render();
                } else {
                    self.sugarloaf.render_with_grids(&mut frame_grids);
                }
                // A dropped frame (no drawable, e.g. right after wake)
                // already consumed this frame's damage; without a retry
                // the content is lost until unrelated PTY traffic.
                if self.sugarloaf.take_frame_dropped() {
                    self.mark_dirty();
                    self.context_manager.request_render();
                }
            } else {
                // Nothing to draw this frame, but `Renderer::run`
                // (plus overlays, borders, scrollbars, …) already
                // pushed into sugarloaf's per-frame queues. Drain
                // them so the next presented frame doesn't
                // composite them on top of their re-pushed selves.
                self.sugarloaf.discard_frame();
            }

            // Return each panel's snapshot buffers to the matching
            // context's `renderable_content` so the next frame's
            // `Renderer::run` can reuse the existing allocations
            // (rows, per-cell styles, extras table). Closed routes
            // simply drop their PanelFrame; the context (and its
            // renderable_content) is gone too.
            for (_, item) in self
                .context_manager
                .current_grid_mut()
                .contexts_mut()
                .iter_mut()
            {
                let route_id = item.val.route_id;
                if let Some(idx) = panels.iter().position(|p| p.route_id == route_id) {
                    let p = panels.swap_remove(idx);
                    item.val.renderable_content.visible_rows = p.visible_rows;
                    item.val.renderable_content.row_styles = p.row_styles;
                    item.val.renderable_content.extras = p.extras;
                    item.val.renderable_content.hint_labels = p.hint_labels;
                }
            }
            panels.clear();
        }

        // Mark as dirty if we need continuous rendering (e.g.,
        // indeterminate progress bar, trail cursor animation). UI-only
        // — terminal cells didn't change, but we want the next vsync
        // to fire a render so overlays/animations tick forward.
        if has_animation {
            self.context_manager
                .current_mut()
                .renderable_content
                .pending_update
                .set_dirty();
        }

        if let Some(wake_in) = self.renderer.scrollbar.next_wake_in() {
            self.context_manager
                .schedule_render_on_route(wake_in.as_millis() as u64);
        }

        // In case the configuration of blinking cursor is enabled
        // TODO: enable blinking for selection after adding debounce (https://github.com/raphamorim/rio/issues/437)
        if self.renderer.is_window_focused
            && self.renderer.config_has_blinking_enabled
            && self.selection_is_empty()
            && self
                .context_manager
                .current()
                .renderable_content
                .has_blinking_enabled
        {
            self.context_manager
                .blink_cursor(self.renderer.config_blinking_interval);
        }

        window_update
    }

    /// Update IME cursor position based on terminal cursor position
    /// This should be called after rendering to ensure cursor position is current
    pub fn update_ime_cursor_position_if_needed(
        &mut self,
        window: &rio_window::window::Window,
    ) {
        // Check if IME cursor positioning is enabled in config
        if !self.context_manager.config.keyboard.ime_cursor_positioning {
            return;
        }

        let current_grid = self.context_manager.current_grid();
        let scaled_margin = current_grid.get_scaled_margin();

        let Some(current_item) = current_grid.current_item() else {
            return;
        };

        let layout = current_item.val.dimension;
        let cursor_pos = current_item.val.renderable_content.cursor.state.pos;

        // While composing, the candidate popup follows the IME caret,
        // not the terminal cursor: the composition renders inline and
        // can slide away from the cursor cell, and a popup opening
        // tens of cells from the caret overlaps the freshly drawn
        // text. The reported area's width spans from the caret to the
        // composition's end, so the OS also knows how much freshly
        // drawn text to avoid covering.
        // Mirrors the layout the renderer uses (same inputs).
        let content = &current_item.val.renderable_content;
        let (anchor_row, anchor_col, anchor_cells) =
            match self.ime.preedit().filter(|_| {
                content.display_offset == 0
                    && content.columns > 0
                    && content.screen_lines > 0
            }) {
                Some(preedit) => match rio_grid::preedit::PreeditLine::new(
                    &preedit.text,
                    preedit.cursor,
                    (cursor_pos.row.0.max(0) as usize).min(content.screen_lines - 1),
                    cursor_pos.col.0,
                    content.columns,
                ) {
                    Some(line) => {
                        let col = line.popup_anchor_col().min(content.columns - 1);
                        let cells =
                            line.end_col().min(content.columns).max(col + 1) - col;
                        (line.row, col, cells)
                    }
                    None => (cursor_pos.row.0.max(0) as usize, cursor_pos.col.0, 1),
                },
                None => (cursor_pos.row.0.max(0) as usize, cursor_pos.col.0, 1),
            };

        // Calculate pixel position of cursor — canonical integer
        // stride (line_height already baked into cell_height).
        let cell_width = layout.cell.cell_width as f32;
        let cell_height = layout.cell.cell_height as f32;

        // Validate dimensions before calculation
        if cell_width <= 0.0 || cell_height <= 0.0 {
            tracing::warn!(
                "Invalid cell dimensions for IME cursor positioning: {}x{}",
                cell_width,
                cell_height
            );
            return;
        }

        // Panel origin: layout_rect is relative to root container,
        // add scaled_margin to get absolute screen position
        let panel_rect = current_item.layout_rect;
        let origin_x = panel_rect[0] + scaled_margin.left;
        let origin_y = panel_rect[1] + scaled_margin.top;

        // Convert grid position to pixel position
        let pixel_x = origin_x + (anchor_col as f32 * cell_width) + (cell_width * 0.5);
        let pixel_y = origin_y + (anchor_row as f32 * cell_height);

        // Validate final coordinates
        if pixel_x.is_nan() || pixel_y.is_nan() || pixel_x < 0.0 || pixel_y < 0.0 {
            tracing::warn!("Invalid IME cursor coordinates: ({}, {})", pixel_x, pixel_y);
            return;
        }

        // A PastEnd caret sits one cell past the composition, where
        // `end_col - col` is 0; the area is always at least one cell.
        let area_width = anchor_cells as f32 * cell_width;

        // Check if the area changed significantly to avoid unnecessary updates
        if let Some((last_x, last_y, last_w)) = self.last_ime_cursor_pos {
            if (pixel_x - last_x).abs() < 1.0
                && (pixel_y - last_y).abs() < 1.0
                && (area_width - last_w).abs() < 1.0
            {
                return; // Area hasn't changed significantly
            }
        }

        // Update last area
        self.last_ime_cursor_pos = Some((pixel_x, pixel_y, area_width));

        // Set IME cursor area
        window.set_ime_cursor_area(
            rio_window::dpi::PhysicalPosition::new(pixel_x as f64, pixel_y as f64),
            rio_window::dpi::PhysicalSize::new(area_width as f64, cell_height as f64),
        );
    }
}
