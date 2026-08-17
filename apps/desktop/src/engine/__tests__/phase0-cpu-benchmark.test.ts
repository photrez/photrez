// Phase 0 baseline profiling — additive, measurement-only. No production changes.
// Times the genuine TS CPU-pixel kernels to decide which qualify for a Rust/WASM
// port (plan gate: >5-10ms on a realistic document). Runs under unit-node (no DOM):
// floodFill/gradientFill only read .data/.width/.height, so a plain buffer object works.

import { describe, it, expect } from "vitest";
import { writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve } from "node:path";
import { floodFill, gradientFill } from "@/features/fill/fillOperations";
import { applyBasicAdjustmentToPixels } from "@/engine/layerAdjustments";

const OUT = resolve(tmpdir(), "phase0-benchmark-result.json");

// Phase 0 benchmark is heavy (50s+). Keep it OUT of the default suite; run
// explicitly with PHASE0_BENCH=1. When run, allow 120s per test (slow jsdom runner).
const benchIt = process.env.PHASE0_BENCH
  ? (name: string, fn: () => void) => it(name, fn, 120000)
  : it.skip;

type Size = { w: number; h: number };
const SIZES: Size[] = [
  { w: 1920, h: 1080 },
  { w: 4000, h: 3000 },
];

function makeBuffer(w: number, h: number, fill: [number, number, number, number]): Uint8ClampedArray {
  const buf = new Uint8ClampedArray(w * h * 4);
  for (let i = 0; i < buf.length; i += 4) {
    buf[i] = fill[0];
    buf[i + 1] = fill[1];
    buf[i + 2] = fill[2];
    buf[i + 3] = fill[3];
  }
  return buf;
}

function timeIt(fn: () => void, warmups = 3, runs = 20): { avg: number; min: number } {
  for (let i = 0; i < warmups; i++) fn();
  let min = Infinity;
  let sum = 0;
  for (let i = 0; i < runs; i++) {
    const t0 = performance.now();
    fn();
    const dt = performance.now() - t0;
    min = Math.min(min, dt);
    sum += dt;
  }
  return { avg: sum / runs, min };
}

const results: Record<string, Record<string, string>> = {};

describe("Phase 0 — TS CPU-pixel kernel baseline (ms)", () => {
  for (const { w, h } of SIZES) {
    const px = (w * h / 1_000_000).toFixed(2);

    benchIt(`floodFill contiguous full-fill @ ${w}x${h} (${px}M px)`, () => {
      // Regenerate a fresh uniform buffer each run: floodFill early-returns once
      // the image already equals the fill colour, which would otherwise hide cost.
      const { avg, min } = timeIt(() => {
        const buf = makeBuffer(w, h, [200, 200, 200, 255]);
        const img = { data: buf, width: w, height: h } as unknown as ImageData;
        floodFill(img, 0, 0, 255, 0, 0, 255, 0, null, true);
      });
      results[`${w}x${h}`] ??= {};
      results[`${w}x${h}`].floodFill = `${avg.toFixed(2)}ms (min ${min.toFixed(2)})`;
      expect(avg).toBeGreaterThan(0);
    });

    benchIt(`gradientFill linear @ ${w}x${h} (${px}M px)`, () => {
      const buf = makeBuffer(w, h, [0, 0, 0, 255]);
      const img = { data: buf, width: w, height: h } as unknown as ImageData;
      const stops = [
        { offset: 0, r: 0, g: 0, b: 0, a: 255 },
        { offset: 1, r: 255, g: 255, b: 255, a: 255 },
      ];
      const { avg, min } = timeIt(() =>
        gradientFill(img, "linear", 0, 0, w, h, stops as any, null),
      );
      results[`${w}x${h}`].gradientFill = `${avg.toFixed(2)}ms (min ${min.toFixed(2)})`;
      expect(avg).toBeGreaterThan(0);
    });

    benchIt(`applyBasicAdjustmentToPixels @ ${w}x${h} (${px}M px)`, () => {
      const buf = makeBuffer(w, h, [128, 128, 128, 255]);
      const adj = { brightness: 10, contrast: 20, saturation: 30 };
      const { avg, min } = timeIt(() => applyBasicAdjustmentToPixels(buf, adj));
      results[`${w}x${h}`].adjustmentBake = `${avg.toFixed(2)}ms (min ${min.toFixed(2)})`;
      expect(avg).toBeGreaterThan(0);
    });
  }

    benchIt("writes ranked summary to file", () => {
      writeFileSync(OUT, JSON.stringify(results, null, 2));
      expect(true).toBe(true);
    });
});
