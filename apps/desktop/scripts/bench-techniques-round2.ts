// @ts-nocheck
// Bench round 2 — technique matrix: TS CPU vs Rust WASM vs GPU WGSL.
// Candidates R2 (selection bbox trim) + R3 (brush stamp/composite).
// TS baselines mirror production verbatim (see bench-cpu-pixel-round2.ts).
// Rust: apps/desktop/src/wasm/pkg (built via `bun run build:wasm`).
// GPU: WebGPU compute (needs Deno): deno run --unstable-webgpu -A <this file>
//      (auto-skipped when navigator.gpu is absent, e.g. under bun/node).
import fs from "node:fs";
import { fileURLToPath } from "node:url";

// ── load wasm pkg via disk bytes (same pattern as src/test/wasmTestShim.ts) ──
const pkgUrl = new URL("../src/wasm/pkg/photrez_core.js", import.meta.url);
const pkg = await import(pkgUrl.href);
const wasmPath = fileURLToPath(
  new URL("../src/wasm/pkg/photrez_core_bg.wasm", import.meta.url),
);
pkg.initSync({ module: new WebAssembly.Module(fs.readFileSync(wasmPath)) });

function median(arr) {
  const s = [...arr].sort((a, b) => a - b);
  return s[Math.floor(s.length / 2)];
}
async function benchIters(name, iters, fn) {
  // warmup x2 — awaited so async GPU fns fully complete (no overlapping maps)
  await fn(); await fn();
  const times = [];
  for (let i = 0; i < iters; i++) {
    const t0 = performance.now();
    await fn();
    times.push(performance.now() - t0);
  }
  return { name, med: median(times) };
}
const fmt = (ms) => `${ms.toFixed(2)}ms`;
const row = (label, a, b, c) =>
  console.log(`  ${label.padEnd(34)} TS=${a.padStart(9)}   WASM=${b.padStart(9)}   GPU=${c}`);

// ── shared fixtures ──────────────────────────────────────────────────────────
const W = 4000, H = 3000, N = W * H;
function makeImageShim(w, h, opaqueRatio) {
  const data = new Uint8ClampedArray(w * h * 4);
  for (let i = 0; i < w * h; i++)
    data[i * 4 + 3] = (i % 1000) / 1000 < opaqueRatio ? 255 : 0;
  return { data, width: w, height: h };
}
const img4K = makeImageShim(W, H, 0.6);

// TS mirrors (verbatim from SelectionOperations.trimTransparent)
function trimTs(imageData) {
  const { width, height, data: pixels } = imageData;
  let top = height, bottom = 0, left = width, right = 0;
  for (let y = 0; y < height; y++)
    for (let x = 0; x < width; x++)
      if (pixels[(y * width + x) * 4 + 3] > 0) {
        if (y < top) top = y;
        if (y > bottom) bottom = y;
        if (x < left) left = x;
        if (x > right) right = x;
      }
  return top > bottom ? null : [left, top, right, bottom];
}

