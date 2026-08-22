// @ts-nocheck
// Runner: load scripts/rust_wasm_adjust.wasm (C-ABI) and bench adjust vs TS CPU.
// Run: deno run --allow-read scripts/bench-rust-wasm.ts
const W = 4000, H = 3000, N = W * H, LEN = N * 4;
const BR = 1.15, CT = 18;
const src = new Uint8Array(LEN);
for (let i = 0; i < N; i++) { const o = i * 4; src[o] = (i * 7) & 255; src[o + 1] = (i * 13) & 255; src[o + 2] = (i * 29) & 255; src[o + 3] = 255; }

async function rustWasm() {
  const bytes = await Deno.readFile("apps/desktop/scripts/rust_wasm_adjust.wasm");
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const exp: any = instance.exports;
  const ptr = exp.alloc(LEN);
  // copy src into wasm linear memory at ptr
  new Uint8Array(exp.memory.buffer, ptr, LEN).set(src);
  let best = Infinity; let out: Uint8Array | null = null;
  for (let k = 0; k < 3; k++) {
    new Uint8Array(exp.memory.buffer, ptr, LEN).set(src);
    const t0 = performance.now();
    exp.adjust(ptr, LEN, BR, CT);
    best = Math.min(best, performance.now() - t0);
    if (k === 2) out = new Uint8Array(exp.memory.buffer, ptr, LEN).slice();
  }
  return { best, out };
}

function tsCPU(): number {
  let best = Infinity;
  for (let k = 0; k < 3; k++) {
    const t0 = performance.now();
    const out = new Uint8Array(src.length);
    for (let i = 0; i < src.length; i += 4) {
      let r = src[i] * BR + CT, g = src[i + 1] * BR + CT, b = src[i + 2] * BR + CT;
      out[i] = r < 0 ? 0 : r > 255 ? 255 : r | 0;
      out[i + 1] = g < 0 ? 0 : g > 255 ? 255 : g | 0;
      out[i + 2] = b < 0 ? 0 : b > 255 ? 255 : b | 0;
      out[i + 3] = src[i + 3];
    }
    best = Math.min(best, performance.now() - t0);
  }
  return best;
}

const tsMs = tsCPU();
const { best: rustMs, out } = await rustWasm();
// verify
let maxDiff = 0; const ref = new Uint8Array(src.length);
for (let i = 0; i < src.length; i += 4) { ref[i] = Math.max(0, Math.min(255, src[i] * BR + CT)); ref[i + 1] = Math.max(0, Math.min(255, src[i + 1] * BR + CT)); ref[i + 2] = Math.max(0, Math.min(255, src[i + 2] * BR + CT)); }
for (let i = 0; i < src.length; i += 4) { const d = Math.abs(out![i] - ref[i]) + Math.abs(out![i + 1] - ref[i + 1]) + Math.abs(out![i + 2] - ref[i + 2]); if (d > maxDiff) maxDiff = d; }
console.log(`[rust-wasm adjust] W=${W} H=${H} px=${N} (48MB)`);
console.log(`  TS CPU       : ${tsMs.toFixed(2)} ms`);
console.log(`  Rust/WASM    : ${rustMs.toFixed(2)} ms  (single-thread, no simd yet)`);
console.log(`  speedup      : ${(tsMs / rustMs).toFixed(2)}x`);
console.log(`  verify maxDiff=${maxDiff} correct=${maxDiff <= 1 ? "true" : "CHECK"}`);
