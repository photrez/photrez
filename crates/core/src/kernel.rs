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
fn apply_basic_adjustment_impl(data: &mut [u8], brightness: f64, contrast: f64, saturation: f64) {
    let b = brightness.clamp(-100.0, 100.0);
    let c = contrast.clamp(-100.0, 100.0);
    let s = saturation.clamp(-100.0, 100.0);
    let contrast_factor = (259.0 * (c + 255.0)) / (255.0 * (259.0 - c));
    let len = data.len() - data.len() % 4;
    let mut i = 0;
    while i < len {
        let r0 = data[i] as f64 / 255.0;
        let g0 = data[i + 1] as f64 / 255.0;
        let b0 = data[i + 2] as f64 / 255.0;
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
        data[i] = to_u8_clamp(cr * 255.0);
        data[i + 1] = to_u8_clamp(cg * 255.0);
        data[i + 2] = to_u8_clamp(cb * 255.0);
        i += 4;
    }
}

#[wasm_bindgen]
pub fn apply_basic_adjustment_wasm(
    buffer: &[u8],
    brightness: f64,
    contrast: f64,
    saturation: f64,
) -> Vec<u8> {
    let mut out = buffer.to_vec();
    apply_basic_adjustment_impl(&mut out, brightness, contrast, saturation);
    out
}

// ── Round-2 porting candidates ────────────────────────────────────────────────
// Benchmarked against TS mirrors in apps/desktop/scripts/bench-cpu-pixel-round2.ts
// (see docs/plans/2026-08-21-rust-gpu-benchmark-matrix.md, R2/R3).

/// R2: bounding box of non-transparent pixels (alpha > 0).
/// Returns [min_x, min_y, max_x, max_y] (inclusive), or empty when the image is
/// fully transparent. Mirrors SelectionOperations.trimTransparent's scan.
#[wasm_bindgen]
pub fn trim_bbox_wasm(pixels: &[u8], width: u32, height: u32) -> Vec<u32> {
    let w = width as usize;
    let total = w.saturating_mul(height as usize);
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    for i in 0..total {
        if pixels[i * 4 + 3] > 0 {
            let x = (i % w) as u32;
            let y = (i / w) as u32;
            if x < min_x {
                min_x = x;
            }
            if x > max_x {
                max_x = x;
            }
            if y < min_y {
                min_y = y;
            }
            if y > max_y {
                max_y = y;
            }
        }
    }
    if max_x < min_x || max_y < min_y {
        Vec::new()
    } else {
        vec![min_x, min_y, max_x, max_y]
    }
}

/// R3: brush dab stamp — bilinear resample of the precomputed tip into the
/// mask with saturating accumulation `cur + (255-cur)*a`. Mirrors
/// brushTipMask.stampBrushTip exactly (same bounds/clamp semantics).
#[wasm_bindgen]
pub fn brush_stamp_wasm(
    mask: &mut [u8],
    mask_width: u32,
    mask_height: u32,
    tip_data: &[f32],
    data_size: u32,
    diameter: f64,
    center_x: f64,
    center_y: f64,
    alpha_scale: f64,
) {
    stamp_into(
        mask,
        mask_width,
        mask_height,
        tip_data,
        data_size,
        diameter,
        center_x,
        center_y,
        alpha_scale,
    );
}

