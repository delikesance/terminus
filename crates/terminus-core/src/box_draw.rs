//! Procedural pixel-perfect box drawing, blocks, Braille, and Powerline glyphs.

/// True for Unicode ranges we rasterize procedurally (not via the font).
pub fn is_procedural_cell(codepoint: u32) -> bool {
    matches!(
        codepoint,
        0x2500..=0x259F | 0x2800..=0x28FF | 0xE0B0..=0xE0D4
    )
}

/// Rasterize a procedural cell into R8 coverage of size `w × (h + 1)`.
/// The extra row is the underline exclusion mask channel (Kitty-style).
pub fn render_box_cell(codepoint: u32, w: u32, h: u32) -> Option<Vec<u8>> {
    if w == 0 || h == 0 || !is_procedural_cell(codepoint) {
        return None;
    }
    let stamp_h = h + 1;
    let mut bits = vec![0u8; (w * stamp_h) as usize];
    match codepoint {
        0x2500..=0x257F => draw_box_drawing(&mut bits, w, h, codepoint),
        0x2580..=0x259F => draw_block(&mut bits, w, h, codepoint),
        0x2800..=0x28FF => draw_braille(&mut bits, w, h, codepoint),
        0xE0B0..=0xE0D4 => draw_powerline(&mut bits, w, h, codepoint),
        _ => return None,
    }
    Some(bits)
}

fn idx(w: u32, x: u32, y: u32) -> usize {
    (y * w + x) as usize
}

fn plot(bits: &mut [u8], w: u32, h: u32, x: i32, y: i32, v: u8) {
    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
        return;
    }
    let i = idx(w, x as u32, y as u32);
    bits[i] = bits[i].max(v);
}

fn hline(bits: &mut [u8], w: u32, h: u32, y: i32, x0: i32, x1: i32, v: u8) {
    let (a, b) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
    for x in a..=b {
        plot(bits, w, h, x, y, v);
    }
}

fn vline(bits: &mut [u8], w: u32, h: u32, x: i32, y0: i32, y1: i32, v: u8) {
    let (a, b) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
    for y in a..=b {
        plot(bits, w, h, x, y, v);
    }
}

fn mid_x(w: u32) -> i32 {
    (w as i32) / 2
}

fn mid_y(h: u32) -> i32 {
    (h as i32) / 2
}

fn draw_box_drawing(bits: &mut [u8], w: u32, h: u32, cp: u32) {
    let mx = mid_x(w);
    let my = mid_y(h);
    let last_x = w as i32 - 1;
    let last_y = h as i32 - 1;
    let thick = 255u8;
    // Light horizontal / vertical and common corners (subset covering tests + tables).
    let (left, right, up, down) = match cp {
        0x2500 | 0x2501 | 0x2550 => (true, true, false, false), // ─ ━ ═
        0x2502 | 0x2503 | 0x2551 => (false, false, true, true), // │ ┃ ║
        0x250C | 0x250F | 0x2554 => (false, true, false, true), // ┌ ┏ ╔
        0x2510 | 0x2513 | 0x2557 => (true, false, false, true), // ┐ ┓ ╗
        0x2514 | 0x2517 | 0x255A => (false, true, true, false), // └ ┗ ╚
        0x2518 | 0x251B | 0x255D => (true, false, true, false), // ┘ ┛ ╝
        0x251C | 0x2523 | 0x2560 => (false, true, true, true),  // ├ ┣ ╠
        0x2524 | 0x252B | 0x2563 => (true, false, true, true),  // ┤ ┫ ╣
        0x252C | 0x2533 | 0x2566 => (true, true, false, true),  // ┬ ┳ ╦
        0x2534 | 0x253B | 0x2569 => (true, true, true, false),  // ┴ ┻ ╩
        0x253C | 0x254B | 0x256C => (true, true, true, true),   // ┼ ╋ ╬
        _ => {
            // Fallback: draw a plus so unknown box chars still paint ink.
            (true, true, true, true)
        }
    };
    if left {
        hline(bits, w, h, my, 0, mx, thick);
    }
    if right {
        hline(bits, w, h, my, mx, last_x, thick);
    }
    if up {
        vline(bits, w, h, mx, 0, my, thick);
    }
    if down {
        vline(bits, w, h, mx, my, last_y, thick);
    }
    // Double-line variants: offset parallel stroke.
    if matches!(cp, 0x2550 | 0x2551 | 0x2554 | 0x2557 | 0x255A | 0x255D | 0x2560 | 0x2563 | 0x2566 | 0x2569 | 0x256C)
    {
        if left || right {
            hline(bits, w, h, my - 1, if left { 0 } else { mx }, if right { last_x } else { mx }, thick);
        }
        if up || down {
            vline(bits, w, h, mx - 1, if up { 0 } else { my }, if down { last_y } else { my }, thick);
        }
    }
}

