// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock the compiled wasm package's dynamic import.
let floodFillImpl:
  | ((
      buf: Uint8Array,
      width: number,
      height: number,
      sx: number,
      sy: number,
      fr: number,
      fg: number,
      fb: number,
      fa: number,
      tolerance: number,
      hasMask: boolean,
      mx: number,
      my: number,
      mw: number,
      mh: number,
      shape: number,
      inv: boolean,
      contiguous: boolean,
    ) => Uint8Array)
  | undefined;
let gradientFillImpl:
  | ((
      buf: Uint8Array,
      width: number,
      height: number,
      gradType: number,
      ax: number,
      ay: number,
      bx: number,
      by: number,
      stopOffsets: Float64Array,
      stopColors: Uint8Array,
      hasMask: boolean,
      mx: number,
      my: number,
      mw: number,
      mh: number,
      shape: number,
      inv: boolean,
    ) => Uint8Array)
  | undefined;
let adjustmentsImpl:
  | ((buf: Uint8Array, brightness: number, contrast: number, saturation: number) => Uint8Array)
  | undefined;

vi.mock("@/wasm/pkg/photrez_core", () => ({
  default: vi.fn(async () => {}),
  // Mock kernel: fills the clicked pixel to the fill colour (wiring test only;
  // real algorithm correctness is covered by Rust unit tests + fillOperations.test.ts).
  get flood_fill_wasm() {
    return floodFillImpl;
  },
  // Mock gradient kernel: paints every pixel with the first stop colour (wiring
  // test only; real gradient math is covered by Rust unit tests + fillOperations.test.ts).
  get gradient_fill_wasm() {
    return gradientFillImpl;
  },
  // Mock adjustment kernel: paints every RGB pixel 200 (wiring test only;
  // real math is covered by Rust unit tests + layerAdjustments.test.ts).
  get apply_basic_adjustment_wasm() {
    return adjustmentsImpl;
  },
  set_panic_hook: vi.fn(),
}));

import { getWasmExportModule } from "../wasmExport";
import { floodFill, gradientFill } from "@/features/fill/fillOperations";
import { applyBasicAdjustmentToPixels } from "@/engine/layerAdjustments";

function makeImage(w: number, h: number, fill: [number, number, number, number] = [0, 0, 0, 255]) {
  const data = new Uint8ClampedArray(w * h * 4);
  for (let i = 0; i < w * h; i++) {
    const o = i * 4;
    data[o] = fill[0];
    data[o + 1] = fill[1];
    data[o + 2] = fill[2];
    data[o + 3] = fill[3];
  }
  return { data, width: w, height: h } as unknown as ImageData;
}

describe("kernel WASM wrapper wiring", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    floodFillImpl = vi.fn((buf: Uint8Array, width: number, height: number, sx: number, sy: number, fr: number, fg: number, fb: number, fa: number) => {
      const out = new Uint8Array(buf);
      const idx = (sy * width + sx) * 4;
      out[idx] = fr;
      out[idx + 1] = fg;
      out[idx + 2] = fb;
      out[idx + 3] = fa;
      return out;
    });
    gradientFillImpl = vi.fn(
      (
        buf: Uint8Array,
        _width: number,
        _height: number,
        _gradType: number,
        _ax: number,
        _ay: number,
        _bx: number,
        _by: number,
        _stopOffsets: Float64Array,
        stopColors: Uint8Array,
      ) => {
        const out = new Uint8Array(buf);
        const r = stopColors[0];
        const g = stopColors[1];
        const b = stopColors[2];
        const a = stopColors[3];
        for (let i = 0; i < out.length; i += 4) {
          out[i] = r;
          out[i + 1] = g;
          out[i + 2] = b;
          out[i + 3] = a;
        }
        return out;
      },
    );
    adjustmentsImpl = vi.fn((buf: Uint8Array) => {
      const out = new Uint8Array(buf);
      for (let i = 0; i < out.length; i += 4) {
        out[i] = 200;
        out[i + 1] = 200;
        out[i + 2] = 200;
      }
      return out;
    });
  });

  it("floodFill dispatches to wasm flood_fill_wasm and writes the result back", async () => {
    await getWasmExportModule(); // populate the cached module
    const img = makeImage(4, 4);
    floodFill(img, 0, 0, 255, 0, 0, 255, 0, null, true);
    expect(floodFillImpl).toHaveBeenCalled();
    // clicked pixel (0,0) was filled by the mock kernel; result written back in place
    expect(Array.from(img.data.slice(0, 4))).toEqual([255, 0, 0, 255]);
  });

  it("floodFill falls back to the TS impl when the wasm kernel is absent", () => {
    floodFillImpl = undefined; // simulate kernel missing
    const img = makeImage(4, 4);
    floodFill(img, 0, 0, 255, 0, 0, 255, 0, null, true);
    // uniform image fully filled by the TS fallback (all pixels -> fill colour)
    expect(Array.from(img.data.slice(0, 4))).toEqual([255, 0, 0, 255]);
    expect(Array.from(img.data.slice(img.data.length - 4))).toEqual([255, 0, 0, 255]);
  });

  it("gradientFill dispatches to wasm gradient_fill_wasm and writes back", async () => {
    await getWasmExportModule();
    const img = makeImage(4, 4);
    gradientFill(
      img,
      "linear",
      0,
      0,
      3,
      0,
      [
        { offset: 0, r: 10, g: 20, b: 30, a: 255 },
        { offset: 1, r: 200, g: 100, b: 50, a: 255 },
      ],
      null,
    );
    expect(gradientFillImpl).toHaveBeenCalled();
    // mock kernel paints every pixel with the first stop colour
    expect(Array.from(img.data.slice(0, 4))).toEqual([10, 20, 30, 255]);
  });

  it("gradientFill falls back to the TS impl when the wasm kernel is absent", () => {
    gradientFillImpl = undefined; // simulate kernel missing
    const img = makeImage(4, 4);
    gradientFill(
      img,
      "linear",
      0,
      0,
      3,
      0,
      [
        { offset: 0, r: 10, g: 20, b: 30, a: 255 },
        { offset: 1, r: 200, g: 100, b: 50, a: 255 },
      ],
      null,
    );
    // TS impl applies the start stop colour at t=0 (not the uniform mock colour)
    expect(img.data[0]).toBe(10);
  });

  it("applyBasicAdjustmentToPixels dispatches to wasm and writes back (alpha preserved)", async () => {
    await getWasmExportModule();
    const px = new Uint8ClampedArray([10, 20, 30, 255, 40, 50, 60, 128]);
    const out = applyBasicAdjustmentToPixels(px, { brightness: 50, contrast: 0, saturation: 0 });
    expect(adjustmentsImpl).toHaveBeenCalled();
    expect(Array.from(out.slice(0, 4))).toEqual([200, 200, 200, 255]);
    expect(out[7]).toBe(128);
  });

  it("applyBasicAdjustmentToPixels falls back to TS when wasm absent", () => {
    adjustmentsImpl = undefined;
    const px = new Uint8ClampedArray([10, 20, 30, 255]);
    const out = applyBasicAdjustmentToPixels(px, { brightness: 0, contrast: 0, saturation: 0 });
    // identity adjustment -> unchanged (proves TS fallback path)
    expect(Array.from(out.slice(0, 4))).toEqual([10, 20, 30, 255]);
  });
});