// TS mirror (brushTipMask.stampBrushTip) — see bench-cpu-pixel-round2.ts
function clamp01(v) { return v < 0 ? 0 : v > 1 ? 1 : v; }
function makeTip(diameter) {
  const dataSize = 64;
  const data = new Float32Array(dataSize * dataSize);
  for (let y = 0; y < dataSize; y++)
    for (let x = 0; x < dataSize; x++) {
      const nx = (x - dataSize / 2) / (dataSize / 2);
      const ny = (y - dataSize / 2) / (dataSize / 2);
      const d = Math.sqrt(nx * nx + ny * ny);
      data[y * dataSize + x] = d <= 1 ? Math.pow(1 - d, 0.8) : 0;
    }
  return { diameter, dataSize, data };
}
function stampTs(mask, maskWidth, maskHeight, tip, centerX, centerY, alphaScale) {
  const aScale = clamp01(alphaScale);
  const halfExtent = tip.diameter / 2;
  const centerIndex = halfExtent - 0.5;
  const dataSize = tip.dataSize;
  const dataScale = dataSize / tip.diameter;
  const minX = Math.max(0, Math.floor(centerX - halfExtent));
  const maxX = Math.min(maskWidth - 1, Math.ceil(centerX + halfExtent) - 1);
  const minY = Math.max(0, Math.floor(centerY - halfExtent));
  const maxY = Math.min(maskHeight - 1, Math.ceil(centerY + halfExtent) - 1);
  for (let y = minY; y <= maxY; y += 1) {
    const ty = (y - centerY + centerIndex) * dataScale;
    const y0 = Math.floor(ty), y1 = y0 + 1, wy = ty - y0;
    const y0In = y0 >= 0 && y0 < dataSize, y1In = y1 >= 0 && y1 < dataSize;
    const y0Off = y0 * dataSize, y1Off = y1 * dataSize;
    const rowIdx = y * maskWidth;
    for (let x = minX; x <= maxX; x += 1) {
      const tx = (x - centerX + centerIndex) * dataScale;
      const x0 = Math.floor(tx), x1 = x0 + 1, wx = tx - x0;
      const x0In = x0 >= 0 && x0 < dataSize, x1In = x1 >= 0 && x1 < dataSize;
      const a00 = (y0In && x0In) ? tip.data[y0Off + x0] : 0;
      const a10 = (y0In && x1In) ? tip.data[y0Off + x1] : 0;
      const a01 = (y1In && x0In) ? tip.data[y1Off + x0] : 0;
      const a11 = (y1In && x1In) ? tip.data[y1Off + x1] : 0;
      const a0 = a00 * (1 - wx) + a10 * wx;
      const a1 = a01 * (1 - wx) + a11 * wx;
      const interpolated = a0 * (1 - wy) + a1 * wy;
      if (interpolated <= 0) continue;
      const scaled = interpolated * aScale;
      if (scaled <= 0) continue;
      const idx = rowIdx + x;
      const cur = mask[idx];
      if (cur >= 255) continue;
      mask[idx] = cur + Math.round((255 - cur) * scaled);
    }
  }
}
// TS mirror (compositeMaskToImageDataDirty) — straight-alpha over, dirty rect
function compositeTs(imageData, originX, originY, mask, maskWidth, rect, color, isEraser) {
  const data = imageData.data, imgW = imageData.width;
  const strokeAlpha = color.a;
  for (let y = rect.y0; y < rect.y1; y++) {
    const rowInMask = y * maskWidth;
    const rowInImage = (y - originY) * imgW;
    for (let x = rect.x0; x < rect.x1; x++) {
      const maskAlpha = mask[rowInMask + x] / 255;
      if (maskAlpha <= 0) continue;
      const i = (rowInImage + (x - originX)) << 2;
      const alpha = maskAlpha * strokeAlpha;
      if (isEraser) { data[i + 3] = Math.round(data[i + 3] * (1 - alpha)); continue; }
      const dstA = data[i + 3] / 255;
      const outA = alpha + dstA * (1 - alpha);
      if (outA <= 0) { data[i] = data[i + 1] = data[i + 2] = data[i + 3] = 0; continue; }
      data[i]     = Math.round((color.r * alpha + data[i]     * dstA * (1 - alpha)) / outA);
      data[i + 1] = Math.round((color.g * alpha + data[i + 1] * dstA * (1 - alpha)) / outA);
      data[i + 2] = Math.round((color.b * alpha + data[i + 2] * dstA * (1 - alpha)) / outA);
      data[i + 3] = Math.round(outA * 255);
    }
  }
}

const DABS = 500;
const PAINT = { r: 200, g: 80, b: 40, a: 0.7 };
const CANVAS_W = 1920, CANVAS_H = 1080;

// ═══════════ R2: trim bbox ═══════════
console.log("== R2: selection trim bbox @4K (12M px) ==");
{
  const ts = await benchIters("TS scan", 5, () => trimTs(img4K));
  // zero-copy: upload ONCE, kernel reads wasm-owned memory
  const bufId = pkg.alloc_rgba_buffer(img4K.data.length);
  pkg.rgba_buffer_view(bufId).set(img4K.data); // direct wasm-memory write, no FFI
  const wasm = await benchIters("Rust WASM (zero-copy)", 5, () => pkg.trim_bbox_buffer(bufId, W, H));
  row("bbox scan", fmt(ts.med), fmt(wasm.med), "n/a (readback-bound)");
  console.log(`  parity: TS=${JSON.stringify(trimTs(img4K))} WASM=${JSON.stringify(Array.from(pkg.trim_bbox_buffer(bufId, W, H)))}`);
}

