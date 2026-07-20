import { afterEach, describe, expect, it, vi } from "vitest";
import { WebGL2Backend } from "../webgl2";

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
    NEAREST: 9728,
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
  };

  const handler: ProxyHandler<Record<string, any>> = {
    get(target, prop) {
      if (typeof prop !== "string") return undefined;
      if (prop in target) return target[prop];
      return (...args: any[]) => {
        const cleanArgs = args.map((a) =>
          a instanceof Float32Array ? Array.from(a) : a,
        );
        calls.push({ method: prop, args: cleanArgs });
        switch (prop) {
          case "createShader":
          case "createProgram":
          case "createTexture":
          case "createFramebuffer":
          case "createVertexArray":
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

  const gl = new Proxy(constants, handler);
  return { gl, calls };
}

function makeCanvas(gl: any): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  (canvas as any).getContext = () => gl;
  return canvas;
}

// Fake 2D context so the dirty-rect fast path's drawImage no-ops instead of
// hitting a real CanvasImageSource validation error in the test env.
function stub2DContext() {
  const ctx2d = { drawImage: vi.fn() };
  const orig = HTMLCanvasElement.prototype.getContext;
  HTMLCanvasElement.prototype.getContext = function (this: HTMLCanvasElement, type: string, ...rest: any[]) {
    if (type === "2d") return ctx2d as any;
    return (orig as any).call(this, type, ...rest);
  } as any;
  return () => {
    HTMLCanvasElement.prototype.getContext = orig;
  };
}

const BITMAP = { width: 800, height: 600, close: () => {} } as ImageBitmap;
const OTHER = { width: 800, height: 600, close: () => {} } as ImageBitmap;

describe("WebGL2Backend.uploadImage — dirty-rect fast path", () => {
  let restoreCtx: () => void;
  afterEach(() => restoreCtx?.());

  it("patches only the sub-rect via texSubImage2D when an existing texture is dirtied", () => {
    restoreCtx = stub2DContext();
    const mock = makeGLMock();
    const canvas = makeCanvas(mock.gl);
    const renderer = new WebGL2Backend();
    renderer.initialize(canvas);

    renderer.uploadImage("a", BITMAP); // full upload
    mock.calls.length = 0; // clear so we only observe the second call

    renderer.uploadImage("a", OTHER, { x: 10, y: 20, width: 30, height: 40 });

    const sub = mock.calls.filter((c) => c.method === "texSubImage2D");
    const img = mock.calls.filter((c) => c.method === "texImage2D");

    expect(img).toHaveLength(0);
    expect(sub).toHaveLength(1);
    // texSubImage2D(target, level, xoffset, yoffset, format, type, source)
    expect(sub[0].args[1]).toBe(0);
    expect(sub[0].args[2]).toBe(10);
    expect(sub[0].args[3]).toBe(20);
  });

  it("falls back to a full texImage2D upload when no texture exists yet", () => {
    restoreCtx = stub2DContext();
    const mock = makeGLMock();
    const canvas = makeCanvas(mock.gl);
    const renderer = new WebGL2Backend();
    renderer.initialize(canvas);

    renderer.uploadImage("fresh", BITMAP, { x: 0, y: 0, width: 10, height: 10 });

    const sub = mock.calls.filter((c) => c.method === "texSubImage2D");
    const img = mock.calls.filter((c) => c.method === "texImage2D");

    expect(sub).toHaveLength(0);
    expect(img.length).toBeGreaterThan(0);
  });
});

describe("WebGL2Backend.render — MAG filter switches to NEAREST above 200% zoom", () => {
  let restoreCtx: () => void;
  afterEach(() => restoreCtx?.());

  function makeRenderState(zoom: number): any {
    return {
      documentId: "doc",
      viewport: { panX: 0, panY: 0, zoom, rotation: 0 },
      documentSize: { width: 800, height: 600 },
      layers: [],
      selection: null,
      checkerboard: false,
      backgroundColor: [0, 0, 0, 0],
    };
  }

  it("uses LINEAR at 100% zoom and NEAREST above 200% zoom", () => {
    restoreCtx = stub2DContext();
    const mock = makeGLMock();
    const canvas = makeCanvas(mock.gl);
    const renderer = new WebGL2Backend();
    renderer.initialize(canvas);

    renderer.uploadImage("a", BITMAP); // creates texture, default LINEAR
    mock.calls.length = 0;

    // Below threshold: no filter switch (upload already set LINEAR).
    renderer.render(makeRenderState(1));
    const nearestAt100 = mock.calls.filter(
      (c) => c.method === "texParameteri" && c.args[1] === 10240 && c.args[2] === 9728,
    );
    expect(nearestAt100).toHaveLength(0);

    mock.calls.length = 0;
    renderer.render(makeRenderState(2.5)); // above 200% threshold
    // NEAREST constant = 9728 (WebGL)
    const nearest = mock.calls.filter(
      (c) => c.method === "texParameteri" && c.args[1] === 10240 && c.args[2] === 9728,
    );
    expect(nearest).toHaveLength(1);

    // Switching back below threshold restores LINEAR.
    mock.calls.length = 0;
    renderer.render(makeRenderState(1));
    const restoreLinear = mock.calls.filter(
      (c) => c.method === "texParameteri" && c.args[1] === 10240 && c.args[2] === 9729,
    );
    expect(restoreLinear).toHaveLength(1);
  });
});
