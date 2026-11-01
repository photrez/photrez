// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tests for `pixel::store` (split across sibling modules so each file stays
//! under the 1000-line guard). Uses `use super::*` for the production items it
//! exercises.

use super::*;
use crate::paint::raster::{RasterDab, RasterTip};
use crate::pixel::layer::PixelLayer;
use crate::pixel::reference::assert_byte_eq;
use crate::pixel::tile::TilePatch;

mod crop_bake;
mod history_round_trip;
mod history_unification;
mod invalid_input;
mod native_authority;
mod seed;

fn tile(x: i64, y: i64, w: usize, h: usize, fill: u8) -> TilePatch {
    TilePatch {
        x,
        y,
        w,
        h,
        data: vec![fill; w * h * 4],
    }
}

#[test]
fn snapshot_region_matches_canonical() {
    let w = 128u32;
    let h = 128u32;
    let mut layer = PixelLayer::new(w, h, vec![0u8; w as usize * h as usize * 4]);
    layer.pixels[0..4].copy_from_slice(&[42, 42, 42, 42]);
    let region = layer.snapshot_region(0, 0, 64, 64);
    assert_eq!(region[0], 42);
    assert_eq!(region.len(), 64 * 64 * 4);
}

#[test]
fn all_tiles_covers_full_layer() {
    let w = 512u32;
    let h = 256u32;
    let layer = PixelLayer::new(w, h, vec![0u8; w as usize * h as usize * 4]);
    let tiles = layer.all_tiles();
    // 512/256 = 2 cols x 1 row = 2 tiles of 256x256.
    assert_eq!(tiles.len(), 2);
    let total: usize = tiles.iter().map(|t| t.w * t.h).sum();
    assert_eq!(total, w as usize * h as usize);
    // Every byte of the canonical buffer is represented by exactly one tile.
    let mut cover = vec![0u8; w as usize * h as usize * 4];
    for t in &tiles {
        for row in 0..t.h {
            let dst = ((t.y + row as i64) as usize * w as usize + t.x as usize) * 4;
            let src = row * t.w * 4;
            cover[dst..dst + t.w * 4].copy_from_slice(&t.data[src..src + t.w * 4]);
        }
    }
    assert_eq!(cover, layer.pixels);
}

// -- Lifecycle --

fn reg() -> PixelStoreRegistry {
    PixelStoreRegistry::new()
}

#[test]
fn document_close_releases_all_pixel_storage() {
    let mut r = reg();
    r.open_document("docA");
    r.add_layer("docA", "L1", 64, 64, vec![0; 64 * 64 * 4])
        .unwrap();
    r.add_layer("docA", "L2", 32, 32, vec![0; 32 * 32 * 4])
        .unwrap();
    assert!(r.get_layer("docA", "L1").is_some());
    assert!(r.get_layer("docA", "L2").is_some());
    r.close_document("docA");
    assert!(r.get_layer("docA", "L1").is_none());
    assert!(r.get_layer("docA", "L2").is_none());
}

#[test]
fn remove_layer_releases_storage() {
    let mut r = reg();
    r.open_document("docA");
    r.add_layer("docA", "L1", 64, 64, vec![0; 64 * 64 * 4])
        .unwrap();
    r.remove_layer("docA", "L1");
    assert!(r.get_layer("docA", "L1").is_none());
}

// Removal drops pixel storage but NOT the document-level history stream
// (layer removal in `pixel/store/access.rs` vs the depth probe in
// `pixel/store/history.rs`): the epoch probe must reject the gone
// layer while the read-only depth probe keeps reporting the entry, so a failed
// epoch read can never be read back as "no history exists".
#[test]
fn removed_layer_epoch_rejects_but_history_depth_survives() {
    let mut r = reg();
    r.open_document("docA");
    r.add_layer("docA", "L1", 64, 64, vec![0; 64 * 64 * 4])
        .unwrap();
    let seeded = r.apply_pixel_patch(
        "docA",
        "L1",
        vec![tile(0, 0, 8, 8, 0)],
        vec![tile(0, 0, 8, 8, 9)],
    );
    assert!(
        seeded.is_some(),
        "the patch applies while the layer is alive"
    );

    r.remove_layer("docA", "L1");

    assert_eq!(
        r.get_epoch("docA", "L1").unwrap_err(),
        "layer not initialized: L1"
    );
    let first = r.get_history_depth("docA").expect("depth stays readable");
    let second = r.get_history_depth("docA").expect("depth stays readable");
    assert_eq!(first, second, "two identical read-only calls agree");
    assert_eq!(first.total_depth, 1, "the entry survives the layer removal");
    assert_eq!(first.affected_layer_ids, vec!["L1".to_string()]);
}

#[test]
fn resize_layer_recreates_dimensions() {
    let mut r = reg();
    r.open_document("docA");
    r.add_layer("docA", "L1", 64, 64, vec![0; 64 * 64 * 4])
        .unwrap();
    // Mutate, then "resize" to 32x32 -- old 64x64 index math must no longer apply.
    r.resize_layer("docA", "L1", 32, 32, vec![1; 32 * 32 * 4])
        .unwrap();
    let layer = r.get_layer("docA", "L1").unwrap();
    assert_eq!(layer.width, 32);
    assert_eq!(layer.height, 32);
    assert_eq!(layer.pixels.len(), 32 * 32 * 4);
    // Old 64x64 offset (row 1 at x0,y64) is now out of bounds for the new buffer.
    assert_eq!(layer.pixels[0], 1);
}

