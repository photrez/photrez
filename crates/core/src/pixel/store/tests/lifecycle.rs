// Storage lifecycle: closing a document or removing a layer releases its pixel storage,
// a removed layer's epoch is rejected while history depth survives, and the per-layer
// epoch counters start at zero and stay independent.
use super::{reg, tile};

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
