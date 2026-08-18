// Hand-written WGSL compute shader: invert RGBA.
// One u32 per pixel (little-endian: R=byte0..A=byte3). Invert R,G,B; keep A.
@group(0) @binding(0) var<storage, read_write> data: array<u32>;

@compute @workgroup_size(256)
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
