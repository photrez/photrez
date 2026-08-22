// @ts-nocheck
// Bench: compare Rust-SSOT candidate paths (no Tauri needed).
//   - TS CPU pointwise adjust (baseline)
//   - WebGPU WGSL pointwise adjust: dispatch-only (technique A: Rust owns pixels, no upload)
//                                     vs dispatch+upload (technique B/C: TS uploads 48MB to GPU each frame)
//   - Boundary copy: ArrayBuffer 48MB copy (B) vs SharedArrayBuffer zero-copy (C)
// Real AMD Radeon via Deno WebGPU. Run:
//   deno run --unstable-webgpu --allow-read scripts/bench-techniques-compare.ts

const W = 4000, H = 3000, N = W * H;
const BR = 1.15, CT = 18; // brightness mult, contrast offset

const src = new Uint8Array(N * 4);
for (let i = 0; i < N; i++) {
  const o = i * 4;
  src[o] = (i * 7) & 255; src[o + 1] = (i * 13) & 255; src[o + 2] = (i * 29) & 255; src[o + 3] = 255;
}

// ---- TS CPU pointwise adjust ----
function adjustCPU(px: Uint8Array): Uint8Array {
  const out = new Uint8Array(px.length);
  for (let i = 0; i < px.length; i += 4) {
    let r = px[i] * BR + CT, g = px[i + 1] * BR + CT, b = px[i + 2] * BR + CT;
    out[i] = r < 0 ? 0 : r > 255 ? 255 : r | 0;
    out[i + 1] = g < 0 ? 0 : g > 255 ? 255 : g | 0;
    out[i + 2] = b < 0 ? 0 : b > 255 ? 255 : b | 0;
    out[i + 3] = px[i + 3];
  }
  return out;
}
function tsBaseline(): number {
  let best = Infinity;
  for (let k = 0; k < 3; k++) { const t0 = performance.now(); adjustCPU(src); best = Math.min(best, performance.now() - t0); }
  return best;
}

// ---- WebGPU WGSL pointwise adjust ----
const WGSL = `
struct P { width: f32, height: f32, brightness: f32, contrast: f32 };
@group(0) @binding(0) var<storage, read> src: array<u32>;
@group(0) @binding(1) var<storage, read_write> dst: array<u32>;
@group(0) @binding(2) var<uniform> p: P;
fn gx(v: u32) -> vec4<f32> { return vec4<f32>(f32(v & 255u), f32((v >> 8u) & 255u), f32((v >> 16u) & 255u), f32((v >> 24u) & 255u)); }
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
  let ww = i32(p.width); let hh = i32(p.height);
  let x = i32(id.x); let y = i32(id.y);
  if (x >= ww || y >= hh) { return; }
  let v = gx(src[u32(y * ww + x)]);
  let br = p.brightness; let ct = p.contrast;
  var r = v.r * br + ct; var g = v.g * br + ct; var b = v.b * br + ct;
  r = clamp(r, 0.0, 255.0); g = clamp(g, 0.0, 255.0); b = clamp(b, 0.0, 255.0);
  let a = v.a;
  dst[u32(y * ww + x)] = (u32(r) & 255u) | ((u32(g) & 255u) << 8u) | ((u32(b) & 255u) << 16u) | ((u32(a) & 255u) << 24u);
}`;

