// SPDX-License-Identifier: AGPL-3.0-or-later
// Compare hand-written WGSL compute shader (GPU) vs TS CPU for invert.
// Run in a WebGPU-capable context for the GPU number:
//   - Tauri dev (WebView2 / Edge Chromium, WebGPU on by default), or
//   - any browser with WebGPU, or `deno run --unstable-webgpu` (native wgpu).
// In headless bun (no navigator.gpu) it prints the TS baseline + a notice.
// Run: bun scripts/bench-gpu-shader.ts   (or: deno run --unstable-webgpu scripts/bench-gpu-shader.ts)
const W = 4000, H = 3000, len = W * H * 4, pixels = W * H;

function fill(a: Uint8Array) {
  for (let i = 0; i < len; i += 4) {
    a[i] = (i * 7) & 255; a[i + 1] = (i * 13) & 255; a[i + 2] = (i * 29) & 255; a[i + 3] = 255;
  }
}

// --- TS CPU baseline (mirrors invertRgbaFallback in wasmExport.ts) ---
function invertTS(a: Uint8Array) {
  for (let i = 0; i < a.length; i += 4) {
    a[i] = 255 - a[i]; a[i + 1] = 255 - a[i + 1]; a[i + 2] = 255 - a[i + 2];
  }
}

// WGSL shader lives in invert.wgsl (hand-written, imported as text).
// workgroup_size(256) -> 46875 groups for 12M px, within the 65535 max-workgroups limit.
async function readShader(url: URL): Promise<string> {
  const g: any = globalThis;
  if (g.Deno) return await g.Deno.readTextFile(url);
  return await g.Bun.file(url).text();
}
const SHADER = await readShader(new URL("./invert.wgsl", import.meta.url));

async function benchGPU(src: Uint8Array) {
  const gpu: any = (globalThis as any).navigator?.gpu;
  if (!gpu) {
    console.log("  WebGPU unavailable in this runtime (headless bun). Use Deno (--unstable-webgpu) or a WebGPU browser/Tauri WebView2.");
    return null;
  }
  const adapter = await gpu.requestAdapter();
  if (!adapter) { console.log("  no GPU adapter"); return null; }
  const device = await adapter.requestDevice();
  const bytes = pixels * 4;
  const buf: any = device.createBuffer({ size: bytes, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST | GPUBufferUsage.COPY_SRC });
  device.queue.writeBuffer(buf, 0, src);
  const module = device.createShaderModule({ code: SHADER });
  const pipeline: any = device.createComputePipeline({ layout: "auto", compute: { module, entryPoint: "main" } });
  const bind: any = device.createBindGroup({ layout: pipeline.getBindGroupLayout(0), entries: [{ binding: 0, resource: { buffer: buf } }] });
  const groups = Math.ceil(pixels / 256); // 46875, within maxComputeWorkgroupsPerDimension (65535)

  for (let k = 0; k < 3; k++) {
    const enc = device.createCommandEncoder();
    const pass = enc.beginComputePass();
    pass.setPipeline(pipeline); pass.setBindGroup(0, bind);
    pass.dispatchWorkgroups(groups); pass.end();
    device.queue.submit([enc.finish()]);
  }
  await device.queue.onSubmittedWorkDone();

  const t0 = performance.now();
  for (let k = 0; k < 50; k++) {
    const enc = device.createCommandEncoder();
    const pass = enc.beginComputePass();
    pass.setPipeline(pipeline); pass.setBindGroup(0, bind);
    pass.dispatchWorkgroups(groups); pass.end();
    device.queue.submit([enc.finish()]);
  }
  await device.queue.onSubmittedWorkDone();
  const tGpu = (performance.now() - t0) / 50;

  // verify sampled pixels (cheap, deterministic)
  const rb: any = device.createBuffer({ size: bytes, usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST });
  const enc = device.createCommandEncoder();
  enc.copyBufferToBuffer(buf, 0, rb, 0, bytes);
  device.queue.submit([enc.finish()]);
  await rb.mapAsync(GPUMapMode.READ);
  const out = new Uint8Array(rb.getMappedRange().slice(0));
  rb.unmap();
  const idxs = [0, 1, (pixels >> 1), pixels - 1];
  let ok = true;
  for (const p of idxs) {
    const o = p * 4;
    const er = 255 - src[o], eg = 255 - src[o + 1], eb = 255 - src[o + 2], ea = src[o + 3];
    if (out[o] !== er || out[o + 1] !== eg || out[o + 2] !== eb || out[o + 3] !== ea) { ok = false; break; }
  }
  console.log(`  GPU WGSL compute     = ${tGpu.toFixed(3)}ms  (correct=${ok}, workgroups=${groups})`);
  return tGpu;
}

const tsBuf = new Uint8Array(len);
fill(tsBuf);
const t1 = performance.now();
for (let k = 0; k < 10; k++) invertTS(tsBuf);
const tTs = (performance.now() - t1) / 10;

console.log(`invert ${W}x${H} (${pixels}px):`);
console.log(`  TS CPU in-place      = ${tTs.toFixed(2)}ms  (baseline)`);
const gpuSrc = new Uint8Array(len);
fill(gpuSrc); // fresh input so the GPU path is independent of the TS-mutated buffer
const tGpu = await benchGPU(gpuSrc);
if (tGpu) console.log(`  speedup GPU vs TS    = ${(tTs / tGpu).toFixed(2)}x`);
