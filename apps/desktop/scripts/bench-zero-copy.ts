// SPDX-License-Identifier: AGPL-3.0-or-later
// Proof benchmark for zero-copy WASM (option b): compares
//   old copy-in/out WASM  vs  zero-copy WASM  vs  TS in-place
// Run: bun scripts/bench-zero-copy.ts
import { alloc_rgba_buffer, rgba_buffer_view, invert_rgba_inplace, free_rgba_buffer, invert_rgba_wasm } from "../src/wasm/pkg/photrez_core.js";

const wasm: any = await import("../src/wasm/pkg/photrez_core.js");
if (typeof wasm.default === "function") await wasm.default();

const W = 4000, H = 3000, len = W * H * 4;

function fill(a: Uint8Array) {
  for (let i = 0; i < len; i += 4) {
    a[i] = (i * 7) & 255; a[i + 1] = (i * 13) & 255; a[i + 2] = (i * 29) & 255; a[i + 3] = 255;
  }
}

// --- old copy-in/out WASM ---
const jsOld = new Uint8Array(len);
fill(jsOld);
for (let k = 0; k < 3; k++) wasm.invert_rgba_wasm(jsOld);
const tOld = performance.now();
for (let k = 0; k < 10; k++) wasm.invert_rgba_wasm(jsOld);
const tOldMs = (performance.now() - tOld) / 10;

// --- new zero-copy WASM (buffer owned by wasm) ---
const id = wasm.alloc_rgba_buffer(len);
const view: Uint8Array = wasm.rgba_buffer_view(id); // zero-copy view
fill(view); // upload into wasm memory
for (let k = 0; k < 3; k++) wasm.invert_rgba_inplace(id);
const tZc = performance.now();
for (let k = 0; k < 10; k++) wasm.invert_rgba_inplace(id);
const tZcMs = (performance.now() - tZc) / 10;
wasm.free_rgba_buffer(id);

// --- TS in-place (data owned by JS) ---
const js = new Uint8Array(len);
fill(js);
function invertTS(a: Uint8Array) {
  for (let i = 0; i < a.length; i += 4) { a[i] = 255 - a[i]; a[i + 1] = 255 - a[i + 1]; a[i + 2] = 255 - a[i + 2]; }
}
const tTs = performance.now();
for (let k = 0; k < 10; k++) invertTS(js);
const tTsMs = (performance.now() - tTs) / 10;

console.log(`invert ${W}x${H}:`);
console.log(`  old copy-in/out WASM = ${tOldMs.toFixed(2)}ms  (speedup vs TS = ${(tTsMs / tOldMs).toFixed(2)}x)`);
console.log(`  zero-copy WASM       = ${tZcMs.toFixed(2)}ms  (speedup vs TS = ${(tTsMs / tZcMs).toFixed(2)}x)`);
console.log(`  TS in-place          = ${tTsMs.toFixed(2)}ms  (baseline)`);