#[test]
fn same_layer_id_in_different_documents_isolated() {
    let mut r = reg();
    r.open_document("docA");
    r.open_document("docB");
    r.add_layer("docA", "L", 64, 64, vec![0; 64 * 64 * 4])
        .unwrap();
    r.add_layer("docB", "L", 64, 64, vec![0; 64 * 64 * 4])
        .unwrap();
    // Mutate only docA's "L".
    r.get_layer_mut("docA", "L").unwrap().pixels[0] = 42;
    // docB's "L" canonical must be unchanged (still all-zero).
    let b = r.get_layer("docB", "L").unwrap();
    assert!(b.pixels.iter().all(|&v| v == 0), "docB isolated from docA");
    let a = r.get_layer("docA", "L").unwrap();
    assert_eq!(a.pixels[0], 42);
}

// -- Epoch --

#[test]
fn epoch_starts_zero_and_increments_on_mutation() {
    let mut r = reg();
    r.open_document("docA");
    r.add_layer("docA", "L1", 64, 64, vec![0u8; 64 * 64 * 4])
        .unwrap();
    assert_eq!(r.get_epoch("docA", "L1").unwrap(), 0);
    r.apply_pixel_patch(
        "docA",
        "L1",
        vec![tile(0, 0, 32, 32, 0)],
        vec![tile(0, 0, 32, 32, 5)],
    );
    assert_eq!(r.get_epoch("docA", "L1").unwrap(), 1);
    r.undo_pixel("docA");
    assert_eq!(r.get_epoch("docA", "L1").unwrap(), 2);
    r.redo_pixel("docA");
    assert_eq!(r.get_epoch("docA", "L1").unwrap(), 3);
}

#[test]
fn registry_epoch_per_layer_independent() {
    let mut r = reg();
    r.open_document("docA");
    r.add_layer("docA", "L1", 64, 64, vec![0; 64 * 64 * 4])
        .unwrap();
    r.add_layer("docA", "L2", 64, 64, vec![0; 64 * 64 * 4])
        .unwrap();
    assert_eq!(r.get_epoch("docA", "L1").unwrap(), 0);
    r.apply_pixel_patch(
        "docA",
        "L1",
        vec![tile(0, 0, 32, 32, 0)],
        vec![tile(0, 0, 32, 32, 3)],
    );
    assert_eq!(r.get_epoch("docA", "L1").unwrap(), 1);
    // L2 untouched.
    assert_eq!(r.get_epoch("docA", "L2").unwrap(), 0);
}

// -- Unified pixel history (DocumentCore is the sole authoritative cursor) --

#[test]
fn apply_pixel_patch_moves_cursor_and_bumps_version_once() {
    let mut r = reg();
    r.open_document("docA");
    r.add_layer("docA", "L1", 8, 8, vec![0; 8 * 8 * 4]).unwrap();
    let _ = r.apply_pixel_patch(
        "docA",
        "L1",
        vec![tile(0, 0, 8, 8, 0)],
        vec![tile(0, 0, 8, 8, 10)],
    );
    let _ = r.apply_pixel_patch(
        "docA",
        "L1",
        vec![tile(0, 0, 8, 8, 10)],
        vec![tile(0, 0, 8, 8, 20)],
    );
    let _ = r.apply_pixel_patch(
        "docA",
        "L1",
        vec![tile(0, 0, 8, 8, 20)],
        vec![tile(0, 0, 8, 8, 30)],
    );
    assert_eq!(r.get_history_cursor("docA"), Some(3));
    assert_eq!(r.get_history_version("docA"), Some(3));
    assert_eq!(r.get_layer("docA", "L1").unwrap().pixels[0], 30);
}

#[test]
fn mixed_brush_adjustment_brush_undo_redo_exact() {
    let mut r = reg();
    r.open_document("docA");
    r.add_layer("docA", "L1", 1, 1, vec![0, 0, 0, 255]).unwrap();

    // A: brush -> 11
    r.apply_pixel_patch(
        "docA",
        "L1",
        vec![tile(0, 0, 1, 1, 0)],
        vec![tile(0, 0, 1, 1, 11)],
    );
    // B: adjustment/baked -> 22
    r.apply_pixel_patch(
        "docA",
        "L1",
        vec![tile(0, 0, 1, 1, 11)],
        vec![tile(0, 0, 1, 1, 22)],
    );
    // C: brush -> 33
    r.apply_pixel_patch(
        "docA",
        "L1",
        vec![tile(0, 0, 1, 1, 22)],
        vec![tile(0, 0, 1, 1, 33)],
    );

    assert_eq!(r.get_history_cursor("docA"), Some(3));
    assert_eq!(r.get_layer("docA", "L1").unwrap().pixels[0], 33);

    // undo -> B (22)
    let u = r.undo_pixel("docA").unwrap();
    assert_eq!(u.0, "L1");
    assert_eq!(r.get_layer("docA", "L1").unwrap().pixels[0], 22);
    assert_eq!(r.get_history_cursor("docA"), Some(2));

    // undo -> A (11)
    let _ = r.undo_pixel("docA").unwrap();
    assert_eq!(r.get_layer("docA", "L1").unwrap().pixels[0], 11);
    assert_eq!(r.get_history_cursor("docA"), Some(1));

    // redo -> B (22)
    let _ = r.redo_pixel("docA").unwrap();
    assert_eq!(r.get_layer("docA", "L1").unwrap().pixels[0], 22);
    assert_eq!(r.get_history_cursor("docA"), Some(2));

    // redo -> C (33)
    let _ = r.redo_pixel("docA").unwrap();
    assert_eq!(r.get_layer("docA", "L1").unwrap().pixels[0], 33);
    assert_eq!(r.get_history_cursor("docA"), Some(3));
}

