/** WebGL2 painter for GPU1 atlas + cell frames from terminus-core. */

export type GpuGlyph = {
  id: number;
  x: number;
  y: number;
  w: number;
  h: number;
  ox: number;
  oy: number;
  bits: Uint8Array | null;
};

export type DecodedGpuFrame = {
  cols: number;
  rows: number;
  cellW: number;
  cellH: number;
  atlasW: number;
  atlasH: number;
  glyphs: GpuGlyph[];
  /** Interleaved glyphId, fg(rgba u32), bg(rgba u32) per cell. */
  cells: Uint32Array;
};

const MAGIC = 0x31455047; // 'GPU1' LE

export function isGpuFrame(raw: Uint8Array): boolean {
  if (raw.byteLength < 4) return false;
  const view = new DataView(raw.buffer, raw.byteOffset, raw.byteLength);
  return view.getUint32(0, true) === MAGIC;
}

export function decodeGpuFrame(raw: Uint8Array): DecodedGpuFrame | null {
  if (!isGpuFrame(raw)) return null;
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
  const atlasW = view.getUint32(o, true);
  o += 4;
  const atlasH = view.getUint32(o, true);
  o += 4;
  const glyphN = view.getUint32(o, true);
  o += 4;
  const glyphs: GpuGlyph[] = [];
  for (let i = 0; i < glyphN; i++) {
    const id = view.getUint16(o, true);
    o += 2;
    const x = view.getUint16(o, true);
    o += 2;
    const y = view.getUint16(o, true);
    o += 2;
    const w = view.getUint16(o, true);
    o += 2;
    const h = view.getUint16(o, true);
    o += 2;
    const ox = view.getInt16(o, true);
    o += 2;
    const oy = view.getInt16(o, true);
    o += 2;
    const hasBits = view.getUint8(o);
    o += 2; // has_bits + pad
    let bits: Uint8Array | null = null;
    if (hasBits) {
      const n = w * h;
      bits = raw.subarray(o, o + n);
      o += n;
    }
    glyphs.push({ id, x, y, w, h, ox, oy, bits });
  }
  const cellCount = cols * rows;
  const cells = new Uint32Array(cellCount * 3);
  for (let i = 0; i < cellCount; i++) {
    const glyphId = view.getUint16(o, true);
    o += 2;
    const fg = view.getUint32(o, true);
    o += 4;
    const bg = view.getUint32(o, true);
    o += 4;
    cells[i * 3] = glyphId;
    cells[i * 3 + 1] = fg;
    cells[i * 3 + 2] = bg;
  }
  return { cols, rows, cellW, cellH, atlasW, atlasH, glyphs, cells };
}

const VS = `#version 300 es
precision highp float;
layout(location=0) in vec2 a_pos;
layout(location=1) in vec2 a_uv;
layout(location=2) in vec4 a_fg;
layout(location=3) in vec4 a_bg;
uniform vec2 u_res;
out vec2 v_uv;
out vec4 v_fg;
out vec4 v_bg;
void main() {
  vec2 p = a_pos / u_res * 2.0 - 1.0;
  p.y = -p.y;
  gl_Position = vec4(p, 0.0, 1.0);
  v_uv = a_uv;
  v_fg = a_fg;
  v_bg = a_bg;
}`;

const FS = `#version 300 es
precision highp float;
in vec2 v_uv;
in vec4 v_fg;
in vec4 v_bg;
uniform sampler2D u_atlas;
out vec4 outColor;
void main() {
  float cover = texture(u_atlas, v_uv).r;
  outColor = mix(v_bg, vec4(v_fg.rgb, 1.0), cover);
}`;

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

function rgbaUnpack(u: number): [number, number, number, number] {
  return [(u & 255) / 255, ((u >> 8) & 255) / 255, ((u >> 16) & 255) / 255, ((u >> 24) & 255) / 255];
}

export class TermGlPainter {
  private gl: WebGL2RenderingContext;
  private prog: WebGLProgram;
  private vao: WebGLVertexArrayObject;
  private buf: WebGLBuffer;
  private atlasTex: WebGLTexture;
  private uRes: WebGLUniformLocation;
  private glyphMap = new Map<number, GpuGlyph>();
  private atlasW = 1024;
  private atlasH = 1024;
  private atlasReady = false;

