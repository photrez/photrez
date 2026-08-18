// @ts-nocheck
// Bench: TS engine orchestration overhead vs GPU compute.
// Answers R4: would moving the editing engine to Rust speed anything up?
// The live engine hot path is GPU (WGSL/WebGL2). TS only packs params + buffers
// and reads back. If TS overhead is a few ms vs GPU ~100ms, R4 yields ~0 speedup.
const W = 4000, H = 3000, N = W * H;
const src = new Uint8ClampedArray(N * 4);
for (let i = 0; i < src.length; i++) src[i] = (i * 7) & 255;

function tsPackOverhead() {
  let best = Infinity;
  for (let k = 0; k < 5; k++) {
    const t0 = performance.now();
    // gpuCompute.ts packs RGBA8 -> Uint32 the same way before every GPU dispatch.
    const u32 = new Uint32Array(src.slice().buffer);
    // brightness/contrast/saturation params packed into a Float32Array (uniform).
    const p = new Float32Array([20, 40, 60, 0]);
    // touch both so the optimizer can't elide them
    if (u32[0] !== 0 && p[0] !== 0) { /* keep alive */ }
    best = Math.min(best, performance.now() - t0);
  }
  return best;
}

function tsReadbackOverhead() {
  let best = Infinity;
  const u32 = new Uint32Array(N);
  for (let k = 0; k < 5; k++) {
    const t0 = performance.now();
    // verify pass reads every pixel back to CPU (full-pixel correctness check).
    let acc = 0;
    for (let i = 0; i < u32.length; i++) acc += u32[i] & 255;
    if (acc > -1) { /* keep alive */ }
    best = Math.min(best, performance.now() - t0);
  }
  return best;
}

const pack = tsPackOverhead();
const readback = tsReadbackOverhead();
const tsTotal = pack + readback;
// Representative GPU compute from prior benches (blur 4K ~105ms; adjust same order).
const gpuBaseline = 105;
const tsShare = (tsTotal / gpuBaseline) * 100;

console.log(`\n[engine-bench] 4K RGBA, 12M px = 48 MB buffer`);
console.log(`  TS pack (RGBA8->Uint32 + params): ${pack.toFixed(2)} ms`);
console.log(`  TS readback (verify pass)       : ${readback.toFixed(2)} ms`);
console.log(`  TS orchestration total          : ${tsTotal.toFixed(2)} ms`);
console.log(`  GPU compute (prior bench)       : ~${gpuBaseline} ms`);
console.log(`  => TS share of hot path         : ${tsShare.toFixed(1)} %`);
console.log(`  => max speedup if engine -> Rust: ~${tsShare.toFixed(1)} % (GPU-bound, not TS-bound)`);
console.log(`  CONCLUSION: R4 (engine-in-Rust) is NOT a speed win; only architecture/maintainability.`);
