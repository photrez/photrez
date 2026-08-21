// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Technique A: Rust/WASM owning WebGPU compute pipelines inside the webview.
// Ports the project's proven TS GPU paths (gpuCompute.ts::adjustRgbaGpu / invertRgbaGpu)
// 1:1 into Rust/web-sys, using the same WGSL (shaders/*.wgsl). Bit-exact with
// engine/layerAdjustments and gpuCompute CPU references.

#![cfg(web_sys_unstable_apis)]

use js_sys::Uint8Array;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{
    window, GpuAutoLayoutMode, GpuBindGroupDescriptor, GpuBindGroupEntry, GpuBufferBinding,
    GpuComputePipelineDescriptor, GpuProgrammableStage, GpuShaderModuleDescriptor,
};

// WebGPU buffer usage flags (raw u32 — web-sys exposes these as const-enums
// behind separate features, but the numeric values are stable and simpler).
const U_STORAGE: u32 = 0x80; // STORAGE
const U_COPY_SRC: u32 = 0x4; // COPY_SRC
const U_COPY_DST: u32 = 0x8; // COPY_DST
const U_MAP_READ: u32 = 0x1; // MAP_READ
const U_UNIFORM: u32 = 0x40; // UNIFORM

// Identical to apps/desktop/src/lib/gpu/shaders/adjust.wgsl (B/C/S on packed RGBA).
const ADJUST_WGSL: &str = r#"
struct Params { brightness: f32, contrast: f32, saturation: f32, _pad: f32 };
@group(0) @binding(0) var<storage, read_write> data: array<u32>;
@group(0) @binding(1) var<uniform> params: Params;
@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = gid.x + gid.y * 65535u * 256u;
  if (i >= arrayLength(&data)) { return; }
  let p = data[i];
  let r = f32(p & 0xFFu) / 255.0;
  let g = f32((p >> 8u) & 0xFFu) / 255.0;
  let b = f32((p >> 16u) & 0xFFu) / 255.0;
  let contrastFactor = (259.0 * (params.contrast + 255.0)) / (255.0 * (259.0 - params.contrast));
  var cr = contrastFactor * (r - 0.5) + 0.5;
  var cg = contrastFactor * (g - 0.5) + 0.5;
  var cb = contrastFactor * (b - 0.5) + 0.5;
  let t = params.brightness / 100.0;
  if (t >= 0.0) {
    cr = cr + (1.0 - cr) * t * 0.5;
    cg = cg + (1.0 - cg) * t * 0.5;
    cb = cb + (1.0 - cb) * t * 0.5;
  } else {
    let f = -t;
    cr = cr - cr * f * 0.5;
    cg = cg - cg * f * 0.5;
    cb = cb - cb * f * 0.5;
  }
  let luminance = cr * 0.2126 + cg * 0.7152 + cb * 0.0722;
  let satFactor = 1.0 + params.saturation / 100.0;
  cr = luminance + (cr - luminance) * satFactor;
  cg = luminance + (cg - luminance) * satFactor;
  cb = luminance + (cb - luminance) * satFactor;
  cr = clamp(cr, 0.0, 1.0);
  cg = clamp(cg, 0.0, 1.0);
  cb = clamp(cb, 0.0, 1.0);
  let ro = u32(clamp(round(cr * 255.0), 0.0, 255.0));
  let go = u32(clamp(round(cg * 255.0), 0.0, 255.0));
  let bo = u32(clamp(round(cb * 255.0), 0.0, 255.0));
  let a = p & 0xFF000000u;
  data[i] = a | (bo << 16u) | (go << 8u) | ro;
}
"#;

// Identical to apps/desktop/src/lib/gpu/shaders/invert.wgsl (invert RGB, keep A).
const INVERT_WGSL: &str = r#"
@group(0) @binding(0) var<storage, read_write> data: array<u32>;
@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = gid.x + gid.y * 65535u * 256u;
  if (i >= arrayLength(&data)) { return; }
  let p = data[i];
  let r = p & 0xFFu;
  let g = (p >> 8u) & 0xFFu;
  let b = (p >> 16u) & 0xFFu;
  let a = (p >> 24u) & 0xFFu;
  data[i] = (a << 24u) | (((255u - b) << 16u) | (((255u - g) << 8u) | (255u - r)));
}
"#;

