// SPDX-License-Identifier: AGPL-3.0-or-later
//
// WASM compute kernels. Phase 2 establishes the canonical FFI shape
// (buffer-in `&[u8]` / buffer-out `Vec<u8>` over RGBA) with a reference
// scalar kernel. Phase 3 (floodFill) and Phase 4 (gradientFill,
// adjustments-bake) follow the same pattern; `+simd128` is enabled via
// `.cargo/config.toml` for later SIMD variants.
use js_sys::Uint8Array;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use wasm_bindgen::prelude::*;

// Dev-only panic hook so wasm panics surface in the browser console. Release
// builds use `panic = "abort"` (see Cargo.toml) and omit this export.
#[cfg(all(debug_assertions, target_arch = "wasm32"))]
#[wasm_bindgen]
pub fn set_panic_hook() {
    console_error_panic_hook::set_once();
}

/// Flood-fill mask (rect or ellipse, optionally inverted). Mirrors the TS
/// `FillMask` contract used by the Paint Bucket tool. plain struct.
struct FillMaskRust {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    shape: u8, // 0 = rect/none, 1 = ellipse
    inverted: bool,
}

fn is_inside_ellipse(px: i32, py: i32, cx: i32, cy: i32, cw: i32, ch: i32) -> bool {
    let hw = cw as f64 / 2.0;
    let hh = ch as f64 / 2.0;
    if hw <= 0.0 || hh <= 0.0 {
        return false;
    }
    let nx = (px as f64 - (cx as f64 + hw)) / hw;
    let ny = (py as f64 - (cy as f64 + hh)) / hh;
    nx * nx + ny * ny <= 1.0
}

fn is_inside_mask(px: i32, py: i32, mask: Option<&FillMaskRust>) -> bool {
    match mask {
        None => true,
        Some(m) => {
            if px < m.x || px >= m.x + m.w || py < m.y || py >= m.y + m.h {
                return m.inverted;
            }
            if m.shape == 1 {
                let inside = is_inside_ellipse(px, py, m.x, m.y, m.w, m.h);
                return if m.inverted { !inside } else { inside };
            }
            !m.inverted
        }
    }
}

fn matches_source(
    data: &[u8],
    idx: usize,
    sr: i64,
    sg: i64,
    sb: i64,
    sa: i64,
    tol_sq: i64,
) -> bool {
    let dr = data[idx] as i64 - sr;
    let dg = data[idx + 1] as i64 - sg;
    let db = data[idx + 2] as i64 - sb;
    let da = data[idx + 3] as i64 - sa;
    dr * dr + dg * dg + db * db + da * da <= tol_sq
}

