// Phase D: Tauri document metadata snapshot/restore commands (adapter layer).
// METADATA ONLY — no pixel bytes cross the IPC. Drives the REAL `registry()`
// (process-global Mutex) like the frontend does: no mocks — these commands run
// in the actual Tauri command layer.

use photrez_core::pixel_store::registry;

/// Per-layer metadata entry (no pixels).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct LayerMetaDto {
    pub layer_id: String,
    pub width: u32,
    pub height: u32,
    pub epoch: u64,
    pub pixel_version: u64,
}

/// Document-level metadata snapshot (no pixels).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct DocumentSnapshotDto {
    pub doc_id: String,
    pub version: u64,
    pub layers: Vec<LayerMetaDto>,
}

/// Result of a metadata restore: the new monotonic version + the max layer epoch.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct SnapshotRestoreResult {
    pub version: u64,
    pub max_layer_epoch: u64,
}

/// Phase D: take a document-level metadata snapshot (NO pixel bytes). Returns
/// the document `DocumentVersion` + per-layer `{ layer_id, width, height,
/// epoch, pixel_version }`. Errors when the document (or the registry) is absent.
#[tauri::command]
pub fn document_snapshot(doc_id: String) -> Result<DocumentSnapshotDto, String> {
    let reg = registry();
    let reg = reg
        .as_ref()
        .ok_or_else(|| "pixel store not initialized".to_string())?;
    if !reg.docs.contains_key(&doc_id) {
        return Err(format!("document not open: {doc_id}"));
    }
    let version = reg.get_history_version(&doc_id).unwrap_or(0);
    let doc = reg.docs.get(&doc_id).expect("checked above");
    let mut layers: Vec<LayerMetaDto> = doc
        .layers
        .iter()
        .map(|(lid, layer)| LayerMetaDto {
            layer_id: lid.clone(),
            width: layer.width,
            height: layer.height,
            epoch: layer.epoch(),
            pixel_version: layer.epoch(),
        })
        .collect();
    // Deterministic order: `doc.layers` is a HashMap (random iteration), so sort
    // by layer_id to make the snapshot reproducible / round-trip byte-exact.
    layers.sort_by_key(|l| l.layer_id.clone());
    Ok(DocumentSnapshotDto {
        doc_id,
        version,
        layers,
    })
}

/// Phase D: re-apply document METADATA (NO pixel bytes) for the given snapshot.
/// Metadata-only reconciliation: a layer that exists matches (no-op — restore
/// carries no pixels, so its canonical epoch is already correct); a layer that is
/// missing is a no-op (pixel buffers are NEVER created from metadata). Records
/// ONE logical step through the existing history bump so `DocumentVersion` advances
/// exactly once (no per-layer duplication). Returns the new
/// `{ version, max_layer_epoch }`.
#[tauri::command]
pub fn document_restore(
    doc_id: String,
    snapshot: DocumentSnapshotDto,
) -> Result<SnapshotRestoreResult, String> {
    // Trust boundary: reject a snapshot whose doc_id does not match the target
    // document BEFORE any mutation.
    if snapshot.doc_id != doc_id {
        return Err("snapshot doc_id mismatch".to_string());
    }
    let mut reg = registry();
    let reg = reg.get_or_insert_with(Default::default);
    // Only layers already present are "reconciled"; absent layers stay absent.
    let known: Vec<String> = reg
        .docs
        .get(&doc_id)
        .map(|doc| {
            snapshot
                .layers
                .iter()
                .map(|l| l.layer_id.clone())
                .filter(|lid| doc.layers.contains_key(lid))
                .collect()
        })
        .unwrap_or_default();
    let token = format!("snapshot-{}", snapshot.version);
    reg.record_external(
        &doc_id,
        "document_restore",
        &known,
        "restore-adapter",
        &token,
        0,
    )?;
    let version = reg.get_history_version(&doc_id).unwrap_or(0);
    let max_layer_epoch = reg
        .docs
        .get(&doc_id)
        .map(|doc| doc.layers.values().map(|l| l.epoch()).max().unwrap_or(0))
        .unwrap_or(0);
    Ok(SnapshotRestoreResult {
        version,
        max_layer_epoch,
    })
}

// ── Phase D: document snapshot/restore IPC contract (adapter layer) ──────────
// Drives the REAL `registry()` (process-global Mutex) like the frontend does: no
// mocks — these commands run in the actual Tauri command layer.
#[cfg(test)]
mod phase_d_tests {
    use super::*;
    use crate::paint_parity_cmds::TEST_REGISTRY_LOCK;
    use crate::paint_parity_cmds::{
        rust_pixels_init, rust_pixels_open_document, rust_pixels_undo, rust_pixels_write_region,
    };

    fn reset() {
        *registry() = None;
    }

    fn init_layer(doc: &str, layer: &str) {
        rust_pixels_open_document(doc.to_string());
        rust_pixels_init(
            doc.to_string(),
            layer.to_string(),
            64,
            64,
            vec![0; 64 * 64 * 4],
        )
        .unwrap();
    }

