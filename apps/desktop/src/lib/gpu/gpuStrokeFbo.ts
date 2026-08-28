// SPDX-License-Identifier: AGPL-3.0-or-later
// Fase 3 package 2 PoC: GPU-resident stroke layer (WebGL2, isolated context).
//
// Instanced-quad dab stamping into a persistent layer-sized FBO. Each dab is
// one quad sampling the SAME brush tip mask the CPU path uses (R8 texture,
// linear filter) so falloff values match the Canvas2D path. Accumulation uses
// premultiplied alpha-over (blendFunc ONE, ONE_MINUS_SRC_ALPHA) - correct
// progressive build-up for stacked low-flow dabs.
//
// readRect() returns raw GL bytes (PREMULTIPLIED RGBA). Use
// premultToUnpremult() before feeding Canvas2D-shaped consumers (tiles store
// unpremultiplied ImageData bytes; Canvas2D drawImage does this conversion
// internally on the CPU path).
//
// PoC scope: brush tool only (normal blend), constant alpha per batch
// (matches today's per-flush alphaScale). Eraser/replace semantics deferred.
// Own OffscreenCanvas + context: zero touch on the production renderer;
// context loss => create() returns null next use, CPU path stays truth.

export interface GpuStrokeFboDabBatch {
  /** Flat [x0,y0,x1,y1,...] in layer-local pixels (f64 from the producer). */
  dabs: Float64Array;
  count: number;
  diameter: number;
  /** Constant per-batch ink color, 0..1 linear-ish sRGB channels. */
  r: number;
  g: number;
  b: number;
  /** Per-dab alpha multiplier (today's opacity*flow product). */
  alpha: number;
}

const VERT = `#version 300 es
layout(location=0) in vec2 aCorner;      // unit quad, -0.5..0.5
layout(location=1) in vec2 aCenterPx;    // instance: dab center in pixels
uniform vec2 uLayerSize;
uniform float uDiameter;
out vec2 vUv;
void main() {
  vec2 worldPx = aCenterPx + aCorner * uDiameter;
  // GL clip space with y flip (layer y grows downward)
  vec2 clip = vec2(worldPx.x / uLayerSize.x * 2.0 - 1.0, 1.0 - worldPx.y / uLayerSize.y * 2.0);
  gl_Position = vec4(clip, 0.0, 1.0);
  vUv = aCorner + 0.5;
}`;

const FRAG = `#version 300 es
precision mediump float;
in vec2 vUv;
uniform sampler2D uTip;
uniform vec3 uColor;
uniform float uAlpha;
out vec4 outColor;
void main() {
  float a = texture(uTip, vUv).r * uAlpha;
  outColor = vec4(uColor * a, a); // premultiplied
}`;

function compile(gl: WebGL2RenderingContext, type: number, src: string): WebGLShader | null {
  const sh = gl.createShader(type);
  if (!sh) return null;
  gl.shaderSource(sh, src);
  gl.compileShader(sh);
  if (!gl.getShaderParameter(sh, gl.COMPILE_STATUS)) {
    console.error("[gpuStrokeFbo] shader compile failed:", gl.getShaderInfoLog(sh));
    gl.deleteShader(sh);
    return null;
  }
  return sh;
}

/** Convert premultiplied RGBA bytes to unpremultiplied in place (a=0 guarded). */
export function premultToUnpremult(bytes: Uint8Array): Uint8Array {
  for (let i = 0; i < bytes.length; i += 4) {
    const a = bytes[i + 3];
    if (a === 255 || a === 0) continue;
    // round-to-nearest like browsers: v = round(p * 255 / a)
    bytes[i] = Math.min(255, Math.round((bytes[i] * 255) / a));
    bytes[i + 1] = Math.min(255, Math.round((bytes[i + 1] * 255) / a));
    bytes[i + 2] = Math.min(255, Math.round((bytes[i + 2] * 255) / a));
  }
  return bytes;
}