/// Pure flood-fill over an RGBA byte buffer. Mirrors `floodFill` in
/// `features/fill/fillOperations.ts` exactly (tolerance = squared RGBA distance,
/// optional rect/ellipse mask, contiguous queue BFS vs global replace).
/// Returns true if any pixel changed. private — tests live in-module.
fn flood_fill_impl(
    data: &mut [u8],
    width: usize,
    height: usize,
    sx: usize,
    sy: usize,
    fill_r: u8,
    fill_g: u8,
    fill_b: u8,
    fill_a: u8,
    tolerance: u32,
    mask: Option<FillMaskRust>,
    contiguous: bool,
) -> bool {
    if sx >= width || sy >= height {
        return false;
    }
    let start_idx = (sy * width + sx) * 4;
    let sr = data[start_idx] as i64;
    let sg = data[start_idx + 1] as i64;
    let sb = data[start_idx + 2] as i64;
    let sa = data[start_idx + 3] as i64;
    if sr == fill_r as i64 && sg == fill_g as i64 && sb == fill_b as i64 && sa == fill_a as i64 {
        return false;
    }
    let tol_sq = (tolerance as i64) * (tolerance as i64);
    let mask_ref = mask.as_ref();

    if !contiguous {
        let mut changed = false;
        for py in 0..height as i32 {
            for px in 0..width as i32 {
                if !is_inside_mask(px, py, mask_ref) {
                    continue;
                }
                let idx = ((py as usize) * width + (px as usize)) * 4;
                if matches_source(data, idx, sr, sg, sb, sa, tol_sq) {
                    data[idx] = fill_r;
                    data[idx + 1] = fill_g;
                    data[idx + 2] = fill_b;
                    data[idx + 3] = fill_a;
                    changed = true;
                }
            }
        }
        return changed;
    }

    let mut visited = vec![false; width * height];
    let mut queue: Vec<(usize, usize)> = Vec::with_capacity(width * height);
    queue.push((sx, sy));
    visited[sy * width + sx] = true;
    let mut changed = false;
    let mut head = 0;
    while head < queue.len() {
        let (px, py) = queue[head];
        head += 1;
        if !is_inside_mask(px as i32, py as i32, mask_ref) {
            continue;
        }
        let idx = (py * width + px) * 4;
        if !matches_source(data, idx, sr, sg, sb, sa, tol_sq) {
            continue;
        }
        data[idx] = fill_r;
        data[idx + 1] = fill_g;
        data[idx + 2] = fill_b;
        data[idx + 3] = fill_a;
        changed = true;
        if px > 0 && !visited[py * width + (px - 1)] {
            visited[py * width + (px - 1)] = true;
            queue.push((px - 1, py));
        }
        if px + 1 < width && !visited[py * width + (px + 1)] {
            visited[py * width + (px + 1)] = true;
            queue.push((px + 1, py));
        }
        if py > 0 && !visited[(py - 1) * width + px] {
            visited[(py - 1) * width + px] = true;
            queue.push((px, py - 1));
        }
        if py + 1 < height && !visited[(py + 1) * width + px] {
            visited[(py + 1) * width + px] = true;
            queue.push((px, py + 1));
        }
    }
    changed
}

/// WASM entry point for flood fill. Copies the input RGBA buffer, runs the pure
/// kernel, returns the filled copy (same buffer-out contract as the other pointwise kernels).
/// scalar queue BFS; SIMD not applicable to 4-connected flood.
#[wasm_bindgen]
pub fn flood_fill_wasm(
    buffer: &[u8],
    width: u32,
    height: u32,
    sx: u32,
    sy: u32,
    fill_r: u32,
    fill_g: u32,
    fill_b: u32,
    fill_a: u32,
    tolerance: u32,
    has_mask: bool,
    mask_x: i32,
    mask_y: i32,
    mask_w: i32,
    mask_h: i32,
    mask_shape: u32,
    mask_inverted: bool,
    contiguous: bool,
) -> Vec<u8> {
    let mut data = buffer.to_vec();
    let w = width as usize;
    let h = height as usize;
    let mask = if has_mask {
        Some(FillMaskRust {
            x: mask_x,
            y: mask_y,
            w: mask_w,
            h: mask_h,
            shape: mask_shape as u8,
            inverted: mask_inverted,
        })
    } else {
        None
    };
    flood_fill_impl(
        &mut data,
        w,
        h,
        sx as usize,
        sy as usize,
        fill_r as u8,
        fill_g as u8,
        fill_b as u8,
        fill_a as u8,
        tolerance,
        mask,
        contiguous,
    );
    data
}

// ── Gradient fill (Phase 4) ─────────────────────────────────────────────────────
// Mirrors `gradientFill` in `features/fill/fillOperations.ts`: linear/radial
// gradient over RGBA, masked, with color stops interpolated by offset.
fn lerp_stops(stops: &[(f64, u8, u8, u8, u8)], t: f64) -> (u8, u8, u8, u8) {
    if stops.is_empty() {
        return (0, 0, 0, 255);
    }
    if t <= stops[0].0 {
        let s = stops[0];
        return (s.1, s.2, s.3, s.4);
    }
    let last = stops[stops.len() - 1];
    if t >= last.0 {
        return (last.1, last.2, last.3, last.4);
    }
    for i in 0..stops.len() - 1 {
        let a = stops[i];
        let b = stops[i + 1];
        if t >= a.0 && t <= b.0 {
            let range = b.0 - a.0;
            let frac = if range > 0.0 { (t - a.0) / range } else { 0.0 };
            return (
                (a.1 as f64 + (b.1 as f64 - a.1 as f64) * frac).round() as u8,
                (a.2 as f64 + (b.2 as f64 - a.2 as f64) * frac).round() as u8,
                (a.3 as f64 + (b.3 as f64 - a.3 as f64) * frac).round() as u8,
                (a.4 as f64 + (b.4 as f64 - a.4 as f64) * frac).round() as u8,
            );
        }
    }
    (0, 0, 0, 255)
}