// Gradient fill (linear 2-stop, no mask) — mirrors features/fill/fillOperations.ts gradientFillTs
// for the common case. 20 f32 uniform (80 bytes, 16-aligned): width,height,ax,ay,bx,by,c0(5),c1(5),grad_type + pad.
const GRADIENT_WGSL: &str = r#"
struct GradParams {
  width: f32, height: f32, ax: f32, ay: f32,
  bx: f32, by: f32, c0_offset: f32, c0_r: f32,
  c0_g: f32, c0_b: f32, c0_a: f32, c1_offset: f32,
  c1_r: f32, c1_g: f32, c1_b: f32, c1_a: f32,
  grad_type: f32, _pad: f32, _pad2: f32, _pad3: f32,
};
@group(0) @binding(0) var<storage, read_write> data: array<u32>;
@group(0) @binding(1) var<uniform> p: GradParams;
fn lerpStops(t: f32) -> vec4<f32> {
  if (t <= p.c0_offset) { return vec4<f32>(p.c0_r, p.c0_g, p.c0_b, p.c0_a); }
  if (t >= p.c1_offset) { return vec4<f32>(p.c1_r, p.c1_g, p.c1_b, p.c1_a); }
  let range = p.c1_offset - p.c0_offset;
  let f = select(0.0, (t - p.c0_offset) / range, range > 0.0);
  return vec4<f32>(
    p.c0_r + (p.c1_r - p.c0_r) * f,
    p.c0_g + (p.c1_g - p.c0_g) * f,
    p.c0_b + (p.c1_b - p.c0_b) * f,
    p.c0_a + (p.c1_a - p.c0_a) * f,
  );
}
@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = gid.x + gid.y * 65535u * 256u;
  if (i >= arrayLength(&data)) { return; }
  let w = u32(p.width);
  let px = f32(i % w);
  let py = f32(i / w);
  let dx = p.bx - p.ax;
  let dy = p.by - p.ay;
  let lenSq = dx*dx + dy*dy;
  var t: f32;
  if (p.grad_type < 0.5) {
    t = select(0.0, ((px - p.ax)*dx + (py - p.ay)*dy) / lenSq, lenSq != 0.0);
  } else {
    let pdx = px - p.ax; let pdy = py - p.ay;
    let dist = sqrt(pdx*pdx + pdy*pdy);
    let radius = sqrt(lenSq);
    t = select(0.0, dist / radius, radius > 0.0);
  }
  t = clamp(t, 0.0, 1.0);
  let c = lerpStops(t);
  let r = u32(clamp(round(c.x), 0.0, 255.0));
  let g = u32(clamp(round(c.y), 0.0, 255.0));
  let b = u32(clamp(round(c.z), 0.0, 255.0));
  let a = u32(clamp(round(c.w), 0.0, 255.0));
  data[i] = (a << 24u) | (b << 16u) | (g << 8u) | r;
}
"#;

