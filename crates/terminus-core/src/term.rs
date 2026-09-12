use crate::box_draw::{is_procedural_cell, render_box_cell};
use crate::error::{Error, Result};
use crate::gpu_frame::{
    rgba_to_u32, stamp_bytes, AtlasSprite, GpuCell, GpuFrame, ATTR_BOLD, ATTR_COLORED, ATTR_DIM,
    ATTR_ITALIC, ATTR_STRIKE, ATTR_UNDERLINE_SINGLE, STAMP_FMT_R8, STAMP_FMT_RGBA,
};
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
use rustybuzz::Face;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

const FONT_TTF: &[u8] = include_bytes!("../fonts/CascadiaMonoNF-Regular.ttf");
const SCROLLBACK: usize = 2000;
const SPRITES_PER_LAYER: u16 = 64;

#[derive(Clone)]
struct AtlasEntry {
    format: u8,
    bits: Vec<u8>,
}

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
    /// Optional color-emoji font file bytes for FreeType `FT_LOAD_COLOR`.
    emoji_font_bytes: Option<Vec<u8>>,
    hb_face: Face<'static>,
    glyphs: HashMap<char, (Glyph, Vec<u8>)>,
    /// char / shaped key → (sprite_idx, sprite_layer)
    atlas_glyphs: HashMap<u64, (u16, u16)>,
    atlas_bits: HashMap<(u16, u16), AtlasEntry>,
    atlas_pending: HashSet<(u16, u16)>,
    /// sprite slot → colored flag (ATTR_COLORED) for cells referencing it
    atlas_colored: HashSet<(u16, u16)>,
    atlas_dirty: bool,
    atlas_fill_idx: u16,
    atlas_fill_layer: u16,
    atlas_layer_count: u16,
    sprites_per_layer: u16,
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
        Self::new_with_fonts(cols, rows, font_px, line_height, scale, &[])
    }

    /// Build an emulator with Cascadia primary plus optional extra font faces (for tests / CI).
    pub fn new_with_extra_font_bytes(
        cols: u16,
        rows: u16,
        font_px: f32,
        extra_fonts: &[&[u8]],
    ) -> Result<Self> {
        Self::new_with_fonts(cols, rows, font_px, 1.0, 1.0, extra_fonts)
    }

    fn new_with_fonts(
        cols: u16,
        rows: u16,
        font_px: f32,
        line_height: f32,
        scale: f32,
        extra_fonts: &[&[u8]],
    ) -> Result<Self> {
        let (fonts, emoji_font_bytes) = load_fonts_with_extras(extra_fonts)?;
        let hb_face = Face::from_slice(FONT_TTF, 0).ok_or_else(|| {
            Error::Message("failed to load HarfBuzz face from bundled font".into())
        })?;
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
            emoji_font_bytes,
            hb_face,
            glyphs: HashMap::new(),
            atlas_glyphs: HashMap::new(),
            atlas_bits: HashMap::new(),
            atlas_pending: HashSet::new(),
            atlas_colored: HashSet::new(),
            atlas_dirty: true,
            atlas_fill_idx: 0,
            atlas_fill_layer: 0,
            atlas_layer_count: 1,
            sprites_per_layer: SPRITES_PER_LAYER,
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
        self.atlas_fill_idx = 0;
        self.atlas_fill_layer = 0;
        self.atlas_layer_count = 1;
        self.atlas_glyphs.clear();
        self.atlas_bits.clear();
        self.atlas_pending.clear();
        self.atlas_colored.clear();
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
    /// bit0 APP_CURSOR, bit1 APP_KEYPAD, bit2 ALT_SCREEN, bit3 BRACKETED_PASTE,
    /// bit4 MOUSE_MODE, bit5 SGR_MOUSE, bit6 MOUSE_DRAG/MOTION
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
        if mode.intersects(TermMode::MOUSE_MODE) {
            flags |= 0b1_0000;
        }
        if mode.contains(TermMode::SGR_MOUSE) {
            flags |= 0b10_0000;
        }
        if mode.intersects(TermMode::MOUSE_DRAG | TermMode::MOUSE_MOTION) {
            flags |= 0b100_0000;
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
        let default_fg = rgba_to_u32(self.fg);
        let default_bg = rgba_to_u32(self.bg);
        let mut cells = vec![
            GpuCell {
                fg: default_fg,
                bg: default_bg,
                decoration_fg: default_fg,
                sprite_idx: 0,
                sprite_layer: 0,
                attrs: 0,
            };
            (rows * cols) as usize
        ];

        #[derive(Clone, Copy)]
        enum WideKind {
            Normal,
            Head,
            Spacer,
        }

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
                let wide = if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    WideKind::Spacer
                } else if cell.flags.contains(Flags::WIDE_CHAR) {
                    WideKind::Head
                } else {
                    WideKind::Normal
                };
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
                let mut attrs = 0u32;
                if cell.flags.intersects(Flags::ALL_UNDERLINES) {
                    attrs |= ATTR_UNDERLINE_SINGLE;
                }
                if cell.flags.contains(Flags::STRIKEOUT) {
                    attrs |= ATTR_STRIKE;
                }
                if bold {
                    attrs |= ATTR_BOLD;
                }
                if dim {
                    attrs |= ATTR_DIM;
                }
                if cell.flags.contains(Flags::ITALIC) {
                    attrs |= ATTR_ITALIC;
                }
                snaps.push((col, row, ch, fg, bg, attrs, wide));
            }
            snaps
        };

        // Row-major char grid for HarfBuzz shaping (ligatures).
        let mut grid: Vec<char> = vec![' '; (rows * cols) as usize];
        let mut meta: Vec<Option<([u8; 4], [u8; 4], u32, WideKind)>> =
            vec![None; (rows * cols) as usize];
        for (col, row, ch, fg, bg, attrs, wide) in &snapshots {
            let i = (*row * cols + *col) as usize;
            if !matches!(wide, WideKind::Spacer) {
                grid[i] = *ch;
            }
            meta[i] = Some((*fg, *bg, *attrs, *wide));
        }

        for row in 0..rows {
            let start = (row * cols) as usize;
            let line: String = grid[start..start + cols as usize].iter().collect();
            let shaped = self.shape_line(&line);
            let shaped_active = shaped.iter().any(|s| s.is_some());
            let mut skip_spacer = false;
            for col in 0..cols as usize {
                if skip_spacer {
                    skip_spacer = false;
                    continue;
                }
                let i = start + col;
                let Some((fg, bg, mut attrs, wide)) = meta[i] else {
                    continue;
                };
                let fg_u = rgba_to_u32(crate::gpu_frame::contrast_fg(fg, bg));
                let bg_u = rgba_to_u32(bg);
                match wide {
                    WideKind::Spacer => {
                        // Orphan spacer (no head processed) — bg only.
                        cells[i] = GpuCell {
                            fg: fg_u,
                            bg: bg_u,
                            decoration_fg: fg_u,
                            sprite_idx: 0,
                            sprite_layer: 0,
                            attrs,
                        };
                    }
                    WideKind::Head => {
                        let ch = grid[i];
                        let ((li, ll), (ri, rl), colored) = if ch == ' ' || ch == '\0' {
                            ((0, 0), (0, 0), false)
                        } else {
                            self.ensure_wide_atlas_glyph(ch)
                        };
                        if colored {
                            attrs |= ATTR_COLORED;
                        }
                        cells[i] = GpuCell {
                            fg: fg_u,
                            bg: bg_u,
                            decoration_fg: fg_u,
                            sprite_idx: li,
                            sprite_layer: ll,
                            attrs,
                        };
                        if col + 1 < cols as usize {
                            let si = i + 1;
                            let (sfg, sbg, sattrs) = meta[si]
                                .map(|(a, b, c, _)| (a, b, c))
                                .unwrap_or((fg, bg, attrs));
                            let mut sattrs = sattrs;
                            if colored {
                                sattrs |= ATTR_COLORED;
                            }
                            cells[si] = GpuCell {
                                fg: rgba_to_u32(crate::gpu_frame::contrast_fg(sfg, sbg)),
                                bg: rgba_to_u32(sbg),
                                decoration_fg: rgba_to_u32(crate::gpu_frame::contrast_fg(sfg, sbg)),
                                sprite_idx: ri,
                                sprite_layer: rl,
                                attrs: sattrs,
                            };
                            skip_spacer = true;
                        }
                    }
                    WideKind::Normal => {
                        let (sprite_idx, sprite_layer) = if shaped_active {
                            match shaped.get(col) {
                                Some(Some(key)) => self.ensure_sprite_key(*key),
                                _ => (0, 0),
                            }
                        } else {
                            let ch = grid[i];
                            if ch == ' ' || ch == '\0' {
                                (0, 0)
                            } else {
                                self.ensure_atlas_glyph(ch)
                            }
                        };
                        if self.atlas_colored.contains(&(sprite_idx, sprite_layer)) {
                            attrs |= ATTR_COLORED;
                        }
                        cells[i] = GpuCell {
                            fg: fg_u,
                            bg: bg_u,
                            decoration_fg: fg_u,
                            sprite_idx,
                            sprite_layer,
                            attrs,
                        };
                    }
                }
            }
        }

        let sprites: Vec<AtlasSprite> = self
            .atlas_bits
            .iter()
            .map(|(&(sprite_idx, sprite_layer), entry)| {
                let bits = if self.atlas_pending.remove(&(sprite_idx, sprite_layer)) {
                    Some(entry.bits.clone())
                } else {
                    None
                };
                AtlasSprite {
                    sprite_idx,
                    sprite_layer,
                    format: entry.format,
                    bits,
                }
            })
            .collect();
        self.atlas_dirty = false;
        GpuFrame {
            cols: cols as u16,
            rows: rows as u16,
            cell_w: self.cell_w,
            cell_h: self.cell_h,
            sprites_per_layer: self.sprites_per_layer,
            layer_count: self.atlas_layer_count.max(1),
            sprites,
            cells,
        }
    }

    /// Shape a terminal row. Returns per-column sprite cache keys for ligature heads;
    /// `None` means fall back to single-char glyph (or empty).
    fn shape_line(&self, line: &str) -> Vec<Option<u64>> {
        let cols = line.chars().count();
        let mut out = vec![None; cols];
        if cols == 0 {
            return out;
        }
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(line);
        let glyph_buffer = rustybuzz::shape(&self.hb_face, &[], buffer);
        let infos = glyph_buffer.glyph_infos();
        let positions = glyph_buffer.glyph_positions();
        if infos.len() == cols {
            // 1:1 — no ligature compression; let char path handle raster.
            return out;
        }
        // Map clusters: first cell of each cluster gets a shaped glyph key.
        for (info, pos) in infos.iter().zip(positions.iter()) {
            let cluster = info.cluster as usize;
            if cluster >= cols {
                continue;
            }
            if out[cluster].is_some() {
                continue;
            }
            // Skip empty glyphs / zero-width.
            if info.glyph_id == 0 && pos.x_advance == 0 {
                continue;
            }
            out[cluster] = Some(sprite_key_glyph(info.glyph_id));
        }
        out
    }

    fn ensure_sprite_key(&mut self, key: u64) -> (u16, u16) {
        if let Some(&slot) = self.atlas_glyphs.get(&key) {
            return slot;
        }
        if let Some(gid) = glyph_id_from_key(key) {
            return self.ensure_hb_glyph(gid);
        }
        let ch = char::from_u32((key & 0xffff_ffff) as u32).unwrap_or(' ');
        self.ensure_atlas_glyph(ch)
    }

    fn ensure_hb_glyph(&mut self, glyph_id: u32) -> (u16, u16) {
        let key = sprite_key_glyph(glyph_id);
        if let Some(&slot) = self.atlas_glyphs.get(&key) {
            return slot;
        }
        let (metrics, cover) = self.fonts[0].rasterize_indexed(glyph_id as u16, self.font_px);
        let stamp = pad_stamp(
            &cover,
            metrics.width as u32,
            metrics.height as u32,
            metrics.xmin,
            self.baseline - metrics.ymin - metrics.height as i32,
            self.cell_w,
            self.cell_h,
        );
        self.alloc_sprite(key, STAMP_FMT_R8, stamp)
    }

    fn ensure_atlas_glyph(&mut self, ch: char) -> (u16, u16) {
        if ch == ' ' || ch == '\0' {
            return (0, 0);
        }
        let key = sprite_key_char(ch);
        if let Some(&slot) = self.atlas_glyphs.get(&key) {
            return slot;
        }
        if wants_emoji_presentation(ch) {
            if let Some((stamp, colored)) = self.raster_color_emoji(ch, self.cell_w, self.cell_h) {
                let fmt = if colored {
                    STAMP_FMT_RGBA
                } else {
                    STAMP_FMT_R8
                };
                return self.alloc_sprite(key, fmt, stamp);
            }
        }
        let cp = ch as u32;
        if is_procedural_cell(cp) {
            if let Some(bits) = render_box_cell(cp, self.cell_w, self.cell_h) {
                return self.alloc_sprite(key, STAMP_FMT_R8, bits);
            }
        }
        let (meta, cover) = self.glyph(ch);
        let stamp = if meta.fit == GlyphFit::Cell {
            let scaled = scale_cover(&cover, meta.w, meta.h, self.cell_w, self.cell_h);
            let snapped = snap_cell_edges(&scaled, self.cell_w, self.cell_h, cell_attach(ch));
            pad_stamp(&snapped, self.cell_w, self.cell_h, 0, 0, self.cell_w, self.cell_h)
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
            pad_stamp(&bits, dw, dh, ox, oy, self.cell_w, self.cell_h)
        };
        self.alloc_sprite(key, STAMP_FMT_R8, stamp)
    }

    /// Rasterize a double-width glyph into left/right cell stamps.
    fn ensure_wide_atlas_glyph(&mut self, ch: char) -> ((u16, u16), (u16, u16), bool) {
        let key_l = sprite_key_char_half(ch, 0);
        let key_r = sprite_key_char_half(ch, 1);
        if let (Some(&l), Some(&r)) = (self.atlas_glyphs.get(&key_l), self.atlas_glyphs.get(&key_r))
        {
            let colored = self.atlas_colored.contains(&l);
            return (l, r, colored);
        }
        let canvas_w = self.cell_w * 2;
        if wants_emoji_presentation(ch) {
            if let Some((full, colored)) = self.raster_color_emoji(ch, canvas_w, self.cell_h) {
                let fmt = if colored {
                    STAMP_FMT_RGBA
                } else {
                    STAMP_FMT_R8
                };
                let (left, right) = split_stamp_halves(&full, self.cell_w, self.cell_h, fmt);
                let l = self.alloc_sprite(key_l, fmt, left);
                let r = self.alloc_sprite(key_r, fmt, right);
                return (l, r, colored);
            }
        }
        let (meta, cover) = self.glyph(ch);
        // Fit into 2×cell width so ink spans head + spacer (never squash into one cell).
        let mut dw = meta.w.max(1);
        let mut dh = meta.h.max(1);
        let s = canvas_w as f32 / dw as f32;
        let target_h = ((dh as f32) * s).round().max(1.0) as u32;
        let target_h = target_h.min(self.cell_h.max(1));
        let s_h = target_h as f32 / dh as f32;
        let s = s.min(s_h);
        dw = ((dw as f32) * s).round().max(1.0) as u32;
        dh = ((dh as f32) * s).round().max(1.0) as u32;
        let bits = scale_cover(&cover, meta.w.max(1), meta.h.max(1), dw, dh);
        let ox = ((canvas_w as i32 - dw as i32) / 2).max(0);
        let oy = ((self.cell_h as i32 - dh as i32) / 2).max(0);
        let full = pad_stamp(&bits, dw, dh, ox, oy, canvas_w, self.cell_h);
        let (left, right) = split_stamp_halves(&full, self.cell_w, self.cell_h, STAMP_FMT_R8);
        let l = self.alloc_sprite(key_l, STAMP_FMT_R8, left);
        let r = self.alloc_sprite(key_r, STAMP_FMT_R8, right);
        (l, r, false)
    }

    /// FreeType color emoji raster → RGBA (or R8 fallback) stamp for `dst_w × cell_h`.
    ///
    /// Bitmaps are **scaled to fit** the canvas and centered. Sizing to `cell_h` alone
    /// plus `bitmap_left` often overflowed `2*cell_w` and clipped the right edge.
    fn raster_color_emoji(&self, ch: char, dst_w: u32, dst_h: u32) -> Option<(Vec<u8>, bool)> {
        let bytes = self.emoji_font_bytes.as_ref()?;
        let library = freetype::Library::init().ok()?;
        let face = library.new_memory_face(bytes.clone(), 0).ok()?;
        // Request pixels covering the larger canvas axis, then fit down.
        let req = dst_w.max(dst_h).max(1);
        face.set_pixel_sizes(0, req).ok()?;
        let flags = freetype::face::LoadFlag::COLOR | freetype::face::LoadFlag::RENDER;
        face.load_char(ch as usize, flags).ok()?;
        let glyph = face.glyph();
        let bitmap = glyph.bitmap();
        let w = bitmap.width().max(0) as u32;
        let h = bitmap.rows().max(0) as u32;
        if w == 0 || h == 0 {
            return None;
        }
        let buffer = bitmap.buffer();
        let pitch = bitmap.pitch().unsigned_abs() as usize;
        let mode = bitmap.pixel_mode().ok()?;
        match mode {
            freetype::bitmap::PixelMode::Bgra => {
                let mut rgba = vec![0u8; (w * h * 4) as usize];
                for y in 0..h as usize {
                    for x in 0..w as usize {
                        let src = y * pitch + x * 4;
                        if src + 3 >= buffer.len() {
                            continue;
                        }
                        let b = buffer[src];
                        let g = buffer[src + 1];
                        let r = buffer[src + 2];
                        let a = buffer[src + 3];
                        let dst = (y * w as usize + x) * 4;
                        rgba[dst] = r;
                        rgba[dst + 1] = g;
                        rgba[dst + 2] = b;
                        rgba[dst + 3] = a;
                    }
                }
                Some((fit_rgba_stamp(&rgba, w, h, dst_w, dst_h), true))
            }
            freetype::bitmap::PixelMode::Gray => {
                let mut cover = vec![0u8; (w * h) as usize];
                for y in 0..h as usize {
                    for x in 0..w as usize {
                        let src = y * pitch + x;
                        if src < buffer.len() {
                            cover[y * w as usize + x] = buffer[src];
                        }
                    }
                }
                Some((fit_cover_stamp(&cover, w, h, dst_w, dst_h), false))
            }
            _ => None,
        }
    }

    fn alloc_sprite(&mut self, key: u64, format: u8, stamp: Vec<u8>) -> (u16, u16) {
        let expected = stamp_bytes(self.cell_w, self.cell_h, format);
        debug_assert_eq!(stamp.len(), expected);
        let layers_before = self.atlas_layer_count.max(1);
        if self.atlas_fill_idx >= self.sprites_per_layer {
            self.atlas_fill_idx = 0;
            self.atlas_fill_layer = self.atlas_fill_layer.saturating_add(1);
            self.atlas_layer_count = self.atlas_layer_count.max(self.atlas_fill_layer + 1);
        }
        // Reserve idx 0 as empty.
        if self.atlas_fill_layer == 0 && self.atlas_fill_idx == 0 {
            self.atlas_fill_idx = 1;
        }
        if self.atlas_fill_idx >= self.sprites_per_layer {
            // Layer overflow after skipping 0.
            self.atlas_fill_idx = 0;
            self.atlas_fill_layer = self.atlas_fill_layer.saturating_add(1);
            self.atlas_layer_count = self.atlas_layer_count.max(self.atlas_fill_layer + 1);
        }
        let sprite_idx = self.atlas_fill_idx;
        let sprite_layer = self.atlas_fill_layer;
        self.atlas_fill_idx = self.atlas_fill_idx.saturating_add(1);
        self.atlas_glyphs.insert(key, (sprite_idx, sprite_layer));
        self.atlas_bits.insert(
            (sprite_idx, sprite_layer),
            AtlasEntry {
                format,
                bits: stamp,
            },
        );
        self.atlas_pending.insert((sprite_idx, sprite_layer));
        if format == STAMP_FMT_RGBA {
            self.atlas_colored.insert((sprite_idx, sprite_layer));
        }
        // Client atlases wipe on layer growth; re-pend every stamp so the growth
        // frame can rebuild coverage (Canvas2D zero-fill / WebGL texImage3D null).
        if self.atlas_layer_count > layers_before {
            for slot in self.atlas_bits.keys().copied().collect::<Vec<_>>() {
                self.atlas_pending.insert(slot);
            }
        }
        self.atlas_dirty = true;
        (sprite_idx, sprite_layer)
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
        '\u{2500}'..='\u{259f}'
        | '\u{2800}'..='\u{28ff}'
        | '\u{e0a0}'..='\u{e0d4}'
        | '\u{25e2}'..='\u{25e5}' => GlyphFit::Cell,
        '\u{e000}'..='\u{e09f}' | '\u{e0d5}'..='\u{f8ff}' | '\u{f0000}'..='\u{ffffd}' => GlyphFit::Icon,
        _ => GlyphFit::Text,
    }
}

