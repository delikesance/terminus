/** WebGL2 painter for GPU2 Kitty-style frames (instanced cells + TEXTURE_2D_ARRAY). */

export const GPU_CELL_BYTES = 20;

export type GpuSprite = {
  spriteIdx: number;
  spriteLayer: number;
  bits: Uint8Array | null;
};

export type DecodedGpuFrame = {
  cols: number;
  rows: number;
  cellW: number;
  cellH: number;
  spritesPerLayer: number;
  layerCount: number;
  sprites: GpuSprite[];
  /** Contiguous GpuCell records (20 bytes each). */
  cells: Uint8Array;
};

const MAGIC_GPU2 = 0x32555047; // 'GPU2' LE

export const CELL_VS_SOURCE = `#version 300 es
precision highp float;
layout(location=0) in uint a_fg;
layout(location=1) in uint a_bg;
layout(location=2) in uint a_decoration_fg;
layout(location=3) in uint a_sprite_packed;
layout(location=4) in uint a_attrs;
uniform vec2 u_res;
uniform vec2 u_cell_size;
uniform float u_sprites_per_layer;
uniform float u_cell_h;
out vec2 v_uv;
out float v_layer;
out vec4 v_fg;
out vec4 v_bg;
out vec4 v_dec;
flat out uint v_attrs;
vec4 unpack_rgba(uint c) {
  return vec4(float(c & 255u), float((c >> 8u) & 255u), float((c >> 16u) & 255u), float((c >> 24u) & 255u)) / 255.0;
}
void main() {
  uint instance = uint(gl_InstanceID);
  uint cols = uint(u_res.x / u_cell_size.x);
  uint row = instance / cols;
  uint col = instance - row * cols;
  // TRIANGLE_FAN corners: 0=(1,0) 1=(1,1) 2=(0,1) 3=(0,0)
  vec2 corner;
  if (gl_VertexID == 0) corner = vec2(1.0, 0.0);
  else if (gl_VertexID == 1) corner = vec2(1.0, 1.0);
  else if (gl_VertexID == 2) corner = vec2(0.0, 1.0);
  else corner = vec2(0.0, 0.0);
  vec2 cell_pos = vec2(float(col) * u_cell_size.x, float(row) * u_cell_size.y);
  vec2 pos = cell_pos + corner * u_cell_size;
  vec2 p = pos / u_res * 2.0 - 1.0;
  p.y = -p.y;
  gl_Position = vec4(p, 0.0, 1.0);
  uint sprite_idx = a_sprite_packed & 0xffffu;
  uint sprite_layer = a_sprite_packed >> 16u;
  float spl = max(u_sprites_per_layer, 1.0);
  float u0 = float(sprite_idx) / spl;
  float u1 = float(sprite_idx + 1u) / spl;
  // stamp includes +1 exclusion row; text occupies [0, cell_h / (cell_h+1)]
  float v_scale = u_cell_h / (u_cell_h + 1.0);
  v_uv = vec2(mix(u0, u1, corner.x), corner.y * v_scale);
  v_layer = float(sprite_layer);
  v_fg = unpack_rgba(a_fg);
  v_bg = unpack_rgba(a_bg);
  v_dec = unpack_rgba(a_decoration_fg);
  v_attrs = a_attrs;
}`;

export const CELL_FS_SOURCE = `#version 300 es
precision highp float;
in vec2 v_uv;
in float v_layer;
in vec4 v_fg;
in vec4 v_bg;
in vec4 v_dec;
flat in uint v_attrs;
uniform highp sampler2DArray u_atlas;
uniform float u_cell_h;
out vec4 outColor;
void main() {
  vec4 sample = texture(u_atlas, vec3(v_uv, v_layer));
  float cover = sample.r;
  vec4 color = mix(v_bg, vec4(v_fg.rgb, 1.0), cover);
  uint underline = v_attrs & 0xfu;
  if (underline != 0u) {
    float y = gl_PointCoord.y; // unused; use UV y in cell space
    // Reconstruct cell-local y from v_uv (scaled text region).
    float local_y = v_uv.y * ((u_cell_h + 1.0) / max(u_cell_h, 1.0));
    float line_y = (u_cell_h - 1.5) / max(u_cell_h, 1.0);
    float exclusion = texture(u_atlas, vec3(v_uv.x, (u_cell_h + 0.5) / (u_cell_h + 1.0), v_layer)).r;
    if (abs(local_y - line_y) < (1.2 / max(u_cell_h, 1.0))) {
      float under = 1.0 - exclusion;
      color = mix(color, vec4(v_dec.rgb, 1.0), under);
    }
  }
  if ((v_attrs & 16u) != 0u) {
    float local_y = v_uv.y * ((u_cell_h + 1.0) / max(u_cell_h, 1.0));
    if (abs(local_y - 0.5) < (1.0 / max(u_cell_h, 1.0))) {
      color = vec4(v_dec.rgb, 1.0);
    }
  }
  outColor = color;
}`;

