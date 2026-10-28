// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tests for the paint parity candidate rasterizer.

use crate::paint::parity::{fnv1a, raster_shadow, ParityDab, ParityTip};

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

/// Golden-byte undo/redo determinism: a layer patched via `write_region`
/// must rebuild the EXACT same pixel digest after undo -> redo.
#[test]
fn undo_redo_rebuilt_digest_byte_identical() {
    use crate::pixel::store::PixelStoreRegistry;

    fn layer_digest(pixels: &[u8], w: usize, h: usize) -> String {
        let mut tile_hashes: Vec<(String, String)> = Vec::new();
        for ty in 0..h.div_ceil(256) {
            for tx in 0..w.div_ceil(256) {
                let tw = 256.min(w - tx * 256);
                let th = 256.min(h - ty * 256);
                let mut data = Vec::with_capacity(tw * th * 4);
                for row in 0..th {
                    let off = ((ty * 256 + row) * w + tx * 256) * 4;
                    data.extend_from_slice(&pixels[off..off + tw * 4]);
                }
                tile_hashes.push((format!("{tx},{ty}"), format!("{:08x}", fnv1a(&data))));
            }
        }
        tile_hashes.sort_by(|a, b| a.0.cmp(&b.0));
        let combined: String = tile_hashes
            .iter()
            .map(|(k, h)| format!("{k}:{h};"))
            .collect();
        format!("{:08x}", fnv1a(combined.as_bytes()))
    }

    const N: usize = 64;
    let mut r = PixelStoreRegistry::new();
    r.open_document("d");
    r.add_layer("d", "L", N as u32, N as u32, vec![0u8; N * N * 4])
        .unwrap();

    let empty_digest = layer_digest(&r.get_layer("d", "L").unwrap().pixels, N, N);

    let mut patch = vec![0u8; 16 * 16 * 4];
    for (i, b) in patch.iter_mut().enumerate() {
        *b = ((i * 31 + 7) % 251) as u8;
    }
    r.write_region("d", "L", 8, 8, 16, 16, patch).unwrap();

    let d1 = layer_digest(&r.get_layer("d", "L").unwrap().pixels, N, N);
    assert_ne!(
        d1, empty_digest,
        "write_region must actually change the pixel digest, else D1==D2 is vacuous"
    );

    r.undo_pixel("d");
    let d_undo = layer_digest(&r.get_layer("d", "L").unwrap().pixels, N, N);
    assert_eq!(
        d_undo, empty_digest,
        "undo must restore the exact pre-patch pixels"
    );

    r.redo_pixel("d");
    let d2 = layer_digest(&r.get_layer("d", "L").unwrap().pixels, N, N);
    assert_eq!(
        d2, d1,
        "undo->redo must rebuild the byte-identical pixel digest (D2 == D1)"
    );
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
fn one_pixel_brush_samples_the_tip_center() {
    // A 1 px brush is one stamp pixel; it must read the tip's center, not its
    // transparent top-left corner.
    let mut tip_data = vec![0u8; 4 * 4 * 4];
    let centre = (2 * 4 + 2) * 4;
    tip_data[centre..centre + 4].copy_from_slice(&[255, 0, 0, 255]);
    let tip = ParityTip {
        width: 4,
        height: 4,
        data: &tip_data,
    };
    let dabs = [ParityDab {
        x: 2.5,
        y: 2.5,
        alpha: 1.0,
    }];
    let (_, patches) = raster_shadow(4, 4, true, None, false, 1.0, &dabs, &tip, true);
    let mut pixels = [255u8; 4 * 4 * 4];
    for patch in patches {
        for row in 0..patch.h {
            let src = row * patch.w * 4;
            let dst = ((patch.y as usize + row) * 4 + patch.x as usize) * 4;
            pixels[dst..dst + patch.w * 4].copy_from_slice(&patch.data[src..src + patch.w * 4]);
        }
    }
    let at = (2 * 4 + 2) * 4;
    assert_eq!(&pixels[at..at + 4], &[255, 0, 0, 255]);
}

#[test]
fn large_brush_samples_the_whole_tip_not_its_corner() {
    // A brush wider than the tip data must stretch the tip across the stamp.
    let mut tb = TipBuf { data: Vec::new() };
    let tip = tip_owned(&mut tb, 4, [255, 0, 0, 255]);
    let dabs = [ParityDab {
        x: 4.0,
        y: 4.0,
        alpha: 1.0,
    }];
    let (_, patches) = raster_shadow(8, 8, true, None, false, 8.0, &dabs, &tip, true);
    let mut pixels = [255u8; 8 * 8 * 4];
    for patch in patches {
        for row in 0..patch.h {
            let src = row * patch.w * 4;
            let dst = ((patch.y as usize + row) * 8 + patch.x as usize) * 4;
            pixels[dst..dst + patch.w * 4].copy_from_slice(&patch.data[src..src + patch.w * 4]);
        }
    }
    for (x, y) in [(0, 0), (7, 0), (0, 7), (7, 7)] {
        let at = (y * 8 + x) * 4;
        assert_eq!(
            &pixels[at..at + 4],
            &[255, 0, 0, 255],
            "corner ({x},{y}) must be painted by the stretched tip"
        );
    }
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
