// @ts-nocheck
// R1 BEFORE/AFTER benchmark — production-realistic paths per op.
//   BEFORE = what actually ran before the cfg fix + wiring:
//     invert   -> TS WGSL orchestrator (fresh adapter+device EVERY call)
//     gradient -> WASM CPU kernel  (gradient_fill_wasm)
//     flood    -> WASM CPU kernel  (flood_fill_wasm)
//   AFTER = current tier-1:
//     all -> WebGpuAdjustRenderer (Rust PoA), created once per size.
// Deno wgpu device-pool is limited: run ONE mode+size combo per process.
//   deno run --unstable-webgpu -A <this> invert-before "[1920,1080]"
//   deno run --unstable-webgpu -A <this> after      "[1920,1080]"
//   deno run --unstable-webgpu -A <this> cpu        "[1920,1080]"   (WASM CPU kernels, no GPU pool use)
import fs from "node:fs";
import { fileURLToPath } from "node:url";

const pkg = await import(new URL("../src/wasm/pkg/photrez_core.js", import.meta.url).href);
pkg.initSync({ module: new WebAssembly.Module(fs.readFileSync(fileURLToPath(new URL("../src/wasm/pkg/photrez_core_bg.wasm", import.meta.url)))) });
const adjustWgsl = fs.readFileSync(fileURLToPath(new URL("../src/lib/gpu/shaders/adjust.wgsl", import.meta.url)), "utf8");
const invertWgsl = fs.readFileSync(fileURLToPath(new URL("../src/lib/gpu/shaders/invert.wgsl", import.meta.url)), "utf8");

const mode = (typeof Deno !== "undefined" && Deno.args[0]) || "after";
const sizeArg = (typeof Deno !== "undefined" && Deno.args[1]) || "[1920,1080]";
const [W, H] = JSON.parse(sizeArg);
const N = W * H;
const ADJ = { b: 12, c: -8, s: 25 };

function median(a) { const s = [...a].sort((x, y) => x - y); return s[Math.floor(s.length / 2)]; }
function makeNoise() {
  const v = new Uint8Array(N * 4);
  for (let i = 0; i < N; i++) {
    v[i * 4] = ((i * 7) ^ (i >> 3)) & 255; v[i * 4 + 1] = (i * 11) & 255;
    v[i * 4 + 2] = (i >> 2) & 255; v[i * 4 + 3] = 255;
  }
  return v;
}
async function benchAsync(iters, fn) {
  await fn(); await fn();
  const ts = [];
  for (let i = 0; i < iters; i++) { const t0 = performance.now(); await fn(); ts.push(performance.now() - t0); }
  return median(ts);
}
function benchSync(iters, fn) {
  fn(); fn();
  const ts = [];
  for (let i = 0; i < iters; i++) { const t0 = performance.now(); fn(); ts.push(performance.now() - t0); }
  return median(ts);
}
const fmt = (ms) => `${ms.toFixed(2)}ms`;

const px = makeNoise();

if (mode === "cpu") {
  // ── WASM CPU kernels (the de-facto BEFORE for gradient/flood) ──
  const stops = new Float64Array([0, 1]);
  const colors = new Uint8Array([255, 0, 0, 255, 0, 0, 255, 255]);
  const g = benchSync(8, () => pkg.gradient_fill_wasm(px, W, H, 0, 0, 0, W - 1, H - 1, stops, colors, false, 0, 0, 0, 0, 0, false));
  const f = benchSync(8, () => pkg.flood_fill_wasm(px, W, H, 5, 5, px[0], px[1], px[2], px[3], 0, false, 0, 0, 0, 0, 0, false, false));
  console.log(`BEFORE(CPU) ${W}x${H}: gradient=${fmt(g)} flood=${fmt(f)}`);
} else if (mode === "invert-before") {
  // ── TS WGSL with fresh adapter+device per call (old production behavior) ──
  async function oldInvertOnce() {
    const device = await (await navigator.gpu.requestAdapter()).requestDevice();
    const src = px.slice();
    const u32 = new Uint32Array(src.buffer);
    const buf = device.createBuffer({ size: u32.byteLength, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_SRC | GPUBufferUsage.COPY_DST });
    device.queue.writeBuffer(buf, 0, u32);
    const module = device.createShaderModule({ code: invertWgsl });
    const pipeline = device.createComputePipeline({ layout: "auto", compute: { module, entryPoint: "main" } });
    const bind = device.createBindGroup({ layout: pipeline.getBindGroupLayout(0), entries: [{ binding: 0, resource: { buffer: buf } }] });
    const enc = device.createCommandEncoder();
    const pass = enc.beginComputePass();
    pass.setPipeline(pipeline); pass.setBindGroup(0, bind); pass.dispatchWorkgroups(Math.ceil(N / 256)); pass.end();
    device.queue.submit([enc.finish()]);
    const read = device.createBuffer({ size: u32.byteLength, usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ });
    const enc2 = device.createCommandEncoder();
    enc2.copyBufferToBuffer(buf, 0, read, 0, u32.byteLength);
    device.queue.submit([enc2.finish()]);
    await read.mapAsync(GPUMapMode.READ);
    const out = new Uint8Array(read.getMappedRange().slice(0));
    read.unmap(); read.destroy?.(); buf.destroy?.(); pipeline.__keep = module;
    device.destroy?.();
    return out;
  }
  const times = [];
  for (let i = 0; i < 3; i++) {
    try {
      const t0 = performance.now();
      await oldInvertOnce();
      times.push(performance.now() - t0);
    } catch (e) {
      console.log(`sample ${i} failed (${e.message}) — pool limit reached`);
      break;
    }
  }
  console.log(`BEFORE(TS-WGSL fresh-device) ${W}x${H}: invert med=${fmt(median(times))} samples=[${times.map((t) => t.toFixed(1)).join(", ")}]`);
} else if (mode === "after") {
  // ── Current tier-1: Rust PoA renderer, created once ──
  const r = await pkg.WebGpuAdjustRenderer.create(W, H);
  const createMs = performance.now() - 0; // includes adapter+device+pipelines+buffers
  void createMs;
  const invHot = await benchAsync(8, () => r.invert(px));
  const adjHot = await benchAsync(8, () => r.render(px, ADJ.b, ADJ.c, ADJ.s));
  const stops = null;
  void stops;
  const gradHot = await benchAsync(8, () => r.gradient(px, 0, 0, W - 1, H - 1, 0, 255, 0, 0, 255, 1, 0, 0, 255, 255, 0));
  const flHot = await benchAsync(8, () => r.flood_global(px, px[0], px[1], px[2], px[3], 10, 20, 30, 255, 0, 0, 0, 0, 0, 0, 0, 0));
  console.log(`AFTER(PoA hot) ${W}x${H}: invert=${fmt(invHot)} adjust=${fmt(adjHot)} gradient=${fmt(gradHot)} flood=${fmt(flHot)}`);
}