/// Shared stamp body (used by both the copy and zero-copy entry points).
fn stamp_into(
    mask: &mut [u8],
    mask_width: u32,
    mask_height: u32,
    tip_data: &[f32],
    data_size: u32,
    diameter: f64,
    center_x: f64,
    center_y: f64,
    alpha_scale: f64,
) {
    let a_scale = alpha_scale.clamp(0.0, 1.0) as f32;
    let half_extent = (diameter / 2.0) as f32;
    let center_index = half_extent - 0.5;
    let ds = data_size as usize;
    if ds == 0 {
        return;
    }
    let data_scale = data_size as f32 / diameter as f32;
    let cx = center_x as f32;
    let cy = center_y as f32;
    let mw = mask_width as i64;
    let mh = mask_height as i64;

    let min_x = ((cx - half_extent).floor() as i64).max(0);
    let max_x = (((cx + half_extent).ceil() as i64) - 1).min(mw - 1);
    let min_y = ((cy - half_extent).floor() as i64).max(0);
    let max_y = (((cy + half_extent).ceil() as i64) - 1).min(mh - 1);

    let mut y = min_y;
    while y <= max_y {
        let ty = (y as f32 - cy + center_index) * data_scale;
        let y0 = ty.floor();
        let y1 = y0 + 1.0;
        let wy = ty - y0;
        let y0_in = y0 >= 0.0 && y0 < ds as f32;
        let y1_in = y1 >= 0.0 && y1 < ds as f32;
        let y0_off = (y0.max(0.0) as usize) * ds;
        let y1_off = (y1.max(0.0) as usize) * ds;

        let row_idx = (y * mw) as usize;
        let mut x = min_x;
        while x <= max_x {
            let tx = (x as f32 - cx + center_index) * data_scale;
            let x0 = tx.floor();
            let x1 = x0 + 1.0;
            let wx = tx - x0;
            let x0_in = x0 >= 0.0 && x0 < ds as f32;
            let x1_in = x1 >= 0.0 && x1 < ds as f32;

            let a00 = if y0_in && x0_in {
                tip_data[y0_off + x0.max(0.0) as usize]
            } else {
                0.0
            };
            let a10 = if y0_in && x1_in {
                tip_data[y0_off + x1.max(0.0) as usize]
            } else {
                0.0
            };
            let a01 = if y1_in && x0_in {
                tip_data[y1_off + x0.max(0.0) as usize]
            } else {
                0.0
            };
            let a11 = if y1_in && x1_in {
                tip_data[y1_off + x1.max(0.0) as usize]
            } else {
                0.0
            };

            let a0 = a00 * (1.0 - wx) + a10 * wx;
            let a1 = a01 * (1.0 - wx) + a11 * wx;
            let interpolated = a0 * (1.0 - wy) + a1 * wy;
            if interpolated <= 0.0 {
                x += 1;
                continue;
            }
            let scaled = interpolated * a_scale;
            if scaled <= 0.0 {
                x += 1;
                continue;
            }
            let idx = row_idx + x as usize;
            let cur = mask[idx];
            if cur != 255 {
                // Mirror TS exactly: round the DELTA, then add (not add-then-round).
                let delta = ((255.0 - cur as f32) * scaled).round() as u32;
                mask[idx] = (cur as u32 + delta).min(255) as u8;
            }
            x += 1;
        }
        y += 1;
    }
}

/// R3: straight-alpha-over composite of a paint mask into an RGBA buffer,
/// restricted to a dirty rect. Mirrors brushTipMask.compositeMaskToImageDataDirty.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn composite_mask_wasm(
    dst: &mut [u8],
    dst_width: u32,
    origin_x: i32,
    origin_y: i32,
    mask: &[u8],
    mask_width: u32,
    rect_x0: u32,
    rect_y0: u32,
    rect_x1: u32,
    rect_y1: u32,
    r: f64,
    g: f64,
    b: f64,
    a: f64,
    is_eraser: bool,
) {
    compose_over(
        dst, dst_width, origin_x, origin_y, mask, mask_width, rect_x0, rect_y0, rect_x1, rect_y1,
        r, g, b, a, is_eraser,
    );
}

