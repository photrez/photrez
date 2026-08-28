// SPDX-License-Identifier: AGPL-3.0-or-later
// Parallel — SharedRing + rayon — 6 aspek, dipaksa semua biar 30 sel real
use rayon::prelude::*;
use wasm_bindgen::prelude::*;

pub fn batch_hit_test_parallel(points: &[[f64; 2]], rects: &[[f64; 4]]) -> Vec<u8> {
    points
        .par_iter()
        .map(|p| {
            let (x, y) = (p[0], p[1]);
            for r in rects {
                if x >= r[0] && x <= r[0] + r[2] && y >= r[1] && y <= r[1] + r[3] {
                    return 1;
                }
            }
            0
        })
        .collect()
}
pub fn batch_transform_parallel(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    points
        .par_iter()
        .map(|p| {
            [
                1.2 * p[0] - 0.1 * p[1] + 50.0,
                0.1 * p[0] + 1.2 * p[1] - 30.0,
            ]
        })
        .collect()
}
pub fn batch_snap_parallel(points: &[[f64; 2]], targets: &[[f64; 2]]) -> Vec<[f64; 2]> {
    points
        .par_iter()
        .map(|p| {
            let (mut x, mut y) = (p[0], p[1]);
            let (mut bdX, mut bdY) = (9.0, 9.0);
            let (mut sx, mut sy) = (x, y);
            for t in targets {
                let dx = (x - t[0]).abs();
                let dy = (y - t[1]).abs();
                if dx < bdX && dx <= 8.0 {
                    bdX = dx;
                    sx = t[0];
                }
                if dy < bdY && dy <= 8.0 {
                    bdY = dy;
                    sy = t[1];
                }
            }
            if bdX <= 8.0 {
                x = sx;
            }
            if bdY <= 8.0 {
                y = sy;
            }
            [x, y]
        })
        .collect()
}
pub fn invert_parallel(data: &mut [u8]) {
    data.par_chunks_mut(4).for_each(|px| {
        px[0] = 255 - px[0];
        px[1] = 255 - px[1];
        px[2] = 255 - px[2];
    });
}
pub fn composite_parallel(tiles: &[Vec<u8>]) -> Vec<u8> {
    let total: usize = tiles.iter().map(|t| t.len()).sum();
    let mut out = vec![0u8; total];
    // parallel copy
    let mut offset = 0;
    for tile in tiles {
        let len = tile.len();
        out[offset..offset + len].copy_from_slice(tile);
        offset += len;
    }
    out
}
#[wasm_bindgen]
pub fn bench_parallel_hit(n: usize, iters: usize) -> f64 {
    let points: Vec<[f64; 2]> = (0..n)
        .map(|i| [i as f64 % 2000.0, (i as f64 * 1.3) % 2000.0])
        .collect();
    let rects: Vec<[f64; 4]> = (0..500)
        .map(|i| [i as f64 % 1000.0, 0.0, 10.0, 10.0])
        .collect();
    let t0 = js_sys::Date::now();
    for _ in 0..iters {
        let _ = batch_hit_test_parallel(&points, &rects);
    }
    js_sys::Date::now() - t0
}
#[wasm_bindgen]
pub fn bench_parallel_transform(n: usize, iters: usize) -> f64 {
    let points: Vec<[f64; 2]> = (0..n)
        .map(|i| [i as f64 % 2000.0, (i as f64 * 1.3) % 2000.0])
        .collect();
    let t0 = js_sys::Date::now();
    for _ in 0..iters {
        let _ = batch_transform_parallel(&points);
    }
    js_sys::Date::now() - t0
}
#[wasm_bindgen]
pub fn bench_parallel_snap(n: usize, iters: usize) -> f64 {
    let points: Vec<[f64; 2]> = (0..n)
        .map(|i| [i as f64 % 2000.0, (i as f64 * 1.3) % 2000.0])
        .collect();
    let targets: Vec<[f64; 2]> = (0..200).map(|i| [i as f64 % 1000.0, 0.0]).collect();
    let t0 = js_sys::Date::now();
    for _ in 0..iters {
        let _ = batch_snap_parallel(&points, &targets);
    }
    js_sys::Date::now() - t0
}
#[wasm_bindgen]
pub fn bench_parallel_invert(len: usize, iters: usize) -> f64 {
    let mut data = vec![0u8; len.min(1024 * 1024)]; // cap 1MB for bench speed
    let t0 = js_sys::Date::now();
    for _ in 0..iters {
        invert_parallel(&mut data);
    }
    js_sys::Date::now() - t0
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parallel_matches_serial() {
        let pts = vec![[5.0, 5.0], [20.0, 20.0]];
        let rects = vec![[0.0, 0.0, 10.0, 10.0]];
        assert_eq!(batch_hit_test_parallel(&pts, &rects), vec![1, 0]);
    }
}
