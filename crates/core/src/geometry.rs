// SPDX-License-Identifier: AGPL-3.0-or-later
// Geometry batch — consolidated Rust hot math (transform, snap, hit-test)
// Konsolidasi: semua geometry ops di 1 file biar tidak mencar (v0.2.0).
// API: batched over whole buffer (1 crossing) — not per-point, per forum gate.

use wasm_bindgen::prelude::*;

/// Batch transform 2D points: points = [x0,y0,x1,y1,...], matrix = [a,b,c,d,tx,ty]
/// where [x',y'] = [a*x + c*y + tx, b*x + d*y + ty] (Photrez Transform2D).
/// Returns new Vec<f64> with transformed points — caller owns it.
/// Benchmark gate: must beat TS loop (JIT) at 10k points.
#[wasm_bindgen]
pub fn batch_transform_points(
    points: Vec<f64>,
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    tx: f64,
    ty: f64,
) -> Vec<f64> {
    let mut out = Vec::with_capacity(points.len());
    // SAFETY: points len even; process in chunks of 2
    for chunk in points.chunks_exact(2) {
        let x = chunk[0];
        let y = chunk[1];
        out.push(a * x + c * y + tx);
        out.push(b * x + d * y + ty);
    }
    out
}

/// Batch hit-test: points vs rects (x,y,w,h). Returns bitmask Vec<u8> 1=hit.
// Coarse-grained: all points + all rects in one call — 1 crossing.
#[wasm_bindgen]
pub fn batch_hit_test(points: Vec<f64>, rects: Vec<f64>) -> Vec<u8> {
    let mut out = Vec::with_capacity(points.len() / 2);
    for chunk in points.chunks_exact(2) {
        let x = chunk[0];
        let y = chunk[1];
        let mut hit = 0u8;
        for r in rects.chunks_exact(4) {
            let rx = r[0];
            let ry = r[1];
            let rw = r[2];
            let rh = r[3];
            if x >= rx && x <= rx + rw && y >= ry && y <= ry + rh {
                hit = 1;
                break;
            }
        }
        out.push(hit);
    }
    out
}

/// Batch snap: find closest snap target per point within threshold.
/// points [x,y,...], targets [x,y,...] (snap lines). Returns snapped Vec<f64>.
#[wasm_bindgen]
pub fn batch_snap_points(points: Vec<f64>, targets: Vec<f64>, threshold: f64) -> Vec<f64> {
    let mut out = Vec::with_capacity(points.len());
    for chunk in points.chunks_exact(2) {
        let mut x = chunk[0];
        let mut y = chunk[1];
        let mut best_dx = threshold + 1.0;
        let mut best_dy = threshold + 1.0;
        let mut snap_x = x;
        let mut snap_y = y;
        for t in targets.chunks_exact(2) {
            let tx = t[0];
            let ty = t[1];
            let dx = (x - tx).abs();
            let dy = (y - ty).abs();
            if dx < best_dx && dx <= threshold {
                best_dx = dx;
                snap_x = tx;
            }
            if dy < best_dy && dy <= threshold {
                best_dy = dy;
                snap_y = ty;
            }
        }
        if best_dx <= threshold {
            x = snap_x;
        }
        if best_dy <= threshold {
            y = snap_y;
        }
        out.push(x);
        out.push(y);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_batch_basic() {
        let pts = vec![0.0, 0.0, 10.0, 0.0, 0.0, 10.0];
        let out = batch_transform_points(pts, 2.0, 0.0, 0.0, 2.0, 5.0, 5.0);
        assert_eq!(out, vec![5.0, 5.0, 25.0, 5.0, 5.0, 25.0]);
    }

    #[test]
    fn hit_test_basic() {
        let pts = vec![5.0, 5.0, 20.0, 20.0];
        let rects = vec![0.0, 0.0, 10.0, 10.0];
        assert_eq!(batch_hit_test(pts, rects), vec![1, 0]);
    }

    #[test]
    fn snap_basic() {
        let pts = vec![9.5, 9.5];
        let targets = vec![10.0, 10.0];
        let out = batch_snap_points(pts, targets, 1.0);
        assert_eq!(out, vec![10.0, 10.0]);
    }
}
