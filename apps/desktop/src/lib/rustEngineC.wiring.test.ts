// Wiring test for the Technique C Rust-Engine slice.
// Proves the Rust Engine actually reaches the GL upload / pixel-bake path with
// correctly-adjusted bytes. The wasm pkg loads in headless vitest (proven by
// kernelWasm.wiring) — the guard lives in beforeAll and FAILS LOUD if the
// module is unavailable, instead of silently skipping.
import { describe, it, expect, beforeAll } from "vitest";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import { renderLayerPixelsRust, renderLayerC } from "@/lib/rustEngineC";
import { applyBasicAdjustmentToPixels, type BasicAdjustment } from "@/engine/layerAdjustments";

let wasmMod: any = null;

beforeAll(async () => {
  wasmMod = await getWasmExportModule();
  expect(wasmMod).not.toBeNull();
});

const adj: BasicAdjustment = { brightness: 20, contrast: 0, saturation: 0 };

function makeGlMock() {
  const calls: unknown[][] = [];
  const gl: Record<string, unknown> = {
    RGBA: 0x1908,
    UNSIGNED_BYTE: 0x1401,
    texImage2D: (...args: unknown[]) => {
      calls.push(args);
    },
  };
  return { gl: gl as unknown as WebGL2RenderingContext, calls };
}

describe("Rust Engine C-slice wiring", () => {
  it("renderLayerPixelsRust produces pixels matching the production TS/WASM math", async () => {
    expect(wasmMod).not.toBeNull();
    const px = new Uint8Array([100, 100, 100, 255, 50, 50, 50, 255]);
    const out = await renderLayerPixelsRust(2, 1, px, adj);
    const expected = applyBasicAdjustmentToPixels(new Uint8ClampedArray(px), adj);
    expect(Array.from(out)).toEqual(Array.from(expected));
    expect(out.length).toBe(8);
  });

  it("renderLayerC uploads the Rust-adjusted pixels to WebGL2 (view, not a retained copy)", async () => {
    expect(wasmMod).not.toBeNull();
    const px = new Uint8Array([100, 100, 100, 255, 50, 50, 50, 255]);
    const { gl, calls } = makeGlMock();
    await renderLayerC(2, 1, px, adj, gl, 0);
    expect(calls.length).toBe(1);
    const args = calls[0];
    // texImage2D(target, level, internalformat, width, height, border, format, type, pixels)
    expect(args[0]).toBe(0); // target
    expect(args[3]).toBe(2); // width
    expect(args[4]).toBe(1); // height
    const uploaded = args[8] as Uint8Array;
    expect(uploaded).toBeInstanceOf(Uint8Array);
    const expected = applyBasicAdjustmentToPixels(new Uint8ClampedArray(px), adj);
    expect(Array.from(uploaded)).toEqual(Array.from(expected));
  });
});
