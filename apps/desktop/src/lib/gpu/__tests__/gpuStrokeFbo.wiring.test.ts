// Wiring tests for the Fase 3 GPU stroke-FBO PoC (mocked WebGL2 context).
// Proves call-order contract: premultiplied blend setup, instance buffer
// layout (f64 producer view -> f32 centers), readPixels y-flip handling, and
// the premult->unpremult conversion tiles will consume. jsdom has no real GL;
// the REAL-GPU numbers come from window.__benchGpuStrokeFbo in tauri dev.
import { describe, it, expect } from "vitest";
import {
  GpuStrokeFbo,
  premultToUnpremult,
  flipRowsY,
} from "@/lib/gpu/gpuStrokeFbo";

const C = {
  VERTEX_SHADER: 0x8b31,
  FRAGMENT_SHADER: 0x8b32,
  COMPILE_STATUS: 0x8b81,
  LINK_STATUS: 0x8b82,
  ARRAY_BUFFER: 0x8892,
  STATIC_DRAW: 0x88e4,
  STREAM_DRAW: 0x88e0,
  TEXTURE_2D: 0x0de1,
  RGBA8: 0x8058,
  RGBA: 0x1908,
  R8: 0x8229,
  RED: 0x1903,
  UNSIGNED_BYTE: 0x1401,
  FLOAT: 0x1406,
  FRAMEBUFFER: 0x8d40,
  COLOR_ATTACHMENT0: 0x8ce0,
  FRAMEBUFFER_COMPLETE: 0x8cd5,
  COLOR_BUFFER_BIT: 0x4000,
  BLEND: 0x0be2,
  ONE: 1,
  ONE_MINUS_SRC_ALPHA: 0x0303,
  TRIANGLE_STRIP: 0x0005,
  TEXTURE0: 0x84c0,
  LINEAR: 0x2601,
  NEAREST: 0x2600,
  CLAMP_TO_EDGE: 0x812f,
  PACK_ALIGNMENT: 0x0d05,
  UNPACK_ALIGNMENT: 0x0cf5,
};

type Call = [string, ...unknown[]];

function makeMockGl() {
  const calls: Call[] = [];
  const shaderCounter = { n: 0 };
  const fake = new Map<string, unknown>();
  const idFactory = (() => {
    let n = 1;
    return () => n++;
  })();
  const gl: any = {};
  for (const [k, v] of Object.entries(C)) gl[k] = v;

  const record =
    (name: string, ret?: unknown) =>
    (...args: unknown[]) => {
      calls.push([name, ...args]);
      return ret;
    };

  Object.assign(gl, {
    createShader: () => ++shaderCounter.n,
    shaderSource: record("shaderSource"),
    compileShader: record("compileShader"),
    getShaderParameter: () => true,
    getShaderInfoLog: () => "",
    deleteShader: record("deleteShader"),
    createProgram: () => 99,
    attachShader: record("attachShader"),
    linkProgram: record("linkProgram"),
    getProgramParameter: (_p: unknown, pname: number) => pname === C.LINK_STATUS,
    getProgramInfoLog: () => "",
    useProgram: record("useProgram"),
    createBuffer: () => idFactory(),
    bindBuffer: record("bindBuffer"),
    bufferData: record("bufferData"),
    createTexture: () => idFactory(),
    bindTexture: record("bindTexture"),
    texParameteri: record("texParameteri"),
    texImage2D: record("texImage2D"),
    pixelStorei: record("pixelStorei"),
    activeTexture: record("activeTexture"),
    createFramebuffer: () => 77,
    bindFramebuffer: record("bindFramebuffer"),
    framebufferTexture2D: record("framebufferTexture2D"),
    checkFramebufferStatus: () => C.FRAMEBUFFER_COMPLETE,
    clearColor: record("clearColor"),
    clear: record("clear"),
    enable: record("enable"),
    blendFunc: record("blendFunc"),
    enableVertexAttribArray: record("enableVertexAttribArray"),
    vertexAttribPointer: record("vertexAttribPointer"),
    vertexAttribDivisor: record("vertexAttribDivisor"),
    drawArraysInstanced: record("drawArraysInstanced"),
    viewport: record("viewport"),
    getUniformLocation: (_p: unknown, name: string) => `loc:${name}`,
    uniform1i: record("uniform1i"),
    uniform1f: record("uniform1f"),
    uniform2f: record("uniform2f"),
    uniform3f: record("uniform3f"),
    readPixels: (
      x: number,
      y: number,
      w: number,
      h: number,
      fmt: number,
      type: number,
      out: Uint8Array,
    ) => {
      calls.push(["readPixels", x, y, w, h, fmt, type]);
      // Fill with a row-indexed gradient so flip verification is observable.
      for (let yy = 0; yy < h; yy++)
        for (let xx = 0; xx < w; xx++) {
          const o = (yy * w + xx) * 4;
          out[o] = yy; // GL row index (bottom-up)
          out[o + 1] = 0;
          out[o + 2] = 0;
          out[o + 3] = 255;
        }
    },
    deleteProgram: record("deleteProgram"),
    deleteBuffer: record("deleteBuffer"),
    deleteTexture: record("deleteTexture"),
    deleteFramebuffer: record("deleteFramebuffer"),
  });
  void fake;
  return { gl: gl as WebGL2RenderingContext, calls };
}