fn sprite_key_char(ch: char) -> u64 {
    0x1000_0000_0000_0000 | (ch as u64)
}

fn sprite_key_char_half(ch: char, half: u8) -> u64 {
    0x3000_0000_0000_0000 | ((half as u64) << 32) | (ch as u64)
}

fn sprite_key_glyph(glyph_id: u32) -> u64 {
    0x2000_0000_0000_0000 | (glyph_id as u64)
}

fn glyph_id_from_key(key: u64) -> Option<u32> {
    if key & 0xf000_0000_0000_0000 == 0x2000_0000_0000_0000 {
        Some((key & 0xffff_ffff) as u32)
    } else {
        None
    }
}

/// Place coverage into a cell_w × (cell_h+1) stamp (extra row = underline exclusion).
fn pad_stamp(
    cover: &[u8],
    src_w: u32,
    src_h: u32,
    ox: i32,
    oy: i32,
    cell_w: u32,
    cell_h: u32,
) -> Vec<u8> {
    let stamp_h = cell_h + 1;
    let mut out = vec![0u8; (cell_w * stamp_h) as usize];
    for dy in 0..src_h {
        for dx in 0..src_w {
            let px = ox + dx as i32;
            let py = oy + dy as i32;
            if px < 0 || py < 0 || px >= cell_w as i32 || py >= cell_h as i32 {
                continue;
            }
            let cover_v = cover[(dy * src_w + dx) as usize];
            let i = (py as u32 * cell_w + px as u32) as usize;
            out[i] = out[i].max(cover_v);
        }
    }
    // Exclusion row: copy max coverage of the bottom text row so underlines can fade.
    if cell_h > 0 {
        let y = cell_h - 1;
        for x in 0..cell_w {
            out[(cell_h * cell_w + x) as usize] = out[(y * cell_w + x) as usize];
        }
    }
    out
}