#[test]
fn cross_layer_undo_reverts_only_last_entry_layer() {
    let mut r = reg();
    r.open_document("docA");
    r.add_layer("docA", "L1", 1, 1, vec![0, 0, 0, 255]).unwrap();
    r.add_layer("docA", "L2", 1, 1, vec![0, 0, 0, 255]).unwrap();
    r.apply_pixel_patch(
        "docA",
        "L1",
        vec![tile(0, 0, 1, 1, 0)],
        vec![tile(0, 0, 1, 1, 11)],
    );
    r.apply_pixel_patch(
        "docA",
        "L2",
        vec![tile(0, 0, 1, 1, 0)],
        vec![tile(0, 0, 1, 1, 22)],
    );
    // undo reverts L2 only
    r.undo_pixel("docA").unwrap();
    assert_eq!(r.get_layer("docA", "L2").unwrap().pixels[0], 0);
    assert_eq!(r.get_layer("docA", "L1").unwrap().pixels[0], 11);
}

#[test]
fn apply_pixel_patch_missing_layer_does_not_move_cursor() {
    let mut r = reg();
    r.open_document("docA");
    // No layer seeded -- the command must fail WITHOUT pushing a history entry.
    let res = r.apply_pixel_patch(
        "docA",
        "L-missing",
        vec![tile(0, 0, 1, 1, 0)],
        vec![tile(0, 0, 1, 1, 5)],
    );
    assert!(res.is_none(), "no entry pushed for missing layer");
    assert_eq!(
        r.get_history_cursor("docA"),
        Some(0),
        "cursor unchanged on failure"
    );
}

#[test]
fn document_isolation_of_history_cursor() {
    let mut r = reg();
    r.open_document("docA");
    r.open_document("docB");
    r.add_layer("docA", "L", 1, 1, vec![0, 0, 0, 255]).unwrap();
    r.add_layer("docB", "L", 1, 1, vec![0, 0, 0, 255]).unwrap();
    r.apply_pixel_patch(
        "docA",
        "L",
        vec![tile(0, 0, 1, 1, 0)],
        vec![tile(0, 0, 1, 1, 77)],
    );
    // undo on docB (empty) must be a no-op
    assert!(r.undo_pixel("docB").is_none());
    assert_eq!(r.get_history_cursor("docA"), Some(1));
    assert_eq!(r.get_history_cursor("docB"), Some(0));
}

#[test]
fn new_command_after_undo_invalidates_redo() {
    let mut r = reg();
    r.open_document("docA");
    r.add_layer("docA", "L1", 1, 1, vec![0, 0, 0, 255]).unwrap();
    r.apply_pixel_patch(
        "docA",
        "L1",
        vec![tile(0, 0, 1, 1, 0)],
        vec![tile(0, 0, 1, 1, 1)],
    );
    r.apply_pixel_patch(
        "docA",
        "L1",
        vec![tile(0, 0, 1, 1, 1)],
        vec![tile(0, 0, 1, 1, 2)],
    );
    r.undo_pixel("docA").unwrap(); // cursor 1
                                   // new branch
    r.apply_pixel_patch(
        "docA",
        "L1",
        vec![tile(0, 0, 1, 1, 1)],
        vec![tile(0, 0, 1, 1, 9)],
    );
    assert_eq!(r.get_history_cursor("docA"), Some(2));
    // redo must be inert (truncated)
    assert!(r.redo_pixel("docA").is_none());
    assert_eq!(r.get_layer("docA", "L1").unwrap().pixels[0], 9);
}

#[test]
fn deep_history_cursor_unchanged_across_many_entries() {
    let mut r = reg();
    r.open_document("docA");
    r.add_layer("docA", "L1", 1, 1, vec![0, 0, 0, 255]).unwrap();
    let n: u8 = 50;
    for i in 1..=n {
        r.apply_pixel_patch(
            "docA",
            "L1",
            vec![tile(0, 0, 1, 1, i - 1)],
            vec![tile(0, 0, 1, 1, i)],
        );
    }
    assert_eq!(r.get_history_cursor("docA"), Some(n as usize));
    assert_eq!(r.get_history_version("docA"), Some(n as u64));
    // undo all the way down
    for k in (0..n).rev() {
        let _ = r.undo_pixel("docA").unwrap();
        assert_eq!(r.get_history_cursor("docA"), Some(k as usize));
    }
    assert_eq!(r.get_layer("docA", "L1").unwrap().pixels[0], 0);
    // redo all the way up
    for k in 1..=n {
        let _ = r.redo_pixel("docA").unwrap();
        assert_eq!(r.get_history_cursor("docA"), Some(k as usize));
    }
    assert_eq!(r.get_layer("docA", "L1").unwrap().pixels[0], n);
}