fn linear_gradient_coord(px: f64, py: f64, ax: f64, ay: f64, dx: f64, dy: f64, len_sq: f64) -> f64 {
    if len_sq == 0.0 {
        return 0.0;
    }
    ((px - ax) * dx + (py - ay) * dy) / len_sq
}

fn radial_gradient_coord(
    px: f64,
    py: f64,
    ax: f64,
    ay: f64,
    _dx: f64,
    _dy: f64,
    len_sq: f64,
) -> f64 {
    if len_sq == 0.0 {
        return 0.0;
    }
    let pdx = px - ax;
    let pdy = py - ay;
    let dist = (pdx * pdx + pdy * pdy).sqrt();
    let radius = len_sq.sqrt();
    if radius > 0.0 {
        dist / radius
    } else {
        0.0
    }
}

/// Pure gradient fill. `stops` are (offset, r, g, b, a), sorted by offset here.
/// private — tests live in-module.
fn gradient_fill_impl(
    data: &mut [u8],
    width: usize,
    height: usize,
    grad_type: u8,
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    stops: &[(f64, u8, u8, u8, u8)],
    mask: Option<FillMaskRust>,
) -> bool {
    if stops.len() < 2 {
        return false;
    }
    let mut sorted = stops.to_vec();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let dx = bx - ax;
    let dy = by - ay;
    let len_sq = dx * dx + dy * dy;
    let mask_ref = mask.as_ref();
    let mut changed = false;
    for py in 0..height as i32 {
        for px in 0..width as i32 {
            if !is_inside_mask(px, py, mask_ref) {
                continue;
            }
            let t = if grad_type == 0 {
                linear_gradient_coord(px as f64, py as f64, ax, ay, dx, dy, len_sq)
            } else {
                radial_gradient_coord(px as f64, py as f64, ax, ay, dx, dy, len_sq)
            };
            let tc = t.max(0.0).min(1.0);
            let (cr, cg, cb, ca) = lerp_stops(&sorted, tc);
            let idx = ((py as usize) * width + (px as usize)) * 4;
            data[idx] = cr;
            data[idx + 1] = cg;
            data[idx + 2] = cb;
            data[idx + 3] = ca;
            changed = true;
        }
    }
    changed
}

#[wasm_bindgen]
pub fn gradient_fill_wasm(
    buffer: &[u8],
    width: u32,
    height: u32,
    grad_type: u32,
    ax: f64,
    ay: f64,
    bx: f64,
    by: f64,
    stop_offsets: &[f64],
    stop_colors: &[u8],
    has_mask: bool,
    mask_x: i32,
    mask_y: i32,
    mask_w: i32,
    mask_h: i32,
    mask_shape: u32,
    mask_inverted: bool,
) -> Vec<u8> {
    let mut data = buffer.to_vec();
    let w = width as usize;
    let h = height as usize;
    let n = stop_offsets.len().min(stop_colors.len() / 4);
    let mut stops: Vec<(f64, u8, u8, u8, u8)> = Vec::with_capacity(n);
    for i in 0..n {
        stops.push((
            stop_offsets[i],
            stop_colors[i * 4],
            stop_colors[i * 4 + 1],
            stop_colors[i * 4 + 2],
            stop_colors[i * 4 + 3],
        ));
    }
    let mask = if has_mask {
        Some(FillMaskRust {
            x: mask_x,
            y: mask_y,
            w: mask_w,
            h: mask_h,
            shape: mask_shape as u8,
            inverted: mask_inverted,
        })
    } else {
        None
    };
    gradient_fill_impl(
        &mut data,
        w,
        h,
        grad_type as u8,
        ax,
        ay,
        bx,
        by,
        &stops,
        mask,
    );
    data
}

