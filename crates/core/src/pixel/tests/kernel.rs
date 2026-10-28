// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tests for the native compute kernels.

use crate::pixel::kernel::{
    apply_basic_adjustment_impl, brush_stamp, composite_mask, flood_fill_impl, gradient_fill_impl,
    trim_bbox, FillMaskRust,
};

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

// ── Additional kernels ──

#[test]
fn trim_bbox_finds_bounds_and_handles_empty() {
    // 4x2 image: opaque pixel only at (2,1)
    let mut px = vec![0u8; 4 * 2 * 4];
    px[27] = 255;
    assert_eq!(trim_bbox(&px, 4, 2), vec![2, 1, 2, 1]);
    // fully transparent -> empty
    assert!(trim_bbox(&[0u8; 4 * 2 * 4], 4, 2).is_empty());
}

#[test]
fn brush_stamp_accumulates_toward_saturation() {
    // uniform tip (alpha 1 everywhere), dataScale = 1 (dataSize == diameter)
    let ds = 8u32;
    let tip = vec![1.0f32; (ds * ds) as usize];
    let mut mask = vec![0u8; 64];
    brush_stamp(&mut mask, 8, 8, &tip, ds, 8.0, 4.0, 4.0, 0.5);
    // center pixel: first dab ~128 (255*0.5 rounded)
    assert_eq!(mask[4 * 8 + 4], 128);
    // second dab at same point accumulates: 128 + round(127*0.5) = 128+64
    brush_stamp(&mut mask, 8, 8, &tip, ds, 8.0, 4.0, 4.0, 0.5);
    assert_eq!(mask[4 * 8 + 4], 192);
    // many dabs saturate to exactly 255 and stay there
    for _ in 0..50 {
        brush_stamp(&mut mask, 8, 8, &tip, ds, 8.0, 4.0, 4.0, 0.5);
    }
    assert_eq!(mask[4 * 8 + 4], 255);
}

#[test]
fn composite_over_matches_straight_alpha_and_eraser_halves() {
    // dst: opaque red pixel; mask full coverage, paint blue a=0.5
    let mut dst = vec![255u8, 0, 0, 255];
    let mask = vec![255u8; 1];
    composite_mask(
        &mut dst, 1, 0, 0, &mask, 1, 0, 0, 1, 1, 0.0, 0.0, 255.0, 0.5, false,
    );
    // out_a = 1; r = (0*0.5 + 255*1*0.5)/1 = 127.5 -> 128; b symmetric -> 128
    assert_eq!(&dst[..4], &[128, 0, 128, 255]);

    // eraser with alpha 0.5 halves existing alpha
    let mut dst2 = vec![10u8, 20, 30, 200];
    composite_mask(
        &mut dst2, 1, 0, 0, &mask, 1, 0, 0, 1, 1, 0.0, 0.0, 0.0, 0.5, true,
    );
    assert_eq!(dst2[3], 100);
}