// -- Bug 1: overlapping strokes must composite onto canonical, not wipe --
#[test]
fn commit_pixels_composites_onto_canonical_overlapping() {
    let mut r = PixelStoreRegistry::new();
    r.open_document("d");
    let _ = r.add_layer("d", "L", 4, 4, vec![255; 4 * 4 * 4]); // white 4x4
                                                               // stroke A: 2x2 red opaque dab at (1,1) -- covers (0,0) within tile (0,0).
    let mut tip_a_data = Vec::new();
    for _ in 0..4 {
        tip_a_data.extend_from_slice(&[255u8, 0, 0, 255]);
    }
    let tip_a = RasterTip {
        width: 2,
        height: 2,
        data: &tip_a_data,
    };
    let dab = vec![RasterDab {
        x: 1.0,
        y: 1.0,
        alpha: 1.0,
    }];
    let _ = r
        .commit_pixels(
            "d",
            "L",
            vec![0; 4 * 4 * 4],
            4,
            4,
            false,
            2.0,
            &dab,
            &tip_a,
            true,
        )
        .unwrap();
    let l = r.get_layer("d", "L").unwrap();
    assert_eq!(l.pixels[0], 255); // R
    assert_eq!(l.pixels[1], 0); // G (red) at corner (0,0)
    assert_eq!(l.pixels[(3 * 4 + 3) * 4 + 1], 255); // far corner white

    // stroke B: 1x1 blue opaque dab at (1,1) -- overlaps A within the same tile.
    let tip_b = RasterTip {
        width: 1,
        height: 1,
        data: &[0u8, 0, 255, 255],
    };
    let _ = r
        .commit_pixels(
            "d",
            "L",
            vec![0; 4 * 4 * 4],
            4,
            4,
            false,
            1.0,
            &dab,
            &tip_b,
            true,
        )
        .unwrap();
    let l2 = r.get_layer("d", "L").unwrap();
    // Bug 1 assertion: A's pixel (0,0) must SURVIVE B's tile REPLACE
    // (G stays 0 red, NOT 255 white). Old dab-on-white path would wipe it.
    assert_eq!(
        l2.pixels[1], 0,
        "A's pixel wiped by overlapping B tile REPLACE"
    );
    assert_eq!(l2.pixels[20], 0); // overlap (1,1) now blue R==0
    assert_eq!(l2.pixels[22], 255); // blue B==255
    assert_eq!(l2.pixels[(3 * 4 + 3) * 4 + 1], 255); // far corner still white
}

#[test]
fn bounded_commit_matches_the_full_layer_pass() {
    let n = 1024usize;
    let base: Vec<u8> = (0..n * n * 4)
        .map(|i| match i % 4 {
            3 => 255,
            channel => ((i * 37 + channel * 11) & 0xff) as u8,
        })
        .collect();
    let mut tip_data = Vec::with_capacity(8 * 8 * 4);
    for y in 0..8i32 {
        for x in 0..8i32 {
            let alpha = if (x - 4).pow(2) + (y - 4).pow(2) <= 12 {
                255
            } else {
                0
            };
            tip_data.extend_from_slice(&[200, 120, 40, alpha]);
        }
    }
    let tip = RasterTip {
        width: 8,
        height: 8,
        data: &tip_data,
    };
    let dabs = vec![
        RasterDab {
            x: 40.0,
            y: 40.0,
            alpha: 1.0,
        },
        RasterDab {
            x: 300.5,
            y: 301.25,
            alpha: 1.0,
        },
    ];
    let mut full_reg = PixelStoreRegistry::new();
    full_reg.open_document("d");
    full_reg
        .add_layer("d", "L", n as u32, n as u32, base.clone())
        .unwrap();
    let mut bounded_reg = PixelStoreRegistry::new();
    bounded_reg.open_document("d");
    bounded_reg
        .add_layer("d", "L", n as u32, n as u32, base.clone())
        .unwrap();

    let (full_before, full_after) = full_reg
        .compute_commit_full("d", "L", n, n, false, 8.0, &dabs, &tip, true)
        .expect("full commit computes");
    let mut phases = CommitPhases::default();
    let (before, after, _epoch, _version) = bounded_reg
        .commit_pixels_with_phases(
            "d",
            "L",
            Vec::new(),
            n,
            n,
            false,
            8.0,
            &dabs,
            &tip,
            true,
            Some(&mut phases),
        )
        .expect("bounded commit lands");

    assert_eq!(before, full_before, "before tiles must be identical");
    assert_eq!(after, full_after, "after tiles must be identical");
    assert!(phases.crop_w < n as u32 && phases.crop_h < n as u32);
    assert_eq!((phases.crop_x, phases.crop_y), (0, 0));

    let mut expected = base;
    for patch in &full_after {
        for row in 0..patch.h {
            let src = row * patch.w * 4;
            let dst = ((patch.y as usize + row) * n + patch.x as usize) * 4;
            expected[dst..dst + patch.w * 4].copy_from_slice(&patch.data[src..src + patch.w * 4]);
        }
    }
    assert_eq!(bounded_reg.get_layer("d", "L").unwrap().pixels, expected);
}

