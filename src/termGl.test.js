/**
 * #163 — GPU2 decode + Kitty-style WebGL2 painter contracts.
 */
import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  GPU_CELL_BYTES,
  isGpu2Frame,
  decodeGpu2Frame,
  CELL_VS_SOURCE,
  CELL_FS_SOURCE,
} from "./termGl.js";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const termGlTs = fs.readFileSync(path.join(root, "src/termGl.ts"), "utf8");

assert.equal(GPU_CELL_BYTES, 20);

function writeU16(buf, o, v) {
  buf[o] = v & 255;
  buf[o + 1] = (v >> 8) & 255;
}
function writeU32(buf, o, v) {
  buf[o] = v & 255;
  buf[o + 1] = (v >> 8) & 255;
  buf[o + 2] = (v >> 16) & 255;
  buf[o + 3] = (v >> 24) & 255;
}

/** Minimal GPU2 frame: 2×1 grid, one sprite stamp with bits. */
function packSampleGpu2() {
  const cellW = 8;
  const cellH = 16;
  const stampH = cellH + 1;
  const bits = new Uint8Array(cellW * stampH);
  bits[0] = 255;
  const header = 4 + 2 + 2 + 4 + 4 + 2 + 2 + 4; // magic..sprite_n
  const spriteHdr = 2 + 2 + 1 + 1; // idx, layer, has_bits, pad
  const cells = 2 * 20;
  const out = new Uint8Array(header + spriteHdr + bits.length + cells);
  out[0] = 0x47;
  out[1] = 0x50;
  out[2] = 0x55;
  out[3] = 0x32; // GPU2
  let o = 4;
  writeU16(out, o, 2);
  o += 2;
  writeU16(out, o, 1);
  o += 2;
  writeU32(out, o, cellW);
  o += 4;
  writeU32(out, o, cellH);
  o += 4;
  writeU16(out, o, 32);
  o += 2; // sprites_per_layer
  writeU16(out, o, 1);
  o += 2; // layer_count
  writeU32(out, o, 1);
  o += 4; // sprite_n
  writeU16(out, o, 1);
  o += 2; // sprite_idx
  writeU16(out, o, 0);
  o += 2; // layer
  out[o++] = 1; // has_bits
  out[o++] = 0; // pad
  out.set(bits, o);
  o += bits.length;
  // cell 0
  writeU32(out, o, 0xfff5f5f7);
  o += 4;
  writeU32(out, o, 0xff1c1c1e);
  o += 4;
  writeU32(out, o, 0xfff5f5f7);
  o += 4;
  writeU16(out, o, 1);
  o += 2;
  writeU16(out, o, 0);
  o += 2;
  writeU32(out, o, 1);
  o += 4;
  // cell 1 empty
  writeU32(out, o, 0xfff5f5f7);
  o += 4;
  writeU32(out, o, 0xff1c1c1e);
  o += 4;
  writeU32(out, o, 0xfff5f5f7);
  o += 4;
  writeU16(out, o, 0);
  o += 2;
  writeU16(out, o, 0);
  o += 2;
  writeU32(out, o, 0);
  o += 4;
  return out;
}

// AC1 — magic + decode
{
  const raw = packSampleGpu2();
  assert.equal(isGpu2Frame(raw), true);
  assert.equal(isGpu2Frame(new Uint8Array([0x47, 0x50, 0x55, 0x31])), false);
  const frame = decodeGpu2Frame(raw);
  assert.ok(frame);
  assert.equal(frame.cols, 2);
  assert.equal(frame.rows, 1);
  assert.equal(frame.cellW, 8);
  assert.equal(frame.cellH, 16);
  assert.equal(frame.sprites.length, 1);
  assert.equal(frame.sprites[0].bits.length, 8 * 17);
  assert.equal(frame.cells.byteLength, 2 * 20);
  const view = new DataView(frame.cells.buffer, frame.cells.byteOffset, frame.cells.byteLength);
  assert.equal(view.getUint16(12, true), 1); // sprite_idx of cell 0
}

// AC1 — truncated / corrupt
{
  assert.equal(decodeGpu2Frame(new Uint8Array(0)), null);
  assert.equal(decodeGpu2Frame(new Uint8Array([0x47, 0x50, 0x55, 0x32])), null);
}

// AC2 — procedural VS / array FS (no CPU quad buffers in paint path)
{
  assert.ok(CELL_VS_SOURCE.includes("gl_InstanceID"), "VS must place cells via gl_InstanceID");
  assert.ok(CELL_VS_SOURCE.includes("gl_VertexID"), "VS must build quads via gl_VertexID");
  assert.ok(CELL_FS_SOURCE.includes("sampler2DArray"), "FS must sample TEXTURE_2D_ARRAY");
  assert.ok(
    !termGlTs.includes("bgBuf") && !termGlTs.includes("glyphBuf"),
    "CPU bgBuf/glyphBuf generation must be gone",
  );
  assert.ok(
    termGlTs.includes("drawArraysInstanced"),
    "painter must use a single instanced draw",
  );
}

// AC4 — underline exclusion in fragment shader
{
  assert.ok(
    CELL_FS_SOURCE.includes("underline") || CELL_FS_SOURCE.includes("decoration"),
    "FS must implement underline / decoration",
  );
  assert.ok(
    CELL_FS_SOURCE.includes("exclusion") || CELL_FS_SOURCE.includes("cell_h") || CELL_FS_SOURCE.includes("stamp"),
    "FS should mask underline under descenders",
  );
}

console.log("termGl.test.js: ok");
