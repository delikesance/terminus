use crate::error::{Error, Result};
use crate::gpu_frame::{AtlasGlyph, GpuCell, GpuFrame};
use crate::models::ColorTheme;
use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::grid::Scroll;
use alacritty_terminal::index::{Column, Point};
use alacritty_terminal::term::cell::Flags;
use alacritty_terminal::term::test::TermSize;
use alacritty_terminal::term::{point_to_viewport, viewport_to_point, Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{self, Color, CursorShape, NamedColor};
use fontdue::{Font, FontSettings};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

const FONT_TTF: &[u8] = include_bytes!("../fonts/CascadiaMonoNF-Regular.ttf");
const SCROLLBACK: usize = 2000;
const ATLAS_SIZE: u32 = 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
enum GlyphFit {
    Text,
    Icon,
    Cell,
}

#[derive(Clone, Copy)]
struct Glyph {
    w: u32,
    h: u32,
    xmin: i32,
    ymin: i32,
    fit: GlyphFit,
}

/// Collects emulator→PTY replies during CSI/OSC handling.
struct EventCollector {
    events: Arc<Mutex<Vec<Event>>>,
}

impl EventListener for EventCollector {
    fn send_event(&self, event: Event) {
        match &event {
            Event::PtyWrite(_) | Event::ColorRequest(_, _) | Event::TextAreaSizeRequest(_) => {
                self.events.lock().push(event);
            }
            _ => {}
        }
    }
}

pub struct TerminalEmulator {
    term: Term<EventCollector>,
    events: Arc<Mutex<Vec<Event>>>,
    parser: ansi::Processor,
    cols: u16,
    rows: u16,
    fonts: Vec<Font>,
    glyphs: HashMap<char, (Glyph, Vec<u8>)>,
    atlas_r8: Vec<u8>,
    atlas_w: u32,
    atlas_h: u32,
    atlas_shelf_x: u32,
    atlas_shelf_y: u32,
    atlas_shelf_h: u32,
    atlas_dirty: bool,
    atlas_next_id: u16,
    atlas_glyphs: HashMap<char, AtlasGlyph>,
    atlas_bits: HashMap<u16, Vec<u8>>,
    atlas_pending: std::collections::HashSet<u16>,
    font_px: f32,
    style_px: f32,
    line_height: f32,
    scale: f32,
    cell_w: u32,
    cell_h: u32,
    baseline: i32,
    palette: [[u8; 4]; 256],
    fg: [u8; 4],
    bg: [u8; 4],
    cursor: [u8; 4],
    dirty: bool,
}

pub struct TermFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl TerminalEmulator {
    pub fn new(cols: u16, rows: u16, font_px: f32) -> Result<Self> {
        Self::new_with_style(cols, rows, font_px, 1.0)
    }

    pub fn new_with_style(cols: u16, rows: u16, font_px: f32, line_height: f32) -> Result<Self> {
        Self::new_with_scale(cols, rows, font_px, line_height, 1.0)
    }

    pub fn new_with_scale(
        cols: u16,
        rows: u16,
        font_px: f32,
        line_height: f32,
        scale: f32,
    ) -> Result<Self> {
        let fonts = load_fonts()?;
        let scale = sanitize_scale(scale);
        let (px, cell_w, cell_h, baseline) = metrics_for(&fonts[0], font_px * scale, line_height);
        let cols = cols.max(2);
        let rows = rows.max(1);
        let events = Arc::new(Mutex::new(Vec::new()));
        let collector = EventCollector {
            events: Arc::clone(&events),
        };
        let config = Config {
            scrolling_history: SCROLLBACK,
            ..Config::default()
        };
        let size = TermSize::new(cols as usize, rows as usize);
        let mut emulator = Self {
            term: Term::new(config, &size, collector),
            events,
            parser: ansi::Processor::new(),
            cols,
            rows,
            fonts,
            glyphs: HashMap::new(),
            atlas_r8: vec![0u8; (ATLAS_SIZE * ATLAS_SIZE) as usize],
            atlas_w: ATLAS_SIZE,
            atlas_h: ATLAS_SIZE,
            atlas_shelf_x: 0,
            atlas_shelf_y: 0,
            atlas_shelf_h: 0,
            atlas_dirty: true,
            atlas_next_id: 1,
            atlas_glyphs: HashMap::new(),
            atlas_bits: HashMap::new(),
            atlas_pending: std::collections::HashSet::new(),
            font_px: px,
            style_px: font_px,
            line_height,
            scale,
            cell_w,
            cell_h,
            baseline,
            palette: [[0, 0, 0, 255]; 256],
            fg: [245, 245, 247, 255],
            bg: [28, 28, 30, 255],
            cursor: [10, 132, 255, 255],
            dirty: true,
        };
        emulator.fill_default_palette();
        Ok(emulator)
    }

    pub fn set_style(&mut self, font_px: f32, line_height: f32) {
        self.style_px = font_px;
        self.line_height = line_height;
        self.recompute_metrics();
    }

    pub fn set_scale(&mut self, scale: f32) {
        let scale = sanitize_scale(scale);
        if (self.scale - scale).abs() < 0.001 {
            return;
        }
        self.scale = scale;
        self.recompute_metrics();
    }

    fn recompute_metrics(&mut self) {
        let (px, cell_w, cell_h, baseline) =
            metrics_for(&self.fonts[0], self.style_px * self.scale, self.line_height);
        self.font_px = px;
        self.cell_w = cell_w;
        self.cell_h = cell_h;
        self.baseline = baseline;
        self.glyphs.clear();
        self.reset_atlas();
        self.dirty = true;
    }

    fn reset_atlas(&mut self) {
        self.atlas_r8.fill(0);
        self.atlas_shelf_x = 0;
        self.atlas_shelf_y = 0;
        self.atlas_shelf_h = 0;
        self.atlas_next_id = 1;
        self.atlas_glyphs.clear();
        self.atlas_bits.clear();
        self.atlas_pending.clear();
        self.atlas_dirty = true;
    }

    pub fn apply_theme(&mut self, theme: &ColorTheme) {
        self.bg = parse_hex(&theme.background);
        self.fg = parse_hex(&theme.foreground);
        self.cursor = parse_hex(&theme.cursor);
        let ansi = [
            &theme.black,
            &theme.red,
            &theme.green,
            &theme.yellow,
            &theme.blue,
            &theme.magenta,
            &theme.cyan,
            &theme.white,
            &theme.bright_black,
            &theme.bright_red,
            &theme.bright_green,
            &theme.bright_yellow,
            &theme.bright_blue,
            &theme.bright_magenta,
            &theme.bright_cyan,
            &theme.bright_white,
        ];
        for (i, hex) in ansi.iter().enumerate() {
            self.palette[i] = parse_hex(hex);
        }
        self.dirty = true;
    }

    /// Feed bytes from the PTY/SSH into the emulator.
    ///
    /// Returns protocol replies that must be written back on the same channel
    /// (CSI DA/DSR, color queries, text-area size, etc.).
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<u8> {
        self.parser.advance(&mut self.term, bytes);
        self.dirty = true;
        self.drain_pty_replies()
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        let cols = cols.max(2);
        let rows = rows.max(1);
        self.cols = cols;
        self.rows = rows;
        self.term.resize(TermSize::new(cols as usize, rows as usize));
        self.dirty = true;
    }

    pub fn cell_size(&self) -> (u32, u32) {
        (self.cell_w, self.cell_h)
    }

    /// Bit flags for the frontend input encoder.
    /// bit0 APP_CURSOR, bit1 APP_KEYPAD, bit2 ALT_SCREEN, bit3 BRACKETED_PASTE
    pub fn mode_flags(&self) -> u32 {
        let mode = self.term.mode();
        let mut flags = 0u32;
        if mode.contains(TermMode::APP_CURSOR) {
            flags |= 0b0001;
        }
        if mode.contains(TermMode::APP_KEYPAD) {
            flags |= 0b0010;
        }
        if mode.contains(TermMode::ALT_SCREEN) {
            flags |= 0b0100;
        }
        if mode.contains(TermMode::BRACKETED_PASTE) {
            flags |= 0b1000;
        }
        flags
    }

    /// Scroll the primary screen history. Returns false on the alternate screen
    /// (caller should send arrow/wheel sequences to the PTY instead).
    pub fn scroll_delta(&mut self, lines: i32) -> bool {
        if lines == 0 {
            return true;
        }
        if self.term.mode().contains(TermMode::ALT_SCREEN) {
            return false;
        }
        self.term.scroll_display(Scroll::Delta(lines));
        self.dirty = true;
        true
    }

    pub fn display_offset(&self) -> usize {
        self.term.grid().display_offset()
    }

    /// `(display_offset, history_lines)`. Both zero on the alternate screen.
    pub fn scroll_state(&self) -> (u32, u32) {
        if self.term.mode().contains(TermMode::ALT_SCREEN) {
            return (0, 0);
        }
        let offset = self.term.grid().display_offset() as u32;
        let max = self
            .term
            .total_lines()
            .saturating_sub(self.term.screen_lines()) as u32;
        (offset, max)
    }

    /// Jump to an absolute display offset (clamped). False on alt screen.
    pub fn scroll_to(&mut self, offset: u32) -> bool {
        if self.term.mode().contains(TermMode::ALT_SCREEN) {
            return false;
        }
        let (_, max) = self.scroll_state();
        let target = (offset.min(max)) as usize;
        let cur = self.term.grid().display_offset();
        let delta = target as i32 - cur as i32;
        if delta != 0 {
            self.term.scroll_display(Scroll::Delta(delta));
            self.dirty = true;
        }
        true
    }

    pub fn take_frame(&mut self) -> Option<TermFrame> {
        self.capture_frame(false)
    }

    pub fn capture_frame(&mut self, force: bool) -> Option<TermFrame> {
        if !force && !self.dirty {
            return None;
        }
        self.dirty = false;
        Some(self.raster())
    }

    /// Compact atlas + cell grid for GPU paint (WebGL / future native surface).
    pub fn capture_gpu_frame(&mut self, force: bool) -> Option<GpuFrame> {
        if !force && !self.dirty {
            return None;
        }
        self.dirty = false;
        Some(self.gpu_raster())
    }

    pub fn gpu_raster(&mut self) -> GpuFrame {
        let rows = self.term.screen_lines() as u32;
        let cols = self.term.columns() as u32;
        let mut cells = vec![
            GpuCell {
                glyph_id: 0,
                fg: self.fg,
                bg: self.bg,
            };
            (rows * cols) as usize
        ];

        let snapshots = {
            let content = self.term.renderable_content();
            let display_offset = content.display_offset;
            let cursor = content.cursor;
            let show_cursor = cursor.shape != CursorShape::Hidden;
            let cursor_vp = point_to_viewport(display_offset, cursor.point);
            let colors = content.colors;
            let mut snaps = Vec::with_capacity((rows * cols) as usize);
            for indexed in content.display_iter {
                let Some(vp) = point_to_viewport(display_offset, indexed.point) else {
                    continue;
                };
                let row = vp.line as u32;
                let col = vp.column.0 as u32;
                if row >= rows || col >= cols {
                    continue;
                }
                let cell = &indexed.cell;
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                let bold = cell.flags.contains(Flags::BOLD) && !cell.flags.contains(Flags::DIM);
                let dim = cell.flags.contains(Flags::DIM);
                let mut fg = self.resolve_cell_color(cell.fg, colors, self.fg, bold, dim);
                let mut bg = self.resolve_cell_color(cell.bg, colors, self.bg, false, false);
                if cell.flags.contains(Flags::INVERSE) {
                    std::mem::swap(&mut fg, &mut bg);
                }
                let ch = if cell.flags.contains(Flags::HIDDEN) || cell.c == '\0' {
                    ' '
                } else {
                    cell.c
                };
                if show_cursor {
                    if let Some(cv) = cursor_vp {
                        if cv.line == vp.line && cv.column == vp.column {
                            bg = self.cursor;
                            fg = self.bg;
                        }
                    }
                }
                snaps.push((col, row, ch, fg, bg));
            }
            snaps
        };

        for (col, row, ch, fg, bg) in snapshots {
            let glyph_id = self.ensure_atlas_glyph(ch);
            cells[(row * cols + col) as usize] = GpuCell { glyph_id, fg, bg };
        }

        let glyphs: Vec<AtlasGlyph> = self
            .atlas_glyphs
            .values()
            .map(|g| {
                let mut out = g.clone();
                if self.atlas_pending.remove(&g.id) {
                    out.bits = self.atlas_bits.get(&g.id).cloned();
                } else {
                    out.bits = None;
                }
                out
            })
            .collect();
        self.atlas_dirty = false;
        GpuFrame {
            cols: cols as u16,
            rows: rows as u16,
            cell_w: self.cell_w,
            cell_h: self.cell_h,
            atlas_w: self.atlas_w,
            atlas_h: self.atlas_h,
            glyphs,
            cells,
        }
    }

    fn ensure_atlas_glyph(&mut self, ch: char) -> u16 {
        if ch == ' ' || ch == '\0' {
            return 0;
        }
        if let Some(g) = self.atlas_glyphs.get(&ch) {
            return g.id;
        }
        let (meta, cover) = self.glyph(ch);
        let (bits, w, h, ox, oy) = if meta.fit == GlyphFit::Cell {
            let scaled = scale_cover(&cover, meta.w, meta.h, self.cell_w, self.cell_h);
            let snapped = snap_cell_edges(&scaled, self.cell_w, self.cell_h, cell_attach(ch));
            (snapped, self.cell_w, self.cell_h, 0i32, 0i32)
        } else {
            let mut dw = meta.w;
            let mut dh = meta.h;
            let mut xmin = meta.xmin;
            let mut ymin = meta.ymin;
            let bits = if dw > self.cell_w && dw > 0 {
                let s = self.cell_w as f32 / dw as f32;
                dw = self.cell_w;
                dh = (dh as f32 * s).round().max(1.0) as u32;
                xmin = (xmin as f32 * s).round() as i32;
                ymin = (ymin as f32 * s).round() as i32;
                scale_cover(&cover, meta.w, meta.h, dw, dh)
            } else {
                cover
            };
            let dest_y_base = self.baseline - ymin - dh as i32;
            let oy = if meta.fit == GlyphFit::Icon {
                let mid = (self.cell_h as i32 - dh as i32) / 2;
                (dest_y_base + mid) / 2
            } else {
                dest_y_base
            };
            let ox = xmin.max(0);
            (bits, dw, dh, ox, oy)
        };
        let Some((x, y)) = self.atlas_alloc(w, h) else {
            // Atlas full — fall back to empty glyph rather than panic.
            return 0;
        };
        for dy in 0..h {
            for dx in 0..w {
                let cover_v = bits[(dy * w + dx) as usize];
                let i = ((y + dy) * self.atlas_w + (x + dx)) as usize;
                self.atlas_r8[i] = cover_v;
            }
        }
        let id = self.atlas_next_id;
        self.atlas_next_id = self.atlas_next_id.saturating_add(1);
        let entry = AtlasGlyph {
            id,
            x: x as u16,
            y: y as u16,
            w: w as u16,
            h: h as u16,
            ox: ox as i16,
            oy: oy as i16,
            bits: None,
        };
        self.atlas_glyphs.insert(ch, entry);
        self.atlas_bits.insert(id, bits);
        self.atlas_pending.insert(id);
        self.atlas_dirty = true;
        id
    }

    fn atlas_alloc(&mut self, w: u32, h: u32) -> Option<(u32, u32)> {
        if w == 0 || h == 0 || w > self.atlas_w || h > self.atlas_h {
            return None;
        }
        if self.atlas_shelf_x + w > self.atlas_w {
            self.atlas_shelf_y += self.atlas_shelf_h;
            self.atlas_shelf_x = 0;
            self.atlas_shelf_h = 0;
        }
        if self.atlas_shelf_y + h > self.atlas_h {
            return None;
        }
        let x = self.atlas_shelf_x;
        let y = self.atlas_shelf_y;
        self.atlas_shelf_x += w + 1;
        self.atlas_shelf_h = self.atlas_shelf_h.max(h);
        Some((x, y))
    }

    /// Extract visible screen text for a rectangle [r0..r1] × [c0..c1] (inclusive,
    /// order-independent corners). Each row is right-trimmed; rows are joined with `\n`.
    pub fn extract_text(&self, r0: u16, c0: u16, r1: u16, c1: u16) -> String {
        let rows = self.term.screen_lines() as u32;
        let cols = self.term.columns() as u32;
        let display_offset = self.term.grid().display_offset();
        let ra = r0.min(r1) as u32;
        let rb = r0.max(r1) as u32;
        let ca = c0.min(c1) as u32;
        let cb = c0.max(c1) as u32;
        let last_row = rb.min(rows.saturating_sub(1));
        let last_col = cb.min(cols.saturating_sub(1));
        let mut out = String::new();
        for r in ra..=last_row {
            let mut line = String::new();
            for c in ca..=last_col {
                let point = viewport_to_point(
                    display_offset,
                    Point::new(r as usize, Column(c as usize)),
                );
                let ch = self.term.grid()[point].c;
                line.push(if ch == '\0' { ' ' } else { ch });
            }
            out.push_str(line.trim_end());
            if r < last_row {
                out.push('\n');
            }
        }
        out
    }

    pub fn raster(&mut self) -> TermFrame {
        let rows = self.term.screen_lines() as u32;
        let cols = self.term.columns() as u32;
        let width = cols * self.cell_w;
        let height = rows * self.cell_h;
        let mut rgba = vec![0u8; (width * height * 4) as usize];

        let cells = {
            let content = self.term.renderable_content();
            let display_offset = content.display_offset;
            let cursor = content.cursor;
            let show_cursor = cursor.shape != CursorShape::Hidden;
            let cursor_vp = point_to_viewport(display_offset, cursor.point);
            let colors = content.colors;
            let mut cells = Vec::with_capacity((rows * cols) as usize);
            for indexed in content.display_iter {
                let Some(vp) = point_to_viewport(display_offset, indexed.point) else {
                    continue;
                };
                let row = vp.line as u32;
                let col = vp.column.0 as u32;
                if row >= rows || col >= cols {
                    continue;
                }
                let cell = &indexed.cell;
                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                let bold = cell.flags.contains(Flags::BOLD) && !cell.flags.contains(Flags::DIM);
                let dim = cell.flags.contains(Flags::DIM);
                let mut fg = self.resolve_cell_color(cell.fg, colors, self.fg, bold, dim);
                let mut bg = self.resolve_cell_color(cell.bg, colors, self.bg, false, false);
                if cell.flags.contains(Flags::INVERSE) {
                    std::mem::swap(&mut fg, &mut bg);
                }
                let ch = if cell.flags.contains(Flags::HIDDEN) || cell.c == '\0' {
                    ' '
                } else {
                    cell.c
                };
                if show_cursor {
                    if let Some(cv) = cursor_vp {
                        if cv.line == vp.line && cv.column == vp.column {
                            bg = self.cursor;
                            fg = self.bg;
                        }
                    }
                }
                cells.push((col, row, ch, fg, bg));
            }
            cells
        };

        for &(col, row, _, _, bg) in &cells {
            fill_cell(
                &mut rgba,
                width,
                col * self.cell_w,
                row * self.cell_h,
                self.cell_w,
                self.cell_h,
                bg,
            );
        }
        // Fill any gaps (spacers / missing) with default bg.
        if cells.len() < (rows * cols) as usize {
            let painted: std::collections::HashSet<(u32, u32)> =
                cells.iter().map(|&(c, r, _, _, _)| (c, r)).collect();
            for row in 0..rows {
                for col in 0..cols {
                    if !painted.contains(&(col, row)) {
                        fill_cell(
                            &mut rgba,
                            width,
                            col * self.cell_w,
                            row * self.cell_h,
                            self.cell_w,
                            self.cell_h,
                            self.bg,
                        );
                    }
                }
            }
        }
        let glyphs: Vec<_> = cells
            .into_iter()
            .filter(|(_, _, ch, _, _)| *ch != ' ')
            .collect();
        for (col, row, ch, fg, bg) in glyphs {
            self.blit_glyph(&mut rgba, width, col, row, ch, fg, bg);
        }
        TermFrame { width, height, rgba }
    }

    fn drain_pty_replies(&mut self) -> Vec<u8> {
        let pending = std::mem::take(&mut *self.events.lock());
        if pending.is_empty() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for event in pending {
            match event {
                Event::PtyWrite(text) => out.extend_from_slice(text.as_bytes()),
                Event::ColorRequest(index, formatter) => {
                    let rgb = self.rgb_for_index(index);
                    out.extend_from_slice(formatter(rgb).as_bytes());
                }
                Event::TextAreaSizeRequest(formatter) => {
                    let size = WindowSize {
                        num_lines: self.rows,
                        num_cols: self.cols,
                        cell_width: self.cell_w.min(u16::MAX as u32) as u16,
                        cell_height: self.cell_h.min(u16::MAX as u32) as u16,
                    };
                    out.extend_from_slice(formatter(size).as_bytes());
                }
                _ => {}
            }
        }
        out
    }

    fn rgb_for_index(&self, index: usize) -> ansi::Rgb {
        if let Some(rgb) = self.term.colors()[index] {
            return rgb;
        }
        let rgba = if index < 256 {
            self.palette[index]
        } else if index == NamedColor::Foreground as usize
            || index == NamedColor::BrightForeground as usize
        {
            self.fg
        } else if index == NamedColor::Background as usize {
            self.bg
        } else if index == NamedColor::Cursor as usize {
            self.cursor
        } else if index == NamedColor::DimForeground as usize {
            dim_rgba(self.fg)
        } else if (NamedColor::DimBlack as usize..=NamedColor::DimWhite as usize).contains(&index) {
            let base = index - NamedColor::DimBlack as usize;
            dim_rgba(self.palette[base])
        } else {
            self.fg
        };
        ansi::Rgb {
            r: rgba[0],
            g: rgba[1],
            b: rgba[2],
        }
    }

    fn resolve_cell_color(
        &self,
        color: Color,
        dynamic: &alacritty_terminal::term::color::Colors,
        fallback: [u8; 4],
        bold: bool,
        dim: bool,
    ) -> [u8; 4] {
        let mut rgba = match color {
            Color::Named(mut named) => {
                if bold {
                    named = named.to_bright();
                } else if dim {
                    named = named.to_dim();
                }
                if let Some(rgb) = dynamic[named] {
                    [rgb.r, rgb.g, rgb.b, 255]
                } else {
                    match named {
                        NamedColor::Foreground | NamedColor::BrightForeground => self.fg,
                        NamedColor::Background => self.bg,
                        NamedColor::Cursor => self.cursor,
                        NamedColor::DimForeground => dim_rgba(self.fg),
                        other if (other as usize) < 16 => self.palette[other as usize],
                        other if (NamedColor::DimBlack as usize
                            ..=NamedColor::DimWhite as usize)
                            .contains(&(other as usize)) =>
                        {
                            let base = other as usize - NamedColor::DimBlack as usize;
                            dim_rgba(self.palette[base])
                        }
                        _ => fallback,
                    }
                }
            }
            Color::Spec(rgb) => [rgb.r, rgb.g, rgb.b, 255],
            Color::Indexed(idx) => {
                let idx = if bold && idx < 8 { idx + 8 } else { idx };
                if let Some(rgb) = dynamic[idx as usize] {
                    [rgb.r, rgb.g, rgb.b, 255]
                } else {
                    self.palette[idx as usize]
                }
            }
        };
        // Named + dim (without bold) already mapped via to_dim. Everything else dims in RGB.
        let named_already_dimmed = matches!(color, Color::Named(_)) && dim && !bold;
        if dim && !named_already_dimmed {
            rgba = dim_rgba(rgba);
        }
        rgba
    }

    fn blit_glyph(
        &mut self,
        rgba: &mut [u8],
        width: u32,
        col: u32,
        row: u32,
        ch: char,
        fg: [u8; 4],
        bg: [u8; 4],
    ) {
        let x0 = col * self.cell_w;
        let y0 = row * self.cell_h;
        let (glyph, cover) = self.glyph(ch);
        let frame_w = width as i32;
        let frame_h = (rgba.len() / 4 / width as usize) as i32;
        let cell_x0 = x0 as i32;
        let cell_y0 = y0 as i32;
        let cell_x1 = cell_x0 + self.cell_w as i32;
        let cell_y1 = cell_y0 + self.cell_h as i32;
        if glyph.fit == GlyphFit::Cell {
            let scaled = scale_cover(&cover, glyph.w, glyph.h, self.cell_w, self.cell_h);
            let snapped = snap_cell_edges(&scaled, self.cell_w, self.cell_h, cell_attach(ch));
            for dy in 0..self.cell_h {
                for dx in 0..self.cell_w {
                    let cover_v = snapped[(dy * self.cell_w + dx) as usize];
                    if cover_v == 0 {
                        continue;
                    }
                    let mixed = blend(bg, [fg[0], fg[1], fg[2], cover_v]);
                    put(rgba, width, x0 + dx, y0 + dy, mixed);
                }
            }
            return;
        }
        let (mut dest_x, dest_y, dest_w, dest_h, bits) = {
            let mut dw = glyph.w;
            let mut dh = glyph.h;
            let mut xmin = glyph.xmin;
            let mut ymin = glyph.ymin;
            let bits = if dw > self.cell_w && dw > 0 {
                let s = self.cell_w as f32 / dw as f32;
                dw = self.cell_w;
                dh = (dh as f32 * s).round().max(1.0) as u32;
                xmin = (xmin as f32 * s).round() as i32;
                ymin = (ymin as f32 * s).round() as i32;
                scale_cover(&cover, glyph.w, glyph.h, dw, dh)
            } else {
                cover
            };
            let dest_x = x0 as i32 + xmin;
            let dest_y_base = y0 as i32 + self.baseline - ymin - dh as i32;
            let dest_y = if glyph.fit == GlyphFit::Icon {
                let dest_y_mid = y0 as i32 + (self.cell_h as i32 - dh as i32) / 2;
                (dest_y_base + dest_y_mid) / 2
            } else {
                dest_y_base
            };
            (dest_x, dest_y, dw, dh, bits)
        };
        if dest_x < cell_x0 {
            dest_x = cell_x0;
        }
        for gy in 0..dest_h {
            for gx in 0..dest_w {
                let cover_v = sharpen_text_cover(bits[(gy * dest_w + gx) as usize]);
                if cover_v == 0 {
                    continue;
                }
                let px = dest_x + gx as i32;
                let py = dest_y + gy as i32;
                if px < cell_x0 || py < cell_y0 || px >= cell_x1 || py >= cell_y1 {
                    continue;
                }
                if px < 0 || py < 0 || px >= frame_w || py >= frame_h {
                    continue;
                }
                let mixed = blend(bg, [fg[0], fg[1], fg[2], cover_v]);
                put(rgba, width, px as u32, py as u32, mixed);
            }
        }
    }

    fn glyph(&mut self, ch: char) -> (Glyph, Vec<u8>) {
        if let Some((g, bits)) = self.glyphs.get(&ch) {
            return (*g, bits.clone());
        }
        let font_idx = pick_font_idx(&self.fonts, ch);
        let fit = glyph_fit(ch, font_idx);
        let px = match fit {
            GlyphFit::Cell => self.cell_h as f32,
            GlyphFit::Icon => self.font_px.max(self.cell_h as f32 * 0.92),
            GlyphFit::Text => self.font_px,
        };
        let (metrics, bitmap) = self.fonts[font_idx].rasterize(ch, px);
        let glyph = Glyph {
            w: metrics.width as u32,
            h: metrics.height as u32,
            xmin: metrics.xmin,
            ymin: metrics.ymin,
            fit,
        };
        self.glyphs.insert(ch, (glyph, bitmap.clone()));
        (glyph, bitmap)
    }

    fn fill_default_palette(&mut self) {
        const ANSI: [[u8; 3]; 16] = [
            [58, 58, 60],
            [255, 69, 58],
            [48, 209, 88],
            [255, 214, 10],
            [10, 132, 255],
            [191, 90, 242],
            [100, 210, 255],
            [229, 229, 234],
            [99, 99, 102],
            [255, 105, 97],
            [48, 219, 91],
            [255, 212, 38],
            [64, 156, 255],
            [218, 143, 255],
            [112, 215, 255],
            [255, 255, 255],
        ];
        for (i, [r, g, b]) in ANSI.into_iter().enumerate() {
            self.palette[i] = [r, g, b, 255];
        }
        for i in 16..232usize {
            let n = (i - 16) as u8;
            let r = cube(n / 36);
            let g = cube((n / 6) % 6);
            let b = cube(n % 6);
            self.palette[i] = [r, g, b, 255];
        }
        for i in 232..256usize {
            let v = (8 + (i - 232) * 10) as u8;
            self.palette[i] = [v, v, v, 255];
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CellAttach {
    Left,
    Right,
    Both,
}

fn cell_attach(ch: char) -> CellAttach {
    match ch {
        '\u{e0b0}' | '\u{e0b1}' | '\u{e0b4}' | '\u{e0b5}' | '\u{e0b8}' | '\u{e0b9}'
        | '\u{e0bc}' | '\u{e0bd}' | '\u{e0c0}' | '\u{e0c1}' | '\u{e0c4}' | '\u{e0c5}'
        | '\u{e0c8}' | '\u{e0cc}' | '\u{e0d0}' | '\u{e0d2}' | '\u{2590}' => CellAttach::Left,
        '\u{e0b2}' | '\u{e0b3}' | '\u{e0b6}' | '\u{e0b7}' | '\u{e0ba}' | '\u{e0bb}'
        | '\u{e0be}' | '\u{e0bf}' | '\u{e0c2}' | '\u{e0c3}' | '\u{e0c6}' | '\u{e0c7}'
        | '\u{e0ca}' | '\u{e0ce}' | '\u{e0d1}' | '\u{e0d4}' | '\u{258c}' => CellAttach::Right,
        _ => CellAttach::Both,
    }
}

fn glyph_fit(ch: char, _font_idx: usize) -> GlyphFit {
    match ch {
        '\u{e0a0}'..='\u{e0d4}' | '\u{2580}'..='\u{259f}' | '\u{25e2}'..='\u{25e5}' => GlyphFit::Cell,
        '\u{e000}'..='\u{e09f}' | '\u{e0d5}'..='\u{f8ff}' | '\u{f0000}'..='\u{ffffd}' => GlyphFit::Icon,
        _ => GlyphFit::Text,
    }
}

fn sanitize_scale(scale: f32) -> f32 {
    if !scale.is_finite() {
        return 1.0;
    }
    scale.clamp(1.0, 4.0)
}

fn sharpen_text_cover(v: u8) -> u8 {
    if v < 22 {
        0
    } else if v > 160 {
        255
    } else {
        let t = v as f32 / 255.0;
        (t.powf(0.62) * 255.0).round().clamp(0.0, 255.0) as u8
    }
}

fn scale_cover(cover: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
        return vec![0; (dst_w * dst_h) as usize];
    }
    if src_w == dst_w && src_h == dst_h {
        return cover.to_vec();
    }
    let mut out = vec![0u8; (dst_w * dst_h) as usize];
    for y in 0..dst_h {
        for x in 0..dst_w {
            out[(y * dst_w + x) as usize] = sample_cover(cover, src_w, src_h, x, y, dst_w, dst_h);
        }
    }
    out
}

fn snap_cell_edges(cover: &[u8], w: u32, h: u32, attach: CellAttach) -> Vec<u8> {
    let mut out = cover.to_vec();
    for v in &mut out {
        *v = if *v > 96 {
            255
        } else if *v < 10 {
            0
        } else {
            *v
        };
    }
    let snap_col = |buf: &mut [u8], x: u32| {
        let mut ink = 0u32;
        for y in 0..h {
            if buf[(y * w + x) as usize] > 40 {
                ink += 1;
            }
        }
        if ink * 2 >= h {
            for y in 0..h {
                if buf[(y * w + x) as usize] > 8 {
                    buf[(y * w + x) as usize] = 255;
                }
            }
        }
        if ink * 5 >= h * 4 {
            for y in 0..h {
                buf[(y * w + x) as usize] = 255;
            }
        }
    };
    let snap_row = |buf: &mut [u8], y: u32| {
        let mut ink = 0u32;
        for x in 0..w {
            if buf[(y * w + x) as usize] > 40 {
                ink += 1;
            }
        }
        if ink * 2 >= w {
            for x in 0..w {
                if buf[(y * w + x) as usize] > 8 {
                    buf[(y * w + x) as usize] = 255;
                }
            }
        }
    };
    match attach {
        CellAttach::Left | CellAttach::Both => {
            snap_col(&mut out, 0);
            if w > 1 {
                snap_col(&mut out, 1);
            }
        }
        CellAttach::Right => {}
    }
    match attach {
        CellAttach::Right | CellAttach::Both => {
            snap_col(&mut out, w - 1);
            if w > 1 {
                snap_col(&mut out, w - 2);
            }
        }
        CellAttach::Left => {}
    }
    snap_row(&mut out, 0);
    snap_row(&mut out, h - 1);
    out
}

fn sample_cover(cover: &[u8], src_w: u32, src_h: u32, x: u32, y: u32, dst_w: u32, dst_h: u32) -> u8 {
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
        return 0;
    }
    let fx = (x as f32 + 0.5) * src_w as f32 / dst_w as f32 - 0.5;
    let fy = (y as f32 + 0.5) * src_h as f32 / dst_h as f32 - 0.5;
    let x0 = fx.floor() as i32;
    let y0 = fy.floor() as i32;
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let tx = fx - x0 as f32;
    let ty = fy - y0 as f32;
    let at = |x: i32, y: i32| -> f32 {
        let x = x.clamp(0, src_w as i32 - 1) as u32;
        let y = y.clamp(0, src_h as i32 - 1) as u32;
        cover[(y * src_w + x) as usize] as f32
    };
    let v = at(x0, y0) * (1.0 - tx) * (1.0 - ty)
        + at(x1, y0) * tx * (1.0 - ty)
        + at(x0, y1) * (1.0 - tx) * ty
        + at(x1, y1) * tx * ty;
    v.round().clamp(0.0, 255.0) as u8
}

fn pick_font_idx(fonts: &[Font], ch: char) -> usize {
    let nerd_range = matches!(ch, '\u{e000}'..='\u{f8ff}' | '\u{f0000}'..='\u{ffffd}');
    if nerd_range {
        if let Some(i) = fonts
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, font)| font.has_glyph(ch))
            .map(|(i, _)| i)
        {
            return i;
        }
    }
    fonts.iter().position(|font| font.has_glyph(ch)).unwrap_or(0)
}

