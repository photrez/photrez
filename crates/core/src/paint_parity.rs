//! R1 SHADOW — Rust candidate rasterizer for the pixel-parity harness.
//! DESIGN scope (2026-08-24): TS remains authoritative; this module produces
//! CANDIDATE tile patches + digests only. No production pixel ownership,
//! no ResourceRegistry / TileStore / resource_read_* naming, no zero-copy.
//!
//! Digest pipeline mirrors apps/desktop/src/lib/benchPixelParity.ts EXACTLY:
//!   key = "tx,ty"; combined = sorted("key:fnv1a(bytes);"...) ; digest = fnv1a(utf8(combined))

/// FNV-1a (32-bit) — must match the TS implementation bit-for-bit.
pub fn fnv1a(bytes: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for &b in bytes {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

fn mul_div_255(c: u64, a: u64) -> u8 {
    ((c * a + 127) / 255) as u8
}

#[derive(Clone, Copy, Debug)]
pub struct ParityDab {
    pub x: f64,
    pub y: f64,
    pub alpha: f64,
}

/// Straight (non-premultiplied) RGBA tip bitmap — produced by the TS-owned tip cache.
pub struct ParityTip<'a> {
    pub width: usize,
    pub height: usize,
    pub data: &'a [u8],
}

#[derive(Debug, Clone)]
pub struct TilePatchOut {
    pub key: String,
    pub x: i64,
    pub y: i64,
    pub w: usize,
    pub h: usize,

    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct ShadowMeta {
    pub digest: String,
    pub tile_count: usize,
    pub changed_bytes: usize,
    pub prep_us: u128,
    pub raster_us: u128,
    pub patchgen_us: u128,
    /// Sorted-by-key "(key:hash)" pairs — lets JS diff WITHOUT shipping bytes.
    pub tile_hashes: Vec<(String, String)>,
}

const TILE: i64 = 256;

pub struct TileRef {
    pub x: i64,
    pub y: i64,
    pub w: usize,
    pub h: usize,
    pub key: String,
}

pub fn tiles_in_rect(x0: i64, y0: i64, x1: i64, y1: i64, w: i64, h: i64) -> Vec<TileRef> {
    // Mirrors tilesInRect(): cx0..cx1 inclusive, edge tiles clamped.
    let cx0 = 0i64.max(x0.div_euclid(TILE));
    let cy0 = 0i64.max(y0.div_euclid(TILE));
    let max_tx = (((w as f64) / TILE as f64).ceil() as i64 - 1).max(0);
    let max_ty = (((h as f64) / TILE as f64).ceil() as i64 - 1).max(0);
    let cx1 = max_tx.min((x1 - 1).div_euclid(TILE));
    let cy1 = max_ty.min((y1 - 1).div_euclid(TILE));
    let mut out = Vec::new();
    let mut ty = cy0;
    while ty <= cy1 {
        let mut tx = cx0;
        while tx <= cx1 {
            let x = tx * TILE;
            let y = ty * TILE;
            out.push(TileRef {
                x,
                y,
                w: (TILE.min(w - x)).max(0) as usize,
                h: (TILE.min(h - y)).max(0) as usize,
                key: format!("{tx},{ty}"),
            });
            tx += 1;
        }
        ty += 1;
    }
    out
}

fn unpremul(buf: &[u8], i: usize) -> [u8; 4] {
    let a = buf[i + 3] as u32;
    if a == 0 {
        return [0, 0, 0, 0];
    }
    let half = a / 2;
    [
        ((buf[i] as u32 * 255 + half) / a) as u8,
        ((buf[i + 1] as u32 * 255 + half) / a) as u8,
        ((buf[i + 2] as u32 * 255 + half) / a) as u8,
        buf[i + 3],
    ]
}

/// Rasterize the dab batch onto a fresh buffer and extract candidate tile patches.
/// `include_tiles` ships full bytes (diagnostics / small cases only).
/// `base`: when `Some`, the raster composites the dab batch onto these EXISTING
/// pixels (the canonical-commit path). When `None`, the buffer is initialized from
/// `prep_white` (white) or the blue sentinel (diagnostic parity shadow).
#[allow(clippy::too_many_arguments)] // flat args mirror the shadow-raster command struct
pub fn raster_shadow(
    w: usize,
    h: usize,
    prep_white: bool,
    base: Option<Vec<u8>>,
    eraser: bool,
    brush: f64,
    dabs: &[ParityDab],
    tip: &ParityTip,
    include_tiles: bool,
) -> (ShadowMeta, Vec<TilePatchOut>) {
    let t0 = std::time::Instant::now();
    let mut buf = match base {
        Some(b) => b,
        None => {
            let mut b = vec![0u8; w * h * 4];
            for i in 0..(w * h) {
                let (r, g, bcol) = if prep_white {
                    (255u8, 255u8, 255u8)
                } else {
                    (30u8, 120u8, 220u8)
                };
                b[i * 4] = r;
                b[i * 4 + 1] = g;
                b[i * 4 + 2] = bcol;
                b[i * 4 + 3] = 255;
            }
            b
        }
    };
    let prep_us = t0.elapsed().as_micros();

    // ── Raster (premultiplied src-over / destination-out, integer fixed-point) ──
    let t1 = std::time::Instant::now();
    let r = brush / 2.0;
    let iw = tip.width;
    let ih = tip.height;
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
                if eraser {
                    for ch in 0..4usize {
                        buf[di + ch] = mul_div_255(buf[di + ch] as u64, inv);
                    }
                } else if sa > 0 {
                    for ch in 0..3usize {
                        let sc = mul_div_255(tip.data[ti + ch] as u64, sa) as u64;
                        let dc = buf[di + ch] as u64;
                        buf[di + ch] = (sc + mul_div_255(dc, inv) as u64).min(255) as u8;
                    }
                    buf[di + 3] = (sa + mul_div_255(buf[di + 3] as u64, inv) as u64).min(255) as u8;
                }
            }
        }
    }
    let raster_us = t1.elapsed().as_micros();

    // ── Dirty bbox + tile extraction (mirrors harness margins exactly) ──
    let t2 = std::time::Instant::now();
    let (mut minx, mut miny, mut maxx, mut maxy) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    for d in dabs {
        minx = minx.min(d.x);
        miny = miny.min(d.y);
        maxx = maxx.max(d.x);
        maxy = maxy.max(d.y);
    }
    let margin = (brush / 2.0).ceil() as i64 + 2;
    let x0 = 0i64.max((minx.floor() as i64) - margin);
    let y0 = 0i64.max((miny.floor() as i64) - margin);
    let x1 = (w as i64).min((maxx.ceil() as i64) + margin);
    let y1 = (h as i64).min((maxy.ceil() as i64) + margin);

    let mut tile_patches: Vec<(String, i64, i64, usize, usize, Vec<u8>)> = Vec::new();
    for tref in tiles_in_rect(x0, y0, x1, y1, w as i64, h as i64) {
        let mut data = Vec::with_capacity(tref.w * tref.h * 4);
        for row in 0..tref.h as i64 {
            for col in 0..tref.w as i64 {
                let i = ((tref.y + row) * w as i64 + (tref.x + col)) as usize * 4;
                data.extend_from_slice(&unpremul(&buf, i));
            }
        }
        tile_patches.push((tref.key, tref.x, tref.y, tref.w, tref.h, data));
    }
    let mut tile_hashes: Vec<(String, String)> = tile_patches
        .iter()
        .map(|(k, _, _, _, _, d)| (k.clone(), format!("{:08x}", fnv1a(d))))
        .collect();
    tile_hashes.sort_by(|a, b| a.0.cmp(&b.0));
    let combined: String = tile_hashes
        .iter()
        .map(|(k, h)| format!("{k}:{h};"))
        .collect();
    let digest = format!("{:08x}", fnv1a(combined.as_bytes()));
    let changed_bytes: usize = tile_patches.iter().map(|(_, _, _, _, _, d)| d.len()).sum();
    let patchgen_us = t2.elapsed().as_micros();

    let mut out_tiles = Vec::new();
    if include_tiles {
        for (key, x, y, tw, th, data) in &tile_patches {
            out_tiles.push(TilePatchOut {
                key: key.clone(),
                x: *x,
                y: *y,
                w: *tw,
                h: *th,
                data: data.clone(),
            });
        }
    }

    (
        ShadowMeta {
            digest,
            tile_count: tile_hashes.len(),
            changed_bytes,
            prep_us,
            raster_us,
            patchgen_us,
            tile_hashes,
        },
        out_tiles,
    )
}

