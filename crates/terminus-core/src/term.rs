use crate::error::{Error, Result};
use crate::models::ColorTheme;
use fontdue::{Font, FontSettings};
use std::collections::HashMap;
use vt100::Color;

const FONT_TTF: &[u8] = include_bytes!("../fonts/IBMPlexMono-Regular.ttf");
const NERD_TTF: &[u8] = include_bytes!("../fonts/SymbolsNerdFontMono-Regular.ttf");

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

pub struct TerminalEmulator {
    parser: vt100::Parser,
    fonts: Vec<Font>,
    glyphs: HashMap<char, (Glyph, Vec<u8>)>,
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
        let mut emulator = Self {
            parser: vt100::Parser::new(rows, cols, 2000),
            fonts,
            glyphs: HashMap::new(),
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
        self.dirty = true;
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

    pub fn feed(&mut self, bytes: &[u8]) {
        self.parser.process(bytes);
        self.dirty = true;
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.parser.set_size(rows, cols);
        self.dirty = true;
    }

    pub fn cell_size(&self) -> (u32, u32) {
        (self.cell_w, self.cell_h)
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

    pub fn raster(&mut self) -> TermFrame {
        let screen = self.parser.screen().clone();
        let rows = screen.size().0 as u32;
        let cols = screen.size().1 as u32;
        let width = cols * self.cell_w;
        let height = rows * self.cell_h;
        let mut rgba = vec![0u8; (width * height * 4) as usize];
        let (cur_row, cur_col) = screen.cursor_position();
        let mut glyphs = Vec::with_capacity((rows * cols) as usize);
        for row in 0..rows {
            for col in 0..cols {
                let cell = screen.cell(row as u16, col as u16);
                let (mut fg, mut bg, ch) = match cell {
                    Some(cell) => {
                        let ch = cell.contents().chars().next().unwrap_or(' ');
                        let mut fg = self.resolve(cell.fgcolor(), self.fg);
                        let mut bg = self.resolve(cell.bgcolor(), self.bg);
                        if cell.inverse() {
                            std::mem::swap(&mut fg, &mut bg);
                        }
                        (fg, bg, ch)
                    }
                    None => (self.fg, self.bg, ' '),
                };
                if row as u16 == cur_row && col as u16 == cur_col && !screen.hide_cursor() {
                    bg = self.cursor;
                    fg = self.bg;
                }
                fill_cell(&mut rgba, width, col * self.cell_w, row * self.cell_h, self.cell_w, self.cell_h, bg);
                if ch != ' ' {
                    glyphs.push((col, row, ch, fg, bg));
                }
            }
        }
        for (col, row, ch, fg, bg) in glyphs {
            self.blit_glyph(&mut rgba, width, col, row, ch, fg, bg);
        }
        TermFrame { width, height, rgba }
    }

    fn resolve(&self, color: Color, fallback: [u8; 4]) -> [u8; 4] {
        match color {
            Color::Default => fallback,
            Color::Idx(idx) => self.palette[idx as usize],
            Color::Rgb(r, g, b) => [r, g, b, 255],
        }
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
    let primary = Font::from_bytes(FONT_TTF, FontSettings::default())
        .map_err(|err| Error::msg(format!("font: {err}")))?;
    let mut fonts = vec![primary];
    if let Ok(nerd) = Font::from_bytes(NERD_TTF, FontSettings::default()) {
        fonts.push(nerd);
    }
    Ok(fonts)
}

fn metrics_for(font: &Font, font_px: f32, line_height: f32) -> (f32, u32, u32, i32) {
    let px = font_px.max(10.0);
    let line = font
        .horizontal_line_metrics(px)
        .expect("IBM Plex Mono has line metrics");
    let em = font.metrics('M', px);
    let cell_w = em.advance_width.ceil().max(1.0) as u32;
    let typo = (line.ascent - line.descent).max(1.0);
    let cell_h = (typo * line_height.max(1.0)).round().max(typo.ceil()) as u32;
    let slack = cell_h as f32 - typo;
    let baseline = (slack / 2.0).floor() as i32 + line.ascent.round() as i32;
    (px, cell_w, cell_h, baseline)
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

pub fn pack_frame(frame: &TermFrame, cell_w: u32, cell_h: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + frame.rgba.len());
    out.extend_from_slice(&frame.width.to_le_bytes());
    out.extend_from_slice(&frame.height.to_le_bytes());
    out.extend_from_slice(&cell_w.to_le_bytes());
    out.extend_from_slice(&cell_h.to_le_bytes());
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
}