/// Shared composite body (used by both the copy and zero-copy entry points).
fn compose_over(
    dst: &mut [u8],
    dst_width: u32,
    origin_x: i32,
    origin_y: i32,
    mask: &[u8],
    mask_width: u32,
    rect_x0: u32,
    rect_y0: u32,
    rect_x1: u32,
    rect_y1: u32,
    r: f64,
    g: f64,
    b: f64,
    a: f64,
    is_eraser: bool,
) {
    let stroke_alpha = a.clamp(0.0, 1.0) as f32;
    let (pr, pg, pb) = (r as f32, g as f32, b as f32);
    let img_w = dst_width as usize;
    let mw = mask_width as usize;

    // The TS caller clamps the dirty rect to mask bounds before invoking
    // (clampDirtyRect); mirror that contract instead of re-validating here.
    for y in rect_y0..rect_y1 {
        let row_in_mask = y as usize * mw;
        let row_in_image = ((y as i32 - origin_y) as usize) * img_w;
        for x in rect_x0..rect_x1 {
            let mask_alpha = mask[row_in_mask + x as usize] as f32 / 255.0;
            if mask_alpha <= 0.0 {
                continue;
            }
            let i = (row_in_image + (x as i32 - origin_x) as usize) << 2;
            let alpha = mask_alpha * stroke_alpha;
            if is_eraser {
                dst[i + 3] = ((dst[i + 3] as f32) * (1.0 - alpha)).round() as u8;
                continue;
            }
            let dst_a = dst[i + 3] as f32 / 255.0;
            let out_a = alpha + dst_a * (1.0 - alpha);
            if out_a <= 0.0 {
                dst[i] = 0;
                dst[i + 1] = 0;
                dst[i + 2] = 0;
                dst[i + 3] = 0;
                continue;
            }
            dst[i] = ((pr * alpha + dst[i] as f32 * dst_a * (1.0 - alpha)) / out_a).round() as u8;
            dst[i + 1] =
                ((pg * alpha + dst[i + 1] as f32 * dst_a * (1.0 - alpha)) / out_a).round() as u8;
            dst[i + 2] =
                ((pb * alpha + dst[i + 2] as f32 * dst_a * (1.0 - alpha)) / out_a).round() as u8;
            dst[i + 3] = (out_a * 255.0).round() as u8;
        }
    }
}

// ── Zero-copy variants (round 2) ─────────────────────────────────────────────
// The &[u8] shapes above copy the whole buffer across the wasm boundary PER
// CALL, which dominates runtime for large masks (measured: 17x slower than TS
// for d32 stamps). These variants keep buffers owned by wasm (BUFFERS map) so
// repeated calls touch data in place — the fair comparison shape.

/// R2 zero-copy: bbox of alpha>0 pixels for buffer `id`.
#[wasm_bindgen]
pub fn trim_bbox_buffer(id: u32, width: u32, height: u32) -> Vec<u32> {
    let px = BUFFERS.with(|b| b.borrow().get(&id).cloned().expect("invalid buffer id"));
    trim_bbox_impl(&px, width, height)
}

/// R3 zero-copy: stamp into owned mask buffer `id`.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn brush_stamp_buffer(
    id: u32,
    mask_width: u32,
    mask_height: u32,
    tip_data: &[f32],
    data_size: u32,
    diameter: f64,
    center_x: f64,
    center_y: f64,
    alpha_scale: f64,
) {
    BUFFERS.with(|b| {
        let mut m = b.borrow_mut();
        let v = m.get_mut(&id).expect("invalid buffer id");
        stamp_into(
            v,
            mask_width,
            mask_height,
            tip_data,
            data_size,
            diameter,
            center_x,
            center_y,
            alpha_scale,
        );
    });
}

/// R3 zero-copy: composite into owned RGBA buffer `id`.
#[wasm_bindgen]
#[allow(clippy::too_many_arguments)]
pub fn composite_mask_buffer(
    id: u32,
    dst_width: u32,
    origin_x: i32,
    origin_y: i32,
    mask_id: u32,
    mask_width: u32,
    rect_x0: u32,
    rect_y0: u32,
    rect_x1: u32,
    rect_y1: u32,
    r: f64,
    g: f64,
    b: f64,
    a: f64,
    is_eraser: bool,
) {
    // Mask read must not alias the mutable dst borrow: copy-on-read is avoided
    // by scoping the two lookups; ids are distinct by contract (caller owns
    // separate buffers), asserted cheaply in tests.
    let mask = BUFFERS.with(|bufs| {
        bufs.borrow()
            .get(&mask_id)
            .cloned()
            .expect("invalid mask buffer id")
    });
    BUFFERS.with(|bufs| {
        let mut m = bufs.borrow_mut();
        let v = m.get_mut(&id).expect("invalid dst buffer id");
        compose_over(
            v, dst_width, origin_x, origin_y, &mask, mask_width, rect_x0, rect_y0, rect_x1,
            rect_y1, r, g, b, a, is_eraser,
        );
    });
}