// ═══════════ R3: brush stamp + composite ═══════════
console.log("\n== R3: brush per-dab (500-dab burst, zero-copy) ==");
for (const diameter of [32, 128, 512]) {
  const tip = makeTip(diameter);
  const tipData = new Float32Array(tip.data);
  const maskLen = CANVAS_W * CANVAS_H;

  const maskId = pkg.alloc_rgba_buffer(maskLen); // wasm-owned, written ONCE
  const wStamp = await benchIters(`WASM stamp d${diameter}`, 3, () => {
    for (let d = 0; d < DABS; d++)
      pkg.brush_stamp_buffer(maskId, CANVAS_W, CANVAS_H, tipData, tip.dataSize, tip.diameter, 960 + (d % 64), 540 + (d % 64), 0.5);
  });
  // bit-exact parity: TS one dab on fresh mask vs WASM one dab on fresh buffer
  const tsMask = new Uint8ClampedArray(maskLen);
  stampTs(tsMask, CANVAS_W, CANVAS_H, tip, 960, 540, 0.5);
  pkg.rgba_buffer_view(maskId).fill(0);
  pkg.brush_stamp_buffer(maskId, CANVAS_W, CANVAS_H, tipData, tip.dataSize, tip.diameter, 960, 540, 0.5);
  const wView = pkg.rgba_buffer_view(maskId);
  let mismatches = 0;
  for (let i = 0; i < maskLen; i++) if (tsMask[i] !== wView[i]) mismatches++;
  console.log(`  stamp parity d${diameter}: ${mismatches === 0 ? "BIT-EXACT" : mismatches + " MISMATCHES"}`);

  const sz = diameter;
  const imgTs = { data: new Uint8ClampedArray(sz * sz * 4), width: sz, height: sz };
  const cmask = new Uint8ClampedArray(sz * sz).fill(180);
  const rect = { x0: 0, y0: 0, x1: sz, y1: sz };
  const tsMask2 = new Uint8ClampedArray(maskLen); // persistent, like production
  const tsStamp = await benchIters(`TS stamp d${diameter}`, 3, () => {
    for (let d = 0; d < DABS; d++)
      stampTs(tsMask2, CANVAS_W, CANVAS_H, tip, 960 + (d % 64), 540 + (d % 64), 0.5);
  });
  const tsComp = await benchIters(`TS composite d${diameter}`, 3, () => {
    for (let d = 0; d < DABS; d++) compositeTs(imgTs, 0, 0, cmask, sz, rect, PAINT, false);
  });
  const dstId = pkg.alloc_rgba_buffer(sz * sz * 4); // wasm-owned
  const cmaskId = pkg.alloc_rgba_buffer(sz * sz);
  pkg.rgba_buffer_view(cmaskId).fill(180);
  const wComp = await benchIters(`WASM composite d${diameter}`, 3, () => {
    for (let d = 0; d < DABS; d++)
      pkg.composite_mask_buffer(dstId, sz, 0, 0, cmaskId, sz, 0, 0, sz, sz, PAINT.r, PAINT.g, PAINT.b, PAINT.a, false);
  });

  const tsDab = (tsStamp.med + tsComp.med) / DABS * 1000;
  const wDab = (wStamp.med + wComp.med) / DABS * 1000;
  console.log(
    `  d${String(diameter).padEnd(4)} TS=${tsDab.toFixed(0).padStart(6)}µs/dab   WASM=${wDab.toFixed(0).padStart(6)}µs/dab   speedup=${(tsDab / wDab).toFixed(1)}×   budget@60fps: ${(16600 / wDab).toFixed(0)} dabs/frame`,
  );
}

