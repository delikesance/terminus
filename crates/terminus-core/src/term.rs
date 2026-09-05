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
        Self::new_with_style(cols, rows, font_px, 1.25)
    }

    pub fn new_with_style(cols: u16, rows: u16, font_px: f32, line_height: f32) -> Result<Self> {
        let fonts = load_fonts()?;
        let (px, cell_w, cell_h, baseline) = metrics_for(&fonts[0], font_px, line_height);
        let mut emulator = Self {
            parser: vt100::Parser::new(rows, cols, 2000),
            fonts,
            glyphs: HashMap::new(),
            font_px: px,
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
        let (px, cell_w, cell_h, baseline) = metrics_for(&self.fonts[0], font_px, line_height);
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
        let (dest_x, dest_y, dest_w, dest_h) = match glyph.fit {
            // Overlap 1px into the previous cell so the cap sits flush on the square.
            GlyphFit::Cell => (x0 as i32 - 1, y0 as i32, self.cell_w + 2, self.cell_h),
            GlyphFit::Icon => {
                let pad_x = ((self.cell_w as i32 - glyph.w as i32) / 2).max(0);
                let pad_y = ((self.cell_h as i32 - glyph.h as i32) / 2).max(0);
                (x0 as i32 + pad_x, y0 as i32 + pad_y, glyph.w, glyph.h)
            }
            GlyphFit::Text => {
                let dest_x = x0 as i32 + glyph.xmin;
                let dest_y = y0 as i32 + self.baseline - glyph.ymin - glyph.h as i32;
                (dest_x, dest_y, glyph.w, glyph.h)
            }
        };
        if glyph.fit == GlyphFit::Cell {
            for dy in 0..dest_h {
                for dx in 0..dest_w {
                    let cover_v = sample_cover(&cover, glyph.w, glyph.h, dx, dy, dest_w, dest_h);
                    if cover_v == 0 {
                        continue;
                    }
                    let px = dest_x + dx as i32;
                    let py = dest_y + dy as i32;
                    if px < 0 || py < 0 || px >= frame_w || py >= frame_h {
                        continue;
                    }
                    let mixed = blend(bg, [fg[0], fg[1], fg[2], cover_v]);
                    put(rgba, width, px as u32, py as u32, mixed);
                }
            }
            return;
        }
        for gy in 0..glyph.h {
            for gx in 0..glyph.w {
                let cover_v = cover[(gy * glyph.w + gx) as usize];
                if cover_v == 0 {
                    continue;
                }
                let px = dest_x + gx as i32;
                let py = dest_y + gy as i32;
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
            GlyphFit::Cell => (self.cell_h as f32).max(self.font_px),
            GlyphFit::Icon => (self.cell_h as f32 * 0.88).max(self.font_px),
            GlyphFit::Text => self.font_px,
        };
        let (metrics, bitmap) = self.fonts[font_idx].rasterize(ch, px);
        let (bitmap, tw, th) = if fit == GlyphFit::Cell {
            trim_cover(&bitmap, metrics.width as u32, metrics.height as u32)
        } else {
            (bitmap, metrics.width as u32, metrics.height as u32)
        };
        let glyph = Glyph {
            w: tw,
            h: th,
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

fn trim_cover(cover: &[u8], width: u32, height: u32) -> (Vec<u8>, u32, u32) {
    if width == 0 || height == 0 {
        return (cover.to_vec(), width, height);
    }
    let opaque = |x: u32, y: u32| cover[(y * width + x) as usize] > 8;
    let mut min_x = width;
    let mut max_x = 0;
    let mut min_y = height;
    let mut max_y = 0;
    for y in 0..height {
        for x in 0..width {
            if opaque(x, y) {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }
    if min_x > max_x {
        return (cover.to_vec(), width, height);
    }
    let tw = max_x - min_x + 1;
    let th = max_y - min_y + 1;
    let mut trimmed = Vec::with_capacity((tw * th) as usize);
    for y in min_y..=max_y {
        let start = (y * width + min_x) as usize;
        trimmed.extend_from_slice(&cover[start..start + tw as usize]);
    }
    (trimmed, tw, th)
}

fn glyph_fit(ch: char, font_idx: usize) -> GlyphFit {
    match ch {
        '\u{e0a0}'..='\u{e0ff}' | '\u{2580}'..='\u{259f}' | '\u{25e2}'..='\u{25e5}' => GlyphFit::Cell,
        _ if font_idx > 0 => GlyphFit::Icon,
        _ => GlyphFit::Text,
    }
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
    let cell_h = (typo * line_height.max(1.0)).ceil().max(typo.ceil()) as u32 + 1;
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