#[test]
fn bounded_commit_matches_full_for_wide_large_brush_strokes() {
    let n = 1024usize;
    let base: Vec<u8> = (0..n * n * 4)
        .map(|i| match i % 4 {
            3 => 255,
            channel => ((i / 7 + channel * 13) & 0xff) as u8,
        })
        .collect();
    let tip_edge = 128usize;
    let mut tip_data = vec![0u8; tip_edge * tip_edge * 4];
    let centre = tip_edge as i32 / 2;
    for y in 0..tip_edge {
        for x in 0..tip_edge {
            let dx = x as i32 - centre;
            let dy = y as i32 - centre;
            if dx * dx + dy * dy <= centre * centre {
                let index = (y * tip_edge + x) * 4;
                tip_data[index..index + 4].copy_from_slice(&[220, 80, 40, 255]);
            }
        }
    }
    let tip = RasterTip {
        width: tip_edge,
        height: tip_edge,
        data: &tip_data,
    };
    // A wide stroke with a brush larger than the tip data: the full pass
    // samples the tip's top-left quadrant, and the crop must reproduce it.
    let dabs: Vec<RasterDab> = (0..=40)
        .map(|i| RasterDab {
            x: 20.0 + i as f64 * 24.0,
            y: 620.0,
            alpha: 1.0,
        })
        .collect();
    let mut full_reg = PixelStoreRegistry::new();
    full_reg.open_document("d");
    full_reg
        .add_layer("d", "L", n as u32, n as u32, base.clone())
        .unwrap();
    let mut bounded_reg = PixelStoreRegistry::new();
    bounded_reg.open_document("d");
    bounded_reg
        .add_layer("d", "L", n as u32, n as u32, base.clone())
        .unwrap();

    let (full_before, full_after) = full_reg
        .compute_commit_full("d", "L", n, n, false, 400.0, &dabs, &tip, true)
        .expect("full commit computes");
    let (before, after, _epoch, _version) = bounded_reg
        .commit_pixels("d", "L", Vec::new(), n, n, false, 400.0, &dabs, &tip, true)
        .expect("bounded commit lands");

    assert_eq!(before, full_before, "before tiles must be identical");
    assert_eq!(after, full_after, "after tiles must be identical");
}

#[test]
fn commit_pixels_reinit_after_close_is_idempotent() {
    let mut r = PixelStoreRegistry::new();
    r.open_document("d");
    let _ = r.add_layer("d", "L", 2, 2, vec![255; 16]);
    let tip = RasterTip {
        width: 1,
        height: 1,
        data: &[10u8, 20, 30, 255],
    };
    let dab = vec![RasterDab {
        x: 0.5,
        y: 0.5,
        alpha: 1.0,
    }];
    let _ = r
        .commit_pixels("d", "L", vec![0; 16], 2, 2, false, 1.0, &dab, &tip, true)
        .unwrap();
    assert_eq!(r.get_layer("d", "L").unwrap().pixels[0], 10);
    // close releases the store (simulating doc close).
    r.close_document("d");
    assert!(r.get_layer("d", "L").is_none());
    // reopen: ensure-if-absent re-inits from bytes; commit still composites.
    r.open_document("d");
    let _ = r
        .commit_pixels("d", "L", vec![255; 16], 2, 2, false, 1.0, &dab, &tip, true)
        .unwrap();
    assert_eq!(r.get_layer("d", "L").unwrap().pixels[0], 10);
}

#[test]
fn commit_pixels_doc_namespace_independent() {
    let mut r = PixelStoreRegistry::new();
    r.open_document("A");
    r.open_document("B");
    let _ = r.add_layer("A", "L", 2, 2, vec![255; 16]);
    let _ = r.add_layer("B", "L", 2, 2, vec![255; 16]); // same layerId, different doc
    let tip = RasterTip {
        width: 1,
        height: 1,
        data: &[1u8, 2, 3, 255],
    };
    let dab = vec![RasterDab {
        x: 0.5,
        y: 0.5,
        alpha: 1.0,
    }];
    let _ = r
        .commit_pixels("A", "L", vec![0; 16], 2, 2, false, 1.0, &dab, &tip, true)
        .unwrap();
    // doc B must be unaffected (separate namespace).
    assert_eq!(r.get_layer("B", "L").unwrap().pixels[0], 255);
    let _ = r
        .commit_pixels("B", "L", vec![0; 16], 2, 2, false, 1.0, &dab, &tip, true)
        .unwrap();
    assert_eq!(r.get_layer("B", "L").unwrap().pixels[0], 1);
    assert_eq!(r.get_layer("A", "L").unwrap().pixels[0], 1);
}

// -- Fill write_region --

#[test]
fn c5_4_write_region_writes_canonical_and_advances_history_once() {
    let mut r = PixelStoreRegistry::new();
    r.open_document("d");
    r.add_layer("d", "L", 8, 8, vec![0u8; 8 * 8 * 4]).unwrap();
    let before_ver = r.get_history_version("d").unwrap();
    let before_epoch = r.get_epoch("d", "L").unwrap();
    let mut rgba = vec![0u8; 4 * 4 * 4];
    for c in rgba.chunks_mut(4) {
        c.copy_from_slice(&[255, 0, 0, 255]);
    }
    let (b, a, epoch, version) = r.write_region("d", "L", 2, 2, 4, 4, rgba).expect("write");
    let px = r.get_layer("d", "L").unwrap().pixels.clone();
    assert_eq!(px[108], 255, "inside region ->† filled");
    assert_eq!(px[0], 0, "outside region ->† untouched");
    // Exactly ONE history step + ONE epoch bump.
    assert_eq!(version, before_ver + 1, "version +1");
    assert_eq!(epoch, before_epoch + 1, "epoch +1");
    assert_eq!(r.get_history_cursor("d"), Some(1), "cursor +1");
    // before/after are full 256-tiles (layer < 256) covering the region.
    assert_eq!(b.len(), 1);
    assert_eq!(a.len(), 1);
}

