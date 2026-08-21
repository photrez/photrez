// @ts-nocheck
// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Dev-only REAL benchmark: C (Rust wasm Engine) vs TS (pure-TS) vs A (Rust wasm + WebGPU compute+readback).
// Runs INSIDE the webview — no Deno needed. Exposes `window.__benchRealEngine(w?,h?,iters?)`.
//
// Usage when `bun run tauri dev` (or `bun run dev` in a WebGPU-enabled browser):
//   await window.__benchRealEngine()              // 1280x720, 30 iters
//   await window.__benchRealEngine(4000,3000)     // 12 Mpx stress
//   await window.__benchRealEngine(1280,720,10)   // fewer iters
//
// Also callable as `await window.__benchRealEngine.run(…)`.
// No UI added — one file, no component.

import { getWasmExportModule } from "@/components/editor/wasmExport";

type Adj = { brightness: number; contrast: number; saturation: number };

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

// pure-TS reference — 1:1 with engine/layerAdjustments.ts:185 applyBasicAdjustmentToPixelsTs (no wasm route)
function clampChannel(v: number): number {
  return v < 0 ? 0 : v > 255 ? 255 : v;
}
function applyAdjustmentToRgb(r: number, g: number, b: number, n: Adj): [number, number, number] {
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
  for (let i = 0; i < next.length; i += 4) {
    const [r, g, b] = applyAdjustmentToRgb(next[i] / 255, next[i + 1] / 255, next[i + 2] / 255, a);
    next[i] = clampChannel(r * 255);
    next[i + 1] = clampChannel(g * 255);
    next[i + 2] = clampChannel(b * 255);
  }
  return next;
}

async function timeAsync(label: string, fn: () => Promise<void> | void, warmup: number, iters: number) {
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
  // eslint-disable-next-line no-console
  console.log(`  ${label.padEnd(6)} median=${median.toFixed(3)}ms mean=${mean.toFixed(3)}ms min=${min.toFixed(3)}ms (n=${iters})`);
  return { median, mean, min, samples };
}

export async function runRealEngineBench(w = 1280, h = 720, iters = 30): Promise<void> {
  const adj: Adj = { brightness: 12, contrast: 18, saturation: -10 };
  const warmup = 5;
  const px = w * h;
  // eslint-disable-next-line no-console
  console.log(`\n[bench] REAL C vs TS vs A — ${w}x${h} = ${(px / 1e6).toFixed(2)} Mpx, adj=${JSON.stringify(adj)}`);

  const m = await getWasmExportModule();
  if (!m) {
    // eslint-disable-next-line no-console
    console.warn("[bench] wasm unavailable (headless test env) — C & A skipped, only TS will run");
  }

  const src = makeBuffer(w, h);

  // C — Rust wasm Engine (1:1 renderLayerPixelsRust)
  if (m?.Engine) {
    await timeAsync("C", async () => {
      const eng: any = new m.Engine(w, h);
      eng.brightness = adj.brightness;
      eng.contrast = adj.contrast;
      eng.saturation = adj.saturation;
      eng.set_layer_pixels(src);
      eng.render();
      const view = m.rgba_buffer_view(eng.buffer_id);
      // touch so the read isn't optimized away; copy is tiny vs 0.5-2ms boundary tax
      const _copy = new Uint8Array(view);
      if (_copy[0] === 255 && _copy[1] === 255) void 0;
      m.free_rgba_buffer(eng.buffer_id);
    }, warmup, iters);
  } else {
    // eslint-disable-next-line no-console
    console.log("  C      SKIPPED (wasm Engine unavailable)");
  }

  // TS — pure-TS (no wasm)
  const tsSrc = new Uint8ClampedArray(src);
  await timeAsync("TS", () => {
    applyBasicAdjustmentToPixelsTs(tsSrc, adj);
  }, warmup, iters);

  // A — Rust wasm owning WebGPU compute + readback (TRUE A, not synthetic)
  const hasGpu = typeof navigator !== "undefined" && !!(navigator as any).gpu;
  if (!hasGpu) {
    // eslint-disable-next-line no-console
    console.log("  A      SKIPPED (no navigator.gpu — need Chrome 113+/Edge with WebGPU or Deno)");
  } else if (!m?.PoaRenderer) {
    // eslint-disable-next-line no-console
    console.log("  A      SKIPPED (PoaRenderer not in wasm pkg — run `bun run --filter photrez-desktop build:wasm`)");
  } else {
    const PoaRenderer: any = m.PoaRenderer;
    let renderer: any;
    try {
      renderer = await PoaRenderer.create(w, h);
    } catch (e) {
      // eslint-disable-next-line no-console
      console.warn("[bench] PoaRenderer.create failed:", e);
      // eslint-disable-next-line no-console
      console.log("  A      FAILED (create error — see above)");
      return;
    }
    // PoaRenderer.render takes Uint8Array (js_sys::Uint8Array) — pass src as Uint8Array
    const srcU8 = src; // already Uint8Array
    await timeAsync("A", async () => {
      const out: Uint8Array = await renderer.render(srcU8, adj.brightness, adj.contrast, adj.saturation);
      if (out.length !== src.length) throw new Error("A length mismatch");
    }, warmup, iters);
  }

  // eslint-disable-next-line no-console
  console.log("[bench] done — compare medians; A vs C decides the SSOT winner. Earlier ~31ms A was synthetic (configure-only) and is superseded.\n");
}

// expose for console when in dev webview
if (typeof window !== "undefined") {
  const w = window as any;
  w.__benchRealEngine = runRealEngineBench;
  w.__benchRealEngine.run = runRealEngineBench;
  if ((import.meta as any)?.env?.DEV) {
    // eslint-disable-next-line no-console
    console.log("[bench] window.__benchRealEngine(w?,h?,iters?) ready — e.g. await window.__benchRealEngine()");
  }
}