// ── Basic adjustment bake (Phase 4) ─────────────────────────────────────────────
// Mirrors `applyBasicAdjustmentToPixels` in `engine/layerAdjustments.ts` (and the
// GPU preview in `renderer/shaders.ts::applyAdjustment`): brightness/contrast/
// saturation in [0,1] straight-alpha space. Alpha passes through unchanged.
/// Round-to-nearest with ties-to-even, matching the JS `Uint8ClampedArray` setter
/// that the TS implementation relies on for the final `clampChannel` writeback.
fn to_u8_clamp(v: f64) -> u8 {
    if !v.is_finite() {
        return 0;
    }
    if v <= 0.0 {
        return 0;
    }
    if v >= 255.0 {
        return 255;
    }
    let m = v.floor();
    let frac = v - m;
    let rounded = if frac < 0.5 {
        m
    } else if frac > 0.5 {
        m + 1.0
    } else if (m as i64) % 2 == 0 {
        m
    } else {
        m + 1.0
    };
    rounded as u8
}

/// Pure per-pixel basic adjustment. `brightness/contrast/saturation` are in
/// [-100, 100]; out-of-range values are clamped like the TS `normalizeBasicAdjustment`.
/// private — tests live in-module.
fn apply_basic_adjustment_impl(
    data: &[u8],
    brightness: f64,
    contrast: f64,
    saturation: f64,
) -> Vec<u8> {
    let b = brightness.clamp(-100.0, 100.0);
    let c = contrast.clamp(-100.0, 100.0);
    let s = saturation.clamp(-100.0, 100.0);
    let contrast_factor = (259.0 * (c + 255.0)) / (255.0 * (259.0 - c));
    let mut out = data.to_vec();
    let len = out.len() - out.len() % 4;
    let mut i = 0;
    while i < len {
        let r0 = out[i] as f64 / 255.0;
        let g0 = out[i + 1] as f64 / 255.0;
        let b0 = out[i + 2] as f64 / 255.0;
        let mut cr = contrast_factor * (r0 - 0.5) + 0.5;
        let mut cg = contrast_factor * (g0 - 0.5) + 0.5;
        let mut cb = contrast_factor * (b0 - 0.5) + 0.5;
        let t = b / 100.0;
        if t >= 0.0 {
            cr = cr + (1.0 - cr) * t * 0.5;
            cg = cg + (1.0 - cg) * t * 0.5;
            cb = cb + (1.0 - cb) * t * 0.5;
        } else {
            let f = -t;
            cr = cr - cr * f * 0.5;
            cg = cg - cg * f * 0.5;
            cb = cb - cb * f * 0.5;
        }
        let lum = cr * 0.2126 + cg * 0.7152 + cb * 0.0722;
        let sat_factor = 1.0 + s / 100.0;
        cr = lum + (cr - lum) * sat_factor;
        cg = lum + (cg - lum) * sat_factor;
        cb = lum + (cb - lum) * sat_factor;
        out[i] = to_u8_clamp(cr * 255.0);
        out[i + 1] = to_u8_clamp(cg * 255.0);
        out[i + 2] = to_u8_clamp(cb * 255.0);
        i += 4;
    }
    out
}

#[wasm_bindgen]
pub fn apply_basic_adjustment_wasm(
    buffer: &[u8],
    brightness: f64,
    contrast: f64,
    saturation: f64,
) -> Vec<u8> {
    apply_basic_adjustment_impl(buffer, brightness, contrast, saturation)
}

