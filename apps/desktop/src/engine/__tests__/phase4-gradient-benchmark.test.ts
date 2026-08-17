// SPDX-License-Identifier: AGPL-3.0-or-later
// Gated benchmark: set PHASE4_BENCH=1 to run. Measures TS gradientFill vs WASM
// gradient_fill_wasm on a large image. Loads the built wasm via bytes (no
// fetch) so it runs in the node runner; honest: only reports when loadable.
import { describe, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { gradientFillTs } from "@/features/fill/fillOperations";

function makeImage(w: number, h: number): ImageData {
  const data = new Uint8ClampedArray(w * h * 4);
  for (let i = 0; i < w * h; i++) {
    const o = i * 4;
    data[o + 3] = 255;
  }
  return { data, width: w, height: h } as unknown as ImageData;
}

async function loadWasm(): Promise<any | null> {
  try {
    const wasmPath = resolve(process.cwd(), "src/wasm/pkg/photrez_core_bg.wasm");
    const bytes = readFileSync(wasmPath);
    const mod = await import("@/wasm/pkg/photrez_core");
    if (typeof mod.default === "function") await mod.default(bytes);
    return mod;
  } catch (e) {
    console.log("[phase4 bench] wasm load failed:", String(e));
    return null;
  }
}

function bench(fn: () => void, runs: number): number {
  for (let i = 0; i < 3; i++) fn();
  const t0 = performance.now();
  for (let i = 0; i < runs; i++) fn();
  return (performance.now() - t0) / runs;
}

describe("Phase 4 gradientFill benchmark (gated PHASE4_BENCH=1)", () => {
  it.skipIf(!process.env.PHASE4_BENCH)(
    "wasm gradient_fill_wasm vs TS gradientFill",
    async () => {
      const w = 1024;
      const h = 1024;
      const runs = 20;
      const stops = [
        { offset: 0, r: 0, g: 0, b: 0, a: 255 },
        { offset: 1, r: 255, g: 255, b: 255, a: 255 },
      ];
      const wasm = await loadWasm();
      if (!wasm || typeof wasm.gradient_fill_wasm !== "function") {
        console.log("[phase4 bench] wasm pkg not loadable in runner — SKIPPED (run via built Tauri bundle to measure)");
        return;
      }
      const tsMs = bench(() => {
        gradientFillTs(makeImage(w, h), "linear", 0, 0, w - 1, 0, stops, null);
      }, runs);
      const wasmMs = bench(() => {
        const c = makeImage(w, h);
        const offs = new Float64Array(stops.map((s) => s.offset));
        const cols = new Uint8Array(stops.length * 4);
        stops.forEach((s, i) => {
          cols[i * 4] = s.r;
          cols[i * 4 + 1] = s.g;
          cols[i * 4 + 2] = s.b;
          cols[i * 4 + 3] = s.a;
        });
        wasm.gradient_fill_wasm(new Uint8Array(c.data), w, h, 0, 0, 0, w - 1, 0, offs, cols, false, 0, 0, 0, 0, 0, false);
      }, runs);
      console.log(
        `[phase4 bench] TS=${tsMs.toFixed(2)}ms  WASM=${wasmMs.toFixed(2)}ms  speedup=${(tsMs / wasmMs).toFixed(2)}x`,
      );
      if (wasmMs > tsMs * 2) {
        console.warn("[phase4 bench] WARNING: wasm >2x slower than TS — investigate before keeping");
      }
    },
    60000,
  );
});
