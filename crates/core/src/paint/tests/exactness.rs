// SPDX-License-Identifier: AGPL-3.0-or-later
//! Differential proof for the hoisted tip-column mapping in `raster_shadow`.
//!
//! The optimised loop precomputes the per-column tip lookup and hoists the
//! per-dab alpha, which changes no arithmetic. This test keeps the original
//! per-pixel expression as a reference and requires byte-identical output, so
//! a future edit that changes sampling semantics fails here rather than in a
//! screenshot.

use crate::paint::parity::{raster_shadow, ParityDab, ParityTip};

fn mul_div_255(c: u64, a: u64) -> u8 {
    ((c * a + 127) / 255) as u8
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

/// The pre-hoist raster loop, verbatim: per-pixel float mapping, per-pixel dab
/// alpha, same fixed-point compositing.
fn reference_raster(
    w: usize,
    h: usize,
    brush: f64,
    dabs: &[ParityDab],
    tip: &ParityTip<'_>,
) -> Vec<u8> {
    let mut buf = vec![0u8; w * h * 4];
    for i in 0..(w * h) {
        buf[i * 4] = 255;
        buf[i * 4 + 1] = 255;
        buf[i * 4 + 2] = 255;
        buf[i * 4 + 3] = 255;
    }
    let r = brush / 2.0;
    let iw = tip.width;
    let ih = tip.height;
    let stamp = brush.max(f64::MIN_POSITIVE);
    for d in dabs {
        let fx = d.x - r;
        let fy = d.y - r;
        let ox = fx.round() as i64;
        let oy = fy.round() as i64;
        let px0 = ox.clamp(0, w as i64);
        let py0 = oy.clamp(0, h as i64);
        let px1 = (ox + brush as i64).clamp(0, w as i64);
        let py1 = (oy + brush as i64).clamp(0, h as i64);
        for py in py0..py1 {
            for px in px0..px1 {
                let txx = ((px as f64 + 0.5 - fx) * iw as f64 / stamp)
                    .floor()
                    .clamp(0.0, iw.saturating_sub(1) as f64);
                let tyy = ((py as f64 + 0.5 - fy) * ih as f64 / stamp)
                    .floor()
                    .clamp(0.0, ih.saturating_sub(1) as f64);
                let ti = ((tyy as usize) * iw + (txx as usize)) * 4;
                let dab_alpha = (d.alpha.clamp(0.0, 1.0) * 255.0).round() as u64;
                let sa = mul_div_255(tip.data[ti + 3] as u64, dab_alpha) as u64;
                let di = (py as usize * w + px as usize) * 4;
                let inv = 255 - sa;
                if sa > 0 {
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
    buf
}

fn fixture_tip() -> Vec<u8> {
    let (tw, th) = (23usize, 17usize);
    let mut data = Vec::with_capacity(tw * th * 4);
    for y in 0..th {
        for x in 0..tw {
            let dx = x as f64 - (tw as f64 - 1.0) / 2.0;
            let dy = y as f64 - (th as f64 - 1.0) / 2.0;
            let dist = (dx * dx + dy * dy).sqrt();
            let edge = ((tw.min(th) as f64) / 2.0).max(1.0);
            let alpha = (255.0 * (1.0 - dist / edge).clamp(0.0, 1.0)).round() as u8;
            data.push((x * 9 % 256) as u8);
            data.push((y * 13 % 256) as u8);
            data.push(((x + y) * 5 % 256) as u8);
            data.push(alpha);
        }
    }
    data
}

#[test]
fn hoisted_tip_mapping_is_byte_identical() {
    let (w, h) = (96usize, 80usize);
    let brush = 37.5f64;
    let tip_data = fixture_tip();
    let tip = ParityTip {
        width: 23,
        height: 17,
        data: &tip_data,
    };
    let dabs = [
        ParityDab {
            x: 20.3,
            y: 21.7,
            alpha: 0.8,
        },
        ParityDab {
            x: 47.2,
            y: 33.1,
            alpha: 0.55,
        },
        ParityDab {
            x: 63.9,
            y: 44.4,
            alpha: 1.0,
        },
        ParityDab {
            x: 12.0,
            y: 70.5,
            alpha: 0.25,
        },
    ];

    let reference = reference_raster(w, h, brush, &dabs, &tip);
    let (meta_shipped, tiles) = raster_shadow(w, h, true, None, false, brush, &dabs, &tip, true);
    assert!(!tiles.is_empty(), "the fixture must produce tiles");

    // The digest-only path hashes the same byte stream in place, without
    // materialising tile bytes. It must agree with the shipped path exactly.
    let (meta_digest_only, none_out) =
        raster_shadow(w, h, true, None, false, brush, &dabs, &tip, false);
    assert!(none_out.is_empty(), "digest-only runs ship no tiles");
    assert_eq!(
        meta_shipped.digest, meta_digest_only.digest,
        "digest-only hashing diverged from the materialised tile bytes"
    );
    assert_eq!(meta_shipped.tile_count, meta_digest_only.tile_count);
    assert_eq!(meta_shipped.changed_bytes, meta_digest_only.changed_bytes);
    assert_eq!(meta_shipped.tile_hashes, meta_digest_only.tile_hashes);

    let mut compared = 0usize;
    for tile in &tiles {
        let mut expected = Vec::with_capacity(tile.data.len());
        for row in 0..tile.h as i64 {
            for col in 0..tile.w as i64 {
                let i = ((tile.y + row) as usize * w + (tile.x + col) as usize) * 4;
                expected.extend_from_slice(&unpremul(&reference, i));
            }
        }
        assert_eq!(
            expected, tile.data,
            "tile {} at ({},{}) differs from the per-pixel reference",
            tile.key, tile.x, tile.y
        );
        compared += 1;
    }
    assert!(compared >= 1);
}

/// Same input, both kernels, one process: the only honest A/B for the raster
/// phase, because the end-to-end pointer workload is not deterministic across
/// runs (different synthetic positions change `changed_pixels` per stroke).
/// Ignored by default so CI stays fast; run explicitly:
/// `cargo test -p photrez-core --lib exactness -- --ignored --nocapture`
#[test]
#[ignore = "timing probe: run explicitly when changing the raster kernel"]
fn raster_kernel_ab_on_identical_input() {
    let (w, h) = (1024usize, 1024usize);
    let brush = 800.0f64;
    let tip_data = fixture_tip();
    let tip = ParityTip {
        width: 23,
        height: 17,
        data: &tip_data,
    };
    let mut dabs = Vec::new();
    for i in 0..8 {
        dabs.push(ParityDab {
            x: 200.0 + i as f64 * 40.0,
            y: 300.0 + (i as f64 * 23.0),
            alpha: 0.6,
        });
    }
    let reps = 5;
    let t_ref = std::time::Instant::now();
    let mut sink = 0u64;
    for _ in 0..reps {
        let buf = reference_raster(w, h, brush, &dabs, &tip);
        sink = sink.wrapping_add(u64::from(buf[(w * h / 2) * 4 + 3]));
    }
    let ref_ms = t_ref.elapsed().as_secs_f64() * 1000.0 / reps as f64;
    let t_new = std::time::Instant::now();
    let mut raster_us_total = 0u128;
    let mut patch_us_total = 0u128;
    for _ in 0..reps {
        let (meta, tiles) = raster_shadow(w, h, true, None, false, brush, &dabs, &tip, true);
        raster_us_total += meta.raster_us;
        patch_us_total += meta.patchgen_us;
        sink = sink.wrapping_add(tiles.len() as u64);
    }
    let wall_new = t_new.elapsed().as_secs_f64() * 1000.0 / reps as f64;
    let new_ms = raster_us_total as f64 / 1000.0 / reps as f64;
    let patch_ms = patch_us_total as f64 / 1000.0 / reps as f64;
    println!(
        "raster A/B (1024x1024, brush 800, 8 dabs, {reps} reps): reference {ref_ms:.1} ms/rep, \
         optimised raster phase {new_ms:.1} ms/rep, patchgen {patch_ms:.1} ms/rep \
         ({wall_new:.1} ms wall), speedup {:.2}x (sink {sink})",
        ref_ms / new_ms
    );
}
