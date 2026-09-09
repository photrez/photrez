//! R1.5 — Bottleneck attribution experiments (measurement only, NOT production paths).
//!
//! Isolates where the shadow pipeline spends time on large workloads:
//!   blend (raster)      — dab compositing into the premultiplied buffer
//!   extraction alloc    — per-tile Vec allocations
//!   unpremultiply math  — fixed-point pm→straight conversion
//!   tile traversal/copy — row-major gather into tile layouts
//!   parallelism         — rayon scaling experiment (explicitly non-committal)

use crate::paint_parity::{ParityDab, ParityTip, TilePatchOut};
use std::time::Instant;

pub struct BlendResult {
    pub buf: Vec<u8>,
    pub blend_us: u128,
    pub touched_pixels: u64,
}

// ── experiment only — row-band parallel blend (no production use yet)
pub fn blend_par_row_bands(
    w: usize,
    h: usize,
    prep_white: bool,
    eraser: bool,
    brush: f64,
    dabs: &[ParityDab],
    tip: &ParityTip,
) -> BlendResult {
    use rayon::prelude::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    let t0 = Instant::now();
    let mut buf = vec![0u8; w * h * 4];
    let base: [u8; 3] = if prep_white {
        [255, 255, 255]
    } else {
        [30, 120, 220]
    };
    for i in 0..(w * h) {
        buf[i * 4] = base[0];
        buf[i * 4 + 1] = base[1];
        buf[i * 4 + 2] = base[2];
        buf[i * 4 + 3] = 255;
    }
    let r = brush / 2.0;
    let iw = tip.width;
    let ih = tip.height;
    let touched = AtomicU64::new(0);
    let band_rows: usize = 64;
    let stride = band_rows * w * 4;
    buf.par_chunks_mut(stride)
        .enumerate()
        .for_each(|(band_idx, chunk)| {
            let y0_band = band_idx * band_rows;
            let y1_band = (y0_band + band_rows).min(h);
            let mut local_touched: u64 = 0;
            for d in dabs {
                let ox = (d.x - r).round() as i64;
                let oy = (d.y - r).round() as i64;
                let px0 = ox.clamp(0, w as i64);
                let py0 = oy.clamp(0, h as i64);
                let px1 = (ox + brush as i64).clamp(0, w as i64);
                let py1 = (oy + brush as i64).clamp(0, h as i64);
                let py_s = py0.max(y0_band as i64);
                let py_e = py1.min(y1_band as i64);
                if py_s >= py_e {
                    continue;
                }
                for py in py_s..py_e {
                    let local_y = (py - y0_band as i64) as usize;
                    for px in px0..px1 {
                        let txx = (px - ox) as usize;
                        let tyy = (py - oy) as usize;
                        if txx >= iw || tyy >= ih {
                            continue;
                        }
                        let ti = (tyy * iw + txx) * 4;
                        let sa = tip.data[ti + 3] as u64;
                        let inv = 255 - sa;
                        if sa == 0 && !eraser {
                            continue;
                        }
                        local_touched += 1;
                        let di = (local_y * w + px as usize) * 4;
                        if eraser {
                            for ch in 0..4usize {
                                chunk[di + ch] = ((chunk[di + ch] as u64 * inv + 127) / 255) as u8;
                            }
                        } else {
                            for ch in 0..3usize {
                                let sc = (tip.data[ti + ch] as u64 * sa + 127) / 255;
                                let dc = chunk[di + ch] as u64;
                                chunk[di + ch] = (sc + (dc * inv + 127) / 255).min(255) as u8;
                            }
                            chunk[di + 3] =
                                (sa + ((chunk[di + 3] as u64 * inv + 127) / 255)).min(255) as u8;
                        }
                    }
                }
            }
            touched.fetch_add(local_touched, Ordering::Relaxed);
        });
    BlendResult {
        buf,
        blend_us: t0.elapsed().as_micros(),
        touched_pixels: touched.load(Ordering::Relaxed),
    }
}

