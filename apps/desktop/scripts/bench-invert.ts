// SPDX-License-Identifier: AGPL-3.0-or-later
// Benchmark Phase 2 invert kernel: WASM (&[u8]->Vec<u8>) vs in-place TS fallback.
// Run: bun scripts/bench-invert.ts
import { invert_rgba_wasm } from "../src/wasm/pkg/photrez_core.js";

const wasm: any = await import("../src/wasm/pkg/photrez_core.js");
if (typeof wasm.default === "function") await wasm.default();

function invertFallback(rgba: Uint8Array): Uint8Array {
  const out = new Uint8Array(rgba.length);
  for (let i = 0; i < rgba.length; i += 4) {
    out[i] = 255 - rgba[i];
    out[i + 1] = 255 - rgba[i + 1];
    out[i + 2] = 255 - rgba[i + 2];
    out[i + 3] = rgba[i + 3];
  }
  return out;
}

const W = 4000, H = 3000;
const buf = new Uint8Array(W * H * 4);
for (let i = 0; i < buf.length; i += 4) {
  buf[i] = (i * 7) & 255; buf[i + 1] = (i * 13) & 255; buf[i + 2] = (i * 29) & 255; buf[i + 3] = 255;
}
for (let k = 0; k < 3; k++) wasm.invert_rgba_wasm(buf); // warmup
const t0 = performance.now();
for (let k = 0; k < 10; k++) wasm.invert_rgba_wasm(buf);
const tw = (performance.now() - t0) / 10;
const t1 = performance.now();
for (let k = 0; k < 10; k++) invertFallback(buf);
const tf = (performance.now() - t1) / 10;
console.log(`invert ${W}x${H}: WASM=${tw.toFixed(2)}ms  TS=${tf.toFixed(2)}ms  speedup=${(tf / tw).toFixed(2)}x`);
