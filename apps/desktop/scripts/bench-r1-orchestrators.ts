// @ts-nocheck
// R1 micro-bench: GPU compute ORCHESTRATOR comparison for the SAME kernel
// (basic adjustment B/C/S on RGBA):
//   TS  = faithful mirror of production gpuCompute.ts adjustRgbaGpu
//         (incl. its per-call requestAdapter/requestDevice!)
//   RUST = WebGpuAdjustRenderer (Technique A, poa/webgpu_adjust.rs) - device
//         created once per size, then render() per op.
// Run: deno run --unstable-webgpu -A apps/desktop/scripts/bench-r1-orchestrators.ts
import fs from "node:fs";
import { fileURLToPath } from "node:url";

const pkg = await import(new URL("../src/wasm/pkg/photrez_core.js", import.meta.url).href);
const wasmPath = fileURLToPath(new URL("../src/wasm/pkg/photrez_core_bg.wasm", import.meta.url));
pkg.initSync({ module: new WebAssembly.Module(fs.readFileSync(wasmPath)) });
const adjustWgsl = fs.readFileSync(
  fileURLToPath(new URL("../src/lib/gpu/shaders/adjust.wgsl", import.meta.url)),
  "utf8",
);

function median(a) { const s = [...a].sort((x, y) => x - y); return s[Math.floor(s.length / 2)]; }
async function benchAsync(iters, fn) {
  await fn(); await fn(); // warmup
  const ts = [];
  for (let i = 0; i < iters; i++) {
    const t0 = performance.now();
    await fn();
    ts.push(performance.now() - t0);
  }
  return median(ts);
}
function makeNoise(w, h) {
  const v = new Uint8Array(w * h * 4);
  for (let i = 0; i < w * h; i++) {
    const x = i % w, y = (i / w) | 0;
    v[i * 4] = ((x * 7) ^ (y * 13)) & 255;
    v[i * 4 + 1] = (x * 11 + y * 3) & 255;
    v[i * 4 + 2] = ((x * x + y * y) >> 6) & 255;
    v[i * 4 + 3] = 255;
  }
  return v;
}
const ADJ = { b: 12, c: -8, s: 25 };