/// Blend stage isolated from prep/patchgen (same math as raster_shadow).
pub fn blend_only(
    w: usize,
    h: usize,
    prep_white: bool,
    eraser: bool,
    brush: f64,
    dabs: &[ParityDab],
    tip: &ParityTip,
) -> BlendResult {
    let t0 = Instant::now();
    let mut buf = vec![0u8; w * h * 4];
    let base: [u8; 3] = if prep_white {
        [255, 255, 255]
    } else {
        [30, 120, 220]
    };
    for i in 0..(w * h) {
        buf[i * 4] = base[0];
        buf[i * 4 + 1] = base[1];
        buf[i * 4 + 2] = base[2];
        buf[i * 4 + 3] = 255;
    }
    let r = brush / 2.0;
    let iw = tip.width;
    let ih = tip.height;
    let mut touched: u64 = 0;
    for d in dabs {
        let ox = (d.x - r).round() as i64;
        let oy = (d.y - r).round() as i64;
        let px0 = ox.clamp(0, w as i64);
        let py0 = oy.clamp(0, h as i64);
        let px1 = (ox + brush as i64).clamp(0, w as i64);
        let py1 = (oy + brush as i64).clamp(0, h as i64);
        for py in py0..py1 {
            for px in px0..px1 {
                let txx = (px - ox) as usize;
                let tyy = (py - oy) as usize;
                if txx >= iw || tyy >= ih {
                    continue;
                }
                let ti = (tyy * iw + txx) * 4;
                let sa = tip.data[ti + 3] as u64;
                let di = (py as usize * w + px as usize) * 4;
                let inv = 255 - sa;
                if sa == 0 && !eraser {
                    continue;
                }
                touched += 1;
                if eraser {
                    for ch in 0..4usize {
                        buf[di + ch] = ((buf[di + ch] as u64 * inv + 127) / 255) as u8;
                    }
                } else {
                    for ch in 0..3usize {
                        let sc = (tip.data[ti + ch] as u64 * sa + 127) / 255;
                        let dc = buf[di + ch] as u64;
                        buf[di + ch] = (sc + (dc * inv + 127) / 255).min(255) as u8;
                    }
                    buf[di + 3] = (sa + ((buf[di + 3] as u64 * inv + 127) / 255)).min(255) as u8;
                }
            }
        }
    }
    BlendResult {
        buf,
        blend_us: t0.elapsed().as_micros(),
        touched_pixels: touched,
    }
}

struct TileRef2 {
    x: i64,
    y: i64,
    w: usize,
    h: usize,
    key: String,
}

fn tiles_for_bbox(x0: i64, y0: i64, x1: i64, y1: i64, w: i64, h: i64) -> Vec<TileRef2> {
    let t = crate::paint_parity::tiles_in_rect(x0, y0, x1, y1, w, h);
    t.into_iter()
        .map(|t| TileRef2 {
            key: t.key,
            x: t.x,
            y: t.y,
            w: t.w,
            h: t.h,
        })
        .collect()
}

fn unpremul_into(dst: &mut Vec<u8>, buf: &[u8], i: usize) {
    let a = buf[i + 3] as u32;
    if a == 0 {
        dst.extend_from_slice(&[0, 0, 0, 0]);
        return;
    }
    let half = a / 2;
    dst.push(((buf[i] as u32 * 255 + half) / a) as u8);
    dst.push(((buf[i + 1] as u32 * 255 + half) / a) as u8);
    dst.push(((buf[i + 2] as u32 * 255 + half) / a) as u8);
    dst.push(buf[i + 3]);
}

pub struct ExtractionVariants {
    pub baseline: Vec<TilePatchOut>,
    pub prealloc: Vec<TilePatchOut>,
    pub raw_copy_pm: Vec<TilePatchOut>,
    pub rayon_par: Vec<TilePatchOut>,
    pub us_baseline: u128,
    pub us_prealloc: u128,
    pub us_raw_copy: u128,
    pub us_rayon: u128,
}