const FLOOD_GLOBAL_WGSL: &str = r#"
struct FloodParams {
  width: f32, height: f32,
  sr: f32, sg: f32, sb: f32, sa: f32,
  fr: f32, fg: f32, fb: f32, fa: f32,
  tolSq: f32, maskX: f32, maskY: f32, maskW: f32,
  maskH: f32, maskShape: f32, hasMask: f32, inverted: f32,
};
@group(0) @binding(0) var<storage, read_write> data: array<u32>;
@group(0) @binding(1) var<uniform> p: FloodParams;
fn isInsideMask(px: f32, py: f32) -> bool {
  if (p.hasMask < 0.5) { return true; }
  if (px < p.maskX || px >= p.maskX + p.maskW || py < p.maskY || py >= p.maskY + p.maskH) {
    return p.inverted > 0.5;
  }
  if (p.maskShape > 0.5) {
    let hw = p.maskW / 2.0; let hh = p.maskH / 2.0;
    if (hw <= 0.0 || hh <= 0.0) { return false; }
    let nx = (px - (p.maskX + hw)) / hw;
    let ny = (py - (p.maskY + hh)) / hh;
    let inside = nx*nx + ny*ny <= 1.0;
    return select(inside, !inside, p.inverted > 0.5);
  }
  return p.inverted < 0.5;
}
@compute @workgroup_size(256)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
  let i = gid.x + gid.y * 65535u * 256u;
  if (i >= arrayLength(&data)) { return; }
  let w = u32(p.width);
  let px = f32(i % w);
  let py = f32(i / w);
  if (!isInsideMask(px, py)) { return; }
  let packed = data[i];
  let r = f32(packed & 0xFFu);
  let g = f32((packed >> 8u) & 0xFFu);
  let b = f32((packed >> 16u) & 0xFFu);
  let a = f32((packed >> 24u) & 0xFFu);
  let dr = r - p.sr; let dg = g - p.sg; let db = b - p.sb; let da = a - p.sa;
  if (dr*dr + dg*dg + db*db + da*da <= p.tolSq) {
    let fr = u32(p.fr); let fg = u32(p.fg); let fb = u32(p.fb); let fa = u32(p.fa);
    data[i] = (fa << 24u) | (fb << 16u) | (fg << 8u) | fr;
  }
}
"#;

/// Rust-owned WebGPU renderer (Technique A). Holds the device, pipelines,
/// and per-size buffers so repeated `render`/`invert`/`gradient` calls only pay
/// upload + dispatch + readback.
#[wasm_bindgen]
pub struct WebGpuAdjustRenderer {
    device: web_sys::GpuDevice,
    adjust_pipeline: web_sys::GpuComputePipeline,
    invert_pipeline: web_sys::GpuComputePipeline,
    gradient_pipeline: web_sys::GpuComputePipeline,
    flood_pipeline: web_sys::GpuComputePipeline,
    work_buffer: web_sys::GpuBuffer,
    param_buffer: web_sys::GpuBuffer,
    grad_buffer: web_sys::GpuBuffer,
    flood_buffer: web_sys::GpuBuffer,
    read_buffer: web_sys::GpuBuffer,
    byte_len: f64,
    width: u32,
    height: u32,
    max_dispatch_x: u32,
}

fn jserr(e: JsValue) -> JsValue {
    JsValue::from_str(&format!("webgpu: {:?}", e))
}

