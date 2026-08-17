// SPDX-License-Identifier: AGPL-3.0-or-later
// Gated benchmark: set PHASE4_ADJ_BENCH=1 to run. Measures TS
// applyBasicAdjustmentToPixels vs WASM apply_basic_adjustment_wasm on a large
// image. Loads the built wasm via bytes (no fetch) so it runs in the node
// runner; honest: only reports when the real wasm pkg is loadable.
import { describe, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { applyBasicAdjustmentToPixelsTs } from "../layerAdjustments";

function makeBuffer(w: number, h: number): Uint8ClampedArray {
  const data = new Uint8ClampedArray(w * h * 4);
  for (let i = 0; i < w * h; i++) {
    const o = i * 4;
    data[o] = (i * 7) % 255;
    data[o + 1] = (i * 13) % 255;
    data[o + 2] = (i * 29) % 255;
    data[o + 3] = 255;
  }
  return data;
}

async function loadWasm(): Promise<any | null> {
  try {
    const wasmPath = resolve(process.cwd(), "src/wasm/pkg/photrez_core_bg.wasm");
    const bytes = readFileSync(wasmPath);
    const mod = await import("@/wasm/pkg/photrez_core");
    if (typeof mod.default === "function") await mod.default(bytes);
    return mod;
  } catch (e) {
    console.log("[phase4-adj bench] wasm load failed:", String(e));
    return null;
  }
}

function bench(fn: () => void, runs: number): number {
  for (let i = 0; i < 3; i++) fn();
  const t0 = performance.now();
  for (let i = 0; i < runs; i++) fn();
  return (performance.now() - t0) / runs;
}

describe("Phase 4 adjustmentBake benchmark (gated PHASE4_ADJ_BENCH=1)", () => {
  it.skipIf(!process.env.PHASE4_ADJ_BENCH)(
    "wasm apply_basic_adjustment_wasm vs TS applyBasicAdjustmentToPixels",
    async () => {
      const w = 1500;
      const h = 1500;
      const runs = 20;
      const adj = { brightness: 12, contrast: -30, saturation: 45 };
      const wasm = await loadWasm();
      if (!wasm || typeof wasm.apply_basic_adjustment_wasm !== "function") {
        console.log("[phase4-adj bench] wasm pkg not loadable in runner — SKIPPED (run via built Tauri bundle to measure)");
        return;
      }
      const tsMs = bench(() => {
        applyBasicAdjustmentToPixelsTs(makeBuffer(w, h), adj);
      }, runs);
      const wasmMs = bench(() => {
        wasm.apply_basic_adjustment_wasm(new Uint8Array(makeBuffer(w, h)), adj.brightness, adj.contrast, adj.saturation);
      }, runs);
      console.log(
        `[phase4-adj bench] TS=${tsMs.toFixed(2)}ms  WASM=${wasmMs.toFixed(2)}ms  speedup=${(tsMs / wasmMs).toFixed(2)}x`,
      );
      if (wasmMs > tsMs * 2) {
        console.warn("[phase4-adj bench] WARNING: wasm >2x slower than TS — investigate before keeping");
      }
    },
    60000,
  );
});