// ═══════════ GPU section (Deno only) ═══════════
if (typeof navigator !== "undefined" && navigator.gpu) {
  console.log("\n== GPU (WebGPU compute) ==");
  const adapter = await navigator.gpu.requestAdapter();
  const device = await adapter.requestDevice();

  // R2 GPU bbox: atomic min/max on packed coords, one thread / 64 px chunk.
  // NOTE: two SEPARATE atomic buffers — a struct{atomic,atomic} misbehaves on
  // this Deno/wgpu build (members clobber each other; verified by diagnostic).
  const bboxModule = device.createShaderModule({
    code: `
    @group(0) @binding(0) var<storage, read> px: array<u32>;
    @group(0) @binding(1) var<storage, read_write> minv: atomic<u32>;
    @group(0) @binding(2) var<storage, read_write> maxv: atomic<u32>;
    @compute @workgroup_size(64)
    fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
      // stride loop: dispatch is clamped to the 65535-workgroups-per-dimension
      // WebGPU limit, so each thread covers multiple elements.
      let total = arrayLength(&px);
      let nthreads = 64u * 65535u;
      var i = gid.x;
      loop {
        if (i >= total) { break; }
        let packed = px[i]; // (r<<0 | g<<8 | b<<16 | a<<24)
        if ((packed >> 24) > 0u) {
          atomicMin(&minv, i);
          atomicMax(&maxv, i);
        }
        i += nthreads;
      }
    }`,
  });
  const pxU32 = new Uint32Array(img4K.data.buffer.slice(0));
  const pxBuf = device.createBuffer({ size: pxU32.byteLength, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST });
  device.queue.writeBuffer(pxBuf, 0, pxU32);
  const minBuf = device.createBuffer({ size: 4, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC });
  const maxBuf = device.createBuffer({ size: 4, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC });
  const rb = device.createBuffer({ size: 8, usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST });
  const bgLayout = device.createBindGroupLayout({
    entries: [
      { binding: 0, visibility: GPUShaderStage.COMPUTE, buffer: { type: "read-only-storage" } },
      { binding: 1, visibility: GPUShaderStage.COMPUTE, buffer: { type: "storage" } },
      { binding: 2, visibility: GPUShaderStage.COMPUTE, buffer: { type: "storage" } },
    ],
  });
  const bg = device.createBindGroup({
    layout: bgLayout,
    entries: [
      { binding: 0, resource: { buffer: pxBuf } },
      { binding: 1, resource: { buffer: minBuf } },
      { binding: 2, resource: { buffer: maxBuf } },
    ],
  });
  const pipe = device.createComputePipeline({
    layout: device.createPipelineLayout({ bindGroupLayouts: [bgLayout] }),
    compute: { module: bboxModule, entryPoint: "main" },
  });
  async function gpuBboxHot() {
    // buffer resident — measures dispatch + full completion + readback
    device.queue.writeBuffer(minBuf, 0, new Uint32Array([0xffffffff])); device.queue.writeBuffer(maxBuf, 0, new Uint32Array([0]));
    const enc = device.createCommandEncoder();
    const pass = enc.beginComputePass();
    pass.setPipeline(pipe);
    pass.setBindGroup(0, bg);
    pass.dispatchWorkgroups(Math.min(Math.ceil(N / 64), 65535));
    pass.end();
    enc.copyBufferToBuffer(minBuf, 0, rb, 0, 4); enc.copyBufferToBuffer(maxBuf, 0, rb, 4, 4);
    device.queue.submit([enc.finish()]);
    await rb.mapAsync(GPUMapMode.READ);
    const [minI, maxI] = new Uint32Array(rb.getMappedRange().slice(0));
    rb.unmap();
    if (maxI < minI) return null;
    return [minI % W, Math.floor(minI / W), maxI % W, Math.floor(maxI / W)];
  }
  const rbCold = device.createBuffer({ size: 8, usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST });
  async function gpuBboxCold() {
    // realistic single-shot: 48MB upload INCLUDED each call
    device.queue.writeBuffer(pxBuf, 0, pxU32);
    device.queue.writeBuffer(minBuf, 0, new Uint32Array([0xffffffff])); device.queue.writeBuffer(maxBuf, 0, new Uint32Array([0]));
    const enc = device.createCommandEncoder();
    const pass = enc.beginComputePass();
    pass.setPipeline(pipe);
    pass.setBindGroup(0, bg);
    pass.dispatchWorkgroups(Math.min(Math.ceil(N / 64), 65535));
    pass.end();
    enc.copyBufferToBuffer(minBuf, 0, rbCold, 0, 4); enc.copyBufferToBuffer(maxBuf, 0, rbCold, 4, 4);
    device.queue.submit([enc.finish()]);
    await rbCold.mapAsync(GPUMapMode.READ);
    const v = new Uint32Array(rbCold.getMappedRange().slice(0));
    rbCold.unmap();
    const [minI, maxI] = v;
    if (maxI < minI) return null;
    return [minI % W, Math.floor(minI / W), maxI % W, Math.floor(maxI / W)];
  }
  const gHot = await benchIters("GPU bbox hot", 5, gpuBboxHot);
  console.log(`  bbox scan GPU (buffer resident) = ${fmt(gHot.med)}`);
  const gres = await gpuBboxHot();
  console.log(`  parity: GPU=${JSON.stringify(gres)}`);
  let gCold;
  try {
    gCold = await benchIters("GPU bbox cold", 5, gpuBboxCold);
    console.log(`  bbox scan GPU (48MB upload incl) = ${fmt(gCold.med)}`);
  } catch (e) {
    console.log(`  bbox scan GPU cold: FAILED (${e.message}) - hot number stands`);
  }
} else {
  console.log("\n== GPU section skipped (no navigator.gpu — run under Deno) ==");
}