/// Scale RGBA to fit inside `dst_w × dst_h` (aspect preserved) and center — no clip.
fn fit_rgba_stamp(rgba: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
        return pad_stamp_rgba(&[], 0, 0, 0, 0, dst_w, dst_h);
    }
    // Slight inset so FreeType AA / rounding never kisses the clip edge.
    let scale =
        (dst_w as f32 / src_w as f32).min(dst_h as f32 / src_h as f32).max(0.0) * 0.96;
    let nw = ((src_w as f32) * scale).floor().max(1.0) as u32;
    let nh = ((src_h as f32) * scale).floor().max(1.0) as u32;
    let nw = nw.min(dst_w);
    let nh = nh.min(dst_h);
    let scaled = scale_rgba(rgba, src_w, src_h, nw, nh);
    let ox = ((dst_w as i32 - nw as i32) / 2).max(0);
    let oy = ((dst_h as i32 - nh as i32) / 2).max(0);
    pad_stamp_rgba(&scaled, nw, nh, ox, oy, dst_w, dst_h)
}

fn fit_cover_stamp(cover: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
        return pad_stamp(&[], 0, 0, 0, 0, dst_w, dst_h);
    }
    let scale =
        (dst_w as f32 / src_w as f32).min(dst_h as f32 / src_h as f32).max(0.0) * 0.96;
    let nw = ((src_w as f32) * scale).floor().max(1.0) as u32;
    let nh = ((src_h as f32) * scale).floor().max(1.0) as u32;
    let nw = nw.min(dst_w);
    let nh = nh.min(dst_h);
    let scaled = scale_cover(cover, src_w, src_h, nw, nh);
    let ox = ((dst_w as i32 - nw as i32) / 2).max(0);
    let oy = ((dst_h as i32 - nh as i32) / 2).max(0);
    pad_stamp(&scaled, nw, nh, ox, oy, dst_w, dst_h)
}

