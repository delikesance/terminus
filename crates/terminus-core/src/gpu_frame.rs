//! Compact GPU frame protocol for atlas + cell-grid paint (no full-screen RGBA).
//!
//! Wire format (`GPU1`):
//! - magic `b"GPU1"`
//! - `cols:u16`, `rows:u16`
//! - `cell_w:u32`, `cell_h:u32`
//! - `atlas_w:u32`, `atlas_h:u32` (UV space; client-owned texture)
//! - `glyph_n:u32`, then per glyph:
//!   `id:u16`, `x:u16`, `y:u16`, `w:u16`, `h:u16`, `ox:i16`, `oy:i16`, `has_bits:u8`, `_pad:u8`
//!   if `has_bits != 0`: `w*h` R8 coverage bytes (new/changed stamps only)
//! - `cols*rows` × (`glyph_id:u16`, `fg:u32` rgba LE, `bg:u32` rgba LE)

pub const GPU_FRAME_MAGIC: &[u8; 4] = b"GPU1";

#[derive(Clone, Debug)]
pub struct AtlasGlyph {
    pub id: u16,
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub ox: i16,
    pub oy: i16,
    /// When set, `bits` is packed after the glyph header (incremental atlas upload).
    pub bits: Option<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct GpuCell {
    pub glyph_id: u16,
    pub fg: [u8; 4],
    pub bg: [u8; 4],
}

#[derive(Clone, Debug)]
pub struct GpuFrame {
    pub cols: u16,
    pub rows: u16,
    pub cell_w: u32,
    pub cell_h: u32,
    pub atlas_w: u32,
    pub atlas_h: u32,
    pub glyphs: Vec<AtlasGlyph>,
    pub cells: Vec<GpuCell>,
}

pub fn pack_gpu_frame(frame: &GpuFrame) -> Vec<u8> {
    let mut out = Vec::with_capacity(estimate_size(frame));
    out.extend_from_slice(GPU_FRAME_MAGIC);
    out.extend_from_slice(&frame.cols.to_le_bytes());
    out.extend_from_slice(&frame.rows.to_le_bytes());
    out.extend_from_slice(&frame.cell_w.to_le_bytes());
    out.extend_from_slice(&frame.cell_h.to_le_bytes());
    out.extend_from_slice(&frame.atlas_w.to_le_bytes());
    out.extend_from_slice(&frame.atlas_h.to_le_bytes());
    let glyph_n = frame.glyphs.len() as u32;
    out.extend_from_slice(&glyph_n.to_le_bytes());
    for g in &frame.glyphs {
        out.extend_from_slice(&g.id.to_le_bytes());
        out.extend_from_slice(&g.x.to_le_bytes());
        out.extend_from_slice(&g.y.to_le_bytes());
        out.extend_from_slice(&g.w.to_le_bytes());
        out.extend_from_slice(&g.h.to_le_bytes());
        out.extend_from_slice(&g.ox.to_le_bytes());
        out.extend_from_slice(&g.oy.to_le_bytes());
        match &g.bits {
            Some(bits) => {
                out.push(1);
                out.push(0);
                debug_assert_eq!(bits.len(), g.w as usize * g.h as usize);
                out.extend_from_slice(bits);
            }
            None => {
                out.push(0);
                out.push(0);
            }
        }
    }
    debug_assert_eq!(frame.cells.len(), frame.cols as usize * frame.rows as usize);
    for cell in &frame.cells {
        out.extend_from_slice(&cell.glyph_id.to_le_bytes());
        out.extend_from_slice(&rgba_u32(cell.fg).to_le_bytes());
        out.extend_from_slice(&rgba_u32(cell.bg).to_le_bytes());
    }
    out
}

fn rgba_u32(c: [u8; 4]) -> u32 {
    u32::from_le_bytes(c)
}

fn estimate_size(frame: &GpuFrame) -> usize {
    let mut n = 4 + 2 + 2 + 4 + 4 + 4 + 4 + 4;
    for g in &frame.glyphs {
        n += 16;
        if let Some(bits) = &g.bits {
            n += bits.len();
        }
    }
    n += frame.cells.len() * 10;
    n
}

/// Returns true when `bytes` starts with the GPU1 magic.
pub fn is_gpu_frame(bytes: &[u8]) -> bool {
    bytes.len() >= 4 && &bytes[..4] == GPU_FRAME_MAGIC
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_frame(with_bits: bool) -> GpuFrame {
        let bits = if with_bits {
            Some(vec![0u8; 8 * 12])
        } else {
            None
        };
        GpuFrame {
            cols: 4,
            rows: 2,
            cell_w: 9,
            cell_h: 18,
            atlas_w: 1024,
            atlas_h: 1024,
            glyphs: vec![AtlasGlyph {
                id: 1,
                x: 0,
                y: 0,
                w: 8,
                h: 12,
                ox: 1,
                oy: 2,
                bits,
            }],
            cells: (0..8)
                .map(|i| GpuCell {
                    glyph_id: if i == 0 { 1 } else { 0 },
                    fg: [245, 245, 247, 255],
                    bg: [28, 28, 30, 255],
                })
                .collect(),
        }
    }

    #[test]
    fn packed_gpu_frame_starts_with_magic() {
        let packed = pack_gpu_frame(&sample_frame(true));
        assert!(is_gpu_frame(&packed));
        assert_eq!(&packed[..4], b"GPU1");
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
            atlas_w: 1024,
            atlas_h: 1024,
            glyphs: vec![AtlasGlyph {
                id: 1,
                x: 0,
                y: 0,
                w: 8,
                h: 12,
                ox: 0,
                oy: 0,
                bits: Some(vec![255u8; 8 * 12]),
            }],
            cells: vec![
                GpuCell {
                    glyph_id: 0,
                    fg: [255, 255, 255, 255],
                    bg: [0, 0, 0, 255],
                };
                (cols * rows) as usize
            ],
        };
        let packed = pack_gpu_frame(&frame);
        assert!(
            packed.len() < rgba_bytes / 8,
            "gpu pack {} should be << rgba {}",
            packed.len(),
            rgba_bytes
        );
        let mut steady = frame.clone();
        steady.glyphs[0].bits = None;
        let steady_pack = pack_gpu_frame(&steady);
        assert!(steady_pack.len() < packed.len());
        assert!(steady_pack.len() < 80 * 24 * 16);
    }

    #[test]
    fn glyph_bits_inflate_payload() {
        let with = pack_gpu_frame(&sample_frame(true));
        let without = pack_gpu_frame(&sample_frame(false));
        assert!(with.len() > without.len());
    }
}