  constructor(canvas: HTMLCanvasElement) {
    const gl = canvas.getContext("webgl2", {
      alpha: false,
      antialias: false,
      preserveDrawingBuffer: true,
    });
    if (!gl) throw new Error("webgl2 unavailable");
    this.gl = gl;
    const vs = compile(gl, gl.VERTEX_SHADER, VS);
    const fs = compile(gl, gl.FRAGMENT_SHADER, FS);
    const prog = gl.createProgram()!;
    gl.attachShader(prog, vs);
    gl.attachShader(prog, fs);
    gl.linkProgram(prog);
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
      throw new Error(gl.getProgramInfoLog(prog) || "link error");
    }
    this.prog = prog;
    this.uRes = gl.getUniformLocation(prog, "u_res")!;
    this.vao = gl.createVertexArray()!;
    this.buf = gl.createBuffer()!;
    this.atlasTex = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, this.atlasTex);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.R8, 1, 1, 0, gl.RED, gl.UNSIGNED_BYTE, new Uint8Array([0]));
  }

  private ensureAtlas(w: number, h: number) {
    if (this.atlasReady && this.atlasW === w && this.atlasH === h) return;
    const gl = this.gl;
    this.atlasW = Math.max(1, w);
    this.atlasH = Math.max(1, h);
    gl.bindTexture(gl.TEXTURE_2D, this.atlasTex);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    gl.texImage2D(
      gl.TEXTURE_2D,
      0,
      gl.R8,
      this.atlasW,
      this.atlasH,
      0,
      gl.RED,
      gl.UNSIGNED_BYTE,
      new Uint8Array(this.atlasW * this.atlasH),
    );
    this.atlasReady = true;
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

    this.ensureAtlas(frame.atlasW, frame.atlasH);
    gl.bindTexture(gl.TEXTURE_2D, this.atlasTex);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    for (const g of frame.glyphs) {
      this.glyphMap.set(g.id, g);
      if (g.bits && g.w > 0 && g.h > 0) {
        gl.texSubImage2D(
          gl.TEXTURE_2D,
          0,
          g.x,
          g.y,
          g.w,
          g.h,
          gl.RED,
          gl.UNSIGNED_BYTE,
          g.bits,
        );
      }
    }

    const floatsPerVert = 12;
    const verts = new Float32Array(frame.cols * frame.rows * 12 * floatsPerVert);
    let vi = 0;
    const aw = Math.max(1, this.atlasW);
    const ah = Math.max(1, this.atlasH);
    for (let row = 0; row < frame.rows; row++) {
      for (let col = 0; col < frame.cols; col++) {
        const i = row * frame.cols + col;
        const glyphId = frame.cells[i * 3];
        const fg = rgbaUnpack(frame.cells[i * 3 + 1]);
        const bg = rgbaUnpack(frame.cells[i * 3 + 2]);
        const x0 = col * frame.cellW;
        const y0 = row * frame.cellH;
        const x1 = x0 + frame.cellW;
        const y1 = y0 + frame.cellH;
        const g = glyphId ? this.glyphMap.get(glyphId) : undefined;
        const push = (
          px0: number,
          py0: number,
          px1: number,
          py1: number,
          uu0: number,
          vv0: number,
          uu1: number,
          vv1: number,
          useFg: boolean,
        ) => {
          const f = useFg ? fg : bg;
          const b = bg;
          const quad = [
            [px0, py0, uu0, vv0],
            [px1, py0, uu1, vv0],
            [px0, py1, uu0, vv1],
            [px0, py1, uu0, vv1],
            [px1, py0, uu1, vv0],
            [px1, py1, uu1, vv1],
          ];
          for (const [px, py, uu, vv] of quad) {
            verts[vi++] = px;
            verts[vi++] = py;
            verts[vi++] = uu;
            verts[vi++] = vv;
            verts[vi++] = f[0];
            verts[vi++] = f[1];
            verts[vi++] = f[2];
            verts[vi++] = f[3];
            verts[vi++] = b[0];
            verts[vi++] = b[1];
            verts[vi++] = b[2];
            verts[vi++] = b[3];
          }
        };
        push(x0, y0, x1, y1, 0, 0, 0, 0, false);
        if (g && g.w > 0 && g.h > 0) {
          const gx0 = x0 + g.ox;
          const gy0 = y0 + g.oy;
          const gx1 = gx0 + g.w;
          const gy1 = gy0 + g.h;
          const u0 = g.x / aw;
          const v0 = g.y / ah;
          const u1 = (g.x + g.w) / aw;
          const v1 = (g.y + g.h) / ah;
          push(gx0, gy0, gx1, gy1, u0, v0, u1, v1, true);
        }
      }
    }

    const used = verts.subarray(0, vi);
    gl.useProgram(this.prog);
    gl.uniform2f(this.uRes, width, height);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.atlasTex);
    gl.uniform1i(gl.getUniformLocation(this.prog, "u_atlas"), 0);
    gl.bindVertexArray(this.vao);
    gl.bindBuffer(gl.ARRAY_BUFFER, this.buf);
    gl.bufferData(gl.ARRAY_BUFFER, used, gl.DYNAMIC_DRAW);
    const stride = floatsPerVert * 4;
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, stride, 0);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribPointer(1, 2, gl.FLOAT, false, stride, 8);
    gl.enableVertexAttribArray(2);
    gl.vertexAttribPointer(2, 4, gl.FLOAT, false, stride, 16);
    gl.enableVertexAttribArray(3);
    gl.vertexAttribPointer(3, 4, gl.FLOAT, false, stride, 32);
    gl.disable(gl.BLEND);
    gl.clearColor(0, 0, 0, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.drawArrays(gl.TRIANGLES, 0, used.length / floatsPerVert);
  }

  dispose() {
    const gl = this.gl;
    gl.deleteBuffer(this.buf);
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