fn load_fonts() -> Result<Vec<Font>> {
    // Cascadia Mono NF: Latin + Braille + Nerd Font icons in one face.
    let primary = Font::from_bytes(FONT_TTF, FontSettings::default())
        .map_err(|err| Error::msg(format!("font: {err}")))?;
    Ok(vec![primary])
}

fn metrics_for(font: &Font, font_px: f32, line_height: f32) -> (f32, u32, u32, i32) {
    let px = font_px.max(10.0);
    let line = font
        .horizontal_line_metrics(px)
        .expect("Cascadia Mono NF has line metrics");
    let em = font.metrics('M', px);
    let cell_w = em.advance_width.ceil().max(1.0) as u32;
    let typo = (line.ascent - line.descent).max(1.0);
    let cell_h = (typo * line_height.max(1.0)).round().max(typo.ceil()) as u32;
    let slack = cell_h as f32 - typo;
    let baseline = (slack / 2.0).floor() as i32 + line.ascent.round() as i32;
    (px, cell_w, cell_h, baseline)
}

fn dim_rgba(c: [u8; 4]) -> [u8; 4] {
    [c[0] / 2, c[1] / 2, c[2] / 2, c[3]]
}

fn fill_cell(rgba: &mut [u8], width: u32, x0: u32, y0: u32, cell_w: u32, cell_h: u32, bg: [u8; 4]) {
    for y in 0..cell_h {
        for x in 0..cell_w {
            put(rgba, width, x0 + x, y0 + y, bg);
        }
    }
}

