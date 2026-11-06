//! Authoritative brush-tip construction + defined resampler.
//!
//! Implements the reference tip-mask and hardness-profile semantics verbatim as
//! explicit Rust contract code, then defines the previously scale-operation-owned
//! upscale step as a deterministic bilinear-premultiplied half-pixel resampler.
//! No production wiring yet: validated offline against captured golden
//! tip rasters via the env-gated test in `brush/tests/tip.rs`.

pub const BRUSH_HARD_EDGE_THRESHOLD: f64 = 0.97;
/// Alpha contour the paint cursor ring marks (reference hardness profile).
pub const BRUSH_CURSOR_ALPHA_CONTOUR: f64 = 0.2;
pub const MIN_RELIABLE_BRUSH_DIAMETER_PX: f64 = 22.0;
pub const MIN_VISIBLE_ALPHA_8BIT: f64 = 0.5 / 255.0;
const HARDNESS_POINTS: [f64; 7] = [0.0, 0.1, 0.25, 0.5, 0.75, 0.9, 1.0];
const SIGMA_POINTS: [f64; 7] = [0.661, 0.738, 0.83, 0.935, 0.99, 1.004, 1.006];
const N_POINTS: [f64; 7] = [2.0, 2.68, 4.07, 8.23, 20.22, 51.2, 60.0];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProfileParams {
    pub sigma: f64,
    pub n: f64,
}

/// Fritsch-Carlson monotone cubic — reproduces the reference interpolator's
/// operations one-for-one.
struct MonotoneCubic {
    xs: Vec<f64>,
    ys: Vec<f64>,
    ms: Vec<f64>,
}

impl MonotoneCubic {
    fn new(xs: &[f64], ys: &[f64]) -> Self {
        let mut dxs = Vec::with_capacity(xs.len() - 1);
        let mut slopes = Vec::with_capacity(xs.len() - 1);
        for i in 0..xs.len() - 1 {
            let dx = xs[i + 1] - xs[i];
            dxs.push(dx);
            slopes.push((ys[i + 1] - ys[i]) / dx);
        }
        let mut ms = vec![slopes[0]];
        for i in 0..slopes.len() - 1 {
            let s0 = slopes[i];
            let s1 = slopes[i + 1];
            if s0 * s1 <= 0.0 {
                ms.push(0.0);
            } else {
                let d0 = dxs[i];
                let d1 = dxs[i + 1];
                let c = d0 + d1;
                ms.push((3.0 * c) / ((c + d1) / s0 + (c + d0) / s1));
            }
        }
        ms.push(slopes[slopes.len() - 1]);
        Self {
            xs: xs.to_vec(),
            ys: ys.to_vec(),
            ms,
        }
    }

    fn interpolate(&self, x: f64) -> f64 {
        let last = self.xs.len() - 1;
        if x <= self.xs[0] {
            return self.ys[0];
        }
        if x >= self.xs[last] {
            return self.ys[last];
        }
        let mut i = 0usize;
        while x > self.xs[i + 1] {
            i += 1;
        }
        let h = self.xs[i + 1] - self.xs[i];
        let t = (x - self.xs[i]) / h;
        let t2 = t * t;
        let t3 = t2 * t;
        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;
        h00 * self.ys[i] + h10 * h * self.ms[i] + h01 * self.ys[i + 1] + h11 * h * self.ms[i + 1]
    }
}

fn clamp01(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}