/** Flip rows top<->bottom (GL readPixels origin is bottom-left). */
export function flipRowsY(bytes: Uint8Array, w: number, h: number): Uint8Array {
  const row = w * 4;
  const tmp = new Uint8Array(row);
  for (let y = 0; y < h >> 1; y++) {
    const top = y * row;
    const bot = (h - 1 - y) * row;
    tmp.set(bytes.subarray(top, top + row));
    bytes.copyWithin(top, bot, bot + row);
    bytes.set(tmp, bot);
  }
  return bytes;
}

export class GpuStrokeFbo {
  private canvas: OffscreenCanvas;
  private gl: WebGL2RenderingContext;
  private prog: WebGLProgram;
  private quadBuf: WebGLBuffer;
  private instBuf: WebGLBuffer;
  private tipTex: WebGLTexture;
  private fboTex: WebGLTexture;
  private fbo: WebGLFramebuffer;
  readonly layerW: number;
  readonly layerH: number;
  private lost = false;

  private constructor(canvas: OffscreenCanvas, gl: WebGL2RenderingContext, layerW: number, layerH: number) {
    this.canvas = canvas;
    this.gl = gl;
    this.layerW = layerW;
    this.layerH = layerH;

    const vs = compile(gl, gl.VERTEX_SHADER, VERT)!;
    const fs = compile(gl, gl.FRAGMENT_SHADER, FRAG)!;
    const prog = gl.createProgram()!;
    gl.attachShader(prog, vs);
    gl.attachShader(prog, fs);
    gl.linkProgram(prog);
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) {
      throw new Error(`[gpuStrokeFbo] link failed: ${gl.getProgramInfoLog(prog)}`);
    }
    gl.deleteShader(vs);
    gl.deleteShader(fs);
    this.prog = prog;