#[test]
fn c5_4_write_region_tiles_localized_and_full() {
    let mut r = PixelStoreRegistry::new();
    r.open_document("d");
    r.add_layer("d", "L", 300, 300, vec![0u8; 300 * 300 * 4])
        .unwrap();
    let mut rgba = vec![0u8; 10 * 10 * 4];
    for c in rgba.chunks_mut(4) {
        c.copy_from_slice(&[0, 255, 0, 255]);
    }
    // Region (250,150,10,10) crosses the x=256 tile boundary.
    let (b, a, _, _) = r
        .write_region("d", "L", 250, 150, 10, 10, rgba)
        .expect("write");
    assert_eq!(b.len(), 2, "exactly two 256-tiles intersect the region");
    for t in b.iter().chain(a.iter()) {
        // Tiles are 256-grid-aligned (edge tiles clipped to layer bounds).
        assert_eq!(t.x % 256, 0, "tile x on 256 grid");
        assert_eq!(t.y % 256, 0, "tile y on 256 grid");
    }
}

#[test]
fn c5_4_write_region_undo_redo_roundtrip() {
    let mut r = PixelStoreRegistry::new();
    r.open_document("d");
    r.add_layer("d", "L", 8, 8, vec![0u8; 8 * 8 * 4]).unwrap();
    let before = r.get_layer("d", "L").unwrap().snapshot_region(0, 0, 8, 8);
    let mut rgba = vec![0u8; 4 * 4 * 4];
    for c in rgba.chunks_mut(4) {
        c.copy_from_slice(&[1, 2, 3, 255]);
    }
    r.write_region("d", "L", 2, 2, 4, 4, rgba).unwrap();
    assert_eq!(r.get_layer("d", "L").unwrap().pixels[108], 1);
    let (_lid, _tiles, _e, _v) = r.undo_pixel("d").expect("undo");
    let after_undo = r.get_layer("d", "L").unwrap().snapshot_region(0, 0, 8, 8);
    assert_eq!(after_undo, before, "undo restores pre-fill canonical");
    let _ = r.redo_pixel("d").expect("redo");
    assert_eq!(
        r.get_layer("d", "L").unwrap().pixels[108],
        1,
        "redo re-applies fill"
    );
}

#[test]
fn c5_4_write_region_new_write_truncates_redo() {
    let mut r = PixelStoreRegistry::new();
    r.open_document("d");
    r.add_layer("d", "L", 8, 8, vec![0u8; 8 * 8 * 4]).unwrap();
    let mut red = vec![0u8; 4 * 4 * 4];
    for c in red.chunks_mut(4) {
        c.copy_from_slice(&[9, 9, 9, 255]);
    }
    r.write_region("d", "L", 0, 0, 4, 4, red.clone()).unwrap();
    r.undo_pixel("d");
    assert!(r.redo_pixel("d").is_some(), "redo available after undo");
    // New write after undo ->† future redo severed.
    r.write_region("d", "L", 4, 4, 4, 4, red.clone()).unwrap();
    assert!(
        r.redo_pixel("d").is_none(),
        "redo truncated after new write"
    );
    assert_eq!(r.get_history_cursor("d"), Some(2));
}

#[test]
fn c5_4_write_region_out_of_bounds_rejected_no_cursor() {
    let mut r = PixelStoreRegistry::new();
    r.open_document("d");
    r.add_layer("d", "L", 8, 8, vec![0u8; 8 * 8 * 4]).unwrap();
    let rgba = vec![0u8; 4 * 4 * 4];
    assert!(
        r.write_region("d", "L", 6, 6, 4, 4, rgba.clone()).is_none(),
        "overflow rejected"
    );
    assert!(
        r.write_region("d", "L", 0, 0, 4, 4, vec![0u8; 3]).is_none(),
        "size mismatch rejected"
    );
    assert_eq!(
        r.get_history_cursor("d"),
        Some(0),
        "no cursor movement on rejected write"
    );
}

#[test]
fn c5_4_write_region_history_bounded_to_50() {
    let mut r = PixelStoreRegistry::new();
    r.open_document("d");
    r.add_layer("d", "L", 64, 64, vec![0u8; 64 * 64 * 4])
        .unwrap();
    let mut rgba = vec![0u8; 4 * 4 * 4];
    for c in rgba.chunks_mut(4) {
        c.copy_from_slice(&[7, 7, 7, 255]);
    }
    for i in 0..60u8 {
        let x = (i % 8) as i64 * 4;
        let y = (i / 8) as i64 * 4;
        r.write_region("d", "L", x, y, 4, 4, rgba.clone()).unwrap();
    }
    assert_eq!(
        r.get_history_cursor("d"),
        Some(50),
        "cursor capped at max_depth"
    );
    for _ in 0..50 {
        assert!(r.undo_pixel("d").is_some());
    }
    assert!(r.undo_pixel("d").is_none(), "history bounded to 50 entries");
}

// (The external-pending barrier rejection is covered by the DocumentCore
//  unit test `pixel_undo_redo_rejected_while_external_pending` in protocol.rs.)

