import { beforeEach, describe, expect, it, vi } from "vitest";
import { WebGL2Backend } from "../webgl2";
import type { RenderBackend } from "../../renderer/types";
import type { BasicAdjustment } from "../../engine/layerAdjustments";

// ─── GL mock (mirrors webgl2-layer-copy.test.ts harness) ───
function makeGLMock() {
  const calls: Array<{ method: string; args: any[] }> = [];
  let handleSeq = 0;

  const constants: Record<string, number> = {
    VERTEX_SHADER: 35633,
    FRAGMENT_SHADER: 35632,
    COMPILE_STATUS: 35713,
    LINK_STATUS: 35714,
    MAX_TEXTURE_SIZE: 3379,
    BLEND: 3042,
    SRC_ALPHA: 770,
    ONE_MINUS_SRC_ALPHA: 771,
    TEXTURE_2D: 3553,
    RGBA: 6408,
    UNSIGNED_BYTE: 5121,
    TEXTURE_MIN_FILTER: 10241,
    LINEAR: 9729,
    TEXTURE_MAG_FILTER: 10240,
    TEXTURE_WRAP_S: 10242,
    CLAMP_TO_EDGE: 33071,
    TEXTURE_WRAP_T: 10243,
    TEXTURE0: 33984,
    TEXTURE1: 33985,
    FRAMEBUFFER: 36160,
    COLOR_ATTACHMENT0: 36064,
    COLOR_BUFFER_BIT: 16384,
    TRIANGLES: 4,
    SCISSOR_TEST: 3089,
    PIXEL_PACK_BUFFER: 35051,
    STREAM_READ: 35040,
  };

  const handler: ProxyHandler<Record<string, any>> = {
    get(target, prop) {
      if (typeof prop !== "string") return undefined;
      if (prop in target) return target[prop];
      return (...args: any[]) => {
        const cleanArgs = args.map((a) => (a instanceof Float32Array ? Array.from(a) : a));
        calls.push({ method: prop, args: cleanArgs });
        switch (prop) {
          case "createShader":
          case "createProgram":
          case "createTexture":
          case "createFramebuffer":
          case "createVertexArray":
          case "createBuffer":
            return { __mock: prop, id: ++handleSeq };
          case "getShaderParameter":
          case "getProgramParameter":
            return true;
          case "getParameter":
            return 4096;
          case "getShaderInfoLog":
          case "getProgramInfoLog":
            return "";
          case "getUniformLocation":
            return { name: args[1] };
        }
        return undefined;
      };
    },
  };

  return { gl: new Proxy(constants, handler) as any, calls };
}

function makeCanvas(gl: any): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  (canvas as any).getContext = () => gl;
  return canvas;
}

const BITMAP = { width: 8, height: 8, close: () => {} } as ImageBitmap;

describe("RenderBackend.bakeLayerToBitmap interface", () => {
  it("is part of the interface and returns an ImageBitmap or null", () => {
    const r = {
      uploadImage: vi.fn(),
      requestRender: vi.fn(),
      bakeLayerToBitmap: vi.fn(() => null),
    } as unknown as RenderBackend;
    expect(typeof r.bakeLayerToBitmap).toBe("function");
    const out = r.bakeLayerToBitmap("layer-1", 4, 4, {
      brightness: 10,
      contrast: 0,
      saturation: 0,
    } as BasicAdjustment);
    expect(out).toBeNull(); // mock returns null → CPU fallback path
  });
});