/** Grow (or replace) an R8 atlas buffer, copying prior stamps when width is unchanged. */
export function growAtlasR8(
  prev: Uint8Array | null,
  prevW: number,
  prevH: number,
  nextW: number,
  nextH: number,
): Uint8Array {
  const next = new Uint8Array(Math.max(0, nextW * nextH));
  if (prev && prevW === nextW && prevW > 0) {
    next.set(prev.subarray(0, Math.min(prev.length, next.length)));
  }
  return next;
}

export function isGpu2Frame(raw: Uint8Array): boolean {
  if (raw.byteLength < 4) return false;
  const view = new DataView(raw.buffer, raw.byteOffset, raw.byteLength);
  return view.getUint32(0, true) === MAGIC_GPU2;
}

/** @deprecated use isGpu2Frame */
export function isGpuFrame(raw: Uint8Array): boolean {
  return isGpu2Frame(raw);
}

export function decodeGpu2Frame(raw: Uint8Array): DecodedGpuFrame | null {
  if (!isGpu2Frame(raw) || raw.byteLength < 24) return null;
  const view = new DataView(raw.buffer, raw.byteOffset, raw.byteLength);
  let o = 4;
  const cols = view.getUint16(o, true);
  o += 2;
  const rows = view.getUint16(o, true);
  o += 2;
  const cellW = view.getUint32(o, true);
  o += 4;
  const cellH = view.getUint32(o, true);
  o += 4;
  const spritesPerLayer = view.getUint16(o, true);
  o += 2;
  const layerCount = view.getUint16(o, true);
  o += 2;
  const spriteN = view.getUint32(o, true);
  o += 4;
  const stamp = cellW * (cellH + 1);
  const sprites: GpuSprite[] = [];
  for (let i = 0; i < spriteN; i++) {
    if (o + 6 > raw.byteLength) return null;
    const spriteIdx = view.getUint16(o, true);
    o += 2;
    const spriteLayer = view.getUint16(o, true);
    o += 2;
    const hasBits = view.getUint8(o);
    o += 2;
    let bits: Uint8Array | null = null;
    if (hasBits) {
      if (o + stamp > raw.byteLength) return null;
      bits = raw.subarray(o, o + stamp);
      o += stamp;
    }
    sprites.push({ spriteIdx, spriteLayer, bits });
  }
  const cellCount = cols * rows;
  const cellBytes = cellCount * GPU_CELL_BYTES;
  if (o + cellBytes > raw.byteLength) return null;
  const cells = raw.subarray(o, o + cellBytes);
  return { cols, rows, cellW, cellH, spritesPerLayer, layerCount, sprites, cells };
}

/** @deprecated use decodeGpu2Frame */
export function decodeGpuFrame(raw: Uint8Array): DecodedGpuFrame | null {
  return decodeGpu2Frame(raw);
}

function compile(gl: WebGL2RenderingContext, type: number, src: string): WebGLShader {
  const sh = gl.createShader(type)!;
  gl.shaderSource(sh, src);
  gl.compileShader(sh);
  if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) {
    const info = gl.getShaderInfoLog(sh) || "shader error";
    gl.deleteShader(sh);
    throw new Error(info);
  }
  return sh;
}

export class TermGlPainter {
  private gl: WebGL2RenderingContext;
  private prog: WebGLProgram;
  private vao: WebGLVertexArrayObject;
  private cellBuf: WebGLBuffer;
  private atlasTex: WebGLTexture;
  private uRes: WebGLUniformLocation;
  private uCellSize: WebGLUniformLocation;
  private uSpritesPerLayer: WebGLUniformLocation;
  private uCellH: WebGLUniformLocation;
  private layers = 0;
  private spritesPerLayer = 64;
  private cellW = 1;
  private cellH = 1;