#[wasm_bindgen]
impl WebGpuAdjustRenderer {
    /// Create the renderer for a fixed `width` x `height` RGBA buffer.
    /// Returns an error string (not a panic) when WebGPU is unavailable.
    pub async fn create(width: u32, height: u32) -> Result<WebGpuAdjustRenderer, JsValue> {
        let win = window().ok_or_else(|| JsValue::from_str("webgpu: no window"))?;
        let gpu = win.navigator().gpu();

        let adapter_val = JsFuture::from(gpu.request_adapter()).await?;
        if adapter_val.is_null() || adapter_val.is_undefined() {
            return Err(JsValue::from_str("webgpu: no WebGPU adapter"));
        }
        let adapter: web_sys::GpuAdapter = adapter_val.unchecked_into();
        let device: web_sys::GpuDevice = JsFuture::from(adapter.request_device())
            .await?
            .unchecked_into();

        let byte_len = (width as f64) * (height as f64) * 4.0;

        let limits = adapter.limits();
        let max_dispatch_x = limits.max_compute_workgroups_per_dimension();
        let max_buf = limits.max_storage_buffer_binding_size();
        // Best practice: check single buffer size against maxStorageBufferBindingSize (often 128MB)
        // If the image is too large for a single storage buffer, fallback to CPU (handled by JS caller)
        // This avoids Invalid Buffer errors on super-large images (e.g. 8000×8000 = 256MB > 128MB).
        if byte_len > max_buf {
            return Err(JsValue::from_str(&format!(
                "webgpu: image too large for single buffer ({} bytes > maxStorageBufferBindingSize {}), tiling required — fallback to CPU",
                byte_len as u64, max_buf as u64
            )));
        }

        let work_desc =
            web_sys::GpuBufferDescriptor::new(byte_len as u32, U_STORAGE | U_COPY_SRC | U_COPY_DST);
        let work_buffer = device.create_buffer(&work_desc).map_err(jserr)?;

        let param_desc = web_sys::GpuBufferDescriptor::new(16, U_UNIFORM | U_COPY_DST);
        let param_buffer = device.create_buffer(&param_desc).map_err(jserr)?;

        let grad_desc = web_sys::GpuBufferDescriptor::new(80, U_UNIFORM | U_COPY_DST);
        let grad_buffer = device.create_buffer(&grad_desc).map_err(jserr)?;

        let flood_desc_buf = web_sys::GpuBufferDescriptor::new(80, U_UNIFORM | U_COPY_DST);
        let flood_buffer = device.create_buffer(&flood_desc_buf).map_err(jserr)?;

        let read_desc = web_sys::GpuBufferDescriptor::new(byte_len as u32, U_MAP_READ | U_COPY_DST);
        let read_buffer = device.create_buffer(&read_desc).map_err(jserr)?;

        // Generate WGSL with the actual max_dispatch_x so the shader's stride matches the dispatch
        let stride = max_dispatch_x as u32 * 256u32;
        let adjust_wgsl =
            ADJUST_WGSL.replace("65535u * 256u", &format!("{}u * 256u", max_dispatch_x));
        let invert_wgsl =
            INVERT_WGSL.replace("65535u * 256u", &format!("{}u * 256u", max_dispatch_x));
        let grad_wgsl =
            GRADIENT_WGSL.replace("65535u * 256u", &format!("{}u * 256u", max_dispatch_x));
        let flood_wgsl =
            FLOOD_GLOBAL_WGSL.replace("65535u * 256u", &format!("{}u * 256u", max_dispatch_x));
        // Fallback: if the const WGSL didn't contain the placeholder (e.g. already updated), use as-is
        let _ = stride; // keep for future tiling logic

        let adjust_module =
            device.create_shader_module(&GpuShaderModuleDescriptor::new(&adjust_wgsl));
        let adjust_stage = GpuProgrammableStage::new(&adjust_module);
        adjust_stage.set_entry_point("main");
        let adjust_desc = GpuComputePipelineDescriptor::new_with_gpu_auto_layout_mode(
            GpuAutoLayoutMode::Auto,
            &adjust_stage,
        );
        let adjust_pipeline = device.create_compute_pipeline(&adjust_desc);

        let invert_module =
            device.create_shader_module(&GpuShaderModuleDescriptor::new(&invert_wgsl));
        let invert_stage = GpuProgrammableStage::new(&invert_module);
        invert_stage.set_entry_point("main");
        let invert_desc = GpuComputePipelineDescriptor::new_with_gpu_auto_layout_mode(
            GpuAutoLayoutMode::Auto,
            &invert_stage,
        );
        let invert_pipeline = device.create_compute_pipeline(&invert_desc);

        let grad_module = device.create_shader_module(&GpuShaderModuleDescriptor::new(&grad_wgsl));
        let grad_stage = GpuProgrammableStage::new(&grad_module);
        grad_stage.set_entry_point("main");
        let grad_desc = GpuComputePipelineDescriptor::new_with_gpu_auto_layout_mode(
            GpuAutoLayoutMode::Auto,
            &grad_stage,
        );
        let gradient_pipeline = device.create_compute_pipeline(&grad_desc);

        let flood_module =
            device.create_shader_module(&GpuShaderModuleDescriptor::new(&flood_wgsl));
        let flood_stage = GpuProgrammableStage::new(&flood_module);
        flood_stage.set_entry_point("main");
        let flood_desc = GpuComputePipelineDescriptor::new_with_gpu_auto_layout_mode(
            GpuAutoLayoutMode::Auto,
            &flood_stage,
        );
        let flood_pipeline = device.create_compute_pipeline(&flood_desc);

        Ok(WebGpuAdjustRenderer {
            device,
            adjust_pipeline,
            invert_pipeline,
            gradient_pipeline,
            flood_pipeline,
            work_buffer,
            param_buffer,
            grad_buffer,
            flood_buffer,
            read_buffer,
            byte_len,
            width,
            height,
            max_dispatch_x,
        })
    }

