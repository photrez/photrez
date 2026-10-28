// SPDX-License-Identifier: AGPL-3.0-or-later

use super::{reg, tile};
use crate::pixel::store::PixelStoreRegistry;

#[derive(Debug, PartialEq, Eq)]
struct ObservableLayerState {
    width: u32,
    height: u32,
    bytes: Vec<u8>,
    epoch: u64,
    cursor: Option<usize>,
    version: Option<u64>,
    history: Option<(usize, usize, usize, Vec<String>)>,
}

fn observable_layer_state(
    r: &PixelStoreRegistry,
    doc_id: &str,
    layer_id: &str,
) -> ObservableLayerState {
    let layer = r.get_layer(doc_id, layer_id).expect("layer exists");
    let depth = r.get_history_depth(doc_id).map(|depth| {
        (
            depth.total_depth,
            depth.undo_depth,
            depth.redo_depth,
            depth.affected_layer_ids,
        )
    });
    ObservableLayerState {
        width: layer.width,
        height: layer.height,
        bytes: layer.pixels.clone(),
        epoch: r.get_epoch(doc_id, layer_id).expect("epoch exists"),
        cursor: r.get_history_cursor(doc_id),
        version: r.get_history_version(doc_id),
        history: depth,
    }
}

fn seed_layer_with_history(r: &mut PixelStoreRegistry) {
    r.add_layer("invalid-input", "layer", 2, 2, vec![0; 16])
        .expect("valid seed");
    r.apply_pixel_patch("invalid-input", "layer", vec![], vec![tile(0, 0, 2, 2, 7)])
        .expect("valid history entry");
}

#[test]
fn invalid_layer_seed_returns_error_without_mutation() {
    let mut r = reg();
    seed_layer_with_history(&mut r);
    let before = observable_layer_state(&r, "invalid-input", "layer");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        r.add_layer("invalid-input", "layer", 2, 2, vec![9; 15])
    }));
    assert!(matches!(result, Ok(Err(ref error)) if error == "pixel buffer size mismatch"));
    assert_eq!(observable_layer_state(&r, "invalid-input", "layer"), before);

    let missing_doc_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        r.add_layer("invalid-doc", "layer", 1, 1, Vec::new())
    }));
    assert!(
        matches!(missing_doc_result, Ok(Err(ref error)) if error == "pixel buffer size mismatch")
    );
    assert_eq!(r.get_history_cursor("invalid-doc"), None);
}

#[test]
fn invalid_layer_resize_returns_error_without_mutation() {
    let mut r = reg();
    seed_layer_with_history(&mut r);
    let before = observable_layer_state(&r, "invalid-input", "layer");

    for (width, height, expected_error) in [
        (0, 4, "invalid pixel dimensions"),
        (u32::MAX, u32::MAX, "pixel buffer size overflow"),
        (3, 2, "pixel buffer size mismatch"),
    ] {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            r.resize_layer("invalid-input", "layer", width, height, Vec::new())
        }));
        assert!(
            matches!(result, Ok(Err(ref error)) if error == expected_error),
            "resize {width}x{height} should return {expected_error:?}"
        );
        assert_eq!(observable_layer_state(&r, "invalid-input", "layer"), before);
    }
}

#[test]
fn zero_by_zero_empty_layer_remains_valid() {
    let mut r = reg();
    r.add_layer("empty", "layer", 0, 0, Vec::new())
        .expect("empty layer seed");

    let layer = r.get_layer("empty", "layer").expect("empty layer exists");
    assert_eq!((layer.width, layer.height), (0, 0));
    assert!(layer.pixels.is_empty());
    assert_eq!(r.get_epoch("empty", "layer").unwrap(), 0);
    assert_eq!(r.get_history_cursor("empty"), Some(0));
    assert_eq!(r.get_history_depth("empty").unwrap().total_depth, 0);
}

#[test]
fn write_region_rejects_overflowing_geometry_without_mutation() {
    let mut r = reg();
    r.add_layer("invalid-input", "layer", 4, 4, vec![0; 4 * 4 * 4])
        .expect("valid seed");
    r.write_region("invalid-input", "layer", 0, 0, 1, 1, vec![7; 4])
        .expect("valid history entry");
    let before = observable_layer_state(&r, "invalid-input", "layer");

    for (x, y, width, height, rgba) in [
        (i64::MAX, 0, 1, 1, vec![9; 4]),
        (0, i64::MAX, 1, 1, vec![9; 4]),
        (1, 1, usize::MAX, 1, Vec::new()),
    ] {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            r.write_region("invalid-input", "layer", x, y, width, height, rgba)
        }));
        assert!(
            matches!(result, Ok(None)),
            "overflowing region {x},{y} {width}x{height} must return None without panic"
        );
        assert_eq!(observable_layer_state(&r, "invalid-input", "layer"), before);
    }
}

#[test]
fn write_region_rejects_length_mismatch_without_mutation_then_accepts_valid_write() {
    let mut r = reg();
    seed_layer_with_history(&mut r);
    let before = observable_layer_state(&r, "invalid-input", "layer");

    let invalid = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        r.write_region("invalid-input", "layer", 0, 0, 1, 1, vec![9; 3])
    }));
    assert!(matches!(invalid, Ok(None)));
    assert_eq!(observable_layer_state(&r, "invalid-input", "layer"), before);

    let (_, _, epoch, version) = r
        .write_region("invalid-input", "layer", 1, 1, 1, 1, vec![9; 4])
        .expect("valid write after refusal");
    let after = observable_layer_state(&r, "invalid-input", "layer");
    assert_eq!(&after.bytes[12..16], &[9; 4]);
    assert_eq!(after.epoch, before.epoch + 1);
    assert_eq!(after.cursor, before.cursor.map(|cursor| cursor + 1));
    assert_ne!(after.version, before.version);
    assert_eq!(after.version, Some(version));
    assert_eq!(epoch, after.epoch);
    assert_ne!(after.history, before.history);
}

#[test]
fn write_region_rejects_rgba_length_overflow_without_mutation() {
    let mut r = reg();
    seed_layer_with_history(&mut r);
    let before = observable_layer_state(&r, "invalid-input", "layer");
    let width = usize::MAX / 4 + 1;

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        r.write_region("invalid-input", "layer", 0, 0, width, 2, Vec::new())
    }));
    assert!(matches!(result, Ok(None)));
    assert_eq!(observable_layer_state(&r, "invalid-input", "layer"), before);
}