#[test]
fn c4_dirty_region_brush_contract() {
    let mut r = PixelStoreRegistry::new();
    r.open_document("d");
    r.add_layer("d", "L", 256, 256, vec![0u8; 256 * 256 * 4])
        .unwrap();
    // stroke 1: dirty region (40,40,60,60) painted red
    let mut red = vec![0u8; 60 * 60 * 4];
    for c in red.chunks_mut(4) {
        c.copy_from_slice(&[255, 0, 0, 255]);
    }
    let (before, after, epoch, version) = r
        .write_region("d", "L", 40, 40, 60, 60, red.clone())
        .unwrap();
    assert_eq!(epoch, 1);
    assert_eq!(version, 1);
    assert_eq!(
        before.len(),
        1,
        "before = one tile patch for the dirty rect"
    );
    assert_eq!(after.len(), 1, "after = one tile patch for the dirty rect");
    // Canonical pixels are the source of truth (TilePatch.data is raw RGBA).
    let at = r.get_layer("d", "L").unwrap().pixels[(40 * 256 + 40) * 4..(40 * 256 + 40) * 4 + 4]
        .to_vec();
    assert_eq!(
        at,
        vec![255, 0, 0, 255],
        "stroke 1 region painted red in canonical"
    );
    let untouched = r.get_layer("d", "L").unwrap().pixels[0..4].to_vec();
    assert_eq!(untouched, vec![0, 0, 0, 0], "outside dirty rect stays zero");
    // stroke 2: DISCONNECTED region (200,200,30,30) painted blue -> both accumulate
    let mut blue = vec![0u8; 30 * 30 * 4];
    for c in blue.chunks_mut(4) {
        c.copy_from_slice(&[0, 0, 255, 255]);
    }
    let res2 = r
        .write_region("d", "L", 200, 200, 30, 30, blue.clone())
        .unwrap();
    assert_eq!(res2.2, 2);
    assert_eq!(res2.3, 2);
    let px = r.get_layer("d", "L").unwrap().pixels[(40 * 256 + 40) * 4..(40 * 256 + 40) * 4 + 4]
        .to_vec();
    assert_eq!(
        px,
        vec![255, 0, 0, 255],
        "region 1 preserved after disconnected region 2"
    );
    let px2 = r.get_layer("d", "L").unwrap().pixels
        [(200 * 256 + 200) * 4..(200 * 256 + 200) * 4 + 4]
        .to_vec();
    assert_eq!(
        px2,
        vec![0, 0, 255, 255],
        "region 2 painted blue in canonical"
    );
}

// â”€â”€ Resize corrective: layer resize drops pixel history (no stale resurrect) â”€â”€
// Resizing a layer MUST drop its pixel-history entries so undo/redo CANNOT
// replay a stale-dim `Arc<StateNode>` onto the new buffer. Before the fix
// this panicked (write out of bounds) or silently resurrected pre-resize
// pixels over the re-seeded buffer.
#[test]
fn resize_invalidates_layer_undo_history_no_stale_resurrect() {
    let mut r = reg();
    r.open_document("docA");
    r.add_layer("docA", "L", 300, 300, vec![7u8; 300 * 300 * 4])
        .unwrap();
    let mut rgba = vec![0u8; 10 * 10 * 4];
    for c in rgba.chunks_mut(4) {
        c.copy_from_slice(&[1, 2, 3, 255]);
    }
    r.write_region("docA", "L", 40, 40, 10, 10, rgba.clone())
        .unwrap();
    assert_eq!(r.get_history_cursor("docA"), Some(1));

    r.resize_layer("docA", "L", 100, 100, vec![9u8; 100 * 100 * 4])
        .unwrap();

    let res = r.undo_pixel("docA");
    assert!(
        res.is_none(),
        "resize discards the resized layer's undo history"
    );
    let l = r.get_layer("docA", "L").unwrap();
    assert_eq!(l.width, 100);
    assert_eq!(l.height, 100);
    assert_eq!(l.pixels.len(), 100 * 100 * 4);
    assert_eq!(l.pixels[0], 9, "resized seed stays, no stale resurrect");
    assert_eq!(r.get_history_cursor("docA"), Some(0));
}

// Desync guard: a DIRECT write via `get_layer_mut` is invisible to the
// per-layer StateNode canon; the next commit must re-seed the canon from the
// mutated bytes so undo restores the direct-mutated value (not a stale base).
#[test]
fn direct_mutation_via_get_layer_mut_forces_state_node_reseed() {
    let mut r = reg();
    r.open_document("docA");
    r.add_layer("docA", "L", 8, 8, vec![0u8; 8 * 8 * 4])
        .unwrap();
    let mut rgba0 = vec![0u8; 4 * 4 * 4];
    rgba0[0] = 10;
    r.write_region("docA", "L", 0, 0, 4, 4, rgba0).unwrap();
    r.get_layer_mut("docA", "L").unwrap().pixels[0] = 42;
    let mut rgba1 = vec![0u8; 4 * 4 * 4];
    rgba1[0] = 99;
    r.write_region("docA", "L", 0, 0, 4, 4, rgba1).unwrap();
    assert!(r.undo_pixel("docA").is_some());
    assert_eq!(r.get_layer("docA", "L").unwrap().pixels[0], 42);
    assert!(r.undo_pixel("docA").is_some());
    assert_eq!(r.get_layer("docA", "L").unwrap().pixels[0], 0);
}

// Spot-check: with the row-major default (flag OFF) the commit's before/
// after TilePatches are BYTE-IDENTICAL to the row-major semantics.
#[test]
fn row_major_default_patches_byte_identical() {
    let mut r = reg();
    r.open_document("d");
    r.add_layer("d", "L", 300, 300, vec![0u8; 300 * 300 * 4])
        .unwrap();
    let mut rgba = vec![0u8; 10 * 10 * 4];
    for c in rgba.chunks_mut(4) {
        c.copy_from_slice(&[0, 255, 0, 255]);
    }
    let (b, a, _e, _v) = r
        .write_region("d", "L", 250, 150, 10, 10, rgba)
        .expect("write");
    assert_eq!(b.len(), 2, "two 256-tiles intersect");
    assert_eq!(a.len(), 2);
    for t in b.iter().chain(a.iter()) {
        assert_eq!(t.x % 256, 0, "tile on 256 grid");
        assert_eq!(t.y % 256, 0, "tile on 256 grid");
    }
    for t in &b {
        assert!(t.data.iter().all(|&x| x == 0), "pre-image zeros");
    }
    let a0 = &a[0];
    assert_eq!(a0.x, 0, "first after tile at x=0");
    assert_eq!(a0.y, 0, "first after tile at y=0");
    assert_eq!(a0.w, 256, "first after tile width clipped to 256");
    let inside = (150 * a0.w * 4 + 250 * 4)..(150 * a0.w * 4 + 250 * 4 + 4);
    assert_eq!(
        &a0.data[inside],
        &[0, 255, 0, 255],
        "after tile spliced green"
    );
    let outside = 0..4;
    assert_eq!(
        &a0.data[outside],
        &[0, 0, 0, 0],
        "outside region stays zero"
    );
}

