// Phase D: Tauri document metadata snapshot/restore commands (adapter layer).
// METADATA ONLY — no pixel bytes cross the IPC. Drives the REAL `registry()`
// (process-global Mutex) like the frontend does: no mocks — these commands run
// in the actual Tauri command layer.

use photrez_core::pixel_store::registry;

/// Per-layer metadata entry (no pixels). `bitmap_token` is the OPAQUE reference
/// to the ImageBitmap TS owns; Rust never dereferences it (it only round-trips
/// it so a snapshot undo/redo can re-attach the SAME bitmap by token).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LayerMetaDto {
    pub layer_id: String,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub bitmap_token: Option<String>,
    pub epoch: u64,
    pub pixel_version: u64,
}

/// Document-level metadata snapshot (no pixels).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSnapshotDto {
    pub doc_id: String,
    pub version: u64,
    pub layers: Vec<LayerMetaDto>,
}

/// Result of a metadata restore: the new monotonic version + the max layer epoch.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotRestoreResult {
    pub version: u64,
    pub max_layer_epoch: u64,
}

/// Result of recording a snapshot entry: the new monotonic version + epoch.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotRecordResult {
    pub version: u64,
    pub epoch: u64,
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
            // metadata-only snapshot: Rust has no token for the layer bitmap, so
            // this adapter deliberately leaves it None (the TS side owns tokens).
            bitmap_token: None,
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

// ── Phase C: bitmap-token snapshot history (adapter layer) ────────────────
// Drives the REAL `registry()` (process-global Mutex) like the frontend does: no
// mocks — these commands run in the actual Tauri command layer. Records an
// atomic metadata+bitmap-token `Snapshot` entry (storing BOTH before + after) and
// undoes/redoes it by returning the `before`/`after` snapshot so the TS side can
// re-attach the exact ImageBitmap by token (never a detached one). METADATA ONLY
// — no pixel bytes cross the IPC.

fn dto_to_core_snapshot(dto: DocumentSnapshotDto) -> photrez_core::snapshot::DocumentSnapshot {
    let mut snap = photrez_core::snapshot::DocumentSnapshot::new(&dto.doc_id, dto.version);
    for l in dto.layers {
        let mut ls = photrez_core::snapshot::LayerSnapshot::new(&l.layer_id, l.width, l.height);
        ls.epoch = l.epoch;
        ls.pixel_version = l.pixel_version;
        if let Some(token) = l.bitmap_token {
            ls = ls.with_bitmap_token(token);
        }
        snap = snap.with_layer(ls);
    }
    snap
}

fn core_to_dto(snap: photrez_core::snapshot::DocumentSnapshot) -> DocumentSnapshotDto {
    let layers = snap
        .layers
        .into_iter()
        .map(|l| LayerMetaDto {
            layer_id: l.layer_id,
            width: l.width,
            height: l.height,
            bitmap_token: l.bitmap_token,
            epoch: l.epoch,
            pixel_version: l.pixel_version,
        })
        .collect();
    DocumentSnapshotDto {
        doc_id: snap.doc_id,
        version: snap.version,
        layers,
    }
}

fn max_layer_epoch(reg: &photrez_core::pixel_store::PixelStoreRegistry, doc_id: &str) -> u64 {
    reg.docs
        .get(doc_id)
        .map(|doc| doc.layers.values().map(|l| l.epoch()).max().unwrap_or(0))
        .unwrap_or(0)
}

/// Phase C: record an atomic metadata+bitmap-token snapshot entry (before/after)
/// into the document's unified `ProtocolEngine` cursor. Bumps `DocumentVersion`
/// exactly once and truncates the redo branch, like `record_external` — but stores
/// BOTH directions so a later undo returns the `before` snapshot and redo re-applies
/// the `after`, letting the TS side restore pixels by bitmap token. Returns the new
/// `{ version, epoch }`.
#[tauri::command]
pub fn rust_pixels_record_snapshot(
    doc_id: String,
    before: DocumentSnapshotDto,
    after: DocumentSnapshotDto,
) -> Result<SnapshotRecordResult, String> {
    let mut reg = registry();
    let reg = reg.get_or_insert_with(Default::default);
    reg.record_snapshot(
        &doc_id,
        dto_to_core_snapshot(before),
        dto_to_core_snapshot(after),
    )?;
    let version = reg.get_history_version(&doc_id).unwrap_or(0);
    let epoch = max_layer_epoch(reg, &doc_id);
    Ok(SnapshotRecordResult { version, epoch })
}

/// Phase C: undo the entry just below the cursor IF it is a `Snapshot` entry,
/// returning the `before` snapshot (with its per-layer bitmap tokens) so the TS
/// side can re-attach the exact ImageBitmap by token. Non-snapshot tips/invalid
/// docs are handled: invalid doc -> Err; non-snapshot tip -> Ok(None) (cursor not
/// moved, caller continues with the normal undo path).
#[tauri::command]
pub fn rust_pixels_undo_snapshot(doc_id: String) -> Result<Option<DocumentSnapshotDto>, String> {
    let mut reg = registry();
    let reg = reg.get_or_insert_with(Default::default);
    if !reg.docs.contains_key(&doc_id) {
        return Err(format!("document not open: {doc_id}"));
    }
    Ok(reg.undo_snapshot(&doc_id).map(core_to_dto))
}