fn cube(n: u8) -> u8 {
    if n == 0 {
        0
    } else {
        55 + 40 * n
    }
}

fn parse_hex(hex: &str) -> [u8; 4] {
    let raw = hex.trim().trim_start_matches('#');
    let full = if raw.len() == 3 {
        raw.chars().flat_map(|c| [c, c]).collect::<String>()
    } else {
        raw.to_string()
    };
    let n = u32::from_str_radix(full.get(..6).unwrap_or("000000"), 16).unwrap_or(0);
    [
        ((n >> 16) & 0xff) as u8,
        ((n >> 8) & 0xff) as u8,
        (n & 0xff) as u8,
        255,
    ]
}

fn put(rgba: &mut [u8], width: u32, x: u32, y: u32, color: [u8; 4]) {
    let i = ((y * width + x) * 4) as usize;
    if i + 3 < rgba.len() {
        rgba[i..i + 4].copy_from_slice(&color);
    }
}

fn blend(bg: [u8; 4], fg: [u8; 4]) -> [u8; 4] {
    let a = fg[3] as u16;
    if a == 0 {
        return bg;
    }
    if a == 255 {
        return [fg[0], fg[1], fg[2], 255];
    }
    let ia = 255 - a;
    [
        ((fg[0] as u16 * a + bg[0] as u16 * ia) / 255) as u8,
        ((fg[1] as u16 * a + bg[1] as u16 * ia) / 255) as u8,
        ((fg[2] as u16 * a + bg[2] as u16 * ia) / 255) as u8,
        255,
    ]
}