function makeCanvas(w: number, h: number) {
  const listeners: Array<[string, EventListener]> = [];
  return {
    width: w,
    height: h,
    addEventListener(t: string, l: EventListener) {
      listeners.push([t, l]);
    },
    __listeners: listeners,
  };
}

function makeFbo(w = 64, h = 64) {
  const { gl, calls } = makeMockGl();
  const canvas = makeCanvas(w, h);
  const fbo = GpuStrokeFbo.fromContext(canvas as any, gl);
  return { fbo, gl, calls, canvas };
}

describe("GpuStrokeFbo wiring (mocked GL)", () => {
  it("stamps one instanced draw per batch with f32 centers and premult blend", () => {
    const { fbo, calls } = makeFbo();
    const dabs = new Float64Array([10.5, 20.25, 30.125, 40.0625]);
    fbo.setTipMask(new Float32Array(16).fill(1), 4);
    fbo.stampBatch({ dabs, count: 2, diameter: 100, r: 1, g: 0.5, b: 0, alpha: 0.7 });

    const blend = calls.find((c) => c[0] === "blendFunc")!;
    expect(blend[1]).toBe(C.ONE);
    expect(blend[2]).toBe(C.ONE_MINUS_SRC_ALPHA);

    const data = calls.filter((c) => c[0] === "bufferData").map((c) => c[2] as Float32Array);
    const centers = data.find((d) => d.length === 4)!;
    expect(Array.from(centers)).toEqual([
      expect.closeTo(10.5),
      expect.closeTo(20.25),
      expect.closeTo(30.125),
      expect.closeTo(40.0625),
    ]);

    const draw = calls.find((c) => c[0] === "drawArraysInstanced")!;
    // drawArraysInstanced(mode, first, countPerInstance, instanceCount)
    expect(draw[1]).toBe(C.TRIANGLE_STRIP);
    expect(draw[2]).toBe(0);
    expect(draw[3]).toBe(4);
    expect(draw[4]).toBe(2);

    const alpha = calls.find((c) => c[0] === "uniform1f" && c[1] === "loc:uAlpha");
    expect(alpha![2]).toBeCloseTo(0.7);
  });

  it("skips empty batches without touching GL state", () => {
    const { fbo, calls } = makeFbo();
    fbo.stampBatch({ dabs: new Float64Array(0), count: 0, diameter: 10, r: 0, g: 0, b: 0, alpha: 1 });
    expect(calls.find((c) => c[0] === "drawArraysInstanced")).toBeUndefined();
  });

  it("readRect flips GL bottom-up rows to top-down and clamps bounds", () => {
    const { fbo, calls } = makeFbo(8, 4); // layerH=4 so row indices are small
    const out = fbo.readRect(0, 0, 8, 4)!;
    const rp = calls.find((c) => c[0] === "readPixels")!;
    // requested from flipped row: layerH - y - h = 4 - 0 - 4 = 0
    expect(rp[2]).toBe(0);
    // after JS flip: output top row must be GL's LAST row (index 3)
    expect(out[0]).toBe(3);
    // bottom output row = GL row 0
    expect(out[(3 * 8) * 4]).toBe(0);
  });

  it("marks lost on webglcontextlost and refuses further reads", () => {
    const { fbo, canvas } = makeFbo();
    expect(fbo.isLost()).toBe(false);
    const ev = { preventDefault: () => {} } as unknown as Event;
    for (const [, l] of (canvas as any).__listeners) l(ev);
    expect(fbo.isLost()).toBe(true);
    expect(fbo.readRect(0, 0, 4, 4)).toBeNull();
  });
});

describe("premult conversions", () => {
  it("premultToUnpremult round-trips known values and guards a=0/255", () => {
    const bytes = new Uint8Array([255, 255, 255, 255, 127, 0, 0, 128, 9, 9, 9, 0]);
    premultToUnpremult(bytes);
    expect(Array.from(bytes.subarray(0, 4))).toEqual([255, 255, 255, 255]); // a=255 passthrough
    expect(Math.abs(bytes[4] - Math.round((127 * 255) / 128))).toBeLessThanOrEqual(1); // un-premult pixel 2 red
    expect(Array.from(bytes.slice(8))).toEqual([9, 9, 9, 0]); // a=0 untouched
  });

  it("flipRowsY is an involution", () => {
    const src = new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
    const flipped = flipRowsY(new Uint8Array(src), 1, 3);
    expect(Array.from(flipped)).toEqual([9, 10, 11, 12, 5, 6, 7, 8, 1, 2, 3, 4]);
    const twice = flipRowsY(new Uint8Array(flipped), 1, 3);
    expect(Array.from(twice)).toEqual(Array.from(src));
  });
});