    this.quadBuf = gl.createBuffer()!;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.quadBuf);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-0.5, -0.5, 0.5, -0.5, -0.5, 0.5, 0.5, 0.5]), gl.STATIC_DRAW);

    this.instBuf = gl.createBuffer()!;

    this.tipTex = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, this.tipTex);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.LINEAR);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);

    this.fboTex = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, this.fboTex);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA8, layerW, layerH, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);

    this.fbo = gl.createFramebuffer()!;
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.fbo);
    gl.framebufferTexture2D(gl.FRAMEBUFFER, gl.COLOR_ATTACHMENT0, gl.TEXTURE_2D, this.fboTex, 0);
    if (gl.checkFramebufferStatus(gl.FRAMEBUFFER) !== gl.FRAMEBUFFER_COMPLETE) {
      throw new Error("[gpuStrokeFbo] framebuffer incomplete");
    }
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);

    const onLost = (e: Event) => {
      e.preventDefault();
      this.lost = true;
    };
    canvas.addEventListener("webglcontextlost", onLost as EventListener);
  }

  static create(layerW: number, layerH: number): GpuStrokeFbo | null {
    try {
      const canvas =
        typeof OffscreenCanvas !== "undefined"
          ? new OffscreenCanvas(layerW, layerH)
          : Object.assign(document.createElement("canvas"), { width: layerW, height: layerH });
      const gl = canvas.getContext("webgl2", {
        premultipliedAlpha: true,
        antialias: false,
        depth: false,
        stencil: false,
        powerPreference: "high-performance",
        preserveDrawingBuffer: true,
      }) as WebGL2RenderingContext | null;
      if (!gl) return null;
      return new GpuStrokeFbo(canvas as OffscreenCanvas, gl, layerW, layerH);
    } catch (err) {
      console.warn("[gpuStrokeFbo] create failed:", err);
      return null;
    }
  }

  /** Test/dev injection point: build around a provided canvas+context. */
  static fromContext(
    canvas: { width: number; height: number; addEventListener(t: string, l: EventListener): void },
    gl: WebGL2RenderingContext,
  ): GpuStrokeFbo {
    return new GpuStrokeFbo(canvas as OffscreenCanvas, gl, canvas.width, canvas.height);
  }

  isLost(): boolean {
    return this.lost;
  }

  /** Upload the shared tip mask (Float32 alpha, square dataSize x dataSize). */
  setTipMask(data: Float32Array, dataSize: number): void {
    const gl = this.gl;
    const bytes = new Uint8Array(dataSize * dataSize);
    for (let i = 0; i < bytes.length; i++) bytes[i] = Math.round(Math.max(0, Math.min(1, data[i])) * 255);
    gl.bindTexture(gl.TEXTURE_2D, this.tipTex);
    gl.pixelStorei(gl.UNPACK_ALIGNMENT, 1);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.R8, dataSize, dataSize, 0, gl.RED, gl.UNSIGNED_BYTE, bytes);
  }

  clear(): void {
    const gl = this.gl;
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.fbo);
    gl.clearColor(0, 0, 0, 0);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  /** Stamp one batch of dabs in a single instanced draw call. */
  stampBatch(batch: GpuStrokeFboDabBatch): void {
    if (this.lost || batch.count <= 0) return;
    const gl = this.gl;
    const centers = new Float32Array(batch.count * 2);
    for (let i = 0; i < batch.count; i++) {
      centers[i * 2] = batch.dabs[i * 2];
      centers[i * 2 + 1] = batch.dabs[i * 2 + 1];
    }

    gl.bindFramebuffer(gl.FRAMEBUFFER, this.fbo);
    gl.viewport(0, 0, this.layerW, this.layerH);
    gl.useProgram(this.prog);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA); // premultiplied source-over

    gl.bindBuffer(gl.ARRAY_BUFFER, this.quadBuf);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 2, gl.FLOAT, false, 0, 0);
    gl.vertexAttribDivisor(0, 0);

    gl.bindBuffer(gl.ARRAY_BUFFER, this.instBuf);
    gl.bufferData(gl.ARRAY_BUFFER, centers, gl.STREAM_DRAW);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribPointer(1, 2, gl.FLOAT, false, 0, 0);
    gl.vertexAttribDivisor(1, 1);

    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, this.tipTex);
    gl.uniform1i(gl.getUniformLocation(this.prog, "uTip")!, 0);
    gl.uniform3f(gl.getUniformLocation(this.prog, "uColor")!, batch.r, batch.g, batch.b);
    gl.uniform1f(gl.getUniformLocation(this.prog, "uAlpha")!, batch.alpha);
    gl.uniform1f(gl.getUniformLocation(this.prog, "uDiameter")!, batch.diameter);
    gl.uniform2f(gl.getUniformLocation(this.prog, "uLayerSize")!, this.layerW, this.layerH);

    gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, batch.count);
    gl.vertexAttribDivisor(1, 0);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
  }

  /**
   * Read back a dirty rect into PREMULTIPLIED RGBA bytes, rows flipped to
   * top-down. Clamped to layer bounds. Returns the passed-in buffer when
   * provided and large enough, else a fresh one.
   */
  readRect(x: number, y: number, w: number, h: number, into?: Uint8Array): Uint8Array | null {
    if (this.lost) return null;
    const x0 = Math.max(0, Math.floor(x));
    const y0 = Math.max(0, Math.floor(y));
    const w0 = Math.min(this.layerW - x0, Math.ceil(w));
    const h0 = Math.min(this.layerH - y0, Math.ceil(h));
    if (w0 <= 0 || h0 <= 0) return null;
    const gl = this.gl;
    const need = w0 * h0 * 4;
    let out = into && into.length >= need ? into.subarray(0, need) : new Uint8Array(need);
    gl.bindFramebuffer(gl.FRAMEBUFFER, this.fbo);
    gl.pixelStorei(gl.PACK_ALIGNMENT, 1);
    // GL reads bottom-up; request from the flipped row then flip in JS.
    gl.readPixels(x0, this.layerH - y0 - h0, w0, h0, gl.RGBA, gl.UNSIGNED_BYTE, out);
    gl.bindFramebuffer(gl.FRAMEBUFFER, null);
    flipRowsY(out, w0, h0);
    return out;
  }

  dispose(): void {
    const gl = this.gl;
    gl.deleteProgram(this.prog);
    gl.deleteBuffer(this.quadBuf);
    gl.deleteBuffer(this.instBuf);
    gl.deleteTexture(this.tipTex);
    gl.deleteTexture(this.fboTex);
    gl.deleteFramebuffer(this.fbo);
  }
}