/// Deterministic re-run returning ONLY the requested keys' patches (diff diagnostics).
#[allow(clippy::too_many_arguments)] // flat args mirror the tile-key query command struct
pub fn tiles_for_keys(
    w: usize,
    h: usize,
    prep_white: bool,
    eraser: bool,
    brush: f64,
    dabs: &[ParityDab],
    tip: &ParityTip,
    keys: &[String],
) -> Vec<TilePatchOut> {
    let (_, all) = raster_shadow(w, h, prep_white, None, eraser, brush, dabs, tip, true);
    all.into_iter().filter(|t| keys.contains(&t.key)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Soft round stamp (diameter `size`) — a solid disk of stamp color so the
    /// raster inner loop does the full `size*size` work per dab (a shrinking
    /// tip would short-circuit via `txx >= iw || tyy >= ih` and under-report).
    fn soft_tip(buf: &mut TipBuf, size: usize) -> ParityTip<'_> {
        buf.data = vec![0u8; size * size * 4];
        let r = size as f64 / 2.0;
        for y in 0..size {
            for x in 0..size {
                let dx = x as f64 + 0.5 - r;
                let dy = y as f64 + 0.5 - r;
                let d = (dx * dx + dy * dy).sqrt();
                let a = if d <= r { 255u8 } else { 0u8 };
                let i = (y * size + x) * 4;
                buf.data[i..i + 4].copy_from_slice(&[225, 90, 23, a]);
            }
        }
        ParityTip {
            width: size,
            height: size,
            data: &buf.data,
        }
    }

    /// Perf baseline for Rust dab-semantics commit cost. `#[ignore]`d so normal
    /// `cargo test` is unaffected. Run with:
    ///   cargo test -p photrez-core --release raster_shadow_bench -- --ignored --nocapture
    /// Measures the canonical-commit path (base=Some: composite onto existing
    /// pixels, so prep is skipped). `raster_us`/`patchgen_us` are self-reported
    /// by ShadowMeta; wall includes the base Vec clone (allocation overhead).
    #[test]
    #[ignore]
    fn raster_shadow_bench_scale() {
        let w = 1024usize;
        let h = 1024usize;
        let base: Vec<u8> = (0..(w * h * 4))
            .map(|i| if i % 4 == 3 { 255u8 } else { 120u8 })
            .collect();
        let brushes: [f64; 4] = [32.0, 64.0, 128.0, 256.0];
        let dab_counts: [usize; 4] = [4, 16, 32, 64];
        let mean = |v: &[u128]| v.iter().sum::<u128>() / v.len() as u128;
        println!("\nraster_shadow baseline (canonical commit: base=Some, 1024x1024)");
        println!("brush  dabs   raster_us  patchgen_us  wall_us   p50_rast  p95_rast");
        for brush in brushes {
            let bs = brush as usize;
            let mut tb = TipBuf { data: Vec::new() };
            let tip = soft_tip(&mut tb, bs);
            for &ncnt in &dab_counts {
                let dabs: Vec<ParityDab> = (0..ncnt)
                    .map(|i| {
                        let t = i as f64 / (ncnt.max(1) as f64);
                        ParityDab {
                            x: 100.0 + t * (w as f64 - 200.0),
                            y: 512.0,
                            alpha: 0.75,
                        }
                    })
                    .collect();
                // warmup
                let _ = raster_shadow(
                    w,
                    h,
                    true,
                    Some(base.clone()),
                    false,
                    brush,
                    &dabs,
                    &tip,
                    false,
                );
                let mut rast = Vec::with_capacity(12);
                let mut patch = Vec::with_capacity(12);
                let mut wall = Vec::with_capacity(12);
                for _ in 0..12 {
                    let s = std::time::Instant::now();
                    let (meta, _) = raster_shadow(
                        w,
                        h,
                        true,
                        Some(base.clone()),
                        false,
                        brush,
                        &dabs,
                        &tip,
                        false,
                    );
                    wall.push(s.elapsed().as_micros());
                    rast.push(meta.raster_us);
                    patch.push(meta.patchgen_us);
                }
                rast.sort();
                patch.sort();
                wall.sort();
                println!(
                    "{:>5} {:<6} {:>10} {:>12} {:>9} {:>9} {:>9}",
                    bs,
                    ncnt,
                    mean(&rast),
                    mean(&patch),
                    mean(&wall),
                    rast[rast.len() / 2],
                    rast[(rast.len() * 95 / 100).min(rast.len() - 1)]
                );
            }
        }
    }

    #[test]
    fn determinism_same_inputs_same_digest() {
        let mut tb = TipBuf { data: Vec::new() };
        let tip = tip_owned(&mut tb, 16, [200, 50, 50, 255]);
        let dabs: Vec<ParityDab> = (0..20)
            .map(|i| ParityDab {
                x: ((i * 37) % 512) as f64,
                y: ((i * 91) % 512) as f64,
                alpha: 0.75,
            })
            .collect();
        let (a, _) = raster_shadow(512, 512, true, None, false, 32.0, &dabs, &tip, false);
        let (b, _) = raster_shadow(512, 512, true, None, false, 32.0, &dabs, &tip, false);
        assert_eq!(a.digest, b.digest);
        assert_eq!(a.tile_count, b.tile_count);
    }

    #[test]
    fn painted_vs_white_bases_differ() {
        let mut tb = TipBuf { data: Vec::new() };
        let tip = tip_owned(&mut tb, 8, [200, 50, 50, 200]);
        let dabs = vec![ParityDab {
            x: 100.0,
            y: 100.0,
            alpha: 1.0,
        }];
        let (w1, _) = raster_shadow(256, 256, true, None, false, 40.0, &dabs, &tip, false);
        let (p1, _) = raster_shadow(256, 256, false, None, false, 40.0, &dabs, &tip, false);
        assert_ne!(w1.digest, p1.digest);
    }

    #[test]
    fn eraser_clears_alpha_on_painted_base() {
        let mut tb = TipBuf { data: Vec::new() };
        let tip = tip_owned(&mut tb, 32, [0, 0, 0, 255]);
        let dabs = vec![ParityDab {
            x: 128.0,
            y: 128.0,
            alpha: 1.0,
        }];
        let (_, base_tiles) = raster_shadow(256, 256, false, None, true, 32.0, &dabs, &tip, true);
        let center = base_tiles.iter().find(|t| t.w == 256).expect("full tile");
        let idx = (128 * 256 + 128) * 4;
        assert_eq!(center.data[idx + 3], 0);
    }

    #[test]
    fn boundary_strokes_split_into_multiple_tiles_and_clipping_is_safe() {
        let mut tb = TipBuf { data: Vec::new() };
        let tip = tip_owned(&mut tb, 16, [200, 50, 50, 255]);
        let boundary: Vec<ParityDab> = (0..12)
            .map(|i| ParityDab {
                x: (256.0 * (1 + i % 3) as f64),
                y: (256.0 * (1 + (i * 7) % 3) as f64),
                alpha: 0.9,
            })
            .collect();
        let (m, tiles) = raster_shadow(1024, 1024, true, None, false, 96.0, &boundary, &tip, true);
        assert!(m.tile_count > 1);
        assert_eq!(tiles.len(), m.tile_count);

        let clip: Vec<ParityDab> = vec![
            ParityDab {
                x: -48.0,
                y: 10.0,
                alpha: 0.9,
            },
            ParityDab {
                x: 1072.0,
                y: 500.0,
                alpha: 0.9,
            },
            ParityDab {
                x: 500.0,
                y: -48.0,
                alpha: 0.9,
            },
        ];
        let (mc, tc) = raster_shadow(1024, 1024, true, None, false, 80.0, &clip, &tip, true);
        assert_eq!(mc.tile_count, tc.len());
        assert!(mc.changed_bytes > 0);
    }

    #[test]
    fn digest_matches_ts_pipeline_shape() {
        let mut tb = TipBuf { data: Vec::new() };
        let tip = tip_owned(&mut tb, 8, [10, 20, 30, 255]);
        let dabs = vec![ParityDab {
            x: 5.0,
            y: 5.0,
            alpha: 1.0,
        }];
        let (m, _) = raster_shadow(256, 256, true, None, false, 8.0, &dabs, &tip, false);
        assert_eq!(m.tile_count, 1);
        assert_eq!(m.tile_hashes.len(), 1);
        assert_eq!(m.tile_hashes[0].0, "0,0");
        assert_eq!(m.tile_hashes[0].1.len(), 8);
    }
}