fn clamp_hardness(h: f64) -> f64 {
    if h.is_finite() {
        h.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

static SIGMA_CURVE: std::sync::OnceLock<MonotoneCubic> = std::sync::OnceLock::new();
static N_CURVE: std::sync::OnceLock<MonotoneCubic> = std::sync::OnceLock::new();

fn curves() -> (&'static MonotoneCubic, &'static MonotoneCubic) {
    (
        SIGMA_CURVE.get_or_init(|| MonotoneCubic::new(&HARDNESS_POINTS, &SIGMA_POINTS)),
        N_CURVE.get_or_init(|| MonotoneCubic::new(&HARDNESS_POINTS, &N_POINTS)),
    )
}

pub fn get_brush_profile_parameters(hardness: f64) -> ProfileParams {
    let h = clamp_hardness(hardness);
    let (sc, nc) = curves();
    ProfileParams {
        sigma: sc.interpolate(h),
        n: nc.interpolate(h),
    }
}

/// Super-gaussian soft profile — the reference brush-alpha curve.
pub fn brush_alpha(r_norm: f64, hardness: f64) -> f64 {
    let h = clamp_hardness(hardness);
    if h >= BRUSH_HARD_EDGE_THRESHOLD {
        return if r_norm <= 1.0 { 1.0 } else { 0.0 };
    }
    let p = get_brush_profile_parameters(h);
    let x = r_norm.max(0.0) / p.sigma;
    (-x.powf(p.n)).exp()
}

/// Radius multiplier that maps the nominal brush radius to the 20% alpha
/// contour.
pub fn get_brush_cursor_radius_scale(hardness: f64) -> f64 {
    let h = clamp_hardness(hardness);
    if h >= BRUSH_HARD_EDGE_THRESHOLD {
        return 1.0;
    }
    let p = get_brush_profile_parameters(h);
    (p.sigma * (-BRUSH_CURSOR_ALPHA_CONTOUR.ln()).powf(1.0 / p.n)).min(1.0)
}

pub fn get_brush_profile_support_norm(hardness: f64) -> f64 {
    let h = clamp_hardness(hardness);
    if h >= BRUSH_HARD_EDGE_THRESHOLD {
        return 1.0;
    }
    let p = get_brush_profile_parameters(h);
    (p.sigma * (-MIN_VISIBLE_ALPHA_8BIT.ln()).powf(1.0 / p.n)).max(1.0)
}

pub fn falloff_soft(v: f64) -> f64 {
    clamp01(v).powf(0.7)
}

fn small_round_alpha(distance: f64, radius: f64) -> f64 {
    clamp01(radius - distance.max(0.0) + 0.5)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Curve {
    Soft,
}

#[derive(Clone, Debug)]
pub struct BrushTip {
    /// Nominal diameter-space stamp size (the space production commits in).
    pub diameter: u32,
    /// Mask/data resolution (the `data_cap` policy); the production tip raster size.
    pub width: u32,
    pub height: u32,
    /// Straight-RGBA at width×height (mask colorized) — the pre-upscale bitmap.
    pub rgba_data_size: Vec<u8>,
    /// Straight-RGBA at diameter² produced by the defined bilinear-premul resampler.
    pub rgba_diameter: Vec<u8>,
    /// Premultiplied-RGBA at diameter² (pre-straight-conversion) — the true
    /// comparison domain for golden round-trip diagnostics (unpremul amplifies).
    pub rgba_diameter_premul: Vec<u8>,
}

pub struct TipSpec {
    pub size: f64,
    pub hardness: f64,
    pub curve: Curve,
    pub color: [u8; 3],
    pub data_cap: u32,
}

impl TipSpec {
    pub fn new(size: f64, hardness: f64, color: [u8; 3]) -> Self {
        Self {
            size,
            hardness,
            curve: Curve::Soft,
            color,
            data_cap: 256,
        }
    }
}

fn outer_radius(radius: f64, hardness: f64, curve: Curve) -> f64 {
    match curve {
        Curve::Soft => {
            if radius * 2.0 < MIN_RELIABLE_BRUSH_DIAMETER_PX {
                radius + 0.5
            } else {
                radius * get_brush_profile_support_norm(hardness)
            }
        }
    }
}

/// Rasterizes the tip mask to match the reference curve: f32 mask storage,
/// half-pixel offsets, scale=diameter/data_size, distance=sqrt, soft ⇒
/// `brush_alpha(dist/R_nominal,h)`, small/hard branches preserved.
fn rasterize_mask(
    diameter_nominal: f64,
    hardness: f64,
    curve: Curve,
    data_cap: u32,
) -> (u32, u32, Vec<f32>) {
    let _t0 = std::time::Instant::now();
    let h = hardness.clamp(0.0, 1.0);
    let r_nominal = diameter_nominal / 2.0;
    let support_norm = if diameter_nominal < MIN_RELIABLE_BRUSH_DIAMETER_PX
        || (curve == Curve::Soft && h >= BRUSH_HARD_EDGE_THRESHOLD)
    {
        1.0
    } else if curve == Curve::Soft {
        get_brush_profile_support_norm(h)
    } else {
        1.0
    };
    let diameter = ((r_nominal * support_norm).ceil() * 2.0 + 2.0) as u32;
    let data_size = diameter.min(data_cap);
    let center = diameter as f64 / 2.0;
    let scale = diameter as f64 / data_size as f64;
    let dsz = data_size as usize;
    let mut data = vec![0f32; dsz * dsz];
    let mode_small = diameter_nominal < MIN_RELIABLE_BRUSH_DIAMETER_PX
        || (curve == Curve::Soft && h >= BRUSH_HARD_EDGE_THRESHOLD);
    for dy in 0..dsz {
        let y_off = (dy as f64 + 0.5) * scale - center;
        let yy = y_off * y_off;
        for dx in 0..dsz {
            let xo = (dx as f64 + 0.5) * scale - center;
            let distance = (xo * xo + yy).sqrt();
            let alpha = if mode_small {
                small_round_alpha(distance, r_nominal)
            } else if curve == Curve::Soft {
                brush_alpha(distance / r_nominal, h)
            } else {
                // non-soft curves: falloff-based (kept for contract completeness)
                let outer = outer_radius(r_nominal, h, curve);
                if distance >= outer {
                    0.0
                } else if h >= 1.0 {
                    1.0
                } else {
                    let core = r_nominal * h;
                    let feather = (outer - core).max(0.0001);
                    falloff_soft(1.0 - (distance - core) / feather)
                }
            };
            data[dy * dsz + dx] = alpha as f32;
        }
    }
    let _ = _t0.elapsed();
    (diameter, data_size, data)
}

/// Bilinear resampler operating on PREMULTIPLIED pixels with
/// half-pixel centers, then emitted back to straight-RGBA (round-half unpremul).
/// This is the explicitly specified replacement for the 0.1 build's
/// filtered image upscale.
pub fn bilinear_resize_premul(src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> Vec<u8> {
    let mut out = vec![0u8; (dw * dh) as usize * 4];
    for dy in 0..dh {
        let sy = ((dy as f64 + 0.5) * sh as f64 / dh as f64 - 0.5).max(0.0);
        let y0 = sy.floor() as i64;
        let fy = sy - y0 as f64;
        let y0c = (y0.max(0) as u32).min(sh - 1);
        let y1c = ((y0 + 1).max(0) as u32).min(sh - 1);
        for dx in 0..dw {
            let sx = ((dx as f64 + 0.5) * sw as f64 / dw as f64 - 0.5).max(0.0);
            let x0 = sx.floor() as i64;
            let fx = sx - x0 as f64;
            let x0c = (x0.max(0) as u32).min(sw - 1);
            let x1c = ((x0 + 1).max(0) as u32).min(sw - 1);
            for ch in 0..4 {
                let p00 = src[(y0c as usize * sw as usize + x0c as usize) * 4 + ch] as f64;
                let p10 = src[(y0c as usize * sw as usize + x1c as usize) * 4 + ch] as f64;
                let p01 = src[(y1c as usize * sw as usize + x0c as usize) * 4 + ch] as f64;
                let p11 = src[(y1c as usize * sw as usize + x1c as usize) * 4 + ch] as f64;
                let top = p00 + (p10 - p00) * fx;
                let bot = p01 + (p11 - p01) * fx;
                let v = (top + (bot - top) * fy).round();
                out[(((dy as usize) * dw as usize + dx as usize) * 4) + ch] =
                    v.clamp(0.0, 255.0) as u8;
            }
        }
    }
    out
}

/// Premultiplied -> straight conversion, in place. Crate-visible so the
/// brush-tip tests can check the resampler's round-trip contract.
pub(crate) fn premul_to_straight_in_place(buf: &mut [u8]) {
    for px in buf.chunks_exact_mut(4) {
        let a = px[3];
        if a == 0 {
            px[0] = 0;
            px[1] = 0;
            px[2] = 0;
            px[3] = 0;
        } else {
            let half = (a / 2) as u32;
            for slot in px.iter_mut().take(3) {
                *slot = (((*slot as u32) * 255 + half) / a as u32) as u8;
            }
        }
    }
}

/// Entry point: build the diameter-space tip with the same result the
/// reference brush commit produced, but with every formerly external
/// scaling step replaced by the documented deterministic operations above.
pub fn build_brush_tip(spec: &TipSpec) -> BrushTip {
    let (diameter, data_size, mask) =
        rasterize_mask(spec.size, spec.hardness, spec.curve, spec.data_cap);
    let rgba_data_size = colorize_mask(&mask, data_size, spec.color);
    // Chrome blends/stores PREMULTIPLIED: convert straight->pm before resampling,
    // resample pm bilinearly, then emit straight (round-half unpremul).
    let mut pm_src = rgba_data_size.clone();
    for px in pm_src.chunks_exact_mut(4) {
        let a = px[3] as u32;
        for slot in px.iter_mut().take(3) {
            *slot = (((*slot as u32) * a + 127) / 255) as u8;
        }
    }
    let up_pm = bilinear_resize_premul(&pm_src, data_size, data_size, diameter, diameter);
    let mut up_straight = up_pm.clone();
    premul_to_straight_in_place(&mut up_straight);
    BrushTip {
        diameter,
        width: data_size,
        height: data_size,
        rgba_data_size,
        rgba_diameter: up_straight,
        rgba_diameter_premul: up_pm,
    }
}

/// Build only the data-resolution tip the native rasterizer reads.
///
/// The diameter-space buffers exist for golden round-trip diagnostics; producing
/// them costs a `diameter²` bilinear resample (an 800 px brush allocates two
/// 2.6 MB buffers and samples ~640k pixels) that the native pointer path never
/// reads. Those fields are empty here by contract.
pub fn build_brush_tip_data(spec: &TipSpec) -> BrushTip {
    let (diameter, data_size, mask) =
        rasterize_mask(spec.size, spec.hardness, spec.curve, spec.data_cap);
    BrushTip {
        diameter,
        width: data_size,
        height: data_size,
        rgba_data_size: colorize_mask(&mask, data_size, spec.color),
        rgba_diameter: Vec::new(),
        rgba_diameter_premul: Vec::new(),
    }
}

fn colorize_mask(mask: &[f32], data_size: u32, color: [u8; 3]) -> Vec<u8> {
    let mut rgba = vec![0u8; (data_size * data_size) as usize * 4];
    for (i, alpha) in mask.iter().enumerate() {
        let m = (*alpha as f64 * 255.0).round(); // f32→f64 then round, mirrors Float32Array read
        rgba[i * 4] = color[0];
        rgba[i * 4 + 1] = color[1];
        rgba[i * 4 + 2] = color[2];
        rgba[i * 4 + 3] = m.clamp(0.0, 255.0) as u8;
    }
    rgba
}