    #[test]
    fn phase_d_snapshot_returns_version_and_layer_metas() {
        let _s = TEST_REGISTRY_LOCK.lock().unwrap();
        reset();
        init_layer("pd_snap", "pd_snap_L");
        let snap = document_snapshot("pd_snap".to_string()).expect("snapshot");
        assert_eq!(snap.doc_id, "pd_snap");
        assert_eq!(snap.version, 0, "fresh doc has version 0");
        assert_eq!(snap.layers.len(), 1, "one seeded layer");
        let m = &snap.layers[0];
        assert_eq!(m.layer_id, "pd_snap_L");
        assert_eq!(m.width, 64);
        assert_eq!(m.height, 64);
        assert_eq!(m.epoch, 0, "fresh layer epoch 0");
        assert_eq!(
            m.pixel_version, 0,
            "metadata-only snapshot has no pixel bytes"
        );
    }

    #[test]
    fn phase_d_snapshot_invalid_doc_errors() {
        let _s = TEST_REGISTRY_LOCK.lock().unwrap();
        reset();
        init_layer("pd_snap", "pd_snap_L");
        // Registry exists but the doc does NOT: must error, never panic.
        let res = document_snapshot("pd_absent".to_string());
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("document not open"));
    }

    #[test]
    fn phase_d_restore_bumps_version_once_no_per_layer_duplication() {
        let _s = TEST_REGISTRY_LOCK.lock().unwrap();
        reset();
        init_layer("pd_restore", "pd_restore_L1");
        init_layer("pd_restore", "pd_restore_L2"); // two layers in one doc
        let snap = document_snapshot("pd_restore".to_string()).expect("snapshot");
        assert_eq!(snap.layers.len(), 2);
        assert_eq!(snap.version, 0);

        // ONE restore for TWO layers must advance version exactly ONCE (not 2).
        let r = document_restore("pd_restore".to_string(), snap.clone()).expect("restore");
        assert_eq!(r.version, 1, "single logical restore step = one bump");
        let snap_after = document_snapshot("pd_restore".to_string()).expect("snapshot2");
        assert_eq!(snap_after.version, 1);

        // A second restore is another independent logical step (monotonic, no dup).
        let r2 = document_restore("pd_restore".to_string(), snap).expect("restore2");
        assert_eq!(r2.version, 2);
    }

    #[test]
    fn phase_d_restore_invalid_doc_errors_no_state_change() {
        let _s = TEST_REGISTRY_LOCK.lock().unwrap();
        reset();
        init_layer("pd_restore", "pd_restore_L1");
        let before = document_snapshot("pd_restore".to_string()).expect("before");
        assert_eq!(before.version, 0);

        // Build a snapshot for a doc that isn't open → restore must Err, no state
        // change, no panic.
        let ghost = DocumentSnapshotDto {
            doc_id: "pd_ghost".to_string(),
            version: 0,
            layers: vec![],
        };
        let res = document_restore("pd_ghost".to_string(), ghost);
        assert!(res.is_err());
        let after = document_snapshot("pd_restore".to_string()).expect("after");
        assert_eq!(after.version, 0, "no state change on failed restore");
    }

    #[test]
    fn phase_d_snapshot_restore_round_trip_version_monotonic() {
        let _s = TEST_REGISTRY_LOCK.lock().unwrap();
        reset();
        init_layer("pd_round", "pd_round_L");
        let snap0 = document_snapshot("pd_round".to_string()).expect("snap0");
        assert_eq!(snap0.version, 0);

        // Modify pixels: epoch advances (1), DocumentVersion advances (1).
        let w = 64i64;
        let h = 64i64;
        let rgba = vec![42u8; (64 * 64 * 4) as usize];
        let _mc = rust_pixels_write_region(
            "pd_round".to_string(),
            "pd_round_L".to_string(),
            0,
            0,
            w,
            h,
            rgba,
        )
        .expect("write_region");
        let snap1 = document_snapshot("pd_round".to_string()).expect("snap1");
        assert_eq!(snap1.version, 1);
        assert_eq!(
            snap1.layers[0].epoch, 1,
            "canonical mutation advanced epoch"
        );

        // Restore (metadata-only): version bumps exactly once; max_layer_epoch is
        // reported, NOT reset (restore carries no pixels).
        let r = document_restore("pd_round".to_string(), snap0.clone()).expect("restore");
        assert_eq!(r.version, 2);
        assert_eq!(
            r.max_layer_epoch, 1,
            "epoch reflects current canonical state"
        );

        let snap2 = document_snapshot("pd_round".to_string()).expect("snap2");
        assert_eq!(snap2.version, 2);
        assert!(
            snap0.version < snap1.version && snap1.version < snap2.version,
            "DocumentVersion strictly monotonic across snapshot/restore"
        );
    }

    #[test]
    fn phase_d_restore_per_doc_isolation() {
        let _s = TEST_REGISTRY_LOCK.lock().unwrap();
        reset();
        init_layer("pd_iso_a", "pd_iso_A");
        init_layer("pd_iso_b", "pd_iso_B");
        let snap_b0 = document_snapshot("pd_iso_b".to_string()).expect("snapB0");
        let snap_a = document_snapshot("pd_iso_a".to_string()).expect("snapA");
        assert_eq!(snap_b0.version, 0);

        // Restoring docA must never bump docB's version.
        document_restore("pd_iso_a".to_string(), snap_a).expect("restoreA");
        let snap_b1 = document_snapshot("pd_iso_b".to_string()).expect("snapB1");
        let snap_a1 = document_snapshot("pd_iso_a".to_string()).expect("snapA1");
        assert_eq!(snap_b1.version, 0, "per-doc isolation: docB untouched");
        assert_eq!(snap_a1.version, 1, "docA bumped by its own restore");
    }

    #[test]
    fn phase_d_restore_missing_layer_is_noop_never_creates_buffer() {
        let _s = TEST_REGISTRY_LOCK.lock().unwrap();
        reset();
        init_layer("pd_missing", "pd_missing_L");
        // Snapshot references a layer that does NOT exist in the store.
        let crafted = DocumentSnapshotDto {
            doc_id: "pd_missing".to_string(),
            version: 0,
            layers: vec![
                LayerMetaDto {
                    layer_id: "pd_missing_L".to_string(),
                    width: 64,
                    height: 64,
                    epoch: 0,
                    pixel_version: 0,
                },
                LayerMetaDto {
                    layer_id: "pd_absent_layer".to_string(),
                    width: 64,
                    height: 64,
                    epoch: 0,
                    pixel_version: 0,
                },
            ],
        };
        let r = document_restore("pd_missing".to_string(), crafted).expect("restore");
        assert_eq!(r.version, 1);
        // The absent layer was NOT materialized from metadata (no pixel buffers).
        let reg = registry();
        let reg = reg.as_ref().expect("registry");
        assert!(reg.get_layer("pd_missing", "pd_missing_L").is_some());
        assert!(
            reg.get_layer("pd_missing", "pd_absent_layer").is_none(),
            "metadata-only restore must not create pixel buffers"
        );
    }

    #[test]
    fn snapshot_doc_id_mismatch_restore_errors() {
        let _s = TEST_REGISTRY_LOCK.lock().unwrap();
        reset();
        init_layer("pd_mismatch", "pd_mismatch_L");
        let before = document_snapshot("pd_mismatch".to_string()).expect("before");
        assert_eq!(before.version, 0);
        // Snapshot that belongs to a DIFFERENT doc: must reject BEFORE mutating.
        let foreign = DocumentSnapshotDto {
            doc_id: "pd_other".to_string(),
            version: 0,
            layers: before.layers.clone(),
        };
        let res = document_restore("pd_mismatch".to_string(), foreign);
        assert!(res.is_err());
        assert!(
            res.unwrap_err().contains("snapshot doc_id mismatch"),
            "doc_id guard message"
        );
        let after = document_snapshot("pd_mismatch".to_string()).expect("after");
        assert_eq!(after.version, 0, "no state change on doc_id mismatch");
    }

    #[test]
    fn restore_when_registry_uninitialized_errors() {
        let _s = TEST_REGISTRY_LOCK.lock().unwrap();
        reset(); // registry() -> None
        let snap = DocumentSnapshotDto {
            doc_id: "pd_uninit".to_string(),
            version: 0,
            layers: vec![],
        };
        let res = document_restore("pd_uninit".to_string(), snap);
        assert!(
            res.is_err(),
            "restore with no registry must Err, never panic"
        );
    }

    #[test]
    fn restore_then_undo_pins_current_behavior() {
        let _s = TEST_REGISTRY_LOCK.lock().unwrap();
        reset();
        init_layer("pd_pin", "pd_pin_L");
        // Commit real pixel work: version 1, layer epoch 1.
        let _mc = rust_pixels_write_region(
            "pd_pin".to_string(),
            "pd_pin_L".to_string(),
            0,
            0,
            64,
            64,
            vec![42; 64 * 64 * 4],
        )
        .expect("write_region");
        assert_eq!(document_snapshot("pd_pin".to_string()).unwrap().version, 1);

        // Restore records ONE External tip entry -> version 2.
        let snap = document_snapshot("pd_pin".to_string()).expect("snap");
        let r = document_restore("pd_pin".to_string(), snap).expect("restore");
        assert_eq!(r.version, 2);
        assert_eq!(r.max_layer_epoch, 1);

        // PIN the CURRENT engine behavior: undoing the External tip entry consumes
        // it (cursor moves, DocumentVersion advances) WITHOUT reverting any pixels.
        // This is pre-existing engine behavior, NOT a fix — pinned so C can resolve
        // routing.
        let u = rust_pixels_undo("pd_pin".to_string(), "pd_pin_L".to_string()).expect("undo");
        assert_eq!(
            u.tiles.len(),
            0,
            "External tip has no pixel tiles to revert"
        );
        assert_eq!(u.epoch, 1, "no pixel revert: layer epoch unchanged");
        assert_eq!(
            u.version, 3,
            "cursor consumed the External entry -> version advances"
        );
    }
}
