// @ts-nocheck
// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Real-engine benchmark: C (Rust wasm Engine) vs TS (pure-TS) vs A (Rust wasm
// owning a WebGPU compute pipeline + readback).
//
// Mirrors the produced C/TS paths and the TRUE Technique A (WebGpuAdjustRenderer actually
// dispatches the compute shader and reads the adjusted pixels back to the CPU —
// no synthetic dispatch-only timing).
//
// RUN (needs a WebGPU runtime: Deno, a browser, or `bun run tauri dev`):
//   deno run --allow-read apps/desktop/src/lib/gpu/bench-real-engine.ts
//
// poa.rs must be built first: `bun run --filter photrez-desktop build:wasm`.
import init, { Engine, WebGpuAdjustRenderer, rgba_buffer_view, free_rgba_buffer } from "../../wasm/pkg/photrez_core.js";

await (init as any)();

interface Adj {
  brightness: number;
  contrast: number;
  saturation: number;
}

function makeBuffer(w: number, h: number): Uint8Array {
  const buf = new Uint8Array(w * h * 4);
  for (let i = 0; i < buf.length; i += 4) {
    buf[i] = (i * 13) & 0xff;
    buf[i + 1] = (i * 7) & 0xff;
    buf[i + 2] = (i * 29) & 0xff;
    buf[i + 3] = 255;
  }
  return buf;
}

async function timeIt(label: string, fn: () => void | Promise<void>, warmup: number, iters: number) {
  for (let i = 0; i < warmup; i++) await fn();
  const samples: number[] = [];
  for (let i = 0; i < iters; i++) {
    const t0 = performance.now();
    await fn();
    const t1 = performance.now();
    samples.push(t1 - t0);
  }
  samples.sort((a, b) => a - b);
  const median = samples[Math.floor(samples.length / 2)]!;
  const mean = samples.reduce((s, v) => s + v, 0) / samples.length;
  const min = samples[0]!;
  console.log(
    `  ${label.padEnd(6)} median=${median.toFixed(3)}ms mean=${mean.toFixed(3)}ms min=${min.toFixed(3)}ms (n=${iters})`,
  );
  return { median, mean, min };
}

async function main() {
  const w = Number(Deno.args[0] ?? 1280);
  const h = Number(Deno.args[1] ?? 720);
  const adj: Adj = { brightness: 12, contrast: 18, saturation: -10 };
  const warmup = 5;
  const iters = 30;
  const src = makeBuffer(w, h);
  const pixels = w * h;
  console.log(`\nBenchmark: ${w}x${h} = ${(pixels / 1e6).toFixed(2)} Mpx, adj=${JSON.stringify(adj)}\n`);

  // C — Rust wasm Engine (1:1 with renderLayerPixelsRust in rustEngineC.ts).
  // Each call allocates + frees the Engine, matching the production CPU-tier path.
  await timeIt("C", () => {
    const eng: any = new Engine(w, h);
    eng.brightness = adj.brightness;
    eng.contrast = adj.contrast;
    eng.saturation = adj.saturation;
    eng.set_layer_pixels(src);
    eng.render();
    const view = rgba_buffer_view(eng.buffer_id);
    new Uint8Array(view as any); // touch so the read isn't optimized away
    free_rgba_buffer(eng.buffer_id);
  }, warmup, iters);

  // TS — pure-TS reference (applyBasicAdjustmentToPixelsTs in layerAdjustments.ts).
  const tsPixels = new Uint8ClampedArray(src);
  await timeIt("TS", () => {
    applyBasicAdjustmentToPixelsTs(tsPixels, adj);
  }, warmup, iters);

  // A — Rust wasm owning the WebGPU compute pipeline + readback (true A).
  if ((navigator as any).gpu) {
    const r: any = await WebGpuAdjustRenderer.create(w, h);
    await timeIt("A", async () => {
      const out: Uint8Array = await r.render(src as any, adj.brightness, adj.contrast, adj.saturation);
      if (out.length !== src.length) throw new Error("A length mismatch");
    }, warmup, iters);
  } else {
    console.log("  A      SKIPPED (no navigator.gpu in this runtime)");
  }
}

// Inlined pure-TS reference so the harness does not depend on the app's Vite graph.
function clampChannel(v: number): number {
  return v < 0 ? 0 : v > 255 ? 255 : v;
}
function normalizeBasicAdjustment(a: Adj): Adj {
  return a;
}
function applyAdjustmentToRgb(
  r: number,
  g: number,
  b: number,
  n: Adj,
): [number, number, number] {
  const contrastFactor = (259 * (n.contrast + 255)) / (255 * (259 - n.contrast));
  let cr = contrastFactor * (r - 0.5) + 0.5;
  let cg = contrastFactor * (g - 0.5) + 0.5;
  let cb = contrastFactor * (b - 0.5) + 0.5;
  const t = n.brightness / 100;
  if (t >= 0) {
    cr += (1 - cr) * t * 0.5;
    cg += (1 - cg) * t * 0.5;
    cb += (1 - cb) * t * 0.5;
  } else {
    const f = -t;
    cr -= cr * f * 0.5;
    cg -= cg * f * 0.5;
    cb -= cb * f * 0.5;
  }
  const lum = cr * 0.2126 + cg * 0.7152 + cb * 0.0722;
  const sat = 1 + n.saturation / 100;
  cr = lum + (cr - lum) * sat;
  cg = lum + (cg - lum) * sat;
  cb = lum + (cb - lum) * sat;
  return [cr, cg, cb];
}
function applyBasicAdjustmentToPixelsTs(pixels: Uint8ClampedArray, a: Adj): Uint8ClampedArray {
  const next = new Uint8ClampedArray(pixels);
  const n = normalizeBasicAdjustment(a);
  for (let i = 0; i < next.length; i += 4) {
    const [r, g, b] = applyAdjustmentToRgb(next[i] / 255, next[i + 1] / 255, next[i + 2] / 255, n);
    next[i] = clampChannel(r * 255);
    next[i + 1] = clampChannel(g * 255);
    next[i + 2] = clampChannel(b * 255);
  }
  return next;
}

await main();
