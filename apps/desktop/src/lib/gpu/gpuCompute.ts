// Minimal WebGPU compute wrapper with CPU fallback.
// GPU path ports the proven bench (scripts/bench-gpu-shader.ts, measured ~5.9x on AMD Radeon).
// CPU fallback mirrors the TS baseline so behavior is identical when navigator.gpu is absent.
// WebGPU globals are referenced via `any` casts to avoid adding @webgpu/types as a dependency.

import invertWgsl from "./shaders/invert.wgsl?raw";
import adjustWgsl from "./shaders/adjust.wgsl?raw";

export interface GpuRunResult {
  data: Uint8Array;
  usedGpu: boolean;
}

export function isGpuComputeAvailable(): boolean {
  return typeof navigator !== "undefined" && !!(navigator as unknown as { gpu?: unknown }).gpu;
}

// Invert RGBA into a new buffer (alpha preserved).
export function invertRgbaCpu(pixels: Uint8Array | Uint8ClampedArray): Uint8Array {
  const out = new Uint8Array(pixels.length);
  for (let i = 0; i < pixels.length; i += 4) {
    out[i] = 255 - pixels[i];
    out[i + 1] = 255 - pixels[i + 1];
    out[i + 2] = 255 - pixels[i + 2];
    out[i + 3] = pixels[i + 3];
  }
  return out;
}

async function invertRgbaGpu(pixels: Uint8Array | Uint8ClampedArray): Promise<Uint8Array> {
  const g = globalThis as unknown as {
    navigator: { gpu: any };
    GPUBufferUsage: { STORAGE: number; COPY_SRC: number; COPY_DST: number; MAP_READ: number };
    GPUMapMode: { READ: number };
  };
  const gpu = g.navigator.gpu;
  const adapter = await gpu.requestAdapter();
  if (!adapter) throw new Error("no WebGPU adapter");
  const device = await adapter.requestDevice();
  const pxCount = pixels.length >>> 2;
  const copy = pixels.slice();
  const u32 = new Uint32Array(copy.buffer);
  const usage = g.GPUBufferUsage.STORAGE | g.GPUBufferUsage.COPY_SRC | g.GPUBufferUsage.COPY_DST;
  const buf = device.createBuffer({ size: u32.byteLength, usage });
  device.queue.writeBuffer(buf, 0, u32);
  const module = device.createShaderModule({ code: invertWgsl });
  const pipeline = device.createComputePipeline({ layout: "auto", compute: { module, entryPoint: "main" } });
  const bind = device.createBindGroup({
    layout: pipeline.getBindGroupLayout(0),
    entries: [{ binding: 0, resource: { buffer: buf } }],
  });
  const groups = Math.ceil(pxCount / 256);
  const enc = device.createCommandEncoder();
  const pass = enc.beginComputePass();
  pass.setPipeline(pipeline);
  pass.setBindGroup(0, bind);
  pass.dispatchWorkgroups(groups);
  pass.end();
  device.queue.submit([enc.finish()]);
  const read = device.createBuffer({ size: u32.byteLength, usage: g.GPUBufferUsage.COPY_DST | g.GPUBufferUsage.MAP_READ });
  const enc2 = device.createCommandEncoder();
  enc2.copyBufferToBuffer(buf, 0, read, 0, u32.byteLength);
  device.queue.submit([enc2.finish()]);
  await read.mapAsync(g.GPUMapMode.READ);
  const mapped = new Uint8Array(read.getMappedRange().slice(0));
  read.unmap();
  return mapped;
}

// ── Technique A (Rust PoA) tier for invert ───────────────────────────────────
// Mirrors tryAdjustRgbaPoa (engine/layerAdjustments.ts): cached WebGpuAdjustRenderer
// per size, null when WebGPU or the wasm module is unavailable. Dynamic import keeps
// lib/gpu dependency-free at the top level.
let invertPoaCache: { w: number; h: number; renderer: any } | null = null;

/** @internal test hook — clears the module-level PoA renderer cache */
export function __resetInvertPoaCacheForTests(): void {
  invertPoaCache = null;
}

async function tryInvertRgbaPoa(
  pixels: Uint8Array | Uint8ClampedArray,
  width: number,
  height: number,
): Promise<Uint8Array | null> {
  if (typeof navigator === "undefined" || !(navigator as unknown as { gpu?: unknown }).gpu) {
    return null;
  }
  const m = await import("@/components/editor/wasmExport").then((mod) => mod.getWasmExportModule());
  if (!m?.WebGpuAdjustRenderer) return null;
  try {
    if (!invertPoaCache || invertPoaCache.w !== width || invertPoaCache.h !== height) {
      invertPoaCache = {
        w: width,
        h: height,
        renderer: await m.WebGpuAdjustRenderer.create(width, height),
      };
    }
    console.log(`[invert] WebGPU A used ${width}x${height}`);
    return await invertPoaCache.renderer.invert(new Uint8Array(pixels));
  } catch (err) {
    console.warn("[invert] PoA path failed, falling back:", err);
    invertPoaCache = null;
    return null;
  }
}

// Public: invert RGBA, preferring GPU, falling back to CPU on any absence/error.
// Tier order: Rust PoA (Technique A) -> TS WGSL -> CPU. Width/height enable the
// PoA tier (renderer is size-bound); omitting them skips straight to TS/CPU.
export async function invertRgba(
  pixels: Uint8Array | Uint8ClampedArray,
  width?: number,
  height?: number,
): Promise<GpuRunResult> {
  if (width !== undefined && height !== undefined && width * height * 4 === pixels.length) {
    const poa = await tryInvertRgbaPoa(pixels, width, height);
    if (poa) return { data: poa, usedGpu: true };
  }
  if (isGpuComputeAvailable()) {
    try {
      return { data: await invertRgbaGpu(pixels), usedGpu: true };
    } catch {
      // GPU path failed at runtime -> safe CPU fallback
    }
  }
  return { data: invertRgbaCpu(pixels), usedGpu: false };
}

