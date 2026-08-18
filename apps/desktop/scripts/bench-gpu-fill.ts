// @ts-nocheck
// Bench: WGSL compute ellipse-fill (inverted) vs TS CPU per-pixel loop, 4K.
// Real AMD Radeon via Deno native WebGPU. Run:
//   deno run --unstable-webgpu --allow-read apps/desktop/scripts/bench-gpu-fill.ts
// Mirrors the per-pixel cost in layerOperations.ts fillLayerWithColor (inverted/normal ellipse).
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

// Ellipse = centered 50% box, like a real selection fill.
const P = {
  cx: W * 0.45,
  cy: H * 0.45,
  rx: W * 0.25,
  ry: H * 0.25,
  fr: 100,
  fg: 200,
  fb: 50,
  inverted: true, // worst case: full-canvas loop, write everything OUTSIDE ellipse
  W,
};

function cpuFill(px: Uint8Array): Uint8Array {
  const out = new Uint8Array(px.length);
  out.set(px);
  const { cx, cy, rx, ry, fr, fg, fb, inverted } = P;
  for (let py = 0; py < H; py++) {
    for (let px2 = 0; px2 < W; px2++) {
      const nx = (px2 - cx) / rx;
      const ny = (py - cy) / ry;
      const inside = nx * nx + ny * ny <= 1;
      const match = inverted ? !inside : inside;
      if (match) {
        const o = (py * W + px2) * 4;
        out[o] = fr;
        out[o + 1] = fg;
        out[o + 2] = fb;
        out[o + 3] = 255;
      }
    }
  }
  return out;
}

function tsBaseline(px: Uint8Array): number {
  let best = Infinity;
  for (let k = 0; k < 3; k++) {
    const t0 = performance.now();
    cpuFill(px);
    best = Math.min(best, performance.now() - t0);
  }
  return best;
}

const WGSL = `
struct Params {
  a: vec4<f32>,  // cx, cy, rx, ry
  b: vec4<f32>,  // fr, fg, fb, inverted
  c: vec4<f32>,  // W, _, _, _
};
@group(0) @binding(0) var<storage, read_write> data: array<u32>;
@group(0) @binding(1) var<uniform> p: Params;

@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = gid.x;
  if (i >= arrayLength(&data)) { return; }
  let cx = p.a.x; let cy = p.a.y; let rx = p.a.z; let ry = p.a.w;
  let fr = p.b.x; let fg = p.b.y; let fb = p.b.z; let inv = p.b.w;
  let W = u32(p.c.x);
  let px = f32(i % W);
  let py = f32(i / W);
  let nx = (px - cx) / rx;
  let ny = (py - cy) / ry;
  let inside = nx * nx + ny * ny <= 1.0;
  let hit = select(inside, !inside, inv > 0.5);
  if (hit) {
    let packed = (u32(fb) << 16u) | (u32(fg) << 8u) | u32(fr) | (255u << 24u);
    data[i] = packed;
  }
}
`;

async function gpuBench(px: Uint8Array): Promise<{ ms: number; out: Uint8Array }> {
  const gpu = navigator.gpu;
  const adapter = await gpu.requestAdapter();
  const device = await adapter.requestDevice();
  device.pushErrorScope("validation");
  const u32 = new Uint32Array(px.slice().buffer);
  const usage = GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC | GPUBufferUsage.COPY_DST;
  const buf = device.createBuffer({ size: u32.byteLength, usage });
  const pbuf = device.createBuffer({ size: 48, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
  device.queue.writeBuffer(pbuf, 0, new Float32Array([P.cx, P.cy, P.rx, P.ry, P.fr, P.fg, P.fb, P.inverted ? 1 : 0, P.W, 0, 0, 0]));
  const module = device.createShaderModule({ code: WGSL });
  const pipeline = device.createComputePipeline({ layout: "auto", compute: { module, entryPoint: "main" } });
  const perr = await device.popErrorScope();
  if (perr) console.log("  PIPELINE VALIDATION ERR:", perr.message);
  device.pushErrorScope("validation");
  const bind = device.createBindGroup({
    layout: pipeline.getBindGroupLayout(0),
    entries: [
      { binding: 0, resource: { buffer: buf } },
      { binding: 1, resource: { buffer: pbuf } },
    ],
  });
  const groups = Math.ceil(N / 256);
  let best = Infinity;
  let out = new Uint8Array(px.length);
  for (let k = 0; k < 3; k++) {
    device.queue.writeBuffer(buf, 0, u32);
    const t0 = performance.now();
    const enc = device.createCommandEncoder();
    const pass = enc.beginComputePass();
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
  const ref = cpuFill(src);
  let maxDiff = 0;
  let mism = 0;
  for (let i = 0; i < out.length; i++) {
    const d = Math.abs(out[i] - ref[i]);
    if (d > 0) mism++;
    maxDiff = Math.max(maxDiff, d);
  }
  const speedup = tsMs / gMs;
  let refFilled = 0, gpuFilled = 0;
  for (let i = 0; i < out.length; i += 4) {
    if (ref[i] === 100 && ref[i + 1] === 200 && ref[i + 2] === 50 && ref[i + 3] === 255) refFilled++;
    if (out[i] === 100 && out[i + 1] === 200 && out[i + 2] === 50 && out[i + 3] === 255) gpuFilled++;
  }
  console.log(`[fill] W=${W} H=${H} px=${N} (inverted ellipse, center 50% box)`);
  console.log(`  TS CPU baseline : ${tsMs.toFixed(2)} ms`);
  console.log(`  GPU WGSL compute: ${gMs.toFixed(2)} ms`);
  console.log(`  speedup: ${speedup.toFixed(2)}x`);
  console.log(`  verify: maxChannelDiff=${maxDiff} mismatchBytes=${mism}`);
  console.log(`  debug refFilled=${refFilled} gpuFilled=${gpuFilled} (expect ~outside-ellipse count)`);
  console.log(`  debug out[0..4]=${Array.from(out.slice(0, 4))} ref[0..4]=${Array.from(ref.slice(0, 4))} src[0..4]=${Array.from(src.slice(0, 4))}`);
  console.log(`  correct=${maxDiff === 0 ? "true" : "false (CHECK)"}`);
} else {
  const tsMs = tsBaseline(src);
  console.log(`[fill] WebGPU not available (headless). TS CPU baseline = ${tsMs.toFixed(2)} ms. Run under Deno for GPU number.`);
}