fn trim_bbox_impl(pixels: &[u8], width: u32, height: u32) -> Vec<u32> {
    let w = width as usize;
    let total = w.saturating_mul(height as usize);
    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    for i in 0..total {
        if pixels.get(i * 4 + 3).copied().unwrap_or(0) > 0 {
            let x = (i % w) as u32;
            let y = (i / w) as u32;
            if x < min_x {
                min_x = x;
            }
            if x > max_x {
                max_x = x;
            }
            if y < min_y {
                min_y = y;
            }
            if y > max_y {
                max_y = y;
            }
        }
    }
    if max_x < min_x || max_y < min_y {
        Vec::new()
    } else {
        vec![min_x, min_y, max_x, max_y]
    }
}

// ── C-slice helpers: let `Engine` own pixel buffers + adjustment state ─────────
// (Technique C: Rust SSOT for state + pixels; TS uploads the zero-copy view.)
pub fn write_buffer(id: u32, src: &[u8]) {
    BUFFERS.with(|b| {
        let mut m = b.borrow_mut();
        let v = m.get_mut(&id).expect("invalid buffer id");
        v.copy_from_slice(src);
    });
}

pub fn apply_adjustment_inplace(id: u32, brightness: f64, contrast: f64, saturation: f64) {
    BUFFERS.with(|b| {
        let mut m = b.borrow_mut();
        let v = m.get_mut(&id).expect("invalid buffer id");
        apply_basic_adjustment_impl(v, brightness, contrast, saturation);
    });
}

#[cfg(test)]
pub(crate) fn buffer_clone(id: u32) -> Vec<u8> {
    BUFFERS.with(|b| {
        let m = b.borrow();
        m.get(&id).expect("invalid buffer id").clone()
    })
}

// ── Zero-copy accelerator prototype (Phase 5 follow-up) ───────────────────────
// Pixels are owned by wasm (allocated here) so kernels mutate in place and avoid
// the 2x memcpy of the `&[u8] -> Vec<u8>` shape. Validated by bench-zero-copy.ts.
thread_local! {
    static BUFFERS: RefCell<HashMap<u32, Vec<u8>>> = RefCell::new(HashMap::new());
}
static NEXT_ID: AtomicU32 = AtomicU32::new(1);

/// Read-only access to a pinned buffer without copying (bench + future tile
/// store seeding). Closure sees the live bytes inside wasm linear memory.
pub(crate) fn with_buffer<R>(id: u32, f: impl FnOnce(&[u8]) -> R) -> R {
    BUFFERS.with(|b| {
        let m = b.borrow();
        f(m.get(&id).expect("invalid buffer id"))
    })
}

