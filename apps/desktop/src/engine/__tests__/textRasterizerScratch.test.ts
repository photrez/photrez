// Scratch-canvas reuse contract for the text rasterizer.
// Same seam as textRasterizer.test.ts: the OffscreenCanvas global is stubbed
// so the REAL rasterizeText body runs, but this mock also counts allocations
// (constructor calls), clearRect calls, and setTransform calls so the tests
// below can prove reuse actually happens. Runs in unit-node (node env).
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { rasterizeText } from "../textRasterizer";
import { DEFAULT_TEXT_DATA } from "../textTypes";

/** Reads the scaled font size out of a CSS font string (e.g. "96px \"Arial\""). */
function fontPxFrom(font: string): number {
  const m = /(\d+(?:\.\d+)?)px/.exec(font);
  return m ? Number(m[1]) : 0;
}

interface PaintCall {
  text: string;
  x: number;
  y: number;
}

interface ScratchRecord {
  width: number;
  height: number;
  paints: PaintCall[];
  strokes: PaintCall[];
  clears: number;
  setTransforms: number[][];
  ctx: Record<string, unknown>;
}

function setupCountingMock(): ScratchRecord[] {
  const instances: ScratchRecord[] = [];
  const MockOffscreenCanvas = function (this: unknown, w: number, h: number) {
    const self = this as {
      width: number;
      height: number;
      getContext: () => unknown;
      transferToImageBitmap: () => unknown;
    };
    self.width = w;
    self.height = h;
    const record = { width: w, height: h } as ScratchRecord;
    const ctx: Record<string, unknown> = {
      font: "",
      fillStyle: "",
      textBaseline: "alphabetic",
      letterSpacing: undefined as string | undefined,
      strokeStyle: "",
      lineWidth: 0,
      lineJoin: "miter",
      miterLimit: 10,
      globalCompositeOperation: "source-over",
      globalAlpha: 1,
      measureText: (s: string) => ({
        width: s.length * (fontPxFrom(ctx.font as string) * 0.5),
        fontBoundingBoxAscent: 80,
        fontBoundingBoxDescent: 24,
      }),
      fillText: (t: string, x: number, y: number) => {
        record.paints.push({ text: t, x, y });
      },
      strokeText: (t: string, x: number, y: number) => {
        record.strokes.push({ text: t, x, y });
      },
      clearRect: () => {
        record.clears += 1;
      },
      setTransform: (...args: number[]) => {
        record.setTransforms.push(args);
      },
      beginPath: () => {},
      moveTo: () => {},
      lineTo: () => {},
      stroke: () => {},
    };
    record.paints = [];
    record.strokes = [];
    record.clears = 0;
    record.setTransforms = [];
    record.ctx = ctx;
    instances.push(record);
    self.getContext = () => ctx;
    // Mirror real OffscreenCanvas: the transferred bitmap has the CURRENT
    // canvas size (rasterizeText resizes after construction).
    self.transferToImageBitmap = () => ({ width: self.width, height: self.height });
  } as unknown as typeof OffscreenCanvas;
  vi.stubGlobal("OffscreenCanvas", MockOffscreenCanvas);
  return instances;
}

function allPaints(instances: ScratchRecord[]): PaintCall[] {
  return instances.flatMap((r) => r.paints);
}

describe("textRasterizer scratch reuse", () => {
  let instances: ScratchRecord[];
  beforeEach(() => {
    instances = setupCountingMock();
  });
  afterEach(() => vi.unstubAllGlobals());

  it("same input twice paints identical op streams (byte-identical contract)", () => {
    const data = { ...DEFAULT_TEXT_DATA, content: "AB", color: "#123456" };
    const mark1 = allPaints(instances).length;
    const r1 = rasterizeText(data);
    const ops1 = allPaints(instances).slice(mark1);
    const b1 = r1.imageBitmap as unknown as { width: number; height: number };
    const mark2 = allPaints(instances).length;
    const r2 = rasterizeText(data);
    const ops2 = allPaints(instances).slice(mark2);
    const b2 = r2.imageBitmap as unknown as { width: number; height: number };
    expect(ops1.length).toBeGreaterThan(0);
    expect(ops2).toEqual(ops1);
    expect({ width: b2.width, height: b2.height }).toEqual({
      width: b1.width,
      height: b1.height,
    });
    expect(r2.width).toBe(r1.width);
    expect(r2.height).toBe(r1.height);
  });

  it("two same-size rasters allocate exactly one canvas (grow-only scratch)", () => {
    const data = { ...DEFAULT_TEXT_DATA, content: "AB" };
    rasterizeText(data);
    rasterizeText(data);
    expect(instances.length).toBe(1);
  });

  it("second raster does not inherit the first raster's draw state", () => {
    // Same text + same stroke width => identical canvas dims, so the second
    // call reuses the backing store without a resize (the path where stale
    // context state would leak into the next raster).
    const a = {
      ...DEFAULT_TEXT_DATA,
      content: "AB",
      color: "#ff0000",
      stroke: { width: 4, color: "#00ff00", align: "outside" as const },
    };
    const b = {
      ...DEFAULT_TEXT_DATA,
      content: "AB",
      color: "#0000ff",
      stroke: { width: 4, color: "#ffff00", align: "outside" as const },
    };
    rasterizeText(a);
    rasterizeText(b);
    const rec = instances[instances.length - 1];
    expect(rec.ctx["fillStyle"]).toBe("#0000ff");
    expect(rec.ctx["strokeStyle"]).toBe("#ffff00");
    // The reuse mechanism itself: identity transform restored, backing store
    // cleared once per raster (stale pixels would ghost into the next frame
    // on the HTMLCanvasElement fallback path, where transfer never detaches).
    expect(rec.setTransforms).toContainEqual([1, 0, 0, 1, 0, 0]);
    expect(rec.clears).toBeGreaterThanOrEqual(2);
  });
});
