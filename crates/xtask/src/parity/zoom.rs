//! Ink integral + peak luminance measurements (`zoom.ts` semantics).
//!
//! Ink is the integral of luminance above the region's background value -
//! threshold-free, so reclassifying a fringe pixel barely moves it while a
//! genuinely wider or brighter stroke moves it proportionally. The recorded
//! acceptance figures: ink ratio ~1.146x (tools, 120 rows) / ~1.167x (swatch,
//! 80 rows), with identical centre luminance - a rasteriser residual, not a
//! defect.

use std::path::Path;

use super::pngio::RgbaImage;
use super::{round3, Anchor, InkRegionReport, RegionSpec};

/// #1B1B1B, the rail background; ink is measured above this on rail-style
/// regions. Callers may pass another base for other backgrounds.
pub const DEFAULT_BG_LUMA: f64 = 27.0;

/// Rec.601 luma, matching `zoom.ts` / `measure.ts` / `classify` exactly.
fn luma(r: u8, g: u8, b: u8) -> f64 {
    0.2126 * f64::from(r) + 0.7152 * f64::from(g) + 0.0722 * f64::from(b)
}

/// Row where a region's band begins, given its edge anchor (TS `startOf`).
fn start_of(img: &RgbaImage, anchor: Anchor, rows: u32) -> u32 {
    match anchor {
        Anchor::Top => 0,
        Anchor::Bottom => img.height.saturating_sub(rows),
    }
}

/// `(ink, peak_luma)` over `rows` starting at `start_y` across the full width.
///
/// `ink` is the integral of `luma - bg_luma` for pixels that clear the
/// background; `peak` is the brightest luma in the band (a colour proxy: a
/// different tint raises the peak). Bands past the image edge are clipped.
pub fn region_ink(img: &RgbaImage, start_y: u32, rows: u32, bg_luma: f64) -> (f64, f64) {
    let mut ink = 0.0f64;
    let mut peak = 0.0f64;
    let end_y = start_y.saturating_add(rows).min(img.height);
    for y in start_y..end_y {
        for x in 0..img.width {
            let [r, g, b, _] = img.pixel(x, y);
            let l = luma(r, g, b);
            if l > peak {
                peak = l;
            }
            let d = l - bg_luma;
            if d > 0.0 {
                ink += d;
            }
        }
    }
    (ink, peak)
}

/// Per-region ink comparison with edge anchoring (both images anchor to the
/// edge the rows are counted from - not optional: the shells differ in height
/// at the bottom). Returns `Err` on width mismatch or a region that does not
/// fit the image it is measured against.
pub fn compare_anchored(
    baseline: &RgbaImage,
    port: &RgbaImage,
    regions: &[RegionSpec],
    bg_luma: f64,
) -> Result<Vec<InkRegionReport>, String> {
    if baseline.width != port.width {
        return Err(format!(
            "width mismatch for ink comparison: baseline {} vs port {}",
            baseline.width, port.width
        ));
    }
    let mut out = Vec::with_capacity(regions.len());
    for r in regions {
        let sa = start_of(baseline, r.anchor, r.rows);
        let sb = start_of(port, r.anchor, r.rows);
        if sa.saturating_add(r.rows) > baseline.height {
            return Err(format!(
                "region `{}` (anchor {:?}, {} rows) does not fit baseline height {}",
                r.name, r.anchor, r.rows, baseline.height
            ));
        }
        if sb.saturating_add(r.rows) > port.height {
            return Err(format!(
                "region `{}` (anchor {:?}, {} rows) does not fit port height {}",
                r.name, r.anchor, r.rows, port.height
            ));
        }
        let (ink_a, peak_a) = region_ink(baseline, sa, r.rows, bg_luma);
        let (ink_b, peak_b) = region_ink(port, sb, r.rows, bg_luma);
        out.push(ink_report(
            r.name.clone(),
            r.anchor.as_str(),
            ink_a,
            ink_b,
            peak_a,
            peak_b,
        ));
    }
    Ok(out)
}

/// Nearest-neighbour upscale of a row band (`zoom` factor) with alpha forced
/// to 255, matching `zoom.ts`.
#[allow(dead_code)] // helper of the side-by-side panel writer below
fn nn_zoom(src: &RgbaImage, start: u32, rows: u32, factor: u32) -> Result<RgbaImage, String> {
    if factor == 0 {
        return Err("zoom factor must be greater than 0".into());
    }
    let end_y = start.saturating_add(rows).min(src.height);
    let region_h = end_y.saturating_sub(start);
    let out_w = src
        .width
        .checked_mul(factor)
        .ok_or("zoom: width overflow")?;
    let out_h = region_h
        .checked_mul(factor)
        .ok_or("zoom: height overflow")?;
    let mut data = vec![0u8; out_w as usize * out_h as usize * 4];
    for y in 0..region_h {
        for x in 0..src.width {
            let [r, g, b, _] = src.pixel(x, start + y);
            for zy in 0..factor {
                for zx in 0..factor {
                    let i = ((y * factor + zy) as usize * out_w as usize
                        + (x * factor + zx) as usize)
                        * 4;
                    data[i] = r;
                    data[i + 1] = g;
                    data[i + 2] = b;
                    data[i + 3] = 255;
                }
            }
        }
    }
    RgbaImage::from_rgba(out_w, out_h, data)
}

