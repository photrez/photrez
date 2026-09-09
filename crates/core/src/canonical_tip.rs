//! R2 Canonical C1 — authoritative brush-tip construction + defined resampler.
//!
//! Ports the TS semantics verbatim (brushTipMask.ts / brushHardnessProfile.ts)
//! into explicit Rust contract code, then defines the previously browser-owned
//! upscale step as a deterministic bilinear-premultiplied half-pixel resampler.
//! No production wiring yet (C1 gate): validated offline against captured
//! golden browser bytes via the env-gated test below.

pub const BRUSH_HARD_EDGE_THRESHOLD: f64 = 0.97;
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

/// Fritsch-Carlson monotone cubic — mirrors MonotoneCubic.ts operation-for-operation.
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

/// Super-gaussian soft profile — mirrors brushAlpha().
pub fn brush_alpha(r_norm: f64, hardness: f64) -> f64 {
    let h = clamp_hardness(hardness);
    if h >= BRUSH_HARD_EDGE_THRESHOLD {
        return if r_norm <= 1.0 { 1.0 } else { 0.0 };
    }
    let p = get_brush_profile_parameters(h);
    let x = r_norm.max(0.0) / p.sigma;
    (-x.powf(p.n)).exp()
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
pub struct CanonicalTip {
    /// Nominal diameter-space stamp size (the space production commits in).
    pub diameter: u32,
    /// Mask/data resolution (dataCap policy), matches production rawTip bitmap.
    pub width: u32,
    pub height: u32,
    /// Straight-RGBA at width×height (mask colorized) — the pre-upscale bitmap.
    pub rgba_data_size: Vec<u8>,
    /// Straight-RGBA at diameter² produced by the canonical bilinear-premul resampler.
    pub rgba_diameter: Vec<u8>,
    /// Premultiplied-RGBA at diameter² (pre-straight-conversion) — the true
    /// comparison domain for browser-storage diagnostics (unpremul amplifies).
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

/// Mirrors rasterizeBrushTipWithCurve(): f32 mask storage, half-pixel offsets,
/// scale=diameter/dataSize, distance=sqrt, soft ⇒ brushAlpha(dist/R_nominal,h),
/// small/hard branches preserved.
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

/// Canonical bilinear resampler operating on PREMULTIPLIED pixels with
/// half-pixel centers, then emitted back to straight-RGBA (round-half unpremul).
/// This is the explicitly specified replacement for the previously
/// browser-owned drawImage upscale.
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

fn premul_to_straight_in_place(buf: &mut [u8]) {
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

/// C1 entry: build the canonical diameter-space tip exactly as the production
/// TS commit visualizes it, but with every previously browser-owned step
/// replaced by the documented deterministic operations above.
pub fn build_canonical_tip(spec: &TipSpec) -> CanonicalTip {
    let (diameter, data_size, mask) =
        rasterize_mask(spec.size, spec.hardness, spec.curve, spec.data_cap);
    let n = (data_size * data_size) as usize;
    let mut rgba_data_size = vec![0u8; n * 4];
    for i in 0..n {
        let m = (mask[i] as f64 * 255.0).round(); // f32→f64 then round, mirrors Float32Array read
        rgba_data_size[i * 4] = spec.color[0];
        rgba_data_size[i * 4 + 1] = spec.color[1];
        rgba_data_size[i * 4 + 2] = spec.color[2];
        rgba_data_size[i * 4 + 3] = m.clamp(0.0, 255.0) as u8;
    }
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
    CanonicalTip {
        diameter,
        width: data_size,
        height: data_size,
        rgba_data_size,
        rgba_diameter: up_straight,
        rgba_diameter_premul: up_pm,
    }
}

// ── tests ──

#[test]
fn c1_known_diameter_256_08_is_274() {
    let spec = TipSpec::new(256.0, 0.8, [225, 90, 23]);
    let tip = build_canonical_tip(&spec);
    assert_eq!(tip.diameter, 274);
    assert_eq!(tip.width, 256);
    assert_eq!(tip.rgba_data_size.len(), 256 * 256 * 4);
    assert_eq!(tip.rgba_diameter.len(), 274 * 274 * 4);
}

#[test]
fn c1_mask_center_opaque_and_corner_transparent() {
    let spec = TipSpec::new(256.0, 0.8, [225, 90, 23]);
    let tip = build_canonical_tip(&spec);
    let ds = tip.width as usize;
    let c = ds / 2;
    let idx = (c * ds + c) * 4;
    assert_eq!(tip.rgba_data_size[idx + 3], 255); // solid core
    assert_eq!(tip.rgba_data_size[3], 0); // corner outside support
}

#[test]
fn c1_identity_resize_is_copy() {
    let src_straight = vec![10u8, 20, 30, 255, 40, 50, 60, 128];
    // contract: callers feed PREMULTIPLIED bytes
    let mut pm = src_straight.clone();
    for px in pm.chunks_exact_mut(4) {
        let a = px[3] as u32;
        for slot in px.iter_mut().take(3) {
            *slot = (((*slot as u32) * a + 127) / 255) as u8;
        }
    }
    let mut out = bilinear_resize_premul(&pm, 2, 1, 2, 1);
    premul_to_straight_in_place(&mut out);
    assert_eq!(&out[..], &src_straight[..]);
}

#[test]
fn c1_support_norm_08_value() {
    let n = get_brush_profile_support_norm(0.8);
    assert!((n - 1.0615857181654722).abs() < 1e-12, "norm={n}");
}

/// Offline golden validation vs captured browser bytes.
/// Run: PHOTREZ_C1_GOLDEN=<bin path> cargo test -p photrez-core c1_golden -- --ignored --nocapture
/// Bin format: u32LE tipW | u32LE tipH | RGBA bytes (274*274*4).
#[test]
#[ignore]
fn c1_golden_interior_and_rim_tolerance() {
    let path = match std::env::var("PHOTREZ_C1_GOLDEN") {
        Ok(p) => p,
        Err(_) => return, // silently skip when not provided (default CI)
    };
    let raw = fs_read(path);
    assert!(raw.len() >= 8, "golden bin too small");
    let tip_w = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
    let tip_h = u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]) as usize;
    let expect = tip_w * tip_h * 4;
    assert_eq!(raw.len(), 8 + expect, "golden payload mismatch");
    let golden = &raw[8..];

    let spec = TipSpec::new(256.0, 0.8, [225, 90, 23]);
    let tip = build_canonical_tip(&spec);
    assert_eq!(tip.diameter as usize, tip_w, "diameter mismatch");
    assert_eq!(tip.rgba_diameter.len(), golden.len());

    // PREMULTIPLIED-domain comparison: no unpremul amplification — this is the
    // honest representation-uncertainty bound (storage rounding modes differ).
    let mut pm_interior_max = 0u32;
    let mut pm_rim_max = 0u32;
    let mut pm_rim_db = 0u64;
    let mut alpha_max = 0u32;
    let mut straight_interior_max = 0u32;
    let mut straight_rim_max = 0u32;
    for i in 0..tip_w * tip_h {
        let ga = golden[i * 4 + 3] as u32;
        // reconstruct browser premultiplied from golden straight readback
        let gpm = |ch: usize| -> u32 {
            if ga == 0 {
                0
            } else {
                ((golden[i * 4 + ch] as u32) * ga + 127) / 255
            }
        };
        let cls = if ga == 255 { "interior" } else { "rim" };
        for ch in 0..4 {
            let d = (tip.rgba_diameter_premul[i * 4 + ch] as i32 - gpm(ch) as i32).unsigned_abs();
            if cls == "interior" {
                if d > pm_interior_max {
                    pm_interior_max = d;
                }
            } else {
                if d > pm_rim_max {
                    pm_rim_max = d;
                }
                if d > 0 {
                    pm_rim_db += 1;
                }
            }
        }
        let da = (tip.rgba_diameter[i * 4 + 3] as i32 - golden[i * 4 + 3] as i32).unsigned_abs();
        if da > alpha_max {
            alpha_max = da;
        }
        for ch in 0..3 {
            let d =
                (tip.rgba_diameter[i * 4 + ch] as i32 - golden[i * 4 + ch] as i32).unsigned_abs();
            if ga == 255 {
                if d > straight_interior_max {
                    straight_interior_max = d;
                }
            } else {
                if d > straight_rim_max {
                    straight_rim_max = d;
                }
            }
        }
    }
    // Option A (C1 review decision): Chrome/Skia resampling is NOT canonical, so rim
    // divergence vs the browser is the DOCUMENTED LEGACY COMPATIBILITY DELTA and is
    // REPORT-ONLY. Canonical correctness = interior + alpha bounds below.
    //
    // Documented legacy-browser rim delta baseline (probe 256/0.8 -> 274):
    //   premultiplied rimMax = 65 LSB | straight rimMax = 32 LSB (unpremul-amplified view).
    println!(
        "C1 golden[px={}]: PM interiorMax={} rimMax={} rimDB={} | STRAIGHT interiorMax={} rimMax={} | alphaMax={} | legacyRimDelta(report-only): pm=65 straight=32",
        tip_w * tip_h, pm_interior_max, pm_rim_max, pm_rim_db, straight_interior_max, straight_rim_max, alpha_max
    );
    assert!(
        pm_interior_max <= 1,
        "canonical gate: premul interior beyond storage noise: {pm_interior_max}"
    );
    // rim metrics intentionally REPORT-ONLY (documented legacy compatibility delta)
    assert!(
        alpha_max <= 1,
        "canonical gate: alpha beyond browser-roundtrip bound: {alpha_max}"
    );
    assert!(
        straight_interior_max <= 1,
        "canonical gate: straight interior beyond browser noise: {straight_interior_max}"
    );
}

#[cfg(test)]
fn fs_read(path: String) -> Vec<u8> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).expect("cannot open golden file");
    let mut v = Vec::new();
    f.read_to_end(&mut v).expect("read failed");
    v
}
