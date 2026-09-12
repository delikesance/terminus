//! Compact GPU2 frame protocol (Kitty-style cell grid + sprite array atlas).
//!
//! Wire format (`GPU2`):
//! - magic `b"GPU2"`
//! - `cols:u16`, `rows:u16`
//! - `cell_w:u32`, `cell_h:u32`
//! - `sprites_per_layer:u16`, `layer_count:u16`
//! - `sprite_n:u32`, then per sprite:
//!   `sprite_idx:u16`, `sprite_layer:u16`, `has_bits:u8`, `format:u8`
//!   if `has_bits != 0`: `cell_w * (cell_h + 1) * bpp` stamp bytes
//!   (`format=0` → R8 bpp=1; `format=1` → RGBA bpp=4)
//! - `cols*rows` × [`GpuCell`] (20 bytes each, contiguous)

pub const GPU2_FRAME_MAGIC: &[u8; 4] = b"GPU2";
pub const GPU_CELL_SIZE: usize = 20;

/// Legacy alias kept for call-site migration during the GPU1→GPU2 cutover.
pub const GPU_FRAME_MAGIC: &[u8; 4] = GPU2_FRAME_MAGIC;

pub const STAMP_FMT_R8: u8 = 0;
pub const STAMP_FMT_RGBA: u8 = 1;

pub const ATTR_UNDERLINE_MASK: u32 = 0xf;
pub const ATTR_UNDERLINE_SINGLE: u32 = 1;
pub const ATTR_STRIKE: u32 = 1 << 4;
pub const ATTR_DIM: u32 = 1 << 5;
pub const ATTR_BOLD: u32 = 1 << 6;
pub const ATTR_ITALIC: u32 = 1 << 7;
pub const ATTR_BLINK: u32 = 1 << 8;
/// Cell uses a colored (RGBA) atlas stamp — composite stamp RGB over bg, ignore fg tint.
pub const ATTR_COLORED: u32 = 1 << 9;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpuCell {
    pub fg: u32,
    pub bg: u32,
    pub decoration_fg: u32,
    pub sprite_idx: u16,
    pub sprite_layer: u16,
    pub attrs: u32,
}

const _: () = assert!(std::mem::size_of::<GpuCell>() == GPU_CELL_SIZE);