    /// Apply brightness/contrast/saturation to `src` on the GPU and return the
    /// adjusted RGBA bytes (alpha preserved). `src` length must be width*height*4.
    pub async fn render(
        &self,
        src: &Uint8Array,
        brightness: f64,
        contrast: f64,
        saturation: f64,
    ) -> Result<Uint8Array, JsValue> {
        let input = src.to_vec();
        self.device
            .queue()
            .write_buffer_with_f64_and_u8_slice(&self.work_buffer, 0.0, &input)
            .map_err(jserr)?;

        let p = [
            brightness as f32,
            contrast as f32,
            saturation as f32,
            0.0f32,
        ];
        let mut pbytes = Vec::with_capacity(16);
        for v in p {
            pbytes.extend_from_slice(&v.to_le_bytes());
        }
        self.device
            .queue()
            .write_buffer_with_f64_and_u8_slice(&self.param_buffer, 0.0, &pbytes)
            .map_err(jserr)?;

        let entries = [
            GpuBindGroupEntry::new_with_gpu_buffer_binding(
                0,
                &GpuBufferBinding::new(&self.work_buffer),
            ),
            GpuBindGroupEntry::new_with_gpu_buffer_binding(
                1,
                &GpuBufferBinding::new(&self.param_buffer),
            ),
        ];
        let bind_group = self.device.create_bind_group(&GpuBindGroupDescriptor::new(
            &entries,
            &self.adjust_pipeline.get_bind_group_layout(0),
        ));

        let px = (self.byte_len as usize) / 4;
        let groups = ((px + 255) / 256) as u32;
        let dispatch_x = groups.min(self.max_dispatch_x);
        let dispatch_y = (groups + self.max_dispatch_x - 1) / self.max_dispatch_x;

        let enc = self.device.create_command_encoder();
        let pass = enc.begin_compute_pass();
        pass.set_pipeline(&self.adjust_pipeline);
        pass.set_bind_group(0, Some(&bind_group));
        if dispatch_y == 1 {
            pass.dispatch_workgroups(dispatch_x);
        } else {
            pass.dispatch_workgroups_with_workgroup_count_y(dispatch_x, dispatch_y);
        }
        pass.end();
        self.device.queue().submit(&[enc.finish()]);

        let enc2 = self.device.create_command_encoder();
        enc2.copy_buffer_to_buffer_with_f64_and_f64_and_f64(
            &self.work_buffer,
            0.0,
            &self.read_buffer,
            0.0,
            self.byte_len,
        )
        .map_err(jserr)?;
        self.device.queue().submit(&[enc2.finish()]);

        let _ = JsFuture::from(self.read_buffer.map_async(U_MAP_READ)).await?;
        let ab = self.read_buffer.get_mapped_range().map_err(jserr)?;
        let out = Uint8Array::new(&ab).to_vec();
        self.read_buffer.unmap();
        Ok(Uint8Array::from(&out[..]))
    }

    /// Invert RGB (keep A) on the GPU and return the inverted RGBA bytes.
    /// `src` length must be width*height*4.
    pub async fn invert(&self, src: &Uint8Array) -> Result<Uint8Array, JsValue> {
        let input = src.to_vec();
        self.device
            .queue()
            .write_buffer_with_f64_and_u8_slice(&self.work_buffer, 0.0, &input)
            .map_err(jserr)?;

        let entries = [GpuBindGroupEntry::new_with_gpu_buffer_binding(
            0,
            &GpuBufferBinding::new(&self.work_buffer),
        )];
        let bind_group = self.device.create_bind_group(&GpuBindGroupDescriptor::new(
            &entries,
            &self.invert_pipeline.get_bind_group_layout(0),
        ));

        let px = (self.byte_len as usize) / 4;
        let groups = ((px + 255) / 256) as u32;
        let dispatch_x = groups.min(self.max_dispatch_x);
        let dispatch_y = (groups + self.max_dispatch_x - 1) / self.max_dispatch_x;

        let enc = self.device.create_command_encoder();
        let pass = enc.begin_compute_pass();
        pass.set_pipeline(&self.invert_pipeline);
        pass.set_bind_group(0, Some(&bind_group));
        if dispatch_y == 1 {
            pass.dispatch_workgroups(dispatch_x);
        } else {
            pass.dispatch_workgroups_with_workgroup_count_y(dispatch_x, dispatch_y);
        }
        pass.end();
        self.device.queue().submit(&[enc.finish()]);

        let enc2 = self.device.create_command_encoder();
        enc2.copy_buffer_to_buffer_with_f64_and_f64_and_f64(
            &self.work_buffer,
            0.0,
            &self.read_buffer,
            0.0,
            self.byte_len,
        )
        .map_err(jserr)?;
        self.device.queue().submit(&[enc2.finish()]);

        let _ = JsFuture::from(self.read_buffer.map_async(U_MAP_READ)).await?;
        let ab = self.read_buffer.get_mapped_range().map_err(jserr)?;
        let out = Uint8Array::new(&ab).to_vec();
        self.read_buffer.unmap();
        Ok(Uint8Array::from(&out[..]))
    }