/// Phase C: redo the entry at the cursor IF it is a `Snapshot` entry, returning
/// the `after` snapshot (with its per-layer bitmap tokens). Symmetric to
/// `rust_pixels_undo_snapshot`.
#[tauri::command]
pub fn rust_pixels_redo_snapshot(doc_id: String) -> Result<Option<DocumentSnapshotDto>, String> {
    let mut reg = registry();
    let reg = reg.get_or_insert_with(Default::default);
    if !reg.docs.contains_key(&doc_id) {
        return Err(format!("document not open: {doc_id}"));
    }
    Ok(reg.redo_snapshot(&doc_id).map(core_to_dto))
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
                    bitmap_token: None,
                    epoch: 0,
                    pixel_version: 0,
                },
                LayerMetaDto {
                    layer_id: "pd_absent_layer".to_string(),
                    width: 64,
                    height: 64,
                    bitmap_token: None,
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

// ── Phase C: bitmap-token snapshot history command tests ──────────────────
// Mirrors the C-core snapshot.rs tests but through the REAL Tauri command layer
// (drives the process-global `registry()`, no mocks). Covers: record bump once,
// undo returns before (with token), redo returns after (with token), invalid doc
// Err (record + undo + redo), and the non-snapshot-tip -> Ok(None) dispatch
// contract (routing must be by `tip_payload_kind`, self-aware here).
#[cfg(test)]
mod snapshot_record_tests {
    use super::*;
    use crate::paint_parity_cmds::TEST_REGISTRY_LOCK;
    use crate::paint_parity_cmds::{rust_pixels_init, rust_pixels_open_document};

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

    fn layer_meta(layer: &str, w: u32, h: u32, token: Option<&str>, epoch: u64) -> LayerMetaDto {
        LayerMetaDto {
            layer_id: layer.to_string(),
            width: w,
            height: h,
            bitmap_token: token.map(|s| s.to_string()),
            epoch,
            pixel_version: epoch,
        }
    }

    fn snapshot_dto(doc: &str, version: u64, layers: Vec<LayerMetaDto>) -> DocumentSnapshotDto {
        DocumentSnapshotDto {
            doc_id: doc.to_string(),
            version,
            layers,
        }
    }

    #[test]
    fn snapshot_record_bumps_version_once_undo_redo_tokens() {
        let _s = TEST_REGISTRY_LOCK.lock().unwrap();
        reset();
        init_layer("pc_snap", "pc_snap_L");
        let before = snapshot_dto(
            "pc_snap",
            1,
            vec![layer_meta("pc_snap_L", 64, 64, Some("tok-before"), 7)],
        );
        let after = snapshot_dto(
            "pc_snap",
            2,
            vec![layer_meta("pc_snap_L", 64, 64, Some("tok-after"), 8)],
        );
        let r = rust_pixels_record_snapshot("pc_snap".to_string(), before, after)
            .expect("record_snapshot");
        assert_eq!(r.version, 1, "version bumps exactly once");
        assert_eq!(
            r.epoch, 0,
            "metadata-only snapshot does not mutate layer store, so epoch stays 0"
        );

        let undo = rust_pixels_undo_snapshot("pc_snap".to_string())
            .expect("undo_snapshot Ok")
            .expect("undo_snapshot Some");
        assert_eq!(undo.doc_id, "pc_snap");
        assert_eq!(undo.layers[0].bitmap_token.as_deref(), Some("tok-before"));
        assert_eq!(undo.layers[0].epoch, 7);

        let redo = rust_pixels_redo_snapshot("pc_snap".to_string())
            .expect("redo_snapshot Ok")
            .expect("redo_snapshot Some");
        assert_eq!(redo.layers[0].bitmap_token.as_deref(), Some("tok-after"));
        assert_eq!(redo.layers[0].epoch, 8);
    }

    #[test]
    fn snapshot_record_invalid_doc_errors_no_state_change() {
        let _s = TEST_REGISTRY_LOCK.lock().unwrap();
        reset();
        let before = snapshot_dto("pc_absent", 1, vec![]);
        let after = snapshot_dto("pc_absent", 2, vec![]);
        let err = rust_pixels_record_snapshot("pc_absent".to_string(), before, after).unwrap_err();
        assert!(err.contains("document not open"), "err: {err}");
    }

    #[test]
    fn snapshot_undo_redo_invalid_doc_errors() {
        let _s = TEST_REGISTRY_LOCK.lock().unwrap();
        reset();
        let err = rust_pixels_undo_snapshot("pc_absent".to_string()).unwrap_err();
        assert!(err.contains("document not open"), "undo err: {err}");
        let err = rust_pixels_redo_snapshot("pc_absent".to_string()).unwrap_err();
        assert!(err.contains("document not open"), "redo err: {err}");
    }

    #[test]
    fn snapshot_undo_when_tip_not_snapshot_returns_none_cursor_unchanged() {
        let _s = TEST_REGISTRY_LOCK.lock().unwrap();
        reset();
        init_layer("pc_pixel", "pc_pixel_L");
        // Commit a Pixel entry (write_region) so the tip is NOT a Snapshot.
        crate::paint_parity_cmds::rust_pixels_write_region(
            "pc_pixel".to_string(),
            "pc_pixel_L".to_string(),
            0,
            0,
            64,
            64,
            vec![42; 64 * 64 * 4],
        )
        .expect("write_region");
        // undo_snapshot must return Ok(None) WITHOUT moving the cursor (the
        // caller dispatches the real undo via undo_pixel).
        let res = rust_pixels_undo_snapshot("pc_pixel".to_string()).expect("undo_snapshot Ok");
        assert!(res.is_none());
    }
}
