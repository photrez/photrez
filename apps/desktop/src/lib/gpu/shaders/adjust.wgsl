// Hand-written WGSL compute shader: Brightness/Contrast/Saturation on packed RGBA.
// One u32 per pixel (R=byte0..A=byte3). Mirrors engine/layerAdjustments.applyAdjustmentToRgb
// so the WGSL bake and the CPU reference agree. Alpha is preserved.
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
   let crC = cr;

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
   let crB = cr;

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
