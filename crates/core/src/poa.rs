// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Technique A — proof of concept (REAL path).
//
// Goal: prove Rust/WASM can OWN a WebGPU compute pipeline INSIDE the webview
// (no upload/readback across processes, no transparent overlay) and actually
// render adjusted pixels back to the CPU. This ports the project's proven TS
// GPU path (`apps/desktop/src/lib/gpu/gpuCompute.ts::adjustRgbaGpu`) 1:1 into
// Rust/web-sys, using the same WGSL (`shaders/adjust.wgsl`). The WGSL math is
// bit-exact with `engine/layerAdjustments.applyBasicAdjustmentToPixels`, so the
// C / TS / A comparison is apples-to-apples.
//
// web-sys exposes WebGPU types only when `web_sys_unstable_apis` is set; gate
// the whole module so host `cargo test` (which doesn't set that cfg) still
// compiles cleanly.
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
  let i = gid.x;
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

/// Rust-owned WebGPU adjustment renderer (Technique A). Holds the device,
/// pipeline, and per-size buffers so repeated `render` calls only pay
/// upload + dispatch + readback.
#[wasm_bindgen]
pub struct PoaRenderer {
    device: web_sys::GpuDevice,
    pipeline: web_sys::GpuComputePipeline,
    work_buffer: web_sys::GpuBuffer,
    param_buffer: web_sys::GpuBuffer,
    read_buffer: web_sys::GpuBuffer,
    byte_len: f64,
}

fn jserr(e: JsValue) -> JsValue {
    JsValue::from_str(&format!("poa: {:?}", e))
}

#[wasm_bindgen]
impl PoaRenderer {
    /// Create the renderer for a fixed `width` x `height` RGBA buffer.
    /// Returns an error string (not a panic) when WebGPU is unavailable.
    pub async fn create(width: u32, height: u32) -> Result<PoaRenderer, JsValue> {
        let win = window().ok_or_else(|| JsValue::from_str("poa: no window"))?;
        let gpu = win.navigator().gpu();

        let adapter_val = JsFuture::from(gpu.request_adapter()).await?;
        if adapter_val.is_null() || adapter_val.is_undefined() {
            return Err(JsValue::from_str("poa: no WebGPU adapter"));
        }
        let adapter: web_sys::GpuAdapter = adapter_val.unchecked_into();
        let device: web_sys::GpuDevice = JsFuture::from(adapter.request_device())
            .await?
            .unchecked_into();

        let byte_len = (width as f64) * (height as f64) * 4.0;

        let work_desc =
            web_sys::GpuBufferDescriptor::new(byte_len as u32, U_STORAGE | U_COPY_SRC | U_COPY_DST);
        let work_buffer = device.create_buffer(&work_desc).map_err(jserr)?;

        let param_desc = web_sys::GpuBufferDescriptor::new(16, U_UNIFORM | U_COPY_DST);
        let param_buffer = device.create_buffer(&param_desc).map_err(jserr)?;

        let read_desc = web_sys::GpuBufferDescriptor::new(byte_len as u32, U_MAP_READ | U_COPY_DST);
        let read_buffer = device.create_buffer(&read_desc).map_err(jserr)?;

        let module = device.create_shader_module(&GpuShaderModuleDescriptor::new(ADJUST_WGSL));
        let stage = GpuProgrammableStage::new(&module);
        stage.set_entry_point("main");
        let pipeline_desc = GpuComputePipelineDescriptor::new_with_gpu_auto_layout_mode(
            GpuAutoLayoutMode::Auto,
            &stage,
        );
        let pipeline = device.create_compute_pipeline(&pipeline_desc);

        Ok(PoaRenderer {
            device,
            pipeline,
            work_buffer,
            param_buffer,
            read_buffer,
            byte_len,
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
            &self.pipeline.get_bind_group_layout(0),
        ));

        let px = (self.byte_len as usize) / 4;
        let groups = ((px + 255) / 256) as u32;

        let enc = self.device.create_command_encoder();
        let pass = enc.begin_compute_pass();
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, Some(&bind_group));
        pass.dispatch_workgroups(groups);
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