// ── Zero-copy accelerator prototype (Phase 5 follow-up) ───────────────────────
// Pixels are owned by wasm (allocated here) so kernels mutate in place and avoid
// the 2x memcpy of the `&[u8] -> Vec<u8>` shape. Validated by bench-zero-copy.ts.
thread_local! {
    static BUFFERS: RefCell<HashMap<u32, Vec<u8>>> = RefCell::new(HashMap::new());
}
static NEXT_ID: AtomicU32 = AtomicU32::new(1);

#[wasm_bindgen]
pub fn alloc_rgba_buffer(len: usize) -> u32 {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    BUFFERS.with(|b| {
        b.borrow_mut().insert(id, vec![0u8; len]);
    });
    id
}

#[wasm_bindgen]
pub fn free_rgba_buffer(id: u32) {
    BUFFERS.with(|b| {
        b.borrow_mut().remove(&id);
    });
}

// Zero-copy view into wasm-owned memory (no copy to JS).
#[wasm_bindgen]
pub fn rgba_buffer_view(id: u32) -> Uint8Array {
    BUFFERS.with(|b| {
        let m = b.borrow();
        let v = m.get(&id).expect("invalid buffer id");
        // SAFETY: `view` borrows wasm linear memory; the Vec is stable (never
        // resized) and is only freed via `free_rgba_buffer`, so the view stays
        // valid until then. JS must not outlive the free call.
        unsafe { Uint8Array::view(v) }
    })
}

#[wasm_bindgen]
pub fn invert_rgba_inplace(id: u32) {
    BUFFERS.with(|b| {
        let mut m = b.borrow_mut();
        let v = m.get_mut(&id).expect("invalid buffer id");
        invert_inplace_impl(v);
    });
}