/// Copy `src` into `dst` at column `at_x` (row 0). Caller guarantees it fits.
#[allow(dead_code)] // helper of the side-by-side panel writer below
fn blit(src: &RgbaImage, dst: &mut [u8], dst_w: u32, at_x: u32) {
    for y in 0..src.height {
        for x in 0..src.width {
            let di = ((y as usize) * dst_w as usize + (at_x + x) as usize) * 4;
            let si = ((y as usize) * src.width as usize + x as usize) * 4;
            dst[di..di + 4].copy_from_slice(&src.data[si..si + 4]);
        }
    }
}

/// Nearest-neighbour zoomed side-by-side panel (no smoothing, edges stay
/// legible), written for eyeballing. `zoom` is the integer upscale factor
/// (the TS original used 6). `gap` between the halves is fixed at 12px, as in
/// `zoom.ts`'s `sideBySide`.
#[allow(dead_code)] // review artifact for ink residuals; the parity report does not emit it yet
pub fn write_zoom_panel(
    original: &RgbaImage,
    start_a: u32,
    port: &RgbaImage,
    start_b: u32,
    rows: u32,
    zoom: u32,
    out: &Path,
) -> Result<(), String> {
    if zoom == 0 {
        return Err("zoom factor must be greater than 0".into());
    }
    let left = nn_zoom(original, start_a, rows, zoom)?;
    let right = nn_zoom(port, start_b, rows, zoom)?;
    let gap: u32 = 12;
    let width = left.width + gap + right.width;
    let height = left.height.max(right.height);
    let mut data = vec![0u8; width as usize * height as usize * 4];
    blit(&left, &mut data, width, 0);
    blit(&right, &mut data, width, left.width + gap);
    RgbaImage::from_rgba(width, height, data)?.save(out)
}

/// Ratio assembly with three-decimal rounding, shared by the entry points.
pub(crate) fn ink_report(
    name: String,
    anchor: &'static str,
    ink_original: f64,
    ink_port: f64,
    peak_original: f64,
    peak_port: f64,
) -> InkRegionReport {
    InkRegionReport {
        name,
        anchor: anchor.to_string(),
        ink_original,
        ink_port,
        ratio: round3(ink_port / ink_original),
        peak_original,
        peak_port,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parity::pngio::RgbaImage;

    fn img(w: u32, h: u32, px: [u8; 4]) -> RgbaImage {
        let mut data = vec![0u8; w as usize * h as usize * 4];
        for p in data.chunks_exact_mut(4) {
            p.copy_from_slice(&px);
        }
        RgbaImage::from_rgba(w, h, data).expect("test image")
    }

    #[test]
    fn region_ink_uniform_above_bg() {
        // 4x4 of luma 52, bg 27: each pixel contributes 25, 16 pixels -> ink 400.
        let a = img(4, 4, [52, 52, 52, 255]);
        let (ink, peak) = region_ink(&a, 0, 4, 27.0);
        assert!((ink - 400.0).abs() < 1e-6, "ink {ink}");
        assert!((peak - 52.0).abs() < 1e-6, "peak {peak}");
    }

    #[test]
    fn region_ink_ramp_clips_and_peaks() {
        // width 4, height 1; luma 27, 40, 60, 127; bg 27.
        // ink = 0 (bg) + 13 + 33 + 100 = 146; peak 127.
        let mut data = vec![0u8; 4 * 4];
        let lumas = [27, 40, 60, 127];
        for (i, &v) in lumas.iter().enumerate() {
            let p = &mut data[i * 4..i * 4 + 4];
            p.copy_from_slice(&[v, v, v, 255]);
        }
        let a = RgbaImage::from_rgba(4, 1, data).expect("ramp");
        let (ink, peak) = region_ink(&a, 0, 1, 27.0);
        assert!((ink - 146.0).abs() < 1e-6, "ink {ink}");
        assert!((peak - 127.0).abs() < 1e-6, "peak {peak}");
    }

    #[test]
    fn region_ink_clips_past_edge() {
        let a = img(2, 2, [80, 80, 80, 255]);
        // start_y=1, rows=4 -> only y=1 in range; 2 pixels @ (80-27)=53 -> 106.
        let (ink, _peak) = region_ink(&a, 1, 4, 27.0);
        assert!((ink - 106.0).abs() < 1e-6, "ink {ink}");
    }

    #[test]
    fn compare_anchored_rejects_width_mismatch() {
        let a = img(4, 4, [52, 52, 52, 255]);
        let b = img(8, 4, [52, 52, 52, 255]);
        let regions = vec![RegionSpec {
            name: "t".into(),
            anchor: Anchor::Top,
            rows: 4,
        }];
        assert!(compare_anchored(&a, &b, &regions, 27.0).is_err());
    }
}