    /// Gradient fill (2-stop linear, no mask) on the GPU. `src` is ignored (overwritten).
    /// `grad_type: 0 = linear, 1 = radial`.
    pub async fn gradient(
        &self,
        src: &Uint8Array,
        ax: f64,
        ay: f64,
        bx: f64,
        by: f64,
        c0_offset: f64,
        c0_r: f64,
        c0_g: f64,
        c0_b: f64,
        c0_a: f64,
        c1_offset: f64,
        c1_r: f64,
        c1_g: f64,
        c1_b: f64,
        c1_a: f64,
        grad_type: f64,
    ) -> Result<Uint8Array, JsValue> {
        let input = src.to_vec();
        self.device
            .queue()
            .write_buffer_with_f64_and_u8_slice(&self.work_buffer, 0.0, &input)
            .map_err(jserr)?;

        let width = self.width as f32;
        let height = self.height as f32;

        let mut gbytes = Vec::with_capacity(80);
        for v in [
            width,
            height,
            ax as f32,
            ay as f32,
            bx as f32,
            by as f32,
            c0_offset as f32,
            c0_r as f32,
            c0_g as f32,
            c0_b as f32,
            c0_a as f32,
            c1_offset as f32,
            c1_r as f32,
            c1_g as f32,
            c1_b as f32,
            c1_a as f32,
            grad_type as f32,
            0.0f32,
            0.0f32,
            0.0f32,
        ] {
            gbytes.extend_from_slice(&v.to_le_bytes());
        }
        self.device
            .queue()
            .write_buffer_with_f64_and_u8_slice(&self.grad_buffer, 0.0, &gbytes)
            .map_err(jserr)?;

        let entries = [
            GpuBindGroupEntry::new_with_gpu_buffer_binding(
                0,
                &GpuBufferBinding::new(&self.work_buffer),
            ),
            GpuBindGroupEntry::new_with_gpu_buffer_binding(
                1,
                &GpuBufferBinding::new(&self.grad_buffer),
            ),
        ];
        let bind_group = self.device.create_bind_group(&GpuBindGroupDescriptor::new(
            &entries,
            &self.gradient_pipeline.get_bind_group_layout(0),
        ));

        let px = (self.byte_len as usize) / 4;
        let groups = ((px + 255) / 256) as u32;
        let dispatch_x = groups.min(self.max_dispatch_x);
        let dispatch_y = (groups + self.max_dispatch_x - 1) / self.max_dispatch_x;

        let enc = self.device.create_command_encoder();
        let pass = enc.begin_compute_pass();
        pass.set_pipeline(&self.gradient_pipeline);
        pass.set_bind_group(0, Some(&bind_group));
        if dispatch_y == 1 {
            pass.dispatch_workgroups(dispatch_x);
        } else {
            pass.dispatch_workgroups_with_workgroup_count_y(dispatch_x, dispatch_y);
        }
        pass.end();
        self.device.queue().submit(&[enc.finish()]);

        let enc2 = self.device.create_command_encoder();
        enc2.copy_buffer_to_buffer_with_f64_and_f64_and_f64(
            &self.work_buffer,
            0.0,
            &self.read_buffer,
            0.0,
            self.byte_len,
        )
        .map_err(jserr)?;
        self.device.queue().submit(&[enc2.finish()]);

        let _ = JsFuture::from(self.read_buffer.map_async(U_MAP_READ)).await?;
        let ab = self.read_buffer.get_mapped_range().map_err(jserr)?;
        let out = Uint8Array::new(&ab).to_vec();
        self.read_buffer.unmap();
        Ok(Uint8Array::from(&out[..]))
    }