fn invert_inplace_impl(data: &mut [u8]) {
    let len = data.len() - data.len() % 4;
    let mut i = 0;
    while i < len {
        data[i] = 255 - data[i];
        data[i + 1] = 255 - data[i + 1];
        data[i + 2] = 255 - data[i + 2];
        i += 4;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: usize, h: usize, r: u8, g: u8, b: u8, a: u8) -> Vec<u8> {
        let mut v = vec![0u8; w * h * 4];
        for i in 0..w * h {
            let o = i * 4;
            v[o] = r;
            v[o + 1] = g;
            v[o + 2] = b;
            v[o + 3] = a;
        }
        v
    }

    #[test]
    fn contiguous_fills_uniform_image() {
        let mut img = solid(10, 10, 0, 0, 0, 255);
        let changed = flood_fill_impl(&mut img, 10, 10, 0, 0, 255, 0, 0, 255, 0, None, true);
        assert!(changed);
        assert_eq!(&img[0..4], &[255, 0, 0, 255]);
        assert_eq!(&img[img.len() - 4..], &[255, 0, 0, 255]);
    }

    #[test]
    fn noop_when_start_equals_fill() {
        let mut img = solid(10, 10, 255, 0, 0, 255);
        let changed = flood_fill_impl(&mut img, 10, 10, 0, 0, 255, 0, 0, 255, 0, None, true);
        assert!(!changed);
    }

    #[test]
    fn tolerance_zero_only_exact_match_region() {
        let mut img = solid(10, 10, 0, 255, 0, 255);
        for y in 8..10 {
            for x in 8..10 {
                let o = (y * 10 + x) * 4;
                img[o] = 255;
                img[o + 1] = 0;
                img[o + 2] = 0;
            }
        }
        let changed = flood_fill_impl(&mut img, 10, 10, 0, 0, 0, 0, 255, 255, 0, None, true);
        assert!(changed);
        for y in 8..10 {
            for x in 8..10 {
                let o = (y * 10 + x) * 4;
                assert_eq!(&img[o..o + 4], &[255, 0, 0, 255]);
            }
        }
        assert_eq!(&img[0..4], &[0, 0, 255, 255]);
    }

    #[test]
    fn ellipse_mask_only_fills_inside() {
        let mut img = solid(20, 20, 0, 0, 0, 255);
        let mask = Some(FillMaskRust {
            x: 0,
            y: 0,
            w: 20,
            h: 20,
            shape: 1,
            inverted: false,
        });
        let changed = flood_fill_impl(&mut img, 20, 20, 10, 10, 255, 255, 255, 255, 0, mask, true);
        assert!(changed);
        assert_eq!(&img[0..4], &[0, 0, 0, 255]);
        let c = (10 * 20 + 10) * 4;
        assert_eq!(&img[c..c + 4], &[255, 255, 255, 255]);
    }

    #[test]
    fn global_replace_fills_all_matching() {
        let mut img = solid(10, 10, 0, 0, 0, 255);
        img[5 * 4 + 1] = 100;
        let changed = flood_fill_impl(&mut img, 10, 10, 0, 0, 255, 255, 255, 255, 0, None, false);
        assert!(changed);
        for i in 0..10 * 10 {
            let o = i * 4;
            if o == 5 * 4 {
                continue;
            }
            assert_eq!(&img[o..o + 4], &[255, 255, 255, 255]);
        }
    }

    #[test]
    fn linear_gradient_interpolates_endpoints() {
        let mut img = solid(10, 1, 0, 0, 0, 255);
        let stops = vec![(0.0, 0u8, 0, 0, 255), (1.0, 255u8, 255, 255, 255)];
        let changed = gradient_fill_impl(&mut img, 10, 1, 0, 0.0, 0.0, 9.0, 0.0, &stops, None);
        assert!(changed);
        assert_eq!(&img[0..4], &[0, 0, 0, 255]);
        assert_eq!(&img[36..40], &[255, 255, 255, 255]);
    }

    #[test]
    fn radial_gradient_fills_center() {
        let mut img = solid(10, 10, 0, 0, 0, 255);
        let stops = vec![(0.0, 255u8, 255, 255, 255), (1.0, 0u8, 0, 0, 255)];
        let changed = gradient_fill_impl(&mut img, 10, 10, 1, 4.5, 4.5, 5.5, 5.5, &stops, None);
        assert!(changed);
        let c = (4 * 10 + 4) * 4;
        assert!(img[c] > img[0]);
        assert_eq!(&img[0..4], &[0, 0, 0, 255]);
    }

    #[test]
    fn fewer_than_two_stops_is_noop() {
        let mut img = solid(10, 10, 100, 100, 100, 255);
        let stops = vec![(0.0, 255u8, 0, 0, 255)];
        let changed = gradient_fill_impl(&mut img, 10, 10, 0, 0.0, 0.0, 9.0, 0.0, &stops, None);
        assert!(!changed);
    }

    #[test]
    fn identity_adjustment_is_noop() {
        let img = vec![10u8, 20, 30, 255, 40, 50, 60, 128];
        let out = apply_basic_adjustment_impl(&img, 0.0, 0.0, 0.0);
        assert_eq!(out, img);
    }

    #[test]
    fn full_brightness_lifts_black_to_midgray() {
        let img = vec![0u8, 0, 0, 255];
        let out = apply_basic_adjustment_impl(&img, 100.0, 0.0, 0.0);
        // t=1, cr = (1-0)*0.5 = 0.5 -> 127.5 -> tie -> 128
        assert_eq!(&out[0..3], &[128, 128, 128]);
        assert_eq!(out[3], 255);
    }

    #[test]
    fn full_saturation_keeps_pure_red() {
        let img = vec![255u8, 0, 0, 255];
        let out = apply_basic_adjustment_impl(&img, 0.0, 0.0, 100.0);
        assert_eq!(&out[0..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn contrast_changes_midtone() {
        let img = vec![200u8, 100, 50, 255];
        let out = apply_basic_adjustment_impl(&img, 0.0, 50.0, 0.0);
        assert_ne!(&out[0..3], &[200, 100, 50]);
    }

    #[test]
    fn invert_inplace_mutates_in_place() {
        let mut img = vec![10u8, 20, 30, 255, 40, 50, 60, 128];
        invert_inplace_impl(&mut img);
        assert_eq!(&img[0..4], &[245, 235, 225, 255]);
        assert_eq!(&img[4..8], &[215, 205, 195, 128]);
    }
}
