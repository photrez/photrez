// SPDX-License-Identifier: AGPL-3.0-or-later
// Minimal 4-pixel WGSL invert debug to verify correctness end-to-end.
const SHADER = /* wgsl */ `
@group(0) @binding(0) var<storage, read_write> data: array<u32>;
@compute @workgroup_size(1)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = gid.x;
  if (i >= arrayLength(&data)) { return; }
  let p = data[i];
  let r = p & 0xFFu;
  let g = (p >> 8u) & 0xFFu;
  let b = (p >> 16u) & 0xFFu;
  let a = (p >> 24u) & 0xFFu;
  data[i] = (a << 24u) | (((255u - b) << 16u) | (((255u - g) << 8u) | (255u - r)));
}
`;

const px = new Uint8Array([10, 20, 30, 255, 0, 0, 0, 255, 255, 255, 255, 255, 1, 2, 3, 4]);
console.log("input  bytes:", Array.from(px));

const gpu: any = (globalThis as any).navigator.gpu;
if (!gpu) { console.log("no navigator.gpu"); Deno.exit(1); }
const adapter = await gpu.requestAdapter();
const device = await adapter.requestDevice();
const n = px.length / 4;
const buf: any = device.createBuffer({ size: px.length, usage: GPUBufferUsage.STORAGE | GPUBufferUsage.COPY_DST | GPUBufferUsage.COPY_SRC });
device.queue.writeBuffer(buf, 0, px);
const module = device.createShaderModule({ code: SHADER });
const pipeline: any = device.createComputePipeline({ layout: "auto", compute: { module, entryPoint: "main" } });
const bind: any = device.createBindGroup({ layout: pipeline.getBindGroupLayout(0), entries: [{ binding: 0, resource: { buffer: buf } }] });
const enc = device.createCommandEncoder();
const pass = enc.beginComputePass();
pass.setPipeline(pipeline); pass.setBindGroup(0, bind);
pass.dispatchWorkgroups(n); pass.end();
device.queue.submit([enc.finish()]);
await device.queue.onSubmittedWorkDone();

const rb: any = device.createBuffer({ size: px.length, usage: GPUBufferUsage.MAP_READ | GPUBufferUsage.COPY_DST });
const enc2 = device.createCommandEncoder();
enc2.copyBufferToBuffer(buf, 0, rb, 0, px.length);
device.queue.submit([enc2.finish()]);
await rb.mapAsync(GPUMapMode.READ);
const out = new Uint8Array(rb.getMappedRange().slice(0));
rb.unmap();
console.log("output bytes:", Array.from(out));
const exp = new Uint8Array(px.length);
for (let i = 0; i < n; i++) {
  exp[i*4] = 255 - px[i*4]; exp[i*4+1] = 255 - px[i*4+1]; exp[i*4+2] = 255 - px[i*4+2]; exp[i*4+3] = px[i*4+3];
}
console.log("expect bytes:", Array.from(exp));
console.log("MATCH:", JSON.stringify(Array.from(out)) === JSON.stringify(Array.from(exp)));
