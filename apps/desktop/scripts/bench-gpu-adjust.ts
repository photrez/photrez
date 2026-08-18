// @ts-nocheck
// Bench: WGSL compute Brightness/Contrast/Saturation vs TS CPU, 4K.
// Real AMD Radeon via Deno native WebGPU. Run:
//   deno run --unstable-webgpu --allow-read scripts/bench-gpu-adjust.ts
import { readFileSync } from "node:fs";

const WGSL = readFileSync(new URL("../src/lib/gpu/shaders/adjust.wgsl", import.meta.url), "utf8");

const W = 4000;
const H = 3000;
const N = W * H;
const src = new Uint8Array(N * 4);
for (let i = 0; i < N; i++) {
  const o = i * 4;
  src[o] = (i * 7) & 255;
  src[o + 1] = (i * 13) & 255;
  src[o + 2] = (i * 29) & 255;
  src[o + 3] = 255;
}

const adj = { brightness: 20, contrast: -40, saturation: 60 };

// Inline reference = applyAdjustmentToRgb (f64), used for both timing baseline + verify.
function cpuAdjust(px: Uint8Array): Uint8Array {
  const b = Math.max(-100, Math.min(100, adj.brightness));
  const c = Math.max(-100, Math.min(100, adj.contrast));
  const s = Math.max(-100, Math.min(100, adj.saturation));
  const cf = (259 * (c + 255)) / (255 * (259 - c));
  const t = b / 100;
  const sat = 1 + s / 100;
  const out = new Uint8Array(px.length);
  for (let i = 0; i < px.length; i += 4) {
    let cr = cf * (px[i] / 255 - 0.5) + 0.5;
    let cg = cf * (px[i + 1] / 255 - 0.5) + 0.5;
    let cb = cf * (px[i + 2] / 255 - 0.5) + 0.5;
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
    cr = lum + (cr - lum) * sat;
    cg = lum + (cg - lum) * sat;
    cb = lum + (cb - lum) * sat;
    out[i] = Math.max(0, Math.min(255, Math.round(cr * 255)));
    out[i + 1] = Math.max(0, Math.min(255, Math.round(cg * 255)));
    out[i + 2] = Math.max(0, Math.min(255, Math.round(cb * 255)));
    out[i + 3] = px[i + 3];
  }
  return out;
}

function tsBaseline(px: Uint8Array): number {
  let best = Infinity;
  for (let k = 0; k < 3; k++) {
    const t0 = performance.now();
    cpuAdjust(px);
    best = Math.min(best, performance.now() - t0);
  }
  return best;
}

async function gpuBench(px: Uint8Array): Promise<{ ms: number; out: Uint8Array }> {
  const gpu = navigator.gpu;
  const adapter = await gpu.requestAdapter();
  const device = await adapter.requestDevice();
  const u32 = new Uint32Array(px.slice().buffer);
  const usage = GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC | GPUBufferUsage.COPY_DST;
  const buf = device.createBuffer({ size: u32.byteLength, usage });
  device.queue.writeBuffer(buf, 0, u32);
  const pbuf = device.createBuffer({ size: 16, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
  device.queue.writeBuffer(pbuf, 0, new Float32Array([adj.brightness, adj.contrast, adj.saturation, 0]));
  const module = device.createShaderModule({ code: WGSL });
  const pipeline = device.createComputePipeline({ layout: "auto", compute: { module, entryPoint: "main" } });
  const bind = device.createBindGroup({
    layout: pipeline.getBindGroupLayout(0),
    entries: [
      { binding: 0, resource: { buffer: buf } },
      { binding: 1, resource: { buffer: pbuf } },
    ],
  });
  const groups = Math.ceil(px.length / 4 / 256);
  // warmup
  let enc = device.createCommandEncoder();
  let pass = enc.beginComputePass();
  pass.setPipeline(pipeline);
  pass.setBindGroup(0, bind);
  pass.dispatchWorkgroups(groups);
  pass.end();
  device.queue.submit([enc.finish()]);

   let best = Infinity;
   let out = new Uint8Array(px.length);
   for (let k = 0; k < 3; k++) {
     // reset to original pixels: shader mutates buf in place, so re-seed each run
     device.queue.writeBuffer(buf, 0, u32);
     const t0 = performance.now();
     enc = device.createCommandEncoder();
     pass = enc.beginComputePass();
     pass.setPipeline(pipeline);
     pass.setBindGroup(0, bind);
     pass.dispatchWorkgroups(groups);
     pass.end();
     device.queue.submit([enc.finish()]);
     const read = device.createBuffer({ size: u32.byteLength, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
     const enc3 = device.createCommandEncoder();
     enc3.copyBufferToBuffer(buf, 0, read, 0, u32.byteLength);
     device.queue.submit([enc3.finish()]);
     await read.mapAsync(GPUMapMode.READ);
     const sampled = new Uint8Array(read.getMappedRange().slice(0));
     read.unmap();
     best = Math.min(best, performance.now() - t0);
     if (k === 2) out = sampled;
   }
   return { ms: best, out };
}

if (navigator.gpu) {
  const tsMs = tsBaseline(src);
  const { ms: gMs, out } = await gpuBench(src);
  const ref = cpuAdjust(src);
  let maxDiff = 0;
  let mism = 0;
  for (let i = 0; i < out.length; i += 977 * 4) {
    const d = Math.abs(out[i] - ref[i]) + Math.abs(out[i + 1] - ref[i + 1]) + Math.abs(out[i + 2] - ref[i + 2]);
    if (d > 0) mism++;
    maxDiff = Math.max(maxDiff, d);
  }
  const speedup = tsMs / gMs;
  console.log(`[adjust] W=${W} H=${H} px=${N} (B/C/S ${adj.brightness}/${adj.contrast}/${adj.saturation})`);
  console.log(`  TS CPU baseline : ${tsMs.toFixed(2)} ms`);
  console.log(`  GPU WGSL compute: ${gMs.toFixed(2)} ms`);
  console.log(`  speedup: ${speedup.toFixed(2)}x`);
  console.log(`  src[0..4]=${Array.from(src.slice(0, 4))}`);
  console.log(`  gpu[0..4]=${Array.from(out.slice(0, 4))}`);
  console.log(`  ref[0..4]=${Array.from(ref.slice(0, 4))}`);
  console.log(`  verify: maxChannelDiff(sample)=${maxDiff} sampledMismatchPixels=${mism} (f32 vs f64 tolerance)`);
  console.log(`  correct=${maxDiff <= 2 ? "true" : "false (CHECK)"}`);
} else {
  const tsMs = tsBaseline(src);
  console.log(`[adjust] WebGPU not available (headless). TS CPU baseline = ${tsMs.toFixed(2)} ms. Run under Deno for GPU number.`);
}