describe("WebGL2Backend.bakeLayerToBitmap", () => {
  let renderer: WebGL2Backend;
  let calls: Array<{ method: string; args: any[] }>;

  beforeEach(() => {
    const mock = makeGLMock();
    calls = mock.calls;
    const canvas = makeCanvas(mock.gl);
    renderer = new WebGL2Backend();
    renderer.initialize(canvas);
    renderer.uploadImage("l1", BITMAP);
  });

  it("renders the layer through the adjustment shader and reads pixels back", () => {
    const out = renderer.bakeLayerToBitmap("l1", 8, 8, {
      brightness: 20,
      contrast: 0,
      saturation: 0,
    });
    // In jsdom there is no real GL, so the readback may yield null — that's the
    // documented null contract. We assert the GPU pipeline actually ran.
    expect(out === null || typeof out === "object").toBe(true);
    const methods = calls.map((c) => c.method);
    expect(methods).toContain("useProgram");
    expect(methods).toContain("drawArrays");
    expect(methods).toContain("readPixels");
  });

  it("emits straight-alpha top-down output so no JS post-pass is needed", () => {
    renderer.bakeLayerToBitmap("l1", 8, 8, {
      brightness: 5,
      contrast: 0,
      saturation: 0,
    });
    const setUniform = (name: string) =>
      calls.find((c) => c.method === "uniform1i" && c.args[0]?.name === name)?.args?.[1];
    // flipTexY=1 flips the sample so the readback is already top-down,
    // u_outputStraight=1 un-premultiplies in-shader.
    expect(setUniform("u_flipTexY")).toBe(1);
    expect(setUniform("u_outputStraight")).toBe(1);
  });

  it("returns null when the layer has no uploaded texture", () => {
    expect(
      renderer.bakeLayerToBitmap("missing", 8, 8, { brightness: 10, contrast: 0, saturation: 0 }),
    ).toBeNull();
  });
});

describe("WebGL2Backend.bakeLayerToBitmapAsync (PBO readback)", () => {
  let renderer: WebGL2Backend;
  let calls: Array<{ method: string; args: any[] }>;

  beforeEach(() => {
    const mock = makeGLMock();
    calls = mock.calls;
    // Provide the async readback API like WebView2 / Chromium 110+.
    (mock.gl as any).getBufferSubDataAsync = vi.fn(async () => {});
    const canvas = makeCanvas(mock.gl);
    renderer = new WebGL2Backend();
    renderer.initialize(canvas);
    renderer.uploadImage("l1", BITMAP);
  });

  it("packs the readback into a PBO and resolves via getBufferSubDataAsync", async () => {
    const out = await renderer.bakeLayerToBitmapAsync("l1", 8, 8, {
      brightness: 20,
      contrast: 0,
      saturation: 0,
    });
    expect(out === null || typeof out === "object").toBe(true); // jsdom: no 2d ctx → null contract
    // The async API was called (mock is on the GL target, so it is not
    // recorded in `calls` — assert the mock directly).
    expect((renderer as any).gl.getBufferSubDataAsync).toHaveBeenCalled();
    const methods = calls.map((c) => c.method);
    expect(methods).toContain("readPixels");
    const readPixelsArgs = calls.find((c) => c.method === "readPixels")?.args;
    // Numeric offset (not an ArrayBufferView) → packed into the PBO, so the
    // GPU transfer is scheduled async instead of stalling the main thread.
    expect(readPixelsArgs?.[readPixelsArgs.length - 1]).toBe(0);
    // PBO + GL state cleaned up afterwards.
    expect(methods).toContain("deleteBuffer");
    expect(methods).toContain("deleteFramebuffer");
    expect(methods).toContain("deleteTexture");
  });

  it("falls back to the sync bake when getBufferSubDataAsync is unavailable", async () => {
    // Fresh renderer; the async API is present on the GL object but not
    // callable (undefined) → the sync bake must be used, rendering once.
    const mock = makeGLMock();
    (mock.gl as any).getBufferSubDataAsync = undefined;
    const canvas = makeCanvas(mock.gl);
    const syncRenderer = new WebGL2Backend();
    syncRenderer.initialize(canvas);
    syncRenderer.uploadImage("l1", BITMAP);

    const out = await syncRenderer.bakeLayerToBitmapAsync("l1", 8, 8, {
      brightness: 20,
      contrast: 0,
      saturation: 0,
    });
    expect(out === null || typeof out === "object").toBe(true);
    const methods = mock.calls.map((c) => c.method);
    expect(methods).toContain("readPixels");
    expect(methods.filter((m) => m === "readPixels").length).toBe(1); // sync path, single render
    expect(methods.filter((m) => m === "drawArrays").length).toBe(1);
  });
});
