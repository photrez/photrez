// SPDX-License-Identifier: AGPL-3.0-or-later
// Gated benchmark: set PHASE3_BENCH=1 to run. Measures TS floodFill vs WASM
// flood_fill_wasm on a large noisy image. Honest: only reports when the real
// wasm pkg is loadable in the runner; otherwise it SKIPS with a note (do not
// fake a speed claim). Run inside the built Tauri bundle / a wasm-capable
// runner to measure the real win.
import { describe, it } from "vitest";
import { floodFillTs } from "@/features/fill/fillOperations";

function makeNoisy(w: number, h: number): ImageData {
  const data = new Uint8ClampedArray(w * h * 4);
  let seed = 12345;
  for (let i = 0; i < w * h; i++) {
    seed = (seed * 1103515245 + 12345) & 0x7fffffff;
    const o = i * 4;
    data[o] = seed & 255;
    data[o + 1] = (seed >> 8) & 255;
    data[o + 2] = (seed >> 16) & 255;
    data[o + 3] = 255;
  }
  return { data, width: w, height: h } as unknown as ImageData;
}

async function loadWasm(): Promise<any | null> {
  try {
    const mod = await import("@/wasm/pkg/photrez_core");
    if (typeof mod.default === "function") await mod.default();
    return mod;
  } catch {
    return null;
  }
}

function bench(fn: () => void, runs: number): number {
  for (let i = 0; i < 3; i++) fn(); // warmup
  const t0 = performance.now();
  for (let i = 0; i < runs; i++) fn();
  return (performance.now() - t0) / runs;
}

describe("Phase 3 floodFill benchmark (gated PHASE3_BENCH=1)", () => {
  it.skipIf(!process.env.PHASE3_BENCH)(
    "wasm flood_fill_wasm vs TS floodFill",
    async () => {
      const w = 1024;
      const h = 1024;
      const runs = 20;
      const wasm = await loadWasm();
      if (!wasm || typeof wasm.flood_fill_wasm !== "function") {
        console.log("[phase3 bench] wasm pkg not loadable in runner — SKIPPED (run via built Tauri bundle to measure)");
        return;
      }
      const tsMs = bench(() => {
        floodFillTs(makeNoisy(w, h), 0, 0, 255, 0, 0, 255, 0, null, true);
      }, runs);
      const wasmMs = bench(() => {
        const c = makeNoisy(w, h);
        wasm.flood_fill_wasm(new Uint8Array(c.data), w, h, 0, 0, 255, 0, 0, 255, 0, false, 0, 0, 0, 0, 0, false, true);
      }, runs);
      console.log(
        `[phase3 bench] TS=${tsMs.toFixed(2)}ms  WASM=${wasmMs.toFixed(2)}ms  speedup=${(tsMs / wasmMs).toFixed(2)}x`,
      );
      // Honest gate: flag (don't hard-fail) if wasm is dramatically slower, so
      // the acceleration claim is never silently false.
      if (wasmMs > tsMs * 2) {
        console.warn("[phase3 bench] WARNING: wasm >2x slower than TS — investigate before keeping");
      }
    },
    60000,
  );
});