  constructor(canvas: HTMLCanvasElement) {
    const gl = canvas.getContext("webgl2", {
      alpha: false,
      antialias: false,
      preserveDrawingBuffer: true,
    });
    if (!gl) throw new Error("webgl2 unavailable");
    this.gl = gl;
    const vs = compile(gl, gl.VERTEX_SHADER, CELL_VS_SOURCE);
    const fs = compile(gl, gl.FRAGMENT_SHADER, CELL_FS_SOURCE);
    const prog = gl.createProgram()!;
    gl.attachShader(prog, vs);
    gl.attachShader(prog, fs);
    gl.linkProgram(prog);
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
      throw new Error(gl.getProgramInfoLog(prog) || "link error");
    }
    this.prog = prog;
    this.uRes = gl.getUniformLocation(prog, "u_res")!;
    this.uCellSize = gl.getUniformLocation(prog, "u_cell_size")!;
    this.uSpritesPerLayer = gl.getUniformLocation(prog, "u_sprites_per_layer")!;
    this.uCellH = gl.getUniformLocation(prog, "u_cell_h")!;
    this.vao = gl.createVertexArray()!;
    this.cellBuf = gl.createBuffer()!;
    this.atlasTex = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.atlasTex);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D_ARRAY, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
  }

  private ensureAtlas(frame: DecodedGpuFrame) {
    const gl = this.gl;
    const layers = Math.max(1, frame.layerCount);
    const spl = Math.max(1, frame.spritesPerLayer);
    const cw = Math.max(1, frame.cellW);
    const ch = Math.max(1, frame.cellH);
    const stampH = ch + 1;
    const needRebuild =
      !this.layers ||
      this.layers < layers ||
      this.spritesPerLayer !== spl ||
      this.cellW !== cw ||
      this.cellH !== ch;
    if (needRebuild) {
      this.layers = layers;
      this.spritesPerLayer = spl;
      this.cellW = cw;
      this.cellH = ch;
      gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.atlasTex);
      gl.texImage3D(
        gl.TEXTURE_2D_ARRAY,
        0,
        gl.R8,
        spl * cw,
        stampH,
        layers,
        0,
        gl.RED,
        gl.UNSIGNED_BYTE,
        null,
      );
    }
  }

  paint(frame: DecodedGpuFrame, canvas: HTMLCanvasElement, dpr: number) {
    const gl = this.gl;
    const width = frame.cols * frame.cellW;
    const height = frame.rows * frame.cellH;
    canvas.style.width = `${width / dpr}px`;
    canvas.style.height = `${height / dpr}px`;
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = width;
      canvas.height = height;
    }
    gl.viewport(0, 0, width, height);
    this.ensureAtlas(frame);
    gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.atlasTex);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    const stampH = frame.cellH + 1;
    for (const s of frame.sprites) {
      if (!s.bits || s.bits.byteLength < frame.cellW * stampH) continue;
      if (s.spriteLayer >= this.layers) continue;
      const x = s.spriteIdx * frame.cellW;
      gl.texSubImage3D(
        gl.TEXTURE_2D_ARRAY,
        0,
        x,
        0,
        s.spriteLayer,
        frame.cellW,
        stampH,
        1,
        gl.RED,
        gl.UNSIGNED_BYTE,
        s.bits,
      );
    }

    gl.useProgram(this.prog);
    gl.uniform2f(this.uRes, width, height);
    gl.uniform2f(this.uCellSize, frame.cellW, frame.cellH);
    gl.uniform1f(this.uSpritesPerLayer, frame.spritesPerLayer);
    gl.uniform1f(this.uCellH, frame.cellH);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D_ARRAY, this.atlasTex);
    gl.uniform1i(gl.getUniformLocation(this.prog, "u_atlas"), 0);

    gl.bindVertexArray(this.vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.cellBuf);
    gl.bufferData(gl.ARRAY_BUFFER, frame.cells, gl.DYNAMIC_DRAW);
    const stride = GPU_CELL_BYTES;
    // Expand packed cells into attribute-friendly layout via IUI/integer attrs.
    // Layout: fg u32, bg u32, dec u32, sprite_idx u16, sprite_layer u16, attrs u32
    const bindUint = (loc: number, size: number, offset: number) => {
      gl.enableVertexAttribArray(loc);
      gl.vertexAttribIPointer(loc, size, gl.UNSIGNED_INT, stride, offset);
      gl.vertexAttribDivisor(loc, 1);
    };
    bindUint(0, 1, 0);
    bindUint(1, 1, 4);
    bindUint(2, 1, 8);
    // sprite_idx + sprite_layer as one uint
    bindUint(3, 1, 12);
    bindUint(4, 1, 16);

    gl.disable(gl.BLEND);
    gl.clearColor(0, 0, 0, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.drawArraysInstanced(gl.TRIANGLE_FAN, 0, 4, frame.cols * frame.rows);
  }

  dispose() {
    const gl = this.gl;
    gl.deleteBuffer(this.cellBuf);
    gl.deleteVertexArray(this.vao);
    gl.deleteTexture(this.atlasTex);
    gl.deleteProgram(this.prog);
  }
}

export function tryCreateTermGl(canvas: HTMLCanvasElement): TermGlPainter | null {
  try {
    return new TermGlPainter(canvas);
  } catch {
    return null;
  }
}