export interface BasicAdjustmentInput {
  brightness: number;
  contrast: number;
  saturation: number;
}

// Brightness/Contrast/Saturation CPU pass. Inlined (mirrors
// engine/layerAdjustments.applyAdjustmentToRgb) to keep lib/gpu dependency-free;
// gpuCompute.test.ts asserts it stays bit-exact with applyBasicAdjustmentToPixelsTs.
export function adjustRgbaCpu(
  pixels: Uint8Array | Uint8ClampedArray,
  adj: BasicAdjustmentInput,
): Uint8Array {
  const src = pixels instanceof Uint8ClampedArray ? pixels : new Uint8ClampedArray(pixels);
  const out = new Uint8ClampedArray(src.length);
  const b = Math.max(-100, Math.min(100, adj.brightness));
  const c = Math.max(-100, Math.min(100, adj.contrast));
  const s = Math.max(-100, Math.min(100, adj.saturation));
  const contrastFactor = (259 * (c + 255)) / (255 * (259 - c));
  const t = b / 100;
  const satFactor = 1 + s / 100;
  for (let i = 0; i < src.length; i += 4) {
    let cr = contrastFactor * (src[i] / 255 - 0.5) + 0.5;
    let cg = contrastFactor * (src[i + 1] / 255 - 0.5) + 0.5;
    let cb = contrastFactor * (src[i + 2] / 255 - 0.5) + 0.5;
    if (t >= 0) {
      cr = cr + (1 - cr) * t * 0.5;
      cg = cg + (1 - cg) * t * 0.5;
      cb = cb + (1 - cb) * t * 0.5;
    } else {
      const f = -t;
      cr = cr - cr * f * 0.5;
      cg = cg - cg * f * 0.5;
      cb = cb - cb * f * 0.5;
    }
    const lum = cr * 0.2126 + cg * 0.7152 + cb * 0.0722;
    cr = lum + (cr - lum) * satFactor;
    cg = lum + (cg - lum) * satFactor;
    cb = lum + (cb - lum) * satFactor;
    out[i] = cr * 255;
    out[i + 1] = cg * 255;
    out[i + 2] = cb * 255;
    out[i + 3] = src[i + 3];
  }
  return new Uint8Array(out);
}

async function adjustRgbaGpu(
  pixels: Uint8Array | Uint8ClampedArray,
  adj: BasicAdjustmentInput,
): Promise<Uint8Array> {
  const g = globalThis as unknown as {
    navigator: { gpu: any };
    GPUBufferUsage: {
      STORAGE: number; COPY_SRC: number; COPY_DST: number; MAP_READ: number; UNIFORM: number;
    };
    GPUMapMode: { READ: number };
  };
  const gpu = g.navigator.gpu;
  const adapter = await gpu.requestAdapter();
  if (!adapter) throw new Error("no WebGPU adapter");
  const device = await adapter.requestDevice();
  const src = new Uint8Array(pixels.buffer, pixels.byteOffset, pixels.byteLength).slice();
  const u32 = new Uint32Array(src.buffer);
  const usage = g.GPUBufferUsage.STORAGE | g.GPUBufferUsage.COPY_SRC | g.GPUBufferUsage.COPY_DST;
  const buf = device.createBuffer({ size: u32.byteLength, usage });
  device.queue.writeBuffer(buf, 0, u32);
  const pbuf = device.createBuffer({
    size: 16,
    usage: g.GPUBufferUsage.UNIFORM | g.GPUBufferUsage.COPY_DST,
  });
  device.queue.writeBuffer(pbuf, 0, new Float32Array([adj.brightness, adj.contrast, adj.saturation, 0]));
  const module = device.createShaderModule({ code: adjustWgsl });
  const pipeline = device.createComputePipeline({ layout: "auto", compute: { module, entryPoint: "main" } });
  const bind = device.createBindGroup({
    layout: pipeline.getBindGroupLayout(0),
    entries: [
      { binding: 0, resource: { buffer: buf } },
      { binding: 1, resource: { buffer: pbuf } },
    ],
  });
  const groups = Math.ceil((pixels.length >>> 2) / 256);
  const enc = device.createCommandEncoder();
  const pass = enc.beginComputePass();
  pass.setPipeline(pipeline);
  pass.setBindGroup(0, bind);
  pass.dispatchWorkgroups(groups);
  pass.end();
  device.queue.submit([enc.finish()]);
  const read = device.createBuffer({
    size: u32.byteLength,
    usage: g.GPUBufferUsage.COPY_DST | g.GPUBufferUsage.MAP_READ,
  });
  const enc2 = device.createCommandEncoder();
  enc2.copyBufferToBuffer(buf, 0, read, 0, u32.byteLength);
  device.queue.submit([enc2.finish()]);
  await read.mapAsync(g.GPUMapMode.READ);
  const mapped = new Uint8Array(read.getMappedRange().slice(0));
  read.unmap();
  return mapped;
}

// Public: B/C/S adjust, preferring GPU, falling back to CPU on any absence/error.
export async function adjustRgba(
  pixels: Uint8Array | Uint8ClampedArray,
  adj: BasicAdjustmentInput,
): Promise<GpuRunResult> {
  if (isGpuComputeAvailable()) {
    try {
      return { data: await adjustRgbaGpu(pixels, adj), usedGpu: true };
    } catch {
      // GPU path failed at runtime -> safe CPU fallback
    }
  }
  return { data: adjustRgbaCpu(pixels, adj), usedGpu: false };
}
