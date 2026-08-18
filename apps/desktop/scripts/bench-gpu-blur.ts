// @ts-nocheck
// Bench: WGSL separable Gaussian blur (2-pass) vs TS CPU, 4K.
// Real AMD Radeon via Deno native WebGPU. Run:
//   deno run --unstable-webgpu --allow-read scripts/bench-gpu-blur.ts
// Self-contained proof-of-concept (no production edit). Mirrors bench-gpu-adjust.ts.

const W = 4000;
const H = 3000;
const N = W * H;
const RADIUS = 8;
const SIGMA = 4;

const src = new Uint8Array(N * 4);
for (let i = 0; i < N; i++) {
  const o = i * 4;
  src[o] = (i * 7) & 255;
  src[o + 1] = (i * 13) & 255;
  src[o + 2] = (i * 29) & 255;
  src[o + 3] = 255;
}

function w(k: number, sigma: number): number {
  return Math.exp(-(k * k) / (2 * sigma * sigma));
}

// TS fallback: separable Gaussian (horizontal pass -> vertical pass), float accum.
function cpuBlur(px: Uint8Array, radius: number, sigma: number): Uint8Array {
  const tmp = new Float32Array(N * 3);
  // horizontal
  for (let y = 0; y < H; y++) {
    for (let x = 0; x < W; x++) {
      let r = 0, g = 0, b = 0, ws = 0;
      for (let k = -radius; k <= radius; k++) {
        const xx = Math.max(0, Math.min(W - 1, x + k));
        const o = (y * W + xx) * 4;
        const wt = w(k, sigma);
        r += px[o] * wt; g += px[o + 1] * wt; b += px[o + 2] * wt; ws += wt;
      }
      const t = (y * W + x) * 3;
      tmp[t] = r / ws; tmp[t + 1] = g / ws; tmp[t + 2] = b / ws;
    }
  }
  // vertical
  const out = new Uint8Array(N * 4);
  for (let y = 0; y < H; y++) {
    for (let x = 0; x < W; x++) {
      let r = 0, g = 0, b = 0, ws = 0;
      for (let k = -radius; k <= radius; k++) {
        const yy = Math.max(0, Math.min(H - 1, y + k));
        const t = (yy * W + x) * 3;
        const wt = w(k, sigma);
        r += tmp[t] * wt; g += tmp[t + 1] * wt; b += tmp[t + 2] * wt; ws += wt;
      }
      const o = (y * W + x) * 4;
      out[o] = Math.max(0, Math.min(255, Math.round(r / ws)));
      out[o + 1] = Math.max(0, Math.min(255, Math.round(g / ws)));
      out[o + 2] = Math.max(0, Math.min(255, Math.round(b / ws)));
      out[o + 3] = px[o + 3];
    }
  }
  return out;
}

function tsBaseline(px: Uint8Array): number {
  let best = Infinity;
  for (let k = 0; k < 3; k++) {
    const t0 = performance.now();
    cpuBlur(px, RADIUS, SIGMA);
    best = Math.min(best, performance.now() - t0);
  }
  return best;
}

const WGSL = `
struct Params { width: f32, height: f32, radius: f32, sigma: f32, dirX: f32, dirY: f32, _p0: f32, _p1: f32 };
@group(0) @binding(0) var<storage, read> src: array<u32>;
@group(0) @binding(1) var<storage, read_write> dst: array<u32>;
@group(0) @binding(2) var<uniform> p: Params;

fn getpx(x: i32, y: i32) -> vec4<f32> {
  let ww = i32(p.width); let hh = i32(p.height);
  let cx = clamp(x, 0, ww - 1); let cy = clamp(y, 0, hh - 1);
  let v = src[u32(cy * ww + cx)];
  return vec4<f32>(f32(v & 255u), f32((v >> 8u) & 255u), f32((v >> 16u) & 255u), f32((v >> 24u) & 255u));
}
@compute @workgroup_size(8, 8, 1)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
  let ww = i32(p.width); let hh = i32(p.height);
  let x = i32(id.x); let y = i32(id.y);
  if (x >= ww || y >= hh) { return; }
  let r = i32(p.radius);
  let sigma = p.sigma;
  let dx = i32(p.dirX); let dy = i32(p.dirY);
  var col = vec3<f32>(0.0);
  var wsum = 0.0;
  for (var k = -r; k <= r; k = k + 1) {
    let wt = exp(-f32(k * k) / (2.0 * sigma * sigma));
    let s = getpx(x + dx * k, y + dy * k);
    col = col + s.rgb * wt;
    wsum = wsum + wt;
  }
  col = col / wsum;
  let a = getpx(x, y).a;
  let bi = u32(y * ww + x);
  dst[bi] = (u32(col.r) & 255u) | ((u32(col.g) & 255u) << 8u) | ((u32(col.b) & 255u) << 16u) | ((u32(a) & 255u) << 24u);
}
`;

