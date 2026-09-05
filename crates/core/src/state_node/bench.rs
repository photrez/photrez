// SPDX-License-Identifier: AGPL-3.0-or-later
//! Permanent benchmark, marked `#[ignore]`: the packed-store result must stay
//! reproducible from the repo. Run on demand:
//!   `cargo test -p photrez-core bench_packed_tile_store --release -- --ignored --nocapture`
//! Reports the packed ingest (single pre-sized allocation), the zero-copy/zero-alloc
//! extract path, the per-tile COW cost, and the splice-COW (`cow_batch`) cost.

use super::*;
use std::time::Instant;

fn row_major(w: u32, h: u32, v: u8) -> Vec<u8> {
    vec![v; (w * h * 4) as usize]
}

#[test]
#[ignore]
fn bench_packed_tile_store() {
    let w = 2048u32;
    let h = 2048u32;
    let bytes = row_major(w, h, 1);

    // INGEST: build the pre-sized packed base. This is ONE
    // `Arc<[u8]>` allocation (packed tile-major) shared by every tile.
    let t0 = Instant::now();
    let mut ls = LayerState::new("L", w, h, &bytes, 0);
    let ingest_ms = t0.elapsed().as_secs_f64() * 1e3;
    let state = ls.current_state();
    let tile_count = state.tiles.len();
    let single_alloc = state
        .tiles
        .iter()
        .all(|t| Arc::ptr_eq(&state.tiles[0].block, &t.block));
    let strong_count = Arc::strong_count(&state.tiles[0].block);
    eprintln!(
        "[bench_packed_tile_store] ingest 2048x2048: {ingest_ms:.3} ms ({tile_count} tiles, \
         packed single-alloc={single_alloc}, strong_count={strong_count}, tile_bytes={})",
        state.tiles[0].block.len()
    );

    // EXTRACT: read every tile's `bytes()` slice. Must be zero-copy (aliases the
    // packed `Arc<[u8]>`) and zero-alloc (a `&[u8]`, not a `Vec<u8>`).
    let mut zero_copy_ok = true;
    let mut total = 0usize;
    let t1 = Instant::now();
    for _ in 0..64 {
        for t in &state.tiles {
            let b = t.bytes();
            let aliases = std::ptr::eq(b.as_ptr(), t.block[t.offset..].as_ptr());
            zero_copy_ok &= aliases;
            total += b.len();
        }
    }
    let extract_ms = t1.elapsed().as_secs_f64() * 1e3;
    eprintln!(
        "[bench_packed_tile_store] extract full layer x64: {extract_ms:.3} ms, \
         zero_copy={zero_copy_ok}, total_bytes_scanned={total}"
    );

    // COW via `cow_batch`: one 512x512 region re-tiles the tiles it intersects.
    let t3 = Instant::now();
    let region = RegionChange::new(0, 0, 512, 512, vec![7u8; 512 * 512 * 4]);
    let (_b, _a) = ls.cow_batch(&[region], 0);
    let batch_ms = t3.elapsed().as_secs_f64() * 1e3;
    eprintln!("[bench_packed_tile_store] cow_batch 512x512 region: {batch_ms:.3} ms");

    assert!(
        single_alloc,
        "base packed buffer must be a single allocation"
    );
    assert!(
        zero_copy_ok,
        "extract must be zero-copy (aliases the packed Arc)"
    );
}
