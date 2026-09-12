//! #163 — Kitty-style GPU2 wire protocol, box drawing, and contrast contracts.
//! Red phase: these symbols/behaviors must exist after implementation.

use std::mem::{align_of, size_of};
use terminus_core::box_draw::{is_procedural_cell, render_box_cell};
use terminus_core::gpu_frame::{
    contrast_fg, is_gpu2_frame, pack_gpu2_frame, unpack_gpu2_frame, AtlasSprite, GpuCell, GpuFrame,
    GPU2_FRAME_MAGIC, GPU_CELL_SIZE,
};

fn sample_frame(with_bits: bool) -> GpuFrame {
    let cell_w = 9u32;
    let cell_h = 18u32;
    let stamp_h = cell_h + 1;
    let bits = if with_bits {
        Some(vec![0u8; (cell_w * stamp_h) as usize])
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
            bits,
        }],
        cells: (0..8)
            .map(|i| GpuCell {
                fg: u32::from_le_bytes([0xf7, 0xf5, 0xf5, 0xff]),
                bg: u32::from_le_bytes([0x1e, 0x1c, 0x1c, 0xff]),
                decoration_fg: u32::from_le_bytes([0xf7, 0xf5, 0xf5, 0xff]),
                sprite_idx: if i == 0 { 1 } else { 0 },
                sprite_layer: 0,
                attrs: if i == 0 { 1 } else { 0 }, // underline single
            })
            .collect(),
    }
}

#[test]
fn ac1_gpu_cell_is_exactly_20_bytes_and_c_layout() {
    assert_eq!(GPU_CELL_SIZE, 20);
    assert_eq!(size_of::<GpuCell>(), 20);
    assert!(align_of::<GpuCell>() >= 4);
}

#[test]
fn ac1_packed_frame_starts_with_gpu2_magic() {
    assert_eq!(GPU2_FRAME_MAGIC, b"GPU2");
    let packed = pack_gpu2_frame(&sample_frame(true));
    assert!(is_gpu2_frame(&packed));
    assert_eq!(&packed[..4], b"GPU2");
}

#[test]
fn ac1_round_trip_preserves_cells_and_sprite_delta() {
    let frame = sample_frame(true);
    let packed = pack_gpu2_frame(&frame);
    let back = unpack_gpu2_frame(&packed).expect("unpack");
    assert_eq!(back.cols, frame.cols);
    assert_eq!(back.rows, frame.rows);
    assert_eq!(back.cell_w, frame.cell_w);
    assert_eq!(back.cell_h, frame.cell_h);
    assert_eq!(back.cells.len(), frame.cells.len());
    assert_eq!(back.cells[0].sprite_idx, 1);
    assert_eq!(back.cells[0].attrs & 0xf, 1);
    assert_eq!(back.sprites.len(), 1);
    assert_eq!(
        back.sprites[0].bits.as_ref().map(|b| b.len()),
        Some((frame.cell_w * (frame.cell_h + 1)) as usize)
    );
}

#[test]
fn ac1_cells_are_contiguous_20_byte_records_on_wire() {
    let frame = sample_frame(false);
    let packed = pack_gpu2_frame(&frame);
    let back = unpack_gpu2_frame(&packed).expect("unpack");
    let cell_bytes = back.cols as usize * back.rows as usize * GPU_CELL_SIZE;
    // Steady frame: magic+header+sprite headers without bits + cell block.
    assert!(
        packed.len() >= 4 + cell_bytes,
        "packed {} must contain contiguous cell block of {}",
        packed.len(),
        cell_bytes
    );
    assert_eq!(back.cells[1].sprite_idx, 0);
}

#[test]
fn ac1_corrupt_or_truncated_payload_returns_none() {
    assert!(unpack_gpu2_frame(b"").is_none());
    assert!(unpack_gpu2_frame(b"GPU1").is_none());
    assert!(unpack_gpu2_frame(b"GPU2").is_none());
    let mut packed = pack_gpu2_frame(&sample_frame(false));
    packed.truncate(8);
    assert!(unpack_gpu2_frame(&packed).is_none());
}

#[test]
fn ac1_gpu2_frame_is_far_smaller_than_full_rgba() {
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
            bits: Some(vec![255u8; (cell_w * (cell_h + 1)) as usize]),
        }],
        cells: vec![
            GpuCell {
                fg: u32::from_le_bytes([255, 255, 255, 255]),
                bg: u32::from_le_bytes([0, 0, 0, 255]),
                decoration_fg: u32::from_le_bytes([255, 255, 255, 255]),
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
        "gpu2 pack {} should be << rgba {}",
        packed.len(),
        rgba_bytes
    );
}

#[test]
fn ac3_box_drawing_and_braille_are_procedural() {
    assert!(is_procedural_cell(0x2500)); // ─
    assert!(is_procedural_cell(0x2550)); // ═
    assert!(is_procedural_cell(0x2588)); // █
    assert!(is_procedural_cell(0x2800)); // braille
    assert!(is_procedural_cell(0xe0b0)); // powerline
    assert!(!is_procedural_cell(b'A' as u32));

    let bits = render_box_cell(0x2500, 10, 20).expect("horizontal line");
    assert_eq!(bits.len(), 10 * 21); // cell_h + 1 exclusion row
    assert!(
        bits.iter().any(|&p| p > 0),
        "procedural box cell must paint ink"
    );
}

#[test]
fn ac3_powerline_arrow_is_procedural_and_nonempty() {
    let bits = render_box_cell(0xe0b0, 12, 24).expect("powerline");
    assert_eq!(bits.len(), 12 * 25);
    assert!(bits.iter().any(|&p| p > 200));
}

#[test]
fn ac4_contrast_compensation_separates_near_colors() {
    let fg = [40, 40, 40, 255];
    let bg = [30, 30, 30, 255];
    let out = contrast_fg(fg, bg);
    let before = (fg[0] as i32 - bg[0] as i32).abs();
    let after = (out[0] as i32 - bg[0] as i32).abs();
    assert!(
        after > before,
        "low-contrast fg must be pushed away from bg ({out:?} vs {fg:?} on {bg:?})"
    );
}