// ── TS orchestrator: EXACT structure of gpuCompute.adjustRgbaGpu ─────────────
async function makeTsRunner(hoistDevice) {
  // hoistDevice=false mirrors production: adapter+device EVERY call.
  let gpu, device;
  if (hoistDevice) {
    gpu = navigator.gpu;
    device = await (await gpu.requestAdapter()).requestDevice();
  }
  return async function tsAdjust(pixels) {
    let dev = device;
    if (!dev) {
      const g = globalThis;
      const ad = await g.navigator.gpu.requestAdapter();
      if (!ad) throw new Error("no adapter");
      dev = await ad.requestDevice();
    }
    const src = pixels.slice();
    const u32 = new Uint32Array(src.buffer);
    const usage = GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC | GPUBufferUsage.COPY_DST;
    const buf = dev.createBuffer({ size: u32.byteLength, usage });
    dev.queue.writeBuffer(buf, 0, u32);
    const pbuf = dev.createBuffer({ size: 16, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    dev.queue.writeBuffer(pbuf, 0, new Float32Array([ADJ.b, ADJ.c, ADJ.s, 0]));
    const module = dev.createShaderModule({ code: adjustWgsl });
    const pipeline = dev.createComputePipeline({ layout: "auto", compute: { module, entryPoint: "main" } });
    const bind = dev.createBindGroup({
      layout: pipeline.getBindGroupLayout(0),
      entries: [
        { binding: 0, resource: { buffer: buf } },
        { binding: 1, resource: { buffer: pbuf } },
      ],
    });
    const groups = Math.ceil((pixels.length >>> 2) / 256);
    const enc = dev.createCommandEncoder();
    const pass = enc.beginComputePass();
    pass.setPipeline(pipeline);
    pass.setBindGroup(0, bind);
    pass.dispatchWorkgroups(groups);
    pass.end();
    dev.queue.submit([enc.finish()]);
    const read = dev.createBuffer({ size: u32.byteLength, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
    const enc2 = dev.createCommandEncoder();
    enc2.copyBufferToBuffer(buf, 0, read, 0, u32.byteLength);
    dev.queue.submit([enc2.finish()]);
    await read.mapAsync(GPUMapMode.READ);
    const out = new Uint8Array(read.getMappedRange().slice(0));
    read.unmap();
    read.destroy?.();
    buf.destroy?.();
    pbuf.destroy?.();
    if (!hoistDevice) {
      // bench hygiene: Deno/wgpu exhausts after ~3 live devices; production
      // relies on browser recycling instead, so free eagerly here.
      dev.destroy?.();
    }
    return out;
  };
}

console.log("== R1: GPU orchestrator comparison (same kernel: basic adjustment) ==");
// Deno wgpu has a per-process device/memory budget ("Not enough memory left"
// after a few devices); run ONE size per process when needed:
//   deno run --unstable-webgpu -A <this> "[1920,1080]"
const ALL_SIZES = [[1280, 720], [1920, 1080]];
const arg = typeof Deno !== "undefined" ? Deno.args[0] : undefined;
const SIZES = arg ? [JSON.parse(arg)] : ALL_SIZES;
for (const [w, h] of SIZES) {
  const px = makeNoise(w, h);
  const mp = ((w * h) / 1e6).toFixed(1);

  // Rust PoA: create ONCE (like production poaCache)
  console.log(`[stage] ${w}x${h} PoA create...`);
  const t0 = performance.now();
  const renderer = await pkg.WebGpuAdjustRenderer.create(w, h);
  const createMs = performance.now() - t0;
  const rustOut = await renderer.render(px, ADJ.b, ADJ.c, ADJ.s);
  console.log(`[stage] PoA render ok`);
  const rustHot = await benchAsync(8, () => renderer.render(px, ADJ.b, ADJ.c, ADJ.s));
  console.log(`[stage] PoA hot ok`);

  // TS: as-produced today = fresh adapter+device per call. Measure as SINGLE
  // shots (real usage has user-think gaps); tight hot-looping this variant
  // leaks devices in Deno/wgpu and poisons the run.
  async function timeSingle(fn) {
    const t0 = performance.now();
    await fn();
    return performance.now() - t0;
  }
  const tsProdTimes = [];
  let tsOutBytes = null;
  for (let i = 0; i < 3; i++) {
    try {
      const runner = await makeTsRunner(false);
      console.log(`[stage] TS single #${i} ...`);
      const t0 = performance.now();
      const out = await runner(px);
      tsProdTimes.push(performance.now() - t0);
      if (!tsOutBytes) tsOutBytes = out;
      console.log(`[stage] TS single #${i} ok`);
    } catch (e) {
      console.log(`[stage] TS single #${i} FAILED (${e.message}) - pool limit, skipping rest`);
      break;
    }
  }
  if (tsProdTimes.length === 0) throw new Error("no successful TS prod samples");
  const tsProdHot = median(tsProdTimes);
  let tsHoisHot = NaN;
  try {
    const tsHoisted = await makeTsRunner(true);
    tsHoisHot = await benchAsync(8, () => tsHoisted(px));
  } catch (e) {
    console.log(`[stage] TS hoisted FAILED (${e.message}) - reporting prod number only`);
  }

  let maxDiff = 0;
  for (let i = 0; i < px.length; i++) {
    const d = Math.abs(tsOutBytes[i] - rustOut[i]);
    if (d > maxDiff) maxDiff = d;
  }

  console.log(`\n-- ${w}x${h} (${mp} MP) --`);
  console.log(`  RUST PoA : create=${createMs.toFixed(1)}ms (once)   hot=${rustHot.toFixed(2)}ms`);
  console.log(`  TS prod  : single-shot med=${tsProdHot.toFixed(2)}ms (fresh device per call)`);
  console.log(`  TS hoist : hot=${tsHoisHot.toFixed(2)}ms`);
  console.log(`  speedup rust-vs-ts-prod = ${(tsProdHot / rustHot).toFixed(2)}x   parity maxDiff=${maxDiff}`);
}
