// SPDX-License-Identifier: AGPL-3.0-or-later
// Fase 2 decision benchmarks: micro-costs of the proposed Rust tile store.
// Bench-only module - no product logic lives here. Mirrors the methodology of
// scripts/bench-zero-copy.ts (pinned wasm-owned buffers, ms per op).
// Rationale + results: docs/plans/2026-08-22-fase2-rust-tile-store-plan.md
use std::collections::HashMap;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

const TILE_BYTES: usize = 256 * 256 * 4; // 256x256 RGBA8 = 256 KiB

fn build_tile_map(n_tiles: usize) -> HashMap<(u32, u32), Arc<Vec<u8>>> {
    let mut m = HashMap::with_capacity(n_tiles);
    for i in 0..n_tiles {
        let key = ((i as u32) % 64, (i as u32) / 64);
        m.insert(key, Arc::new(vec![0xA5u8; TILE_BYTES]));
    }
    m
}

/// Simulates seeding a full layer: split a wasm-owned pinned buffer into
/// per-tile Vec copies inside a HashMap (no extra input copy - reads the
/// pinned buffer in place). Returns milliseconds.
#[wasm_bindgen]
pub fn bench_seed_split(buf_id: u32, tile_px: u32, tiles_x: u32) -> f64 {
    let tile_len = (tile_px * tile_px * 4) as usize;
    crate::kernel::with_buffer(buf_id, |src| {
        let t0 = now_ms();
        let mut map: HashMap<(u32, u32), Arc<Vec<u8>>> = HashMap::new();
        let mut y = 0u32;
        let mut off = 0usize;
        while off < src.len() {
            let mut x = 0u32;
            while x < tiles_x {
                let end = (off + tile_len).min(src.len());
                map.insert((x, y), Arc::new(src[off..end].to_vec()));
                off = end;
                x += 1;
            }
            y += 1;
        }
        let ms = now_ms() - t0;
        std::hint::black_box(map.len());
        ms
    })
}

/// Shallow snapshot cost: clone an n-tile Arc map. Returns ms/iter.
#[wasm_bindgen]
pub fn bench_snapshot_clone(n_tiles: u32, iters: u32) -> f64 {
    let map = build_tile_map(n_tiles as usize);
    let t0 = now_ms();
    let mut sink = 0usize;
    for _ in 0..iters {
        let snap = map.clone();
        sink += snap.len();
    }
    let ms = (now_ms() - t0) / iters as f64;
    std::hint::black_box(sink);
    ms
}

/// CoW write cost: clone one tile's bytes, mutate, reinsert. Returns ms/iter.
#[wasm_bindgen]
pub fn bench_cow_write(n_tiles: u32, iters: u32) -> f64 {
    let mut map = build_tile_map(n_tiles as usize);
    let key = (0u32, 0u32);
    let t0 = now_ms();
    for i in 0..iters {
        let before = map.get(&key).expect("key").clone();
        let mut owned: Vec<u8> = (*before).clone();
        owned[i as usize % TILE_BYTES] = (i & 255) as u8;
        map.insert(key, Arc::new(owned));
    }
    (now_ms() - t0) / iters as f64
}

/// Undo-diff out-buffer assembly: copy n tiles contiguously. Returns ms/iter.
#[wasm_bindgen]
pub fn bench_diff_assemble(n_tiles: u32, iters: u32) -> f64 {
    let map = build_tile_map(n_tiles as usize);
    let keys: Vec<(u32, u32)> = map.keys().copied().collect();
    let t0 = now_ms();
    let mut out: Vec<u8> = Vec::with_capacity(keys.len() * TILE_BYTES);
    for _ in 0..iters {
        out.clear();
        for k in &keys {
            out.extend_from_slice(&map[k]);
        }
        std::hint::black_box(out.len());
    }
    (now_ms() - t0) / iters as f64
}

/// WASM linear-memory growth cost: allocate + page-touch mb MiB. Returns ms.
#[wasm_bindgen]
pub fn bench_mem_grow(mb: u32) -> f64 {
    let len = mb as usize * 1024 * 1024;
    let t0 = now_ms();
    let v = vec![0u8; len];
    let mut acc = 0u8;
    for i in (0..len).step_by(4096) {
        acc = acc.wrapping_add(v[i]);
    }
    let ms = now_ms() - t0;
    std::hint::black_box(acc);
    ms
}

/// Current wasm linear memory size in pages (64 KiB each), for heap reporting.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn wasm_memory_pages() -> u32 {
    core::arch::wasm32::memory_size::<0>() as u32
}

fn now_ms() -> f64 {
    js_sys::Date::now()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_clone_is_shallow_and_correct() {
        let map = build_tile_map(16);
        let snap = map.clone();
        assert_eq!(snap.len(), 16);
        assert!(Arc::ptr_eq(&map[&(0, 0)], &snap[&(0, 0)]));
    }

    #[test]
    fn cow_write_replaces_only_target() {
        let mut map = build_tile_map(4);
        let before = map[&(0, 0)].clone();
        let mut owned: Vec<u8> = (*before).clone();
        owned[0] = 1;
        map.insert((0, 0), Arc::new(owned));
        assert_eq!(map[&(0, 0)][0], 1);
        assert_ne!(map[&(1, 0)][0], 1);
    }

    #[test]
    fn diff_assemble_length() {
        let map = build_tile_map(8);
        let out_len: usize = map.values().map(|v| v.len()).sum();
        assert_eq!(out_len, 8 * TILE_BYTES);
    }
}