// Malformed input at the registry seam MUST be gracefully skipped --
// no panic, no cursor move, no history entry. `cow_batch`'s internal asserts
// are now a safety net only, because this boundary validates first.
#[test]
fn apply_pixel_patch_rejects_malformed_input_gracefully() {
    let mut r = reg();
    r.open_document("d");
    r.add_layer("d", "L", 64, 64, vec![0u8; 64 * 64 * 4])
        .unwrap();
    assert_eq!(r.get_history_cursor("d"), Some(0));

    let oob = tile(60, 0, 64, 64, 7);
    assert!(
        r.apply_pixel_patch("d", "L", vec![], vec![oob]).is_none(),
        "OOB patch must be rejected gracefully"
    );
    let zero_w = tile(0, 0, 0, 64, 7);
    assert!(
        r.apply_pixel_patch("d", "L", vec![], vec![zero_w])
            .is_none(),
        "zero-width patch must be rejected gracefully"
    );
    let mm = TilePatch {
        x: 0,
        y: 0,
        w: 4,
        h: 4,
        data: vec![0u8; 4 * 4 * 4 - 1],
    };
    assert!(
        r.apply_pixel_patch("d", "L", vec![], vec![mm]).is_none(),
        "len-mismatch patch must be rejected gracefully"
    );

    assert_eq!(r.get_history_cursor("d"), Some(0));
    assert!(r.undo_pixel("d").is_none(), "no undo history created");
    assert_eq!(r.get_epoch("d", "L").unwrap(), 0, "epoch unchanged");
}

// A write region clipped against a right/bottom EDGE tile (513x513
// layer, 3x3 grid; edge tile at (512,512) is 1x1-clipped). The splice must
// land in the correct clipped tile bytes and not corrupt adjacent pixels.
#[test]
fn write_region_edge_clip_right_bottom_tile() {
    let mut r = reg();
    r.open_document("d");
    let w = 513u32;
    let h = 513u32;
    r.add_layer("d", "L", w, h, vec![0u8; (w * h * 4) as usize])
        .unwrap();

    let mut rgba = Vec::with_capacity(2 * 2 * 4);
    for v in [1u8, 2, 3, 4] {
        rgba.extend_from_slice(&[v, v, v, 255]);
    }
    let (b, a, _e, _v) = r
        .write_region("d", "L", 511, 511, 2, 2, rgba)
        .expect("write");

    let edge = a
        .iter()
        .find(|t| t.x == 512 && t.y == 512)
        .expect("edge tile present");
    assert_eq!(edge.w, 1, "edge tile width clipped to 1");
    assert_eq!(edge.h, 1, "edge tile height clipped to 1");
    assert_eq!(
        &edge.data[0..4],
        &[4, 4, 4, 255],
        "edge pixel spliced at (512,512)"
    );

    let l = r.get_layer("d", "L").unwrap();
    let pix = &l.pixels;
    let at = |x: usize, y: usize| {
        let i = (y * w as usize + x) * 4;
        &pix[i..i + 4]
    };
    assert_eq!(at(511, 511), &[1, 1, 1, 255]);
    assert_eq!(at(512, 511), &[2, 2, 2, 255]);
    assert_eq!(at(511, 512), &[3, 3, 3, 255]);
    assert_eq!(at(512, 512), &[4, 4, 4, 255]);
    assert_eq!(at(512, 512 - 2), &[0, 0, 0, 0]);
    let _ = b;
}

// Canonical bytes vs the independent reference copy on a
// `write_region` commit, so the spliced canonical pixels are validated
// end-to-end (not just tile-geometry).
#[test]
fn write_region_bytes_match_reference_copy() {
    let mut r = reg();
    r.open_document("d");
    let w = 513u32;
    let h = 513u32;
    r.add_layer("d", "L", w, h, vec![0u8; (w * h * 4) as usize])
        .unwrap();

    let mut reference: Vec<u8> = vec![0u8; (w * h * 4) as usize];
    let mut fill = |x: i64, y: i64, ww: usize, hh: usize, color: [u8; 4]| {
        let mut rgba = Vec::with_capacity(ww * hh * 4);
        for _ in 0..(ww * hh) {
            rgba.extend_from_slice(&color);
        }
        r.write_region("d", "L", x, y, ww, hh, rgba.clone())
            .unwrap();
        for row in 0..hh {
            let dst = ((y + row as i64) as usize * w as usize + x as usize) * 4;
            let src = row * ww * 4;
            reference[dst..dst + ww * 4].copy_from_slice(&rgba[src..src + ww * 4]);
        }
    };
    fill(0, 0, 4, 4, [1, 2, 3, 255]);
    fill(500, 500, 8, 8, [9, 8, 7, 255]);

    let l = r.get_layer("d", "L").unwrap();
    assert_byte_eq(&l.pixels, &reference, "write_region canonical bytes");
}