async function gpuBench() {
  const gpu = navigator.gpu;
  const adapter = await gpu.requestAdapter();
  const device = await adapter.requestDevice();
  const u32 = new Uint32Array(src.slice().buffer);
  const inU = GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST;
  const outU = GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC | GPUBufferUsage.COPY_DST;
  const bufA = device.createBuffer({ size: u32.byteLength, usage: inU });
  const bufB = device.createBuffer({ size: u32.byteLength, usage: outU });
  const uni = new Float32Array([W, H, BR, CT]);
  const pbuf = device.createBuffer({ size: 16, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
  device.queue.writeBuffer(pbuf, 0, uni);
  const module = device.createShaderModule({ code: WGSL });
  const pipeline = device.createComputePipeline({ layout: "auto", compute: { module, entryPoint: "main" } });
  const bg = device.createBindGroup({ layout: pipeline.getBindGroupLayout(0), entries: [
    { binding: 0, resource: { buffer: bufA } }, { binding: 1, resource: { buffer: bufB } }, { binding: 2, resource: { buffer: pbuf } } ] });
  const gx = Math.ceil(W / 8), gy = Math.ceil(H / 8);

  // upload-only timing (technique B/C cost: TS -> GPU each frame)
  let uploadBest = Infinity;
  for (let k = 0; k < 3; k++) { const t0 = performance.now(); device.queue.writeBuffer(bufA, 0, u32); device.queue.submit([]); uploadBest = Math.min(uploadBest, performance.now() - t0); }

  // pure dispatch (technique A cost: data already on GPU, Rust owns it, NO readback)
  device.queue.writeBuffer(bufA, 0, u32);
  let dispBest = Infinity;
  for (let k = 0; k < 3; k++) {
    const t0 = performance.now();
    const enc = device.createCommandEncoder(); const pass = enc.beginComputePass();
    pass.setPipeline(pipeline); pass.setBindGroup(0, bg); pass.dispatchWorkgroups(gx, gy); pass.end();
    device.queue.submit([enc.finish()]);
    await device.queue.onSubmittedWorkDone();
    dispBest = Math.min(dispBest, performance.now() - t0);
  }
  // dispatch + readback (reference: path that reads pixels back to CPU)
  let dispRB = Infinity; let out = new Uint8Array(src.length);
  for (let k = 0; k < 3; k++) {
    const t0 = performance.now();
    const enc = device.createCommandEncoder(); const pass = enc.beginComputePass();
    pass.setPipeline(pipeline); pass.setBindGroup(0, bg); pass.dispatchWorkgroups(gx, gy); pass.end();
    device.queue.submit([enc.finish()]);
    const read = device.createBuffer({ size: u32.byteLength, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
    const enc2 = device.createCommandEncoder(); enc2.copyBufferToBuffer(bufB, 0, read, 0, u32.byteLength); device.queue.submit([enc2.finish()]);
    await read.mapAsync(GPUMapMode.READ);
    const sampled = new Uint8Array(read.getMappedRange().slice(0)); read.unmap();
    dispRB = Math.min(dispRB, performance.now() - t0);
    if (k === 2) out = sampled;
  }
  return { uploadBest, dispBest, dispRB, out };
}

// ---- Boundary copy: ArrayBuffer vs SharedArrayBuffer ----
function copyBench() {
  let abBest = Infinity;
  for (let k = 0; k < 3; k++) { const t0 = performance.now(); const c = new Uint8Array(src); abBest = Math.min(abBest, performance.now() - t0); }
  let sabBest = Infinity;
  for (let k = 0; k < 3; k++) { const t0 = performance.now(); const sab = new SharedArrayBuffer(src.byteLength); const v = new Uint8Array(sab); sabBest = Math.min(sabBest, performance.now() - t0); }
  return { abBest, sabBest };
}

if (navigator.gpu) {
  const tsMs = tsBaseline();
  const { uploadBest, dispBest, dispRB, out } = await gpuBench();
  const ref = adjustCPU(src);
  let maxDiff = 0, sum = 0, mism = 0; const total = out.length / 4;
  for (let i = 0; i < out.length; i += 4) {
    const d = Math.abs(out[i] - ref[i]) + Math.abs(out[i + 1] - ref[i + 1]) + Math.abs(out[i + 2] - ref[i + 2]);
    if (d > 0) mism++; if (d > maxDiff) maxDiff = d; sum += d;
  }
  const { abBest, sabBest } = copyBench();
  console.log(`[adjust] W=${W} H=${H} px=${N} (48MB RGBA) brightness=${BR} contrast=${CT}`);
  console.log(`  TS CPU            : ${tsMs.toFixed(2)} ms`);
  console.log(`  GPU dispatch-only : ${dispBest.toFixed(2)} ms  (technique A: Rust owns pixels, no upload, no readback)`);
  console.log(`  GPU + readback    : ${dispRB.toFixed(2)} ms  (reference: path that reads 48MB back to CPU)`);
  console.log(`  GPU + upload      : ${(dispBest + uploadBest).toFixed(2)} ms  (technique B/C: TS uploads 48MB to GPU each frame; upload=${uploadBest.toFixed(2)} ms)`);
  console.log(`  ArrayBuffer copy  : ${abBest.toFixed(2)} ms  (technique B boundary cost)`);
  console.log(`  SharedArrayBuffer : ${sabBest.toFixed(2)} ms  (technique C zero-copy)`);
  console.log(`  GPU speedup vs TS : ${(tsMs / dispBest).toFixed(2)}x`);
  console.log(`  verify: maxChannelDiff=${maxDiff} meanDiff=${(sum / total).toFixed(3)} diffPx=${mism}/${total} correct=${maxDiff <= 2 ? "true" : "CHECK"}`);
} else {
  const tsMs = tsBaseline();
  const { abBest, sabBest } = copyBench();
  console.log(`[adjust] WebGPU unavailable. TS CPU=${tsMs.toFixed(2)} ms | ArrayBuffer copy=${abBest.toFixed(2)} ms | SharedArrayBuffer=${sabBest.toFixed(2)} ms`);
}
