// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Native compute kernels over RGBA planes. Canonical shape: buffer-in
// (`&[u8]` / `&mut [u8]`), buffer-out (`Vec<u8>` or in-place). The flood-fill,
// gradient-fill and adjustments-bake kernels all follow it.

/// Flood-fill mask (rect or ellipse, optionally inverted) - the fill
/// contract used by the Paint Bucket tool. plain struct.
pub(crate) struct FillMaskRust {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) w: i32,
    pub(crate) h: i32,
    pub(crate) shape: u8, // 0 = rect/none, 1 = ellipse
    pub(crate) inverted: bool,
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

/// Pure flood-fill over an RGBA byte buffer: tolerance is the squared RGBA
/// distance, with an optional rect/ellipse mask and a contiguous queue BFS vs
/// global replace.
/// Returns true if any pixel changed. private — tests live in-module.
#[allow(clippy::too_many_arguments)] // flat args keep the fill command surface simple
pub(crate) fn flood_fill_impl(
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

/// Native entry point for flood fill. Copies the input RGBA buffer, runs the pure
/// kernel, returns the filled copy (same buffer-out contract as the other pointwise kernels).
/// scalar queue BFS; SIMD not applicable to 4-connected flood.
#[allow(clippy::too_many_arguments)]
pub fn flood_fill(
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

// ── Gradient fill  ─────────────────────────────────────────────────────
// Linear/radial gradient fill over RGBA, masked, with color stops interpolated
// by offset.
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
#[allow(clippy::too_many_arguments)] // flat args keep the fill command surface simple
pub(crate) fn gradient_fill_impl(
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
            let tc = t.clamp(0.0, 1.0);
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

#[allow(clippy::too_many_arguments)]
pub fn gradient_fill(
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

mod adjustments;

pub(crate) use adjustments::apply_basic_adjustment_impl;
pub use adjustments::{apply_basic_adjustment, brush_stamp, composite_mask, trim_bbox};