/// Mutable access to a pinned buffer (decode-into shape: kernels write
/// results straight into wasm-owned memory, JS reads via rgba_buffer_view).
pub(crate) fn with_buffer_mut<R>(id: u32, f: impl FnOnce(&mut [u8]) -> R) -> R {
    BUFFERS.with(|b| {
        let mut m = b.borrow_mut();
        f(m.get_mut(&id).expect("invalid buffer id"))
    })
}

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
        let mut img = vec![10u8, 20, 30, 255, 40, 50, 60, 128];
        apply_basic_adjustment_impl(&mut img, 0.0, 0.0, 0.0);
        assert_eq!(img, vec![10u8, 20, 30, 255, 40, 50, 60, 128]);
    }

    #[test]
    fn full_brightness_lifts_black_to_midgray() {
        let mut img = vec![0u8, 0, 0, 255];
        apply_basic_adjustment_impl(&mut img, 100.0, 0.0, 0.0);
        // t=1, cr = (1-0)*0.5 = 0.5 -> 127.5 -> tie -> 128
        assert_eq!(&img[0..3], &[128, 128, 128]);
        assert_eq!(img[3], 255);
    }

    #[test]
    fn full_saturation_keeps_pure_red() {
        let mut img = vec![255u8, 0, 0, 255];
        apply_basic_adjustment_impl(&mut img, 0.0, 0.0, 100.0);
        assert_eq!(&img[0..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn contrast_changes_midtone() {
        let mut img = vec![200u8, 100, 50, 255];
        apply_basic_adjustment_impl(&mut img, 0.0, 50.0, 0.0);
        assert_ne!(&img[0..3], &[200, 100, 50]);
    }

    #[test]
    fn invert_inplace_mutates_in_place() {
        let mut img = vec![10u8, 20, 30, 255, 40, 50, 60, 128];
        invert_inplace_impl(&mut img);
        assert_eq!(&img[0..4], &[245, 235, 225, 255]);
        assert_eq!(&img[4..8], &[215, 205, 195, 128]);
    }

    // ── Round-2 kernels ──

    #[test]
    fn trim_bbox_finds_bounds_and_handles_empty() {
        // 4x2 image: opaque pixel only at (2,1)
        let mut px = vec![0u8; 4 * 2 * 4];
        px[(1 * 4 + 2) * 4 + 3] = 255;
        assert_eq!(trim_bbox_wasm(&px, 4, 2), vec![2, 1, 2, 1]);
        // fully transparent -> empty
        assert!(trim_bbox_wasm(&[0u8; 4 * 2 * 4], 4, 2).is_empty());
    }

    #[test]
    fn brush_stamp_accumulates_toward_saturation() {
        // uniform tip (alpha 1 everywhere), dataScale = 1 (dataSize == diameter)
        let ds = 8u32;
        let tip = vec![1.0f32; (ds * ds) as usize];
        let mut mask = vec![0u8; 64];
        brush_stamp_wasm(&mut mask, 8, 8, &tip, ds, 8.0, 4.0, 4.0, 0.5);
        // center pixel: first dab ~128 (255*0.5 rounded)
        assert_eq!(mask[4 * 8 + 4], 128);
        // second dab at same point accumulates: 128 + round(127*0.5) = 128+64
        brush_stamp_wasm(&mut mask, 8, 8, &tip, ds, 8.0, 4.0, 4.0, 0.5);
        assert_eq!(mask[4 * 8 + 4], 192);
        // many dabs saturate to exactly 255 and stay there
        for _ in 0..50 {
            brush_stamp_wasm(&mut mask, 8, 8, &tip, ds, 8.0, 4.0, 4.0, 0.5);
        }
        assert_eq!(mask[4 * 8 + 4], 255);
    }

    #[test]
    fn composite_over_matches_straight_alpha_and_eraser_halves() {
        // dst: opaque red pixel; mask full coverage, paint blue a=0.5
        let mut dst = vec![255u8, 0, 0, 255];
        let mask = vec![255u8; 1];
        composite_mask_wasm(
            &mut dst, 1, 0, 0, &mask, 1, 0, 0, 1, 1, 0.0, 0.0, 255.0, 0.5, false,
        );
        // out_a = 1; r = (0*0.5 + 255*1*0.5)/1 = 127.5 -> 128; b symmetric -> 128
        assert_eq!(&dst[..4], &[128, 0, 128, 255]);

        // eraser with alpha 0.5 halves existing alpha
        let mut dst2 = vec![10u8, 20, 30, 200];
        composite_mask_wasm(
            &mut dst2, 1, 0, 0, &mask, 1, 0, 0, 1, 1, 0.0, 0.0, 0.0, 0.5, true,
        );
        assert_eq!(dst2[3], 100);
    }
}