/// Extraction variants over an ALREADY-BLENDED buffer (isolates patchGen cost).
pub fn extraction_variants(
    buf: &[u8],
    w: usize,
    h: usize,
    x0: i64,
    y0: i64,
    x1: i64,
    y1: i64,
) -> ExtractionVariants {
    use rayon::prelude::*;
    let tiles = tiles_for_bbox(x0, y0, x1, y1, w as i64, h as i64);

    // Baseline: identical algorithm to raster_shadow's readback stage.
    let t = Instant::now();
    let baseline: Vec<TilePatchOut> = tiles
        .iter()
        .map(|t| {
            let mut data = Vec::with_capacity(t.w * t.h * 4);
            for row in 0..t.h as i64 {
                for col in 0..t.w as i64 {
                    let i = ((t.y + row) * w as i64 + (t.x + col)) as usize * 4;
                    unpremul_into(&mut data, buf, i);
                }
            }
            TilePatchOut {
                key: t.key.clone(),
                x: t.x,
                y: t.y,
                w: t.w,
                h: t.h,
                data,
            }
        })
        .collect();
    let us_baseline = t.elapsed().as_micros();

    // Prealloc: one contiguous output buffer, then per-tile range copies.
    let t = Instant::now();
    let total: usize = tiles.iter().map(|t| t.w * t.h * 4).sum();
    let mut flat: Vec<u8> = Vec::with_capacity(total);
    for t in &tiles {
        for row in 0..t.h as i64 {
            for col in 0..t.w as i64 {
                let i = ((t.y + row) * w as i64 + (t.x + col)) as usize * 4;
                unpremul_into(&mut flat, buf, i);
            }
        }
    }
    let mut off = 0usize;
    let prealloc: Vec<TilePatchOut> = tiles
        .iter()
        .map(|t| {
            let len = t.w * t.h * 4;
            let out = TilePatchOut {
                key: t.key.clone(),
                x: t.x,
                y: t.y,
                w: t.w,
                h: t.h,
                data: flat[off..off + len].to_vec(),
            };
            off += len;
            out
        })
        .collect();
    let us_prealloc = t.elapsed().as_micros();

    // Raw PM copy: NO unpremultiply math — isolates traversal+allocation cost.
    let t = Instant::now();
    let raw: Vec<TilePatchOut> = tiles
        .iter()
        .map(|t| {
            let mut data = Vec::with_capacity(t.w * t.h * 4);
            for row in 0..t.h as i64 {
                for col in 0..t.w as i64 {
                    let i = ((t.y + row) * w as i64 + (t.x + col)) as usize * 4;
                    data.extend_from_slice(&buf[i..i + 4]);
                }
            }
            TilePatchOut {
                key: t.key.clone(),
                x: t.x,
                y: t.y,
                w: t.w,
                h: t.h,
                data,
            }
        })
        .collect();
    let us_raw_copy = t.elapsed().as_micros();

    // Rayon: baseline algorithm parallelized across tiles (experiment only).
    let t = Instant::now();
    let par: Vec<TilePatchOut> = tiles
        .par_iter()
        .map(|t| {
            let mut data = Vec::with_capacity(t.w * t.h * 4);
            for row in 0..t.h as i64 {
                for col in 0..t.w as i64 {
                    let i = ((t.y + row) * w as i64 + (t.x + col)) as usize * 4;
                    unpremul_into(&mut data, buf, i);
                }
            }
            TilePatchOut {
                key: t.key.clone(),
                x: t.x,
                y: t.y,
                w: t.w,
                h: t.h,
                data,
            }
        })
        .collect();
    let us_rayon = t.elapsed().as_micros();

    ExtractionVariants {
        baseline,
        prealloc,
        raw_copy_pm: raw,
        rayon_par: par,
        us_baseline,
        us_prealloc,
        us_raw_copy,
        us_rayon,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint_parity::raster_shadow;

    struct TipBuf {
        data: Vec<u8>,
    }

    fn tip_owned(buf: &mut TipBuf, size: usize, rgba: [u8; 4]) -> ParityTip<'_> {
        buf.data = vec![0u8; size * size * 4];
        for i in 0..(size * size) {
            buf.data[i * 4..i * 4 + 4].copy_from_slice(&rgba);
        }
        ParityTip {
            width: size,
            height: size,
            data: &buf.data,
        }
    }

    /// Stress-scale attribution: 6912×3888, brush 3000, 10 dabs (v2-matrix mirror).
    #[test]
    #[ignore = "run explicitly: cargo test -p photrez-core --release r15_stress_attribution -- --ignored --nocapture"]
    fn r15_stress_attribution() {
        let mut tb = TipBuf { data: Vec::new() };
        // Soft radial tip approximating the real 3000px mask (alpha ramp).
        let size = 3000usize;
        tb.data = vec![0u8; size * size * 4];
        let r = size as f32 / 2.0;
        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 - r;
                let dy = y as f32 - r;
                let dist = (dx * dx + dy * dy).sqrt();
                let a = if dist >= r {
                    0.0
                } else if dist < r * 0.8 {
                    1.0
                } else {
                    1.0 - (dist - r * 0.8) / (r * 0.2)
                };
                let i = (y * size + x) * 4;
                tb.data[i] = 200;
                tb.data[i + 1] = 50;
                tb.data[i + 2] = 50;
                tb.data[i + 3] = (a * 229.5) as u8; // pressure 0.9
            }
        }
        let tip = ParityTip {
            width: size,
            height: size,
            data: &tb.data,
        };

        let mut seed: u64 = 0x5eed;
        let mut rnd = move || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            ((seed >> 8) & 0xFF_FFFF) as f64 / 16_777_216.0
        };
        let dabs: Vec<ParityDab> = (0..10)
            .map(|_| ParityDab {
                x: rnd() * 6912.0,
                y: rnd() * 3888.0,
                alpha: 0.75,
            })
            .collect();

        let blend = blend_only(6912, 3888, true, false, 3000.0, &dabs, &tip);
        println!(
            "blend_us={} touched_px={}",
            blend.blend_us, blend.touched_pixels
        );
        assert!(
            blend.touched_pixels > 1_000_000,
            "zero work - dab generation broken"
        );
        assert!(!dabs.is_empty());
        // experiment only — blend row-band parallel variant
        let blend_par = blend_par_row_bands(6912, 3888, true, false, 3000.0, &dabs, &tip);
        println!(
            "blend_par_us={} touched_px_par={}",
            blend_par.blend_us, blend_par.touched_pixels
        );
        assert_eq!(blend.buf, blend_par.buf, "blend_par byte mismatch");
        assert_eq!(
            blend.touched_pixels, blend_par.touched_pixels,
            "touched count mismatch"
        );

        let margin = 1502i64;
        let minx = dabs.iter().fold(f64::INFINITY, |m, d| m.min(d.x)) as i64 - margin;
        let miny = dabs.iter().fold(f64::INFINITY, |m, d| m.min(d.y)) as i64 - margin;
        let maxx = dabs.iter().fold(0f64, |m, d| m.max(d.x)) as i64 + margin;
        let maxy = dabs.iter().fold(0f64, |m, d| m.max(d.y)) as i64 + margin;

        let v = extraction_variants(
            &blend.buf,
            6912,
            3888,
            minx.max(0),
            miny.max(0),
            maxx.min(6912),
            maxy.min(3888),
        );
        println!(
            "extract_us: baseline={} prealloc={} raw_pm_copy={} rayon={}",
            v.us_baseline, v.us_prealloc, v.us_raw_copy, v.us_rayon
        );
        // Correctness: parallel + prealloc must be byte-identical to baseline.
        for (b, p) in v.baseline.iter().zip(v.rayon_par.iter()) {
            assert_eq!(b.key, p.key);
            assert_eq!(b.data, p.data);
        }
        for (b, p) in v.baseline.iter().zip(v.prealloc.iter()) {
            assert_eq!(b.data, p.data);
        }
        // Full-pipeline cross-check against raster_shadow digest stability.
        let (meta, _) = raster_shadow(6912, 3888, true, None, false, 3000.0, &dabs, &tip, false);
        println!(
            "digest={} tiles={} changedMB={:.1}",
            meta.digest,
            meta.tile_count,
            meta.changed_bytes as f64 / 1e6
        );
    }

    /// Raw stage-by-stage reconciliation for the 10.6s -> ~0.9-1.2s projection.
    /// Methodology: identical to r15_stress_attribution (same workload, tip, seed,
    /// LCG, margin, Instant::now micros, release build). Reports per-stage raw
    /// durations and explicit subtotal/total so the projection reconciles.
    #[test]
    #[ignore = "run explicitly: cargo test -p photrez-core --release r15_projection_reconciliation -- --ignored --nocapture"]
    fn r15_projection_reconciliation() {
        let mut tb = TipBuf { data: Vec::new() };
        let size = 3000usize;
        tb.data = vec![0u8; size * size * 4];
        let r = size as f32 / 2.0;
        for y in 0..size {
            for x in 0..size {
                let dx = x as f32 - r;
                let dy = y as f32 - r;
                let dist = (dx * dx + dy * dy).sqrt();
                let a = if dist >= r {
                    0.0
                } else if dist < r * 0.8 {
                    1.0
                } else {
                    1.0 - (dist - r * 0.8) / (r * 0.2)
                };
                let i = (y * size + x) * 4;
                tb.data[i] = 200;
                tb.data[i + 1] = 50;
                tb.data[i + 2] = 50;
                tb.data[i + 3] = (a * 229.5) as u8;
            }
        }
        let tip = ParityTip {
            width: size,
            height: size,
            data: &tb.data,
        };
        let mut seed: u64 = 0x5eed;
        let mut rnd = move || {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            ((seed >> 8) & 0xFF_FFFF) as f64 / 16_777_216.0
        };
        let dabs: Vec<ParityDab> = (0..10)
            .map(|_| ParityDab {
                x: rnd() * 6912.0,
                y: rnd() * 3888.0,
                alpha: 0.75,
            })
            .collect();
        // Full-pipeline serial via raster_shadow (prep + raster + patchgen) — single source of truth for stage split
        let (meta_serial, _) =
            raster_shadow(6912, 3888, true, None, false, 3000.0, &dabs, &tip, true);
        let serial_total_us = meta_serial.prep_us + meta_serial.raster_us + meta_serial.patchgen_us;
        println!("serial_full: prep_us={} raster_us={} patchgen_us={} total_us={} tiles={} changedMB={:.1} digest={}", meta_serial.prep_us, meta_serial.raster_us, meta_serial.patchgen_us, serial_total_us, meta_serial.tile_count, meta_serial.changed_bytes as f64/1e6, meta_serial.digest);
        // Isolated stages for projection
        let blend = blend_only(6912, 3888, true, false, 3000.0, &dabs, &tip);
        let blend_par = blend_par_row_bands(6912, 3888, true, false, 3000.0, &dabs, &tip);
        assert_eq!(blend.buf, blend_par.buf);
        let margin = 1502i64;
        let minx = dabs.iter().fold(f64::INFINITY, |m, d| m.min(d.x)) as i64 - margin;
        let miny = dabs.iter().fold(f64::INFINITY, |m, d| m.min(d.y)) as i64 - margin;
        let maxx = dabs.iter().fold(0f64, |m, d| m.max(d.x)) as i64 + margin;
        let maxy = dabs.iter().fold(0f64, |m, d| m.max(d.y)) as i64 + margin;
        let v = extraction_variants(
            &blend.buf,
            6912,
            3888,
            minx.max(0),
            miny.max(0),
            maxx.min(6912),
            maxy.min(3888),
        );
        let proj_serial_us = blend.blend_us + v.us_baseline;
        let proj_parallel_us = blend_par.blend_us + v.us_rayon;
        println!("isolated: blend_us={} blend_par_us={} extract_baseline_us={} extract_rayon_us={} raw_copy_us={} prealloc_us={}", blend.blend_us, blend_par.blend_us, v.us_baseline, v.us_rayon, v.us_raw_copy, v.us_prealloc);
        println!(
            "projection: serial_isolated_total_us={} parallel_isolated_total_us={} delta_us={}",
            proj_serial_us,
            proj_parallel_us,
            proj_serial_us as i128 - proj_parallel_us as i128
        );
        println!("reconciliation: serial_full==isolated? full_total_us={} isolated_total_us={} diff_us={}", serial_total_us, proj_serial_us, serial_total_us as i128 - proj_serial_us as i128);
        // Transport is external to Rust (harness wall - compute); prior in-shell stress transport 37ms (wall 10.6s debug build) — not re-measured here, reported as separate stage
        println!("note: transportAndSerMs is external (harness wall - compute), prior stress wall transport ~37ms in debug harness; not included in Rust us totals above");
    }

    /// Small-scale determinism guard for the variants themselves (always runs).
    #[test]
    fn r15_variants_equal_small() {
        let mut tb = TipBuf { data: Vec::new() };
        let tip = tip_owned(&mut tb, 24, [200, 50, 50, 220]);
        let dabs: Vec<ParityDab> = (0..8)
            .map(|i| ParityDab {
                x: (i * 61 % 400) as f64,
                y: (i * 113 % 300) as f64,
                alpha: 0.8,
            })
            .collect();
        let blend = blend_only(512, 512, true, false, 48.0, &dabs, &tip);
        let blend_par = blend_par_row_bands(512, 512, true, false, 48.0, &dabs, &tip);
        assert_eq!(blend.buf, blend_par.buf);
        assert_eq!(blend.touched_pixels, blend_par.touched_pixels);
        let v = extraction_variants(&blend.buf, 512, 512, 0, 0, 512, 512);
        for (b, p) in v.baseline.iter().zip(v.rayon_par.iter()) {
            assert_eq!(b.key, p.key);
            assert_eq!(b.data, p.data);
        }
        for (b, p) in v.baseline.iter().zip(v.prealloc.iter()) {
            assert_eq!(b.data, p.data);
        }
    }
}