fn scale_rgba(src: &[u8], src_w: u32, src_h: u32, dst_w: u32, dst_h: u32) -> Vec<u8> {
    let mut out = vec![0u8; (dst_w * dst_h * 4) as usize];
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
        return out;
    }
    for y in 0..dst_h {
        for x in 0..dst_w {
            let sx = (((x as f32 + 0.5) * src_w as f32 / dst_w as f32) as u32).min(src_w - 1);
            let sy = (((y as f32 + 0.5) * src_h as f32 / dst_h as f32) as u32).min(src_h - 1);
            let si = ((sy * src_w + sx) * 4) as usize;
            let di = ((y * dst_w + x) * 4) as usize;
            if si + 3 < src.len() && di + 3 < out.len() {
                out[di..di + 4].copy_from_slice(&src[si..si + 4]);
            }
        }
    }
    out
}

fn pad_stamp_rgba(
    rgba: &[u8],
    src_w: u32,
    src_h: u32,
    ox: i32,
    oy: i32,
    cell_w: u32,
    cell_h: u32,
) -> Vec<u8> {
    let stamp_h = cell_h + 1;
    let mut out = vec![0u8; (cell_w * stamp_h * 4) as usize];
    for dy in 0..src_h {
        for dx in 0..src_w {
            let px = ox + dx as i32;
            let py = oy + dy as i32;
            if px < 0 || py < 0 || px >= cell_w as i32 || py >= cell_h as i32 {
                continue;
            }
            let si = ((dy * src_w + dx) * 4) as usize;
            let di = ((py as u32 * cell_w + px as u32) * 4) as usize;
            if si + 3 < rgba.len() && di + 3 < out.len() {
                out[di..di + 4].copy_from_slice(&rgba[si..si + 4]);
            }
        }
    }
            if cell_h > 0 {
                let y = cell_h - 1;
                for x in 0..cell_w {
                    let src = ((y * cell_w + x) * 4) as usize;
                    let dst = ((cell_h * cell_w + x) * 4) as usize;
                    let px = [out[src], out[src + 1], out[src + 2], out[src + 3]];
                    out[dst..dst + 4].copy_from_slice(&px);
                }
            }
    out
}

