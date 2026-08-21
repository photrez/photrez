// Wiring test for the Technique C Rust-Engine slice.
// Proves the Rust Engine actually reaches the GL upload / pixel-bake path with
// correctly-adjusted bytes. Requires the wasm pkg to be fetchable (a serving
// environment such as `bun run tauri dev` or a CI with a vite server). In
// headless vitest the wasm cannot be fetched, so the suite is skipped — the engine
// correctness itself is covered by `cargo test -p photrez-core` (18 tests).
import { describe, it, expect } from "vitest";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import { renderLayerPixelsRust, renderLayerC } from "@/lib/rustEngineC";
import { applyBasicAdjustmentToPixels, type BasicAdjustment } from "@/engine/layerAdjustments";

const wasmMod = await getWasmExportModule();
const engineSuite = wasmMod ? describe : describe.skip;

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

engineSuite("Rust Engine C-slice wiring", () => {
  it("renderLayerPixelsRust produces pixels matching the production TS/WASM math", async () => {
    const px = new Uint8Array([100, 100, 100, 255, 50, 50, 50, 255]);
    const out = await renderLayerPixelsRust(2, 1, px, adj);
    const expected = applyBasicAdjustmentToPixels(new Uint8ClampedArray(px), adj);
    expect(Array.from(out)).toEqual(Array.from(expected));
    expect(out.length).toBe(8);
  });

  it("renderLayerC uploads the Rust-adjusted pixels to WebGL2 (view, not a retained copy)", async () => {
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
