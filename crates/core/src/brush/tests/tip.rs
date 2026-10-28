// SPDX-License-Identifier: AGPL-3.0-or-later
//! Brush-tip tests: the diameter-space construction, the resampler contract,
//! and the offline golden comparison against captured browser bytes.

use crate::brush::tip::{
    bilinear_resize_premul, build_brush_tip, get_brush_profile_support_norm,
    premul_to_straight_in_place, TipSpec,
};

#[test]
fn c1_known_diameter_256_08_is_274() {
    let spec = TipSpec::new(256.0, 0.8, [225, 90, 23]);
    let tip = build_brush_tip(&spec);
    assert_eq!(tip.diameter, 274);
    assert_eq!(tip.width, 256);
    assert_eq!(tip.rgba_data_size.len(), 256 * 256 * 4);
    assert_eq!(tip.rgba_diameter.len(), 274 * 274 * 4);
}

#[test]
fn c1_mask_center_opaque_and_corner_transparent() {
    let spec = TipSpec::new(256.0, 0.8, [225, 90, 23]);
    let tip = build_brush_tip(&spec);
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
/// Run: PHOTREZ_C1_GOLDEN=<bin path> cargo test -p photrez-core brush::tests::tip c1_golden -- --ignored --nocapture
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
    let tip = build_brush_tip(&spec);
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
    // Review decision: Chrome/Skia resampling is NOT canonical, so rim
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

fn fs_read(path: String) -> Vec<u8> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).expect("cannot open golden file");
    let mut v = Vec::new();
    f.read_to_end(&mut v).expect("read failed");
    v
}