#[derive(Clone, Debug)]
pub struct AtlasSprite {
    pub sprite_idx: u16,
    pub sprite_layer: u16,
    /// `STAMP_FMT_R8` or `STAMP_FMT_RGBA`.
    pub format: u8,
    /// When set, `bits` is packed after the sprite header (incremental atlas upload).
    pub bits: Option<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct GpuFrame {
    pub cols: u16,
    pub rows: u16,
    pub cell_w: u32,
    pub cell_h: u32,
    pub sprites_per_layer: u16,
    pub layer_count: u16,
    pub sprites: Vec<AtlasSprite>,
    pub cells: Vec<GpuCell>,
}

pub fn rgba_to_u32(c: [u8; 4]) -> u32 {
    u32::from_le_bytes(c)
}

pub fn u32_to_rgba(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

pub fn stamp_bpp(format: u8) -> u32 {
    if format == STAMP_FMT_RGBA {
        4
    } else {
        1
    }
}

pub fn stamp_bytes(cell_w: u32, cell_h: u32, format: u8) -> usize {
    (cell_w.saturating_mul(cell_h.saturating_add(1)).saturating_mul(stamp_bpp(format))) as usize
}

pub fn pack_gpu2_frame(frame: &GpuFrame) -> Vec<u8> {
    let mut out = Vec::with_capacity(estimate_size(frame));
    out.extend_from_slice(GPU2_FRAME_MAGIC);
    out.extend_from_slice(&frame.cols.to_le_bytes());
    out.extend_from_slice(&frame.rows.to_le_bytes());
    out.extend_from_slice(&frame.cell_w.to_le_bytes());
    out.extend_from_slice(&frame.cell_h.to_le_bytes());
    out.extend_from_slice(&frame.sprites_per_layer.to_le_bytes());
    out.extend_from_slice(&frame.layer_count.to_le_bytes());
    let sprite_n = frame.sprites.len() as u32;
    out.extend_from_slice(&sprite_n.to_le_bytes());
    for s in &frame.sprites {
        out.extend_from_slice(&s.sprite_idx.to_le_bytes());
        out.extend_from_slice(&s.sprite_layer.to_le_bytes());
        match &s.bits {
            Some(bits) => {
                out.push(1);
                out.push(s.format);
                let expect = stamp_bytes(frame.cell_w, frame.cell_h, s.format);
                debug_assert_eq!(bits.len(), expect);
                out.extend_from_slice(bits);
            }
            None => {
                out.push(0);
                out.push(s.format);
            }
        }
    }
    debug_assert_eq!(frame.cells.len(), frame.cols as usize * frame.rows as usize);
    for cell in &frame.cells {
        out.extend_from_slice(&cell.fg.to_le_bytes());
        out.extend_from_slice(&cell.bg.to_le_bytes());
        out.extend_from_slice(&cell.decoration_fg.to_le_bytes());
        out.extend_from_slice(&cell.sprite_idx.to_le_bytes());
        out.extend_from_slice(&cell.sprite_layer.to_le_bytes());
        out.extend_from_slice(&cell.attrs.to_le_bytes());
    }
    out
}

/// Alias used by older call sites; packs GPU2.
pub fn pack_gpu_frame(frame: &GpuFrame) -> Vec<u8> {
    pack_gpu2_frame(frame)
}

pub fn unpack_gpu2_frame(bytes: &[u8]) -> Option<GpuFrame> {
    if !is_gpu2_frame(bytes) || bytes.len() < 24 {
        return None;
    }
    let mut o = 4usize;
    let cols = read_u16(bytes, &mut o)?;
    let rows = read_u16(bytes, &mut o)?;
    let cell_w = read_u32(bytes, &mut o)?;
    let cell_h = read_u32(bytes, &mut o)?;
    let sprites_per_layer = read_u16(bytes, &mut o)?;
    let layer_count = read_u16(bytes, &mut o)?;
    let sprite_n = read_u32(bytes, &mut o)? as usize;
    let mut sprites = Vec::with_capacity(sprite_n);
    for _ in 0..sprite_n {
        let sprite_idx = read_u16(bytes, &mut o)?;
        let sprite_layer = read_u16(bytes, &mut o)?;
        let has_bits = *bytes.get(o)?;
        o += 1;
        let format = *bytes.get(o)?;
        o += 1;
        let bits = if has_bits != 0 {
            let stamp = stamp_bytes(cell_w, cell_h, format);
            if o + stamp > bytes.len() {
                return None;
            }
            let slice = bytes[o..o + stamp].to_vec();
            o += stamp;
            Some(slice)
        } else {
            None
        };
        sprites.push(AtlasSprite {
            sprite_idx,
            sprite_layer,
            format,
            bits,
        });
    }
    let cell_count = cols as usize * rows as usize;
    if o + cell_count * GPU_CELL_SIZE > bytes.len() {
        return None;
    }
    let mut cells = Vec::with_capacity(cell_count);
    for _ in 0..cell_count {
        let fg = read_u32(bytes, &mut o)?;
        let bg = read_u32(bytes, &mut o)?;
        let decoration_fg = read_u32(bytes, &mut o)?;
        let sprite_idx = read_u16(bytes, &mut o)?;
        let sprite_layer = read_u16(bytes, &mut o)?;
        let attrs = read_u32(bytes, &mut o)?;
        cells.push(GpuCell {
            fg,
            bg,
            decoration_fg,
            sprite_idx,
            sprite_layer,
            attrs,
        });
    }
    Some(GpuFrame {
        cols,
        rows,
        cell_w,
        cell_h,
        sprites_per_layer,
        layer_count,
        sprites,
        cells,
    })
}

pub fn is_gpu2_frame(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && &bytes[..4] == GPU2_FRAME_MAGIC
}

/// Returns true when `bytes` starts with the GPU2 magic.
pub fn is_gpu_frame(bytes: &[u8]) -> bool {
    is_gpu2_frame(bytes)
}

/// Push low-contrast foreground away from background (simple luma separation).
pub fn contrast_fg(fg: [u8; 4], bg: [u8; 4]) -> [u8; 4] {
    let fl = luma(fg);
    let bl = luma(bg);
    let delta = (fl as i32 - bl as i32).abs();
    if delta >= 48 {
        return fg;
    }
    let target = if bl < 128 { 255u8 } else { 0u8 };
    let t = 0.55f32;
    [
        lerp(fg[0], target, t),
        lerp(fg[1], target, t),
        lerp(fg[2], target, t),
        fg[3],
    ]
}

fn luma(c: [u8; 4]) -> u32 {
    (c[0] as u32 * 30 + c[1] as u32 * 59 + c[2] as u32 * 11) / 100
}

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8
}

fn estimate_size(frame: &GpuFrame) -> usize {
    let mut n = 24;
    for s in &frame.sprites {
        n += 6;
        if let Some(bits) = &s.bits {
            n += bits.len();
        }
    }
    n + frame.cells.len() * GPU_CELL_SIZE
}

fn read_u16(bytes: &[u8], o: &mut usize) -> Option<u16> {
    if *o + 2 > bytes.len() {
        return None;
    }
    let v = u16::from_le_bytes([bytes[*o], bytes[*o + 1]]);
    *o += 2;
    Some(v)
}

fn read_u32(bytes: &[u8], o: &mut usize) -> Option<u32> {
    if *o + 4 > bytes.len() {
        return None;
    }
    let v = u32::from_le_bytes([bytes[*o], bytes[*o + 1], bytes[*o + 2], bytes[*o + 3]]);
    *o += 4;
    Some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_frame(with_bits: bool) -> GpuFrame {
        let cell_w = 9u32;
        let cell_h = 18u32;
        let bits = if with_bits {
            Some(vec![0u8; stamp_bytes(cell_w, cell_h, STAMP_FMT_R8)])
        } else {
            None
        };
        GpuFrame {
            cols: 4,
            rows: 2,
            cell_w,
            cell_h,
            sprites_per_layer: 32,
            layer_count: 1,
            sprites: vec![AtlasSprite {
                sprite_idx: 1,
                sprite_layer: 0,
                format: STAMP_FMT_R8,
                bits,
            }],
            cells: (0..8)
                .map(|i| GpuCell {
                    fg: rgba_to_u32([245, 245, 247, 255]),
                    bg: rgba_to_u32([28, 28, 30, 255]),
                    decoration_fg: rgba_to_u32([245, 245, 247, 255]),
                    sprite_idx: if i == 0 { 1 } else { 0 },
                    sprite_layer: 0,
                    attrs: 0,
                })
                .collect(),
        }
    }

    #[test]
    fn packed_gpu_frame_starts_with_magic() {
        let packed = pack_gpu2_frame(&sample_frame(true));
        assert!(is_gpu2_frame(&packed));
        assert_eq!(&packed[..4], b"GPU2");
    }

    #[test]
    fn gpu_frame_is_far_smaller_than_full_rgba() {
        let cols = 80u32;
        let rows = 24u32;
        let cell_w = 9u32;
        let cell_h = 18u32;
        let rgba_bytes = (cols * cell_w * rows * cell_h * 4) as usize;
        let frame = GpuFrame {
            cols: cols as u16,
            rows: rows as u16,
            cell_w,
            cell_h,
            sprites_per_layer: 64,
            layer_count: 1,
            sprites: vec![AtlasSprite {
                sprite_idx: 1,
                sprite_layer: 0,
                format: STAMP_FMT_R8,
                bits: Some(vec![255u8; stamp_bytes(cell_w, cell_h, STAMP_FMT_R8)]),
            }],
            cells: vec![
                GpuCell {
                    fg: rgba_to_u32([255, 255, 255, 255]),
                    bg: rgba_to_u32([0, 0, 0, 255]),
                    decoration_fg: rgba_to_u32([255, 255, 255, 255]),
                    sprite_idx: 0,
                    sprite_layer: 0,
                    attrs: 0,
                };
                (cols * rows) as usize
            ],
        };
        let packed = pack_gpu2_frame(&frame);
        assert!(
            packed.len() < rgba_bytes / 8,
            "gpu pack {} should be << rgba {}",
            packed.len(),
            rgba_bytes
        );
        let mut steady = frame.clone();
        steady.sprites[0].bits = None;
        let steady_pack = pack_gpu2_frame(&steady);
        assert!(steady_pack.len() < packed.len());
        assert!(steady_pack.len() < 80 * 24 * 24);
    }

    #[test]
    fn glyph_bits_inflate_payload() {
        let with = pack_gpu2_frame(&sample_frame(true));
        let without = pack_gpu2_frame(&sample_frame(false));
        assert!(with.len() > without.len());
    }

    #[test]
    fn rgba_stamp_roundtrip_preserves_format_and_bits() {
        let cell_w = 4u32;
        let cell_h = 6u32;
        let stamp = stamp_bytes(cell_w, cell_h, STAMP_FMT_RGBA);
        let mut bits = vec![0u8; stamp];
        for (i, b) in bits.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        let frame = GpuFrame {
            cols: 2,
            rows: 1,
            cell_w,
            cell_h,
            sprites_per_layer: 8,
            layer_count: 1,
            sprites: vec![AtlasSprite {
                sprite_idx: 1,
                sprite_layer: 0,
                format: STAMP_FMT_RGBA,
                bits: Some(bits.clone()),
            }],
            cells: vec![
                GpuCell {
                    fg: 0,
                    bg: 0,
                    decoration_fg: 0,
                    sprite_idx: 1,
                    sprite_layer: 0,
                    attrs: ATTR_COLORED,
                },
                GpuCell {
                    fg: 0,
                    bg: 0,
                    decoration_fg: 0,
                    sprite_idx: 0,
                    sprite_layer: 0,
                    attrs: 0,
                },
            ],
        };
        let packed = pack_gpu2_frame(&frame);
        let back = unpack_gpu2_frame(&packed).expect("unpack rgba");
        assert_eq!(back.sprites[0].format, STAMP_FMT_RGBA);
        assert_eq!(back.sprites[0].bits.as_ref().map(|b| b.len()), Some(stamp));
        assert_eq!(back.sprites[0].bits.as_ref().unwrap(), &bits);
        assert_eq!(back.cells[0].attrs & ATTR_COLORED, ATTR_COLORED);
        // R8 path stays default format=0
        let r8 = sample_frame(true);
        let packed_r8 = pack_gpu2_frame(&r8);
        let back_r8 = unpack_gpu2_frame(&packed_r8).expect("unpack r8");
        assert_eq!(back_r8.sprites[0].format, STAMP_FMT_R8);
    }
}
