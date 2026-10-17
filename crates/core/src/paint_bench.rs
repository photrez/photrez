// SPDX-License-Identifier: AGPL-3.0-or-later
// Fase 2 decision benchmarks: micro-costs of the proposed Rust tile store.
// Bench-only module - no product logic lives here.
// Rationale + results are tracked in the project decision log.
#[cfg(test)]
const TILE_BYTES: usize = 256 * 256 * 4; // 256x256 RGBA8 = 256 KiB

#[cfg(test)]
fn build_tile_map(
    n_tiles: usize,
) -> std::collections::HashMap<(u32, u32), std::sync::Arc<Vec<u8>>> {
    let mut m = std::collections::HashMap::with_capacity(n_tiles);
    for i in 0..n_tiles {
        let key = ((i as u32) % 64, (i as u32) / 64);
        m.insert(key, std::sync::Arc::new(vec![0xA5u8; TILE_BYTES]));
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

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
