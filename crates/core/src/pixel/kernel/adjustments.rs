// SPDX-License-Identifier: AGPL-3.0-or-later
//! Adjustment, brush-stamp and composite kernels over RGBA planes. Split out
//! of the flood/gradient kernel module so each file stays under the 500-line
//! guard.

// ── Basic adjustment bake  ─────────────────────────────────────────────
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
pub(crate) fn apply_basic_adjustment_impl(
    data: &mut [u8],
    brightness: f64,
    contrast: f64,
    saturation: f64,
) {
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

pub fn apply_basic_adjustment(
    buffer: &[u8],
    brightness: f64,
    contrast: f64,
    saturation: f64,
) -> Vec<u8> {
    let mut out = buffer.to_vec();
    apply_basic_adjustment_impl(&mut out, brightness, contrast, saturation);
    out
}

// ── Porting candidates ────────────────────────────────────────────────
// Benchmarked against TS mirrors in apps/desktop/scripts/bench-cpu-pixel-round2.ts
// Benchmarked against TS mirrors in apps/desktop/scripts/bench-cpu-pixel-round2.ts.

/// Bounding box of non-transparent pixels (alpha > 0).
/// Returns [min_x, min_y, max_x, max_y] (inclusive), or empty when the image is
/// fully transparent. Mirrors SelectionOperations.trimTransparent's scan.
pub fn trim_bbox(pixels: &[u8], width: u32, height: u32) -> Vec<u32> {
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

/// Brush dab stamp — bilinear resample of the precomputed tip into the
/// mask with saturating accumulation `cur + (255-cur)*a`. Mirrors
/// brushTipMask.stampBrushTip exactly (same bounds/clamp semantics).
#[allow(clippy::too_many_arguments)] // FFI boundary keeps flat args for the JS call site
pub fn brush_stamp(
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
#[allow(clippy::too_many_arguments)] // FFI boundary keeps flat args for the JS call site
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

/// Straight-alpha-over composite of a paint mask into an RGBA buffer,
/// restricted to a dirty rect. Mirrors brushTipMask.compositeMaskToImageDataDirty.
#[allow(clippy::too_many_arguments)]
pub fn composite_mask(
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
#[allow(clippy::too_many_arguments)] // FFI boundary keeps flat args for the JS call site
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