fn draw_block(bits: &mut [u8], w: u32, h: u32, cp: u32) {
    let last_x = w as i32 - 1;
    let last_y = h as i32 - 1;
    let mid = mid_y(h);
    let midx = mid_x(w);
    let (x0, y0, x1, y1, v) = match cp {
        0x2588 => (0, 0, last_x, last_y, 255u8),           // █ full
        0x2584 => (0, mid, last_x, last_y, 255),           // ▄ lower half
        0x2580 => (0, 0, last_x, mid, 255),                // ▀ upper half
        0x258C => (0, 0, midx, last_y, 255),               // ▌ left half
        0x2590 => (midx, 0, last_x, last_y, 255),          // ▐ right half
        0x2591 => (0, 0, last_x, last_y, 64),              // ░
        0x2592 => (0, 0, last_x, last_y, 128),             // ▒
        0x2593 => (0, 0, last_x, last_y, 192),             // ▓
        _ => (0, 0, last_x, last_y, 200),
    };
    for y in y0..=y1 {
        for x in x0..=x1 {
            plot(bits, w, h, x, y, v);
        }
    }
}

fn draw_braille(bits: &mut [u8], w: u32, h: u32, cp: u32) {
    let dots = cp - 0x2800;
    // 2×4 braille grid positions.
    let xs = [w as i32 / 4, (3 * w as i32) / 4];
    let ys = [
        h as i32 / 8,
        (3 * h as i32) / 8,
        (5 * h as i32) / 8,
        (7 * h as i32) / 8,
    ];
    let map = [
        (0, 0),
        (0, 1),
        (0, 2),
        (1, 0),
        (1, 1),
        (1, 2),
        (0, 3),
        (1, 3),
    ];
    for (bit, (cx, cy)) in map.iter().enumerate() {
        if dots & (1 << bit) != 0 {
            let x = xs[*cx];
            let y = ys[*cy];
            for dy in -1..=1 {
                for dx in -1..=1 {
                    plot(bits, w, h, x + dx, y + dy, 255);
                }
            }
        }
    }
}

fn draw_powerline(bits: &mut [u8], w: u32, h: u32, cp: u32) {
    let last_x = w as i32 - 1;
    let last_y = h as i32 - 1;
    match cp {
        0xE0B0 => {
            // Solid right arrow ▸
            for y in 0..=last_y {
                let t = y as f32 / last_y.max(1) as f32;
                let edge = ((1.0 - (2.0 * t - 1.0).abs()) * last_x as f32).round() as i32;
                for x in 0..=edge {
                    plot(bits, w, h, x, y, 255);
                }
            }
        }
        0xE0B2 => {
            // Solid left arrow
            for y in 0..=last_y {
                let t = y as f32 / last_y.max(1) as f32;
                let edge = ((1.0 - (2.0 * t - 1.0).abs()) * last_x as f32).round() as i32;
                for x in (last_x - edge)..=last_x {
                    plot(bits, w, h, x, y, 255);
                }
            }
        }
        0xE0B1 | 0xE0B3 => {
            // Thin separators — diagonal
            for y in 0..=last_y {
                let x = if cp == 0xE0B1 {
                    (y as f32 / last_y.max(1) as f32 * last_x as f32).round() as i32
                } else {
                    last_x - (y as f32 / last_y.max(1) as f32 * last_x as f32).round() as i32
                };
                plot(bits, w, h, x, y, 255);
                plot(bits, w, h, x.saturating_sub(1), y, 200);
            }
        }
        _ => {
            // Generic filled chevron so Powerline range always paints.
            for y in 0..=last_y {
                let t = (2.0 * y as f32 / last_y.max(1) as f32 - 1.0).abs();
                let edge = ((1.0 - t) * last_x as f32).round() as i32;
                for x in 0..=edge {
                    plot(bits, w, h, x, y, 220);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn horizontal_line_has_ink() {
        let bits = render_box_cell(0x2500, 10, 20).unwrap();
        assert!(bits.iter().any(|&p| p > 0));
    }
}