pub fn pack_frame(
    frame: &TermFrame,
    cell_w: u32,
    cell_h: u32,
    mode_flags: u32,
    scroll_offset: u32,
    scroll_max: u32,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(28 + frame.rgba.len());
    out.extend_from_slice(&frame.width.to_le_bytes());
    out.extend_from_slice(&frame.height.to_le_bytes());
    out.extend_from_slice(&cell_w.to_le_bytes());
    out.extend_from_slice(&cell_h.to_le_bytes());
    out.extend_from_slice(&mode_flags.to_le_bytes());
    out.extend_from_slice(&scroll_offset.to_le_bytes());
    out.extend_from_slice(&scroll_max.to_le_bytes());
    out.extend_from_slice(&frame.rgba);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell_ink(frame: &TermFrame, cell_w: u32, cell_h: u32, col: u32, row: u32) -> (usize, usize, usize) {
        let bg = [28u8, 28, 30, 255];
        let x0 = col * cell_w;
        let y0 = row * cell_h;
        let mut top = 0;
        let mut mid = 0;
        let mut bot = 0;
        for y in 0..cell_h {
            for x in 0..cell_w {
                let i = (((y0 + y) * frame.width + x0 + x) * 4) as usize;
                if frame.rgba[i..i + 4] == bg {
                    continue;
                }
                if y < cell_h / 3 {
                    top += 1;
                } else if y < (cell_h * 2) / 3 {
                    mid += 1;
                } else {
                    bot += 1;
                }
            }
        }
        (top, mid, bot)
    }

    #[test]
    fn latin_glyphs_sit_on_the_baseline() {
        let mut term = TerminalEmulator::new(8, 2, 14.0).unwrap();
        term.feed(b"Hg");
        let (cell_w, cell_h) = term.cell_size();
        let frame = term.raster();
        let (top, mid, bot) = cell_ink(&frame, cell_w, cell_h, 0, 0);
        let ink = top + mid + bot;
        assert!(ink > 20, "H should paint, got {ink} px");
        assert!(
            mid + top > bot,
            "H was clipped to the bottom of the cell (top={top} mid={mid} bot={bot})"
        );
    }

    #[test]
    fn descenders_are_not_clipped() {
        let mut term = TerminalEmulator::new(8, 2, 14.0).unwrap();
        term.feed(b"g");
        let (cell_w, cell_h) = term.cell_size();
        let frame = term.raster();
        let (top, mid, bot) = cell_ink(&frame, cell_w, cell_h, 0, 0);
        let ink = top + mid + bot;
        assert!(ink > 20, "g should paint, got {ink} px");
        assert!(bot > 0, "g lost its descender (top={top} mid={mid} bot={bot})");
    }

    #[test]
    fn nerd_font_icons_paint() {
        let mut term = TerminalEmulator::new(8, 2, 14.0).unwrap();
        term.feed("\u{e0b0}\u{f007}".as_bytes());
        let (cell_w, cell_h) = term.cell_size();
        let frame = term.raster();
        let (top, mid, bot) = cell_ink(&frame, cell_w, cell_h, 0, 0);
        let ink = top + mid + bot;
        assert!(ink > 10, "nerd icon should paint, got {ink} px");
    }

    #[test]
    fn powerline_round_fills_the_cell() {
        let mut term = TerminalEmulator::new(8, 2, 14.0).unwrap();
        term.feed("\u{e0b4}".as_bytes());
        let (cell_w, cell_h) = term.cell_size();
        let frame = term.raster();
        let (top, mid, bot) = cell_ink(&frame, cell_w, cell_h, 0, 0);
        assert!(top > 0, "round cap missing the top of the cell (top={top})");
        assert!(bot > 0, "round cap missing the bottom of the cell (bot={bot})");
        assert!(
            top + mid + bot > (cell_w * cell_h / 5) as usize,
            "round cap too small (top={top} mid={mid} bot={bot} cell={cell_w}x{cell_h})"
        );
    }

    #[test]
    fn powerline_round_sits_flush_left() {
        let mut term = TerminalEmulator::new(8, 2, 14.0).unwrap();
        term.feed("\u{e0b4}".as_bytes());
        let (_cell_w, cell_h) = term.cell_size();
        let frame = term.raster();
        let bg = [28u8, 28, 30, 255];
        let mut ink = 0;
        for y in 0..cell_h {
            let i = ((y * frame.width) * 4) as usize;
            if frame.rgba[i..i + 4] != bg {
                ink += 1;
            }
        }
        assert!(
            ink > cell_h as usize / 2,
            "1px gap on the left of the round cap ({ink}/{cell_h})"
        );
    }

    fn cell_is_bg(frame: &TermFrame, cell_w: u32, cell_h: u32, col: u32, row: u32, bg: [u8; 4]) -> bool {
        let x0 = col * cell_w;
        let y0 = row * cell_h;
        for y in 0..cell_h {
            for x in 0..cell_w {
                let i = (((y0 + y) * frame.width + x0 + x) * 4) as usize;
                if frame.rgba[i..i + 4] != bg {
                    return false;
                }
            }
        }
        true
    }

    #[test]
    fn nerd_folder_icon_stays_inside_its_cell() {
        let mut term = TerminalEmulator::new(8, 2, 14.0).unwrap();
        term.feed("\u{f07b} ".as_bytes());
        let (cell_w, cell_h) = term.cell_size();
        let frame = term.raster();
        let bg = [28u8, 28, 30, 255];
        let (top, mid, bot) = cell_ink(&frame, cell_w, cell_h, 0, 0);
        assert!(top + mid + bot > 10, "folder icon should paint");
        assert!(
            cell_is_bg(&frame, cell_w, cell_h, 1, 0, bg),
            "folder icon overflowed into the next cell"
        );
    }

    #[test]
    fn powerline_round_does_not_bleed_into_next_cell() {
        let mut term = TerminalEmulator::new(8, 2, 14.0).unwrap();
        term.feed("\u{e0b4} ".as_bytes());
        let (cell_w, cell_h) = term.cell_size();
        let frame = term.raster();
        let bg = [28u8, 28, 30, 255];
        assert!(
            cell_is_bg(&frame, cell_w, cell_h, 1, 0, bg),
            "powerline cap bled into the next cell"
        );
    }

    #[test]
    fn powerline_joins_previous_colored_cell() {
        let mut term = TerminalEmulator::new(8, 2, 14.0).unwrap();
        term.feed(b"\x1b[44;37m \x1b[34;49m\xee\x82\xb4\x1b[0m");
        let (cell_w, cell_h) = term.cell_size();
        let frame = term.raster();
        let bg = [28u8, 28, 30, 255];
        let mut gaps = 0usize;
        for y in 0..cell_h {
            let left = (((y * frame.width) + (cell_w - 1)) * 4) as usize;
            let right = (((y * frame.width) + cell_w) * 4) as usize;
            let a = &frame.rgba[left..left + 4];
            let b = &frame.rgba[right..right + 4];
            if a == bg || b == bg {
                gaps += 1;
            }
        }
        assert!(
            gaps * 4 < cell_h as usize,
            "vertical seam between prompt bg and rounded cap ({gaps}/{cell_h})"
        );
    }

    fn ink_height(frame: &TermFrame, cell_w: u32, cell_h: u32, col: u32) -> usize {
        let bg = [28u8, 28, 30, 255];
        let x0 = col * cell_w;
        let mut min_y = cell_h;
        let mut max_y = 0;
        for y in 0..cell_h {
            for x in 0..cell_w {
                let i = ((y * frame.width + x0 + x) * 4) as usize;
                if frame.rgba[i..i + 4] != bg {
                    min_y = min_y.min(y);
                    max_y = max_y.max(y);
                }
            }
        }
        if min_y > max_y {
            0
        } else {
            (max_y - min_y + 1) as usize
        }
    }

    #[test]
    fn chevron_is_text_and_crisp() {
        let mut term = TerminalEmulator::new(8, 2, 14.0).unwrap();
        term.feed("❯ ".as_bytes());
        let (cell_w, cell_h) = term.cell_size();
        let frame = term.raster();
        let bg = [28u8, 28, 30, 255];
        let mut solid = 0usize;
        let mut fringe = 0usize;
        for y in 0..cell_h {
            for x in 0..cell_w {
                let i = ((y * frame.width + x) * 4) as usize;
                let px = &frame.rgba[i..i + 4];
                if px == bg {
                    continue;
                }
                let maxc = px[0].max(px[1]).max(px[2]);
                if maxc > 200 {
                    solid += 1;
                } else if maxc > 40 {
                    fringe += 1;
                }
            }
        }
        assert!(solid + fringe > 16, "chevron produced almost no ink");
        assert!(
            solid * 3 >= fringe,
            "chevron looks washed out (solid={solid} fringe={fringe})"
        );
        assert!(
            cell_is_bg(&frame, cell_w, cell_h, 1, 0, bg),
            "chevron overflowed the next cell"
        );
    }

    #[test]
    fn folder_icon_matches_text_size() {
        let mut icon = TerminalEmulator::new(8, 2, 14.0).unwrap();
        icon.feed("\u{f07b} ".as_bytes());
        let mut text = TerminalEmulator::new(8, 2, 14.0).unwrap();
        text.feed("~M".as_bytes());
        let (cell_w, cell_h) = icon.cell_size();
        let icon_h = ink_height(&icon.raster(), cell_w, cell_h, 0);
        let text_h = ink_height(&text.raster(), cell_w, cell_h, 1);
        assert!(icon_h >= 8, "folder icon too small ({icon_h}px)");
        assert!(
            icon_h * 10 >= text_h * 6,
            "folder icon much smaller than text (icon={icon_h} text={text_h})"
        );
    }

    #[test]
    fn hidpi_scale_grows_the_raster() {
        let mut term = TerminalEmulator::new_with_scale(8, 2, 14.0, 1.0, 2.0).unwrap();
        term.feed(b"A");
        let (cell_w, cell_h) = term.cell_size();
        let frame = term.raster();
        assert!(cell_w >= 16, "2x scale should double cell width, got {cell_w}");
        assert_eq!(frame.width, cell_w * 8);
        assert_eq!(frame.height, cell_h * 2);
    }

    #[test]
    fn force_capture_returns_the_last_screen_when_clean() {
        let mut term = TerminalEmulator::new(8, 2, 14.0).unwrap();
        term.feed(b"hi");
        assert!(term.take_frame().is_some());
        assert!(term.take_frame().is_none(), "clean session has no dirty frame");
        let forced = term.capture_frame(true).expect("tab switch must still raster");
        assert!(forced.width > 0 && forced.height > 0);
        assert!(!forced.rgba.is_empty());
    }

    #[test]
    fn primary_da_query_writes_device_attributes_reply() {
        let mut term = TerminalEmulator::new(80, 24, 14.0).unwrap();
        let replies = term.feed(b"\x1b[c");
        assert_eq!(
            replies, b"\x1b[?6c",
            "Primary Device Attributes must be answered for fish ≥ 4.1"
        );
    }

    #[test]
    fn secondary_da_query_writes_reply() {
        let mut term = TerminalEmulator::new(80, 24, 14.0).unwrap();
        let replies = term.feed(b"\x1b[>c");
        assert!(
            replies.starts_with(b"\x1b[>0;") && replies.ends_with(b"c"),
            "secondary DA reply expected, got {replies:?}"
        );
    }

    #[test]
    fn plain_text_feed_does_not_emit_pty_replies() {
        let mut term = TerminalEmulator::new(80, 24, 14.0).unwrap();
        let replies = term.feed(b"hello");
        assert!(replies.is_empty(), "printable text must not invent PtyWrite bytes");
        let frame = term.capture_frame(true).expect("text should raster");
        assert!(!frame.rgba.is_empty());
    }

    #[test]
    fn gpu_frame_hot_path_is_compact_and_magic() {
        let mut term = TerminalEmulator::new(80, 24, 14.0).unwrap();
        term.feed(b"hello GPU atlas");
        let gpu = term.capture_gpu_frame(true).expect("gpu frame");
        let packed = crate::gpu_frame::pack_gpu_frame(&gpu);
        assert!(crate::gpu_frame::is_gpu_frame(&packed));
        let (cw, ch) = term.cell_size();
        let rgba_size = (80 * cw * 24 * ch * 4) as usize;
        assert!(
            packed.len() < rgba_size / 8,
            "gpu pack {} should beat rgba {}",
            packed.len(),
            rgba_size
        );
        // Second frame without new glyphs should omit glyph bitmaps.
        term.feed(b"hello GPU atlas");
        let gpu2 = term.capture_gpu_frame(true).expect("second gpu frame");
        assert!(
            gpu2.glyphs.iter().all(|g| g.bits.is_none()),
            "repeated glyphs must not resend atlas stamps"
        );
        let packed2 = crate::gpu_frame::pack_gpu_frame(&gpu2);
        assert!(
            packed2.len() < packed.len(),
            "steady frame should drop stamp bytes ({} vs {})",
            packed2.len(),
            packed.len()
        );
    }

    fn cell_ink_avg(
        frame: &TermFrame,
        cell_w: u32,
        cell_h: u32,
        col: u32,
        row: u32,
        bg: [u8; 4],
    ) -> [u32; 3] {
        let x0 = col * cell_w;
        let y0 = row * cell_h;
        let mut sum = [0u32; 3];
        let mut n = 0u32;
        for y in 0..cell_h {
            for x in 0..cell_w {
                let i = (((y0 + y) * frame.width + x0 + x) * 4) as usize;
                let px = &frame.rgba[i..i + 4];
                if px == bg {
                    continue;
                }
                // Skip near-bg anti-alias fringe so attribute color dominates.
                let dr = px[0].abs_diff(bg[0]) as u32;
                let dg = px[1].abs_diff(bg[1]) as u32;
                let db = px[2].abs_diff(bg[2]) as u32;
                if dr + dg + db < 40 {
                    continue;
                }
                sum[0] += px[0] as u32;
                sum[1] += px[1] as u32;
                sum[2] += px[2] as u32;
                n += 1;
            }
        }
        assert!(n > 5, "expected painted ink in cell, got {n} px");
        [sum[0] / n, sum[1] / n, sum[2] / n]
    }

    fn raster_with(sgr: &str) -> ([u32; 3], [u8; 4]) {
        let mut term = TerminalEmulator::new(8, 2, 14.0).unwrap();
        let bg = term.bg;
        // Hide cursor so it does not tint the sample cell.
        let mut seq = String::from("\x1b[?25l");
        seq.push_str(sgr);
        seq.push('#');
        term.feed(seq.as_bytes());
        let (cell_w, cell_h) = term.cell_size();
        let frame = term.raster();
        (cell_ink_avg(&frame, cell_w, cell_h, 0, 0, bg), bg)
    }

    fn luma(rgb: [u32; 3]) -> u32 {
        rgb[0] * 30 + rgb[1] * 59 + rgb[2] * 11
    }

    #[test]
    fn ac1_dim_ansi_red_is_darker_than_normal() {
        let (normal, _) = raster_with("\x1b[31m");
        let (dimmed, _) = raster_with("\x1b[2;31m");
        assert!(
            luma(dimmed) + 200 < luma(normal),
            "DIM must darken ANSI red, not brighten it (dim={dimmed:?} normal={normal:?})"
        );
    }

    #[test]
    fn ac2_bold_ansi_red_is_brighter_than_normal() {
        let (normal, _) = raster_with("\x1b[31m");
        let (bold, _) = raster_with("\x1b[1;31m");
        assert!(
            luma(bold) > luma(normal) + 80,
            "BOLD must brighten ANSI red (bold={bold:?} normal={normal:?})"
        );
    }

    #[test]
    fn dim_plus_bold_ansi_red_is_darker_than_bold_alone() {
        let (bold, _) = raster_with("\x1b[1;31m");
        let (both, _) = raster_with("\x1b[1;2;31m");
        assert!(
            luma(both) + 200 < luma(bold),
            "DIM+BOLD should not look like pure bright (both={both:?} bold={bold:?})"
        );
    }

    #[test]
    fn dim_truecolor_spec_is_darker_than_undimmed() {
        let (normal, _) = raster_with("\x1b[38;2;200;80;40m");
        let (dimmed, _) = raster_with("\x1b[2;38;2;200;80;40m");
        assert!(
            luma(dimmed) + 400 < luma(normal),
            "DIM must darken Color::Spec (dim={dimmed:?} normal={normal:?})"
        );
    }

    #[test]
    fn ac3_app_cursor_mode_sets_pack_frame_flag() {
        let mut term = TerminalEmulator::new(40, 12, 14.0).unwrap();
        assert_eq!(term.mode_flags() & 0b0001, 0, "default: app cursor off");
        // DECCKM set — what htop/ncurses enable under xterm.
        term.feed(b"\x1b[?1h");
        assert_eq!(term.mode_flags() & 0b0001, 0b0001, "DECCKM should set APP_CURSOR");
        let (cw, ch) = term.cell_size();
        let frame = term.capture_frame(true).expect("frame");
        let packed = pack_frame(&frame, cw, ch, term.mode_flags(), 0, 0);
        assert!(packed.len() >= 28 + frame.rgba.len());
        let flags = u32::from_le_bytes(packed[16..20].try_into().unwrap());
        assert_eq!(flags & 0b0001, 0b0001);
        term.feed(b"\x1b[?1l");
        assert_eq!(term.mode_flags() & 0b0001, 0, "DECCKM reset clears APP_CURSOR");
    }

    #[test]
    fn ac1_scroll_delta_moves_into_history() {
        let mut term = TerminalEmulator::new(40, 5, 14.0).unwrap();
        for i in 0..20 {
            term.feed(format!("line{i}\n").as_bytes());
        }
        assert_eq!(term.display_offset(), 0);
        assert!(term.scroll_delta(3));
        assert!(
            term.display_offset() >= 3,
            "wheel-up should reveal history (offset={})",
            term.display_offset()
        );
    }

    #[test]
    fn ac2_scroll_delta_false_on_alt_screen() {
        let mut term = TerminalEmulator::new(40, 5, 14.0).unwrap();
        term.feed(b"\x1b[?1049h");
        assert_eq!(term.mode_flags() & 0b0100, 0b0100);
        assert!(!term.scroll_delta(2));
        assert_eq!(term.display_offset(), 0);
    }

    #[test]
    fn scroll_delta_zero_is_noop() {
        let mut term = TerminalEmulator::new(40, 5, 14.0).unwrap();
        assert!(term.scroll_delta(0));
        assert_eq!(term.display_offset(), 0);
    }

    #[test]
    fn ac1_scroll_state_reports_offset_and_max() {
        let mut term = TerminalEmulator::new(40, 5, 14.0).unwrap();
        for i in 0..20 {
            term.feed(format!("line{i}\n").as_bytes());
        }
        let (offset, max) = term.scroll_state();
        assert_eq!(offset, 0);
        assert!(max >= 10, "expected history above viewport, max={max}");
        assert!(term.scroll_delta(4));
        let (offset2, max2) = term.scroll_state();
        assert_eq!(offset2, 4);
        assert_eq!(max2, max);
    }

    #[test]
    fn ac2_scroll_to_sets_absolute_offset() {
        let mut term = TerminalEmulator::new(40, 5, 14.0).unwrap();
        for i in 0..20 {
            term.feed(format!("line{i}\n").as_bytes());
        }
        let (_, max) = term.scroll_state();
        assert!(term.scroll_to(max.saturating_sub(2)));
        assert_eq!(term.display_offset(), max.saturating_sub(2) as usize);
        assert!(term.scroll_to(0));
        assert_eq!(term.display_offset(), 0);
    }

    #[test]
    fn ac3_scroll_state_zero_on_alt_screen() {
        let mut term = TerminalEmulator::new(40, 5, 14.0).unwrap();
        for i in 0..20 {
            term.feed(format!("line{i}\n").as_bytes());
        }
        term.feed(b"\x1b[?1049h");
        assert_eq!(term.scroll_state(), (0, 0));
        assert!(!term.scroll_to(3));
    }

    #[test]
    fn pack_frame_includes_scroll_metrics() {
        let mut term = TerminalEmulator::new(40, 5, 14.0).unwrap();
        for i in 0..20 {
            term.feed(format!("line{i}\n").as_bytes());
        }
        term.scroll_delta(5);
        let (cw, ch) = term.cell_size();
        let (off, max) = term.scroll_state();
        let frame = term.capture_frame(true).expect("frame");
        let packed = pack_frame(&frame, cw, ch, term.mode_flags(), off, max);
        assert_eq!(packed.len(), 28 + frame.rgba.len());
        let got_off = u32::from_le_bytes(packed[20..24].try_into().unwrap());
        let got_max = u32::from_le_bytes(packed[24..28].try_into().unwrap());
        assert_eq!(got_off, off);
        assert_eq!(got_max, max);
        assert!(got_max > 0);
    }

    #[test]
    fn ac1_braille_spinner_glyphs_have_ink() {
        let mut term = TerminalEmulator::new(8, 2, 14.0).unwrap();
        // Cursor/CLI spinners use Braille (missing in old IBM Plex + Symbols NF).
        term.feed("⠰⠳".as_bytes());
        let (cell_w, cell_h) = term.cell_size();
        let frame = term.raster();
        let (t0, m0, b0) = cell_ink(&frame, cell_w, cell_h, 0, 0);
        let (t1, m1, b1) = cell_ink(&frame, cell_w, cell_h, 1, 0);
        assert!(
            t0 + m0 + b0 > 8,
            "Braille U+2838 must paint (got {})",
            t0 + m0 + b0
        );
        assert!(
            t1 + m1 + b1 > 8,
            "Braille U+283D must paint (got {})",
            t1 + m1 + b1
        );
    }

    #[test]
    fn ac2_nerd_powerline_glyph_has_ink() {
        let mut term = TerminalEmulator::new(8, 2, 14.0).unwrap();
        term.feed("\u{e0b0}".as_bytes());
        let (cell_w, cell_h) = term.cell_size();
        let frame = term.raster();
        let (t, m, b) = cell_ink(&frame, cell_w, cell_h, 0, 0);
        assert!(t + m + b > 20, "Nerd/powerline U+E0B0 must paint (got {})", t + m + b);
    }

    #[test]
    fn bundled_cascadia_mono_nf_is_substantial() {
        assert!(
            FONT_TTF.len() > 2_000_000,
            "expected Cascadia Mono NF Regular embed, got {} bytes",
            FONT_TTF.len()
        );
    }
}
