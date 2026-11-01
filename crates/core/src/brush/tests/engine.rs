// SPDX-License-Identifier: AGPL-3.0-or-later
//! Stroke-engine tests: the dab spacing/carry state machine checked against
//! the reference dab-interpolation cases.

use crate::brush::engine::*;

// Cases below pin the reference dab-interpolation edge cases (zero distance,
// sub-spacing moves, carry overflow) plus segment identity.

fn dab(engine: &mut BrushStrokeEngine, x: f64, y: f64) -> Vec<(f64, f64)> {
    let n = engine.update(x, y);
    let v = engine.dab_slice();
    (0..n).map(|i| (v[i * 2], v[i * 2 + 1])).collect()
}

#[test]
fn dab_spacing_matches_reference_formula() {
    assert_eq!(brush_dab_spacing(100.0), 10.0);
    assert_eq!(brush_dab_spacing(1.0), 1.0);
    assert_eq!(brush_dab_spacing(2.0), 1.0);
    assert_eq!(brush_dab_spacing(400.0), 40.0);
}

#[test]
fn zero_distance_no_dabs_carry_kept() {
    let mut e = BrushStrokeEngine::new(50.0);
    e.begin(10.0, 10.0);
    assert_eq!(dab(&mut e, 10.0, 10.0).len(), 0);
    assert_eq!(e.carry(), 0.0);
}

#[test]
fn short_segment_pure_carry() {
    // from (0,0) to (3,0), spacing 10 -> no dab, carry = 3
    let mut e = BrushStrokeEngine::new(100.0); // spacing 10
    e.begin(0.0, 0.0);
    assert_eq!(dab(&mut e, 3.0, 0.0).len(), 0);
    assert!((e.carry() - 3.0).abs() < 1e-9);
}

#[test]
fn exact_multiple_three_dabs_carry_zero() {
    // 30px at spacing 10 -> dabs at 10,20,30, carry 0
    let mut e = BrushStrokeEngine::new(100.0);
    e.begin(0.0, 0.0);
    let d = dab(&mut e, 30.0, 0.0);
    assert_eq!(d.len(), 3);
    assert!((d[0].0 - 10.0).abs() < 1e-9);
    assert!((d[2].0 - 30.0).abs() < 1e-9);
    assert!(e.carry().abs() < 1e-9);
}

#[test]
fn chained_carry_reduces_first_gap() {
    // 3px (carry 3), then 10px: first dab candidate at 10-3=7 (< 10)
    let mut e = BrushStrokeEngine::new(100.0);
    e.begin(0.0, 0.0);
    dab(&mut e, 3.0, 0.0);
    let d = dab(&mut e, 13.0, 0.0);
    assert_eq!(d.len(), 1);
    assert!((d[0].0 - 10.0).abs() < 1e-9);
    assert!((e.carry() - 3.0).abs() < 1e-9);
}

#[test]
fn injected_wrap_carry_matches_reference() {
    // Reference case: (0,0)->(30,0) spacing 10 carry 12 -> next=-2 (behind
    // anchor), dabs at t=-2/8.., carry = 30 - (38-10) = 2
    let mut e = BrushStrokeEngine::new(100.0);
    e.begin(0.0, 0.0);
    e.seed_carry(12.0);
    let d = dab(&mut e, 30.0, 0.0);
    assert_eq!(d.len(), 4);
    assert!((e.carry() - 2.0).abs() < 1e-9);
}

#[test]
fn diagonal_carry_matches_reference() {
    // Reference case: segment (dx,dy)=(5,5), spacing 10, carry 8 ->
    // distance=sqrt(50), first dab at t=2/distance, carry = 5.07...
    let mut e = BrushStrokeEngine::new(100.0);
    e.begin(18.0, 0.0); // anchor anywhere; math is segment-relative
    e.seed_carry(8.0);
    dab(&mut e, 23.0, 5.0);
    assert!((e.carry() - 5.071067811865476).abs() < 1e-9);
}

#[test]
fn segment_split_equals_direct() {
    // (0,0)->(15,0)->(30,0) with carry chaining == direct (0,0)->(30,0)
    let mut a = BrushStrokeEngine::new(100.0);
    a.begin(0.0, 0.0);
    let mut all = dab(&mut a, 15.0, 0.0);
    all.extend(dab(&mut a, 30.0, 0.0));

    let mut b = BrushStrokeEngine::new(100.0);
    b.begin(0.0, 0.0);
    let direct = dab(&mut b, 30.0, 0.0);

    assert_eq!(all.len(), direct.len());
    for (p, q) in all.iter().zip(direct.iter()) {
        assert!((p.0 - q.0).abs() < 1e-9 && (p.1 - q.1).abs() < 1e-9);
    }
    assert!((a.carry() - b.carry()).abs() < 1e-9);
}

#[test]
fn batch_matches_per_move() {
    let moves = [3.0, 0.0, 13.0, 0.0, 30.0, 0.0];
    let mut batched = BrushStrokeEngine::new(100.0);
    batched.begin(moves[0], moves[1]);
    let n_batch = batched.update_batch(&moves);
    let mut stepped = BrushStrokeEngine::new(100.0);
    stepped.begin(moves[0], moves[1]);
    let mut all: Vec<f64> = Vec::new();
    let mut i = 0;
    while i + 1 < moves.len() {
        stepped.update(moves[i], moves[i + 1]);
        all.extend_from_slice(stepped.dab_slice());
        i += 2;
    }
    assert_eq!(n_batch * 2, all.len());
    assert_eq!(batched.dab_slice(), all.as_slice());
}