fn split_stamp_halves(full: &[u8], cell_w: u32, cell_h: u32, format: u8) -> (Vec<u8>, Vec<u8>) {
    let bpp = if format == STAMP_FMT_RGBA { 4u32 } else { 1 };
    let stamp_h = cell_h + 1;
    let canvas_w = cell_w * 2;
    let mut left = vec![0u8; (cell_w * stamp_h * bpp) as usize];
    let mut right = vec![0u8; (cell_w * stamp_h * bpp) as usize];
    for y in 0..stamp_h {
        for x in 0..cell_w {
            for c in 0..bpp {
                let li = ((y * cell_w + x) * bpp + c) as usize;
                let fl = ((y * canvas_w + x) * bpp + c) as usize;
                let fr = ((y * canvas_w + cell_w + x) * bpp + c) as usize;
                if fl < full.len() {
                    left[li] = full[fl];
                }
                if fr < full.len() {
                    right[li] = full[fr];
                }
            }
        }
    }
    (left, right)
}

fn wants_emoji_presentation(ch: char) -> bool {
    let cp = ch as u32;
    matches!(
        cp,
        0x1F300..=0x1FAFF | 0x2600..=0x27BF | 0xFE0F | 0x1F000..=0x1F2FF
    ) || matches!(ch, '😀'..='🙏' | '🚀'..='🛿')
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

fn load_fonts_with_extras(extra: &[&[u8]]) -> Result<(Vec<Font>, Option<Vec<u8>>)> {
    let primary = Font::from_bytes(FONT_TTF, FontSettings::default())
        .map_err(|err| Error::msg(format!("font: {err}")))?;
    let mut fonts = vec![primary];
    let mut seen_paths = HashSet::new();
    for bytes in extra {
        if let Ok(face) = Font::from_bytes(*bytes, FontSettings::default()) {
            fonts.push(face);
        }
    }
    // System fallback faces via font-kit (CJK / symbols / etc.).
    for bytes in discover_system_fallback_bytes(&mut seen_paths) {
        if let Ok(face) = Font::from_bytes(bytes.as_slice(), FontSettings::default()) {
            fonts.push(face);
        }
    }
    let emoji_font_bytes = discover_emoji_font_bytes(&mut seen_paths);
    Ok((fonts, emoji_font_bytes))
}

fn discover_system_fallback_bytes(seen: &mut HashSet<String>) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let families = [
        "Noto Sans CJK JP",
        "Noto Sans CJK SC",
        "Noto Sans CJK TC",
        "Noto Sans CJK KR",
        "Noto Sans JP",
        "Noto Sans SC",
        "Source Han Sans",
        "WenQuanYi Micro Hei",
        "DejaVu Sans",
        "FreeSans",
        "Liberation Sans",
        "Arial Unicode MS",
        "Segoe UI Symbol",
    ];
    let source = font_kit::source::SystemSource::new();
    for name in families {
        if out.len() >= 8 {
            break;
        }
        let handle = source.select_best_match(
            &[font_kit::family_name::FamilyName::Title(name.into())],
            &font_kit::properties::Properties::new(),
        );
        let Ok(handle) = handle else { continue };
        if let Some(bytes) = font_handle_bytes(&handle, seen) {
            out.push(bytes);
        }
    }
    out
}