async function gpuBench(px: Uint8Array): Promise<{ ms: number; out: Uint8Array }> {
  const gpu = navigator.gpu;
  const adapter = await gpu.requestAdapter();
  const device = await adapter.requestDevice();
  const u32 = new Uint32Array(px.slice().buffer);
  const inUsage = GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST;
  const ioUsage = GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC | GPUBufferUsage.COPY_DST;
  const bufA = device.createBuffer({ size: u32.byteLength, usage: inUsage });
  const bufB = device.createBuffer({ size: u32.byteLength, usage: ioUsage }); // intermediate (H pass out)
  const bufC = device.createBuffer({ size: u32.byteLength, usage: ioUsage }); // output (V pass out)
  device.queue.writeBuffer(bufA, 0, u32);

  const uH = new Float32Array([W, H, RADIUS, SIGMA, 1, 0, 0, 0]); // horizontal
  const uV = new Float32Array([W, H, RADIUS, SIGMA, 0, 1, 0, 0]); // vertical
  const p1 = device.createBuffer({ size: 32, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
  const p2 = device.createBuffer({ size: 32, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
  device.queue.writeBuffer(p1, 0, uH);
  device.queue.writeBuffer(p2, 0, uV);

  const module = device.createShaderModule({ code: WGSL });
  const pipeline = device.createComputePipeline({ layout: "auto", compute: { module, entryPoint: "main" } });
  const bg1 = device.createBindGroup({
    layout: pipeline.getBindGroupLayout(0),
    entries: [
      { binding: 0, resource: { buffer: bufA } },
      { binding: 1, resource: { buffer: bufB } },
      { binding: 2, resource: { buffer: p1 } },
    ],
  });
  const bg2 = device.createBindGroup({
    layout: pipeline.getBindGroupLayout(0),
    entries: [
      { binding: 0, resource: { buffer: bufB } },
      { binding: 1, resource: { buffer: bufC } },
      { binding: 2, resource: { buffer: p2 } },
    ],
  });
  const gx = Math.ceil(W / 8);
  const gy = Math.ceil(H / 8);

  let best = Infinity;
  let out = new Uint8Array(px.length);
  for (let k = 0; k < 3; k++) {
    const t0 = performance.now();
    const enc = device.createCommandEncoder();
    const pass = enc.beginComputePass();
    pass.setPipeline(pipeline);
    pass.setBindGroup(0, bg1);
    pass.dispatchWorkgroups(gx, gy);
    pass.setBindGroup(0, bg2);
    pass.dispatchWorkgroups(gx, gy);
    pass.end();
    device.queue.submit([enc.finish()]);
    const read = device.createBuffer({ size: u32.byteLength, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
    const enc3 = device.createCommandEncoder();
    enc3.copyBufferToBuffer(bufC, 0, read, 0, u32.byteLength);
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
  const ref = cpuBlur(src, RADIUS, SIGMA);
  let maxDiff = 0;
  let sumDiff = 0;
  let mism = 0;
  const total = out.length / 4;
  for (let i = 0; i < out.length; i += 4) {
    const d = Math.abs(out[i] - ref[i]) + Math.abs(out[i + 1] - ref[i + 1]) + Math.abs(out[i + 2] - ref[i + 2]);
    if (d > 0) mism++;
    if (d > maxDiff) maxDiff = d;
    sumDiff += d;
  }
  const meanDiff = sumDiff / total;
  const speedup = tsMs / gMs;
  console.log(`[blur] W=${W} H=${H} px=${N} (separable Gaussian r=${RADIUS} sigma=${SIGMA}, 2-pass)`);
  console.log(`  TS CPU baseline : ${tsMs.toFixed(2)} ms`);
  console.log(`  GPU WGSL compute: ${gMs.toFixed(2)} ms`);
  console.log(`  speedup: ${speedup.toFixed(2)}x`);
  console.log(`  verify: maxChannelDiff(all)=${maxDiff} meanChannelDiff(all)=${meanDiff.toFixed(3)} diffPixels=${mism}/${total} (${(100 * mism / total).toFixed(2)}%)`);
  console.log(`  correct(f32-precision)=${maxDiff <= 6 ? "true" : "false (CHECK)"} (convolution != exact f64; <=6/255 visually lossless)`);
} else {
  const tsMs = tsBaseline(src);
  console.log(`[blur] WebGPU not available (headless). TS CPU baseline = ${tsMs.toFixed(2)} ms. Run under Deno for GPU number.`);
}