    /// Flood fill global replace (non-contiguous, mask-aware) — per-pixel parallel.
    pub async fn flood_global(
        &self,
        src: &Uint8Array,
        sr: f64,
        sg: f64,
        sb: f64,
        sa: f64,
        fr: f64,
        fg: f64,
        fb: f64,
        fa: f64,
        tol_sq: f64,
        mask_x: f64,
        mask_y: f64,
        mask_w: f64,
        mask_h: f64,
        mask_shape: f64,
        has_mask: f64,
        inverted: f64,
    ) -> Result<Uint8Array, JsValue> {
        let input = src.to_vec();
        self.device
            .queue()
            .write_buffer_with_f64_and_u8_slice(&self.work_buffer, 0.0, &input)
            .map_err(jserr)?;

        let width = self.width as f32;
        let height = self.height as f32;
        let mut fbytes = Vec::with_capacity(80);
        for v in [
            width,
            height,
            sr as f32,
            sg as f32,
            sb as f32,
            sa as f32,
            fr as f32,
            fg as f32,
            fb as f32,
            fa as f32,
            tol_sq as f32,
            mask_x as f32,
            mask_y as f32,
            mask_w as f32,
            mask_h as f32,
            mask_shape as f32,
            has_mask as f32,
            inverted as f32,
            0.0,
            0.0,
        ] {
            fbytes.extend_from_slice(&v.to_le_bytes());
        }
        self.device
            .queue()
            .write_buffer_with_f64_and_u8_slice(&self.flood_buffer, 0.0, &fbytes)
            .map_err(jserr)?;

        let entries = [
            GpuBindGroupEntry::new_with_gpu_buffer_binding(
                0,
                &GpuBufferBinding::new(&self.work_buffer),
            ),
            GpuBindGroupEntry::new_with_gpu_buffer_binding(
                1,
                &GpuBufferBinding::new(&self.flood_buffer),
            ),
        ];
        let bind_group = self.device.create_bind_group(&GpuBindGroupDescriptor::new(
            &entries,
            &self.flood_pipeline.get_bind_group_layout(0),
        ));

        let px = (self.byte_len as usize) / 4;
        let groups = ((px + 255) / 256) as u32;
        let dispatch_x = groups.min(self.max_dispatch_x);
        let dispatch_y = (groups + self.max_dispatch_x - 1) / self.max_dispatch_x;

        let enc = self.device.create_command_encoder();
        let pass = enc.begin_compute_pass();
        pass.set_pipeline(&self.flood_pipeline);
        pass.set_bind_group(0, Some(&bind_group));
        if dispatch_y == 1 {
            pass.dispatch_workgroups(dispatch_x);
        } else {
            pass.dispatch_workgroups_with_workgroup_count_y(dispatch_x, dispatch_y);
        }
        pass.end();
        self.device.queue().submit(&[enc.finish()]);

        let enc2 = self.device.create_command_encoder();
        enc2.copy_buffer_to_buffer_with_f64_and_f64_and_f64(
            &self.work_buffer,
            0.0,
            &self.read_buffer,
            0.0,
            self.byte_len,
        )
        .map_err(jserr)?;
        self.device.queue().submit(&[enc2.finish()]);

        let _ = JsFuture::from(self.read_buffer.map_async(U_MAP_READ)).await?;
        let ab = self.read_buffer.get_mapped_range().map_err(jserr)?;
        let out = Uint8Array::new(&ab).to_vec();
        self.read_buffer.unmap();
        Ok(Uint8Array::from(&out[..]))
    }
}
