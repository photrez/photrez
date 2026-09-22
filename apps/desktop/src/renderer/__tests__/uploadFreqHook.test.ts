// Hook test: every WebGL2Backend.uploadImage call records exactly one
// DEV-only upload event, with the kind matching the actual GPU path taken
// (patch only when texSubImage2D ran, full otherwise). The live reupload
// gesture counts these events per undo rep.
import { afterEach, describe, expect, it, vi } from "vitest";
import { WebGL2Backend } from "../webgl2";
import { resetUploadFreq, type UploadFreqEvent } from "@/lib/perf/uploadFreqDev";

function makeGLMock() {
  const calls: Array<{ method: string; args: unknown[] }> = [];
  let handleSeq = 0;
  const constants: Record<string, number> = {
    TEXTURE_2D: 3553,
    RGBA: 6408,
    UNSIGNED_BYTE: 5121,
    TEXTURE_MIN_FILTER: 10241,
    LINEAR_MIPMAP_LINEAR: 9987,
    LINEAR: 9729,
    NEAREST: 9728,
    TEXTURE_MAG_FILTER: 10240,
    TEXTURE_WRAP_S: 10242,
    CLAMP_TO_EDGE: 33071,
    TEXTURE_WRAP_T: 10243,
    MAX_TEXTURE_SIZE: 3379,
    UNPACK_PREMULTIPLY_ALPHA_WEBGL: 37400,
  };
  const handler: ProxyHandler<Record<string, unknown>> = {
    get(target, prop) {
      if (typeof prop !== "string") return undefined;
      if (prop in target) return target[prop];
      return (...args: unknown[]) => {
        calls.push({ method: prop, args });
        switch (prop) {
          case "createShader":
          case "createProgram":
          case "createVertexArray":
          case "createFramebuffer":
          case "createTexture":
            return { __mock: prop, id: ++handleSeq };
          case "getShaderParameter":
          case "getProgramParameter":
            return true;
          case "getShaderInfoLog":
          case "getProgramInfoLog":
            return "";
          case "getUniformLocation":
            return { name: args[1] };
          case "getParameter":
            return 4096;
          case "isContextLost":
            return false;
        }
        return undefined;
      };
    },
  };
  return { gl: new Proxy(constants, handler), calls };
}

function makeCanvas(gl: unknown): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  (canvas as unknown as { getContext: () => unknown }).getContext = () => gl;
  return canvas;
}

function stub2DContext() {
  const ctx2d = { drawImage: vi.fn() };
  const orig = HTMLCanvasElement.prototype.getContext;
  HTMLCanvasElement.prototype.getContext = function (this: HTMLCanvasElement, type: string, ...rest: unknown[]) {
    if (type === "2d") return ctx2d as unknown;
    return (orig as (...a: unknown[]) => unknown).call(this, type, ...rest);
  } as typeof HTMLCanvasElement.prototype.getContext;
  return () => {
    HTMLCanvasElement.prototype.getContext = orig;
  };
}

function uploadEvents(): UploadFreqEvent[] {
  const holder = window as unknown as { __uploadFreq?: UploadFreqEvent[] };
  return Array.isArray(holder.__uploadFreq) ? holder.__uploadFreq : [];
}

const BITMAP = { width: 800, height: 600, close: () => {} } as unknown as ImageBitmap;
const OTHER = { width: 800, height: 600, close: () => {} } as unknown as ImageBitmap;

describe("WebGL2Backend.uploadImage DEV upload recorder hook", () => {
  let restoreCtx: () => void;
  afterEach(() => restoreCtx?.());

  it("a trivial full upload produces exactly 1 full event with pixel bytes", () => {
    restoreCtx = stub2DContext();
    resetUploadFreq();
    const mock = makeGLMock();
    const renderer = new WebGL2Backend();
    renderer.initialize(makeCanvas(mock.gl));

    renderer.uploadImage("a", BITMAP);

    const list = uploadEvents();
    expect(list).toHaveLength(1);
    expect(list[0].layerId).toBe("a");
    expect(list[0].kind).toBe("full");
    expect(list[0].bytes).toBe(800 * 600 * 4);
  });

  it("a patch upload records kind patch with rect bytes", () => {
    restoreCtx = stub2DContext();
    resetUploadFreq();
    const mock = makeGLMock();
    const renderer = new WebGL2Backend();
    renderer.initialize(makeCanvas(mock.gl));

    renderer.uploadImage("a", BITMAP);
    resetUploadFreq();
    renderer.uploadImage("a", OTHER, { x: 10, y: 20, width: 30, height: 40 });

    const list = uploadEvents();
    expect(list).toHaveLength(1);
    expect(list[0].kind).toBe("patch");
    expect(list[0].bytes).toBe(30 * 40 * 4);
  });

  it("dirtyRect with no existing texture falls through to exactly 1 full event", () => {
    restoreCtx = stub2DContext();
    resetUploadFreq();
    const mock = makeGLMock();
    const renderer = new WebGL2Backend();
    renderer.initialize(makeCanvas(mock.gl));

    renderer.uploadImage("a", BITMAP, { x: 10, y: 20, width: 30, height: 40 });

    const list = uploadEvents();
    expect(list).toHaveLength(1);
    expect(list[0].kind).toBe("full");
    expect(list[0].bytes).toBe(800 * 600 * 4);
  });
});