fn discover_emoji_font_bytes(seen: &mut HashSet<String>) -> Option<Vec<u8>> {
    let families = [
        "Noto Color Emoji",
        "Apple Color Emoji",
        "Segoe UI Emoji",
        "Emoji One",
        "Twemoji Mozilla",
    ];
    let source = font_kit::source::SystemSource::new();
    for name in families {
        let handle = source.select_best_match(
            &[font_kit::family_name::FamilyName::Title(name.into())],
            &font_kit::properties::Properties::new(),
        );
        let Ok(handle) = handle else { continue };
        if let Some(bytes) = font_handle_bytes(&handle, seen) {
            return Some(bytes);
        }
    }
    None
}

fn font_handle_bytes(
    handle: &font_kit::handle::Handle,
    seen: &mut HashSet<String>,
) -> Option<Vec<u8>> {
    match handle {
        font_kit::handle::Handle::Path { path, .. } => {
            let key = path.to_string_lossy().into_owned();
            if !seen.insert(key) {
                return None;
            }
            std::fs::read(path).ok()
        }
        font_kit::handle::Handle::Memory { bytes, .. } => {
            let key = format!("mem:{}", bytes.len());
            if !seen.insert(key) {
                return None;
            }
            Some(bytes.as_ref().clone())
        }
    }
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
        let packed = crate::gpu_frame::pack_gpu2_frame(&gpu);
        assert!(crate::gpu_frame::is_gpu2_frame(&packed));
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
            gpu2.sprites.iter().all(|g| g.bits.is_none()),
            "repeated glyphs must not resend atlas stamps"
        );
        let packed2 = crate::gpu_frame::pack_gpu2_frame(&gpu2);
        assert!(
            packed2.len() < packed.len(),
            "steady frame should drop stamp bytes ({} vs {})",
            packed2.len(),
            packed.len()
        );
    }

    /// #165 — when the atlas grows past SPRITES_PER_LAYER, the growth frame must
    /// re-attach stamp bits for *prior* sprites (client atlas wipe otherwise leaves
    /// sparse glyphs after `cat` of a large unique-glyph dump).
    #[test]
    fn atlas_layer_growth_resends_prior_stamp_bits() {
        let mut term = TerminalEmulator::new(80, 24, 14.0).unwrap();
        let alphabet: String = (33u8..127).map(|b| b as char).collect();
        let wave1 = &alphabet[..50];
        term.feed(wave1.as_bytes());
        let f1 = term.capture_gpu_frame(true).expect("wave1 gpu frame");
        assert_eq!(f1.layer_count, 1, "50 glyphs stay on layer 0");
        let prior: HashSet<(u16, u16)> = f1
            .sprites
            .iter()
            .filter(|s| s.bits.is_some())
            .map(|s| (s.sprite_idx, s.sprite_layer))
            .collect();
        assert!(
            prior.len() >= 40,
            "expected many stamped glyphs in wave1, got {}",
            prior.len()
        );

        let wave2 = &alphabet[50..];
        term.feed(b"\n");
        term.feed(wave2.as_bytes());
        let f2 = term.capture_gpu_frame(true).expect("growth gpu frame");
        assert!(
            f2.layer_count >= 2,
            "expected layer growth after {} unique glyphs, layer_count={}",
            alphabet.len(),
            f2.layer_count
        );
        for key in &prior {
            let spr = f2
                .sprites
                .iter()
                .find(|s| (s.sprite_idx, s.sprite_layer) == *key)
                .unwrap_or_else(|| panic!("missing prior sprite {key:?} after layer growth"));
            assert!(
                spr.bits.is_some(),
                "prior stamp {key:?} must be resent when atlas layers grow"
            );
        }

        // AC3 — steady frame after growth stays compact.
        term.feed(wave1.as_bytes());
        let f3 = term.capture_gpu_frame(true).expect("steady gpu frame");
        assert!(
            f3.sprites.iter().all(|s| s.bits.is_none()),
            "repeated glyphs must not resend atlas stamps after layer growth"
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
    fn bracketed_paste_mode_sets_pack_frame_flags() {
        let mut term = TerminalEmulator::new(40, 12, 14.0).unwrap();
        assert_eq!(term.mode_flags() & 0b1000, 0, "default: bracketed paste off");
        // DECSET 2004 — what bash/zsh/readline/vim enable for bracketed paste.
        term.feed(b"\x1b[?2004h");
        assert_eq!(term.mode_flags() & 0b1000, 0b1000, "DECSET 2004 should set BRACKETED_PASTE");
        let (cw, ch) = term.cell_size();
        let frame = term.capture_frame(true).expect("frame");
        let packed = pack_frame(&frame, cw, ch, term.mode_flags(), 0, 0);
        let flags = u32::from_le_bytes(packed[16..20].try_into().unwrap());
        assert_eq!(flags & 0b1000, 0b1000, "mode_flags in packed frame contains BRACKETED_PASTE");
        term.feed(b"\x1b[?2004l");
        assert_eq!(term.mode_flags() & 0b1000, 0, "DECRST 2004 clears BRACKETED_PASTE");
    }

    #[test]
    fn ac1_mouse_mode_sets_pack_frame_flags() {
        let mut term = TerminalEmulator::new(40, 12, 14.0).unwrap();
        assert_eq!(term.mode_flags() & 0b1_0000, 0);
        // Normal mouse tracking + SGR + button-event tracking (vim mouse=a style).
        term.feed(b"\x1b[?1000h\x1b[?1002h\x1b[?1006h");
        let flags = term.mode_flags();
        assert_eq!(flags & 0b1_0000, 0b1_0000, "MOUSE_MODE");
        assert_eq!(flags & 0b10_0000, 0b10_0000, "SGR_MOUSE");
        assert_eq!(flags & 0b100_0000, 0b100_0000, "MOUSE_DRAG");
        let (cw, ch) = term.cell_size();
        let frame = term.capture_frame(true).expect("frame");
        let packed = pack_frame(&frame, cw, ch, flags, 0, 0);
        let packed_flags = u32::from_le_bytes(packed[16..20].try_into().unwrap());
        assert_eq!(packed_flags & 0b111_0000, 0b111_0000);
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

    #[test]
    fn injected_fallback_font_produces_atlas_ink() {
        use crate::test_cjk_font::TEST_CJK_TTF;
        let primary = Font::from_bytes(FONT_TTF, FontSettings::default()).unwrap();
        let ch = '\u{4e00}';
        assert!(
            !primary.has_glyph(ch),
            "Cascadia should lack CJK U+4E00 so fallback is exercised"
        );
        let extra = Font::from_bytes(TEST_CJK_TTF, FontSettings::default())
            .expect("test CJK font must parse");
        assert!(extra.has_glyph(ch), "injected font must provide U+4E00");

        let mut term =
            TerminalEmulator::new_with_extra_font_bytes(8, 3, 14.0, &[TEST_CJK_TTF]).unwrap();
        assert!(
            term.fonts.len() >= 2,
            "extra face must be loaded (got {} faces)",
            term.fonts.len()
        );
        assert!(
            term.fonts.iter().any(|f| f.has_glyph(ch)),
            "fallback chain must include a face with U+4E00"
        );
        term.feed(ch.to_string().as_bytes());
        let frame = term.capture_gpu_frame(true).expect("gpu frame");
        let stamped: Vec<_> = frame
            .sprites
            .iter()
            .filter(|s| s.bits.as_ref().map(|b| b.iter().any(|&p| p > 0)).unwrap_or(false))
            .collect();
        assert!(
            !stamped.is_empty(),
            "fallback face must produce atlas ink for U+4E00"
        );
        let cells_with_sprite = frame.cells.iter().filter(|c| c.sprite_idx > 0).count();
        assert!(
            cells_with_sprite >= 2,
            "wide CJK must stamp head+spacer (got {cells_with_sprite} sprite cells)"
        );
    }

    #[test]
    fn wide_glyph_spans_two_cells_in_gpu_frame() {
        use crate::test_cjk_font::TEST_CJK_TTF;
        let mut term =
            TerminalEmulator::new_with_extra_font_bytes(8, 3, 14.0, &[TEST_CJK_TTF]).unwrap();
        term.feed("\u{4e00}".as_bytes());
        let frame = term.capture_gpu_frame(true).expect("gpu");
        assert!(frame.cols >= 2);
        let c0 = &frame.cells[0];
        let c1 = &frame.cells[1];
        assert!(c0.sprite_idx > 0, "wide head must have a sprite");
        assert!(c1.sprite_idx > 0, "wide spacer must have right-half sprite");
        assert!(
            (c0.sprite_idx, c0.sprite_layer) != (c1.sprite_idx, c1.sprite_layer),
            "left/right halves must be distinct atlas slots"
        );
        let left_bits = frame
            .sprites
            .iter()
            .find(|s| s.sprite_idx == c0.sprite_idx && s.sprite_layer == c0.sprite_layer)
            .and_then(|s| s.bits.as_ref());
        let right_bits = frame
            .sprites
            .iter()
            .find(|s| s.sprite_idx == c1.sprite_idx && s.sprite_layer == c1.sprite_layer)
            .and_then(|s| s.bits.as_ref());
        assert!(
            left_bits.map(|b| b.iter().any(|&p| p > 0)).unwrap_or(false),
            "left half must have ink"
        );
        assert!(
            right_bits.map(|b| b.iter().any(|&p| p > 0)).unwrap_or(false),
            "right half must have ink (not empty spacer)"
        );
    }

    #[test]
    fn color_emoji_integration_skips_without_emoji_font() {
        let mut term = TerminalEmulator::new(8, 3, 14.0).unwrap();
        if term.emoji_font_bytes.is_none() {
            // Protocol + blend covered elsewhere; skip raster when no system emoji font.
            return;
        }
        term.feed("😀".as_bytes());
        let frame = term.capture_gpu_frame(true).expect("gpu");
        let colored = frame
            .sprites
            .iter()
            .any(|s| s.format == STAMP_FMT_RGBA && s.bits.is_some());
        let attr = frame.cells.iter().any(|c| c.attrs & ATTR_COLORED != 0);
        assert!(
            colored || attr,
            "with emoji font available, expect RGBA stamp or ATTR_COLORED"
        );
    }

    /// #170 — source wider than the canvas must scale-to-fit; right-side ink stays inside.
    #[test]
    fn fit_rgba_stamp_keeps_right_edge_inside_canvas() {
        let sw = 40u32;
        let sh = 20u32;
        let mut src = vec![0u8; (sw * sh * 4) as usize];
        for y in 0..sh {
            for x in 0..sw {
                let i = ((y * sw + x) * 4) as usize;
                // Opaque band on the right third (survives nearest-neighbor downscale).
                if x >= (sw * 2) / 3 {
                    src[i] = 255;
                    src[i + 1] = 200;
                    src[i + 2] = 0;
                    src[i + 3] = 255;
                }
            }
        }
        let dw = 18u32;
        let dh = 16u32;
        let stamp = fit_rgba_stamp(&src, sw, sh, dw, dh);
        assert_eq!(stamp.len(), (dw * (dh + 1) * 4) as usize);
        let mut max_x_with_ink = 0u32;
        let mut right_quarter_ink = 0u32;
        for y in 0..dh {
            for x in 0..dw {
                let i = ((y * dw + x) * 4) as usize;
                if stamp[i + 3] > 0 {
                    max_x_with_ink = max_x_with_ink.max(x);
                    if x >= dw * 3 / 4 {
                        right_quarter_ink += 1;
                    }
                }
            }
        }
        assert!(
            right_quarter_ink > 0,
            "right-side source ink must appear after fit (not clipped away)"
        );
        assert!(
            max_x_with_ink + 2 >= dw - 1,
            "right band should reach near canvas right edge (max_x={max_x_with_ink} dw={dw})"
        );
    }
}
