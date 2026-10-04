// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Read-only history probes. Registered as `rust_pixels_history_depth` (stream
// depth) and `rust_pixels_history_tip` (cursor position + per-direction tip
// kinds).
//
// Contract: `rust_pixels_history_depth` returns exactly
// { total_depth, undo_depth, redo_depth, affected_layer_ids } for one document,
// and `rust_pixels_history_tip` returns
// { total_depth, undo_depth, redo_depth, undo_tip_kind, redo_tip_kind }.
// The accessors behind them take `&self` only - no cursor pop, tile write,
// version bump, Arc<StateNode> mutation, or byte return - so two calls on an
// unchanged document are byte-identical. Empty, whitespace-only, unknown and
// oversize doc_id reject with a bare string (Tauri surfaces a Rust Err(String)
// as a bare string); they never resolve to a zero-filled struct.

use photrez_core::pixel_store::{registry, HistoryDepth, HistoryTip, PayloadKind};

/// Longest `doc_id` (UTF-8 bytes) this probe accepts. Production ids are
/// `doc-<uuid>` (41 bytes); 256 keeps headroom while rejecting garbage before
/// it reaches the process-global registry lock. No other command in this crate
/// bounds doc_id, so this threshold is probe-local by necessity.
const MAX_DOC_ID_BYTES: usize = 256;

/// Read-only history-depth probe: never mutates the store, and every invalid
/// `doc_id` rejects with a bare string instead of resolving a zero struct.
#[tauri::command]
pub fn rust_pixels_history_depth(doc_id: String) -> Result<HistoryDepth, String> {
    if doc_id.trim().is_empty() {
        return Err("empty doc_id".to_string());
    }
    if doc_id.len() > MAX_DOC_ID_BYTES {
        return Err(format!(
            "oversize doc_id: {} bytes (max {MAX_DOC_ID_BYTES})",
            doc_id.len()
        ));
    }
    let reg = registry();
    let reg = reg
        .as_ref()
        .ok_or_else(|| "pixel store not initialized".to_string())?;
    reg.get_history_depth(&doc_id)
        .ok_or_else(|| format!("document not open: {doc_id}"))
}

/// Read-only cursor-position probe: the stream's depth plus the payload kind
/// each direction would consume, so a caller can compare this stream's cursor
/// against the TypeScript undo stack's own depth.
///
/// Mutates nothing - the accessor is `&self` only and returns no pixel bytes or
/// payload data. Every invalid `doc_id` rejects with a bare string rather than
/// resolving a zero cursor that would read as "no history".
#[tauri::command]
pub fn rust_pixels_history_tip(doc_id: String) -> Result<HistoryTip, String> {
    if doc_id.trim().is_empty() {
        return Err("empty doc_id".to_string());
    }
    if doc_id.len() > MAX_DOC_ID_BYTES {
        return Err(format!(
            "oversize doc_id: {} bytes (max {MAX_DOC_ID_BYTES})",
            doc_id.len()
        ));
    }
    let reg = registry();
    let reg = reg
        .as_ref()
        .ok_or_else(|| "pixel store not initialized".to_string())?;
    reg.get_history_tip(&doc_id)
        .ok_or_else(|| format!("document not open: {doc_id}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document_snapshot_cmds::{rust_pixels_record_snapshot, DocumentSnapshotDto};
    use crate::paint_parity_cmds::{
        apply_tile_patch, rust_pixels_init, rust_pixels_open_document, rust_pixels_record_external,
        rust_pixels_redo, rust_pixels_undo, rust_pixels_write_region, TilePatchWire,
        TEST_REGISTRY_LOCK,
    };

    fn wire(x: i64, y: i64, w: usize, h: usize, data: Vec<u8>) -> TilePatchWire {
        TilePatchWire { x, y, w, h, data }
    }

    fn cursor_and_version(doc: &str) -> (Option<usize>, Option<u64>) {
        let reg = registry();
        let reg = reg.as_ref().expect("registry initialized");
        (reg.get_history_cursor(doc), reg.get_history_version(doc))
    }

    fn cursor(doc: &str) -> usize {
        cursor_and_version(doc).0.expect("cursor")
    }

    fn version(doc: &str) -> u64 {
        cursor_and_version(doc).1.expect("version")
    }

    #[test]
    fn depth_probe_is_read_only_and_repeatable() {
        let _g = TEST_REGISTRY_LOCK.lock().unwrap();
        *registry() = None;

        let doc = "depth-doc-1".to_string();
        let layer = "depth-layer-1".to_string();
        rust_pixels_open_document(doc.clone());
        rust_pixels_init(doc.clone(), layer.clone(), 4, 4, vec![0; 4 * 4 * 4]).expect("init");
        let before = vec![wire(0, 0, 4, 4, vec![0; 4 * 4 * 4])];
        let after = vec![wire(0, 0, 4, 4, vec![9; 4 * 4 * 4])];
        apply_tile_patch(doc.clone(), layer.clone(), before, after).expect("patch");

        let (cursor_before, version_before) = cursor_and_version(&doc);

        let first = rust_pixels_history_depth(doc.clone()).expect("first depth read");
        let second = rust_pixels_history_depth(doc.clone()).expect("second depth read");
        assert_eq!(first, second, "two identical read-only calls");
        assert_eq!(first.total_depth, 1, "one committed entry");
        assert_eq!(first.undo_depth, 1, "cursor sits after the one entry");
        assert_eq!(first.redo_depth, 0, "no redo branch");
        assert_eq!(first.affected_layer_ids, vec![layer.clone()]);

        let (cursor_after, version_after) = cursor_and_version(&doc);
        assert_eq!(cursor_before, cursor_after, "cursor must not advance");
        assert_eq!(version_before, version_after, "version must not bump");

        // Undo is a mutation by the UNDO command, not by the probe: depth then
        // reports the redo branch without having touched anything itself.
        rust_pixels_undo(doc.clone(), layer.clone()).expect("undo");
        let after_undo = rust_pixels_history_depth(doc.clone()).expect("depth after undo");
        assert_eq!(after_undo.undo_depth, 0, "cursor popped back");
        assert_eq!(after_undo.redo_depth, 1, "entry moved to the redo branch");
        assert_eq!(after_undo.total_depth, 1, "stream length unchanged");
    }

    #[test]
    fn invalid_doc_ids_reject_with_a_bare_string() {
        let _g = TEST_REGISTRY_LOCK.lock().unwrap();

        *registry() = None;

        assert!(
            rust_pixels_history_depth(String::new()).is_err(),
            "empty doc_id"
        );
        assert!(
            rust_pixels_history_depth("   \t\n".to_string()).is_err(),
            "whitespace-only doc_id"
        );
        assert!(
            rust_pixels_history_depth("no-such-document".to_string()).is_err(),
            "unknown doc_id"
        );
        assert!(
            rust_pixels_history_depth("x".repeat(MAX_DOC_ID_BYTES + 1)).is_err(),
            "oversize doc_id"
        );

        // Valid length, store never initialized -> also an Err, never Ok(zero).
        assert!(rust_pixels_history_depth("d".to_string()).is_err());
    }

    #[test]
    fn tip_probe_is_read_only_and_repeatable_across_a_mixed_stream() {
        let _g = TEST_REGISTRY_LOCK.lock().unwrap();
        *registry() = None;

        let doc = "tip-doc-1".to_string();
        let layer = "tip-layer-1".to_string();
        rust_pixels_open_document(doc.clone());
        rust_pixels_init(doc.clone(), layer.clone(), 4, 4, vec![0; 4 * 4 * 4]).expect("init");
        let before = vec![wire(0, 0, 4, 4, vec![0; 4 * 4 * 4])];
        let after = vec![wire(0, 0, 4, 4, vec![9; 4 * 4 * 4])];
        // A pixel step (apply_tile_patch -> Pixel entry) then a host metadata step
        // (record_external -> External entry): the mixed stream is the case this
        // probe exists for, because the two directions can sit on different kinds.
        apply_tile_patch(doc.clone(), layer.clone(), before, after).expect("patch");
        rust_pixels_record_external(
            doc.clone(),
            "Add Layer".to_string(),
            vec![layer.clone()],
            "ts".to_string(),
            "token-1".to_string(),
            None,
            None,
            None,
        )
        .expect("record external");

        let (cursor_before, version_before) = cursor_and_version(&doc);

        let first = rust_pixels_history_tip(doc.clone()).expect("first tip read");
        let second = rust_pixels_history_tip(doc.clone()).expect("second tip read");
        assert_eq!(first, second, "two identical read-only calls");
        assert_eq!(first.total_depth, 2, "both entries recorded");
        assert_eq!(first.undo_depth, 2, "cursor sits after both entries");
        assert_eq!(first.redo_depth, 0, "no redo branch");
        assert_eq!(
            first.undo_tip_kind,
            Some(PayloadKind::External),
            "the next undo consumes the metadata entry"
        );
        assert_eq!(
            first.redo_tip_kind, None,
            "nothing left to redo at the stream end"
        );

        let (cursor_after, version_after) = cursor_and_version(&doc);
        assert_eq!(cursor_before, cursor_after, "cursor must not advance");
        assert_eq!(version_before, version_after, "version must not bump");

        // The UNDO command is what moves the cursor, not the probe: afterwards the
        // two directions sit on different entry kinds and both are reported.
        rust_pixels_undo(doc.clone(), layer.clone()).expect("undo");
        let after_undo = rust_pixels_history_tip(doc.clone()).expect("tip after undo");
        assert_eq!(after_undo.undo_depth, 1, "cursor popped back one entry");
        assert_eq!(after_undo.redo_depth, 1, "the undone entry sits ahead");
        assert_eq!(
            after_undo.undo_tip_kind,
            Some(PayloadKind::Pixel),
            "the next undo now consumes the pixel entry"
        );
        assert_eq!(
            after_undo.redo_tip_kind,
            Some(PayloadKind::External),
            "the next redo consumes the metadata entry"
        );
        assert_eq!(
            after_undo.total_depth, 2,
            "the probe's own read never changes the stream"
        );
    }

    /// C7: a `rust_pixels_undo` that lands on an `External` tip CONSUMES that
    /// entry - it moves the cursor and bumps the version - while returning an
    /// empty tile yield, and the command reports success. Measured here against
    /// the real registry and the real commands, because the consequence is
    /// invisible to the caller: `PixelStoreRegistry::undo_pixel` collapses the
    /// empty `(None, None, None)` to `None` AFTER the cursor has already moved
    /// (pixel_store.rs `_ => None`), and `rust_pixels_undo`'s `None` arm returns
    /// `Ok` with empty tiles (paint_parity_cmds.rs).
    ///
    /// The cursor state below is exactly what a metadata undo leaves behind. The
    /// host pops its own entry for that step and the Rust cursor stays at 2
    /// (useEditorCommands.ts keeps the cursor sync inside the tile branch, so the
    /// snapshot restore path never calls `rust_pixels_undo`), so the NEXT
    /// tile-path undo arrives with an `External` tip while the host believes it
    /// is undoing the paint step.
    #[test]
    fn undo_over_an_external_tip_consumes_it_and_returns_no_tiles() {
        let _g = TEST_REGISTRY_LOCK.lock().unwrap();
        *registry() = None;

        let doc = "c7-doc".to_string();
        let layer = "c7-layer".to_string();
        rust_pixels_open_document(doc.clone());
        rust_pixels_init(doc.clone(), layer.clone(), 4, 4, vec![0; 4 * 4 * 4]).expect("init");

        // Step 1: the canonical writer, which is NOT bridge-gated - it appends a
        // `Pixel` entry and advances the cursor in production on every stroke.
        let written =
            rust_pixels_write_region(doc.clone(), layer.clone(), 0, 0, 4, 4, vec![7; 4 * 4 * 4])
                .expect("write_region");
        assert!(
            !written.after.is_empty(),
            "the writer changed pixels, so the Pixel entry is real"
        );

        // Step 2: a host metadata step appends an `External` entry.
        rust_pixels_record_external(
            doc.clone(),
            "Add Layer".to_string(),
            vec![layer.clone()],
            "ts".to_string(),
            "token-1".to_string(),
            None,
            None,
            None,
        )
        .expect("record external");

        let before = rust_pixels_history_tip(doc.clone()).expect("tip at the stream end");
        assert_eq!(before.total_depth, 2, "one Pixel entry, one External entry");
        assert_eq!(before.undo_depth, 2);
        assert_eq!(before.redo_depth, 0);
        assert_eq!(before.undo_tip_kind, Some(PayloadKind::External));
        assert_eq!(before.redo_tip_kind, None);
        let (_, version_before) = cursor_and_version(&doc);

        // Step 3: the next tile-path undo, arriving with that External tip.
        let res = rust_pixels_undo(doc.clone(), layer.clone()).expect("undo succeeds");

        let after = rust_pixels_history_tip(doc.clone()).expect("tip after the undo");
        assert!(
            res.tiles.is_empty(),
            "an External tip yields no tiles (history.rs:229-233 returns (None, None, None))"
        );
        assert_eq!(
            after.undo_depth, 1,
            "the cursor moved anyway - the External entry was consumed"
        );
        assert_eq!(after.redo_depth, 1);
        assert_eq!(
            after.total_depth, 2,
            "consumed, neither created nor destroyed"
        );
        assert_eq!(
            after.undo_tip_kind,
            Some(PayloadKind::Pixel),
            "the still-unundone paint step is now the undo tip"
        );
        assert_eq!(
            after.redo_tip_kind,
            Some(PayloadKind::External),
            "the consumed External entry sits in the redo branch"
        );
        let (_, version_after) = cursor_and_version(&doc);
        assert!(
            version_after > version_before,
            "the consumed entry also bumped the document version ({version_before:?} -> {version_after:?})"
        );

        // Control: the same command, the same layer, the same writer - only
        // without the External step, so the tip is a `Pixel` entry. It DOES
        // return tiles. Without this the empty yield above could be an artefact
        // of an all-zero layer rather than the External arm, and the assertion
        // would be unfalsifiable.
        let ctrl = "c7-control".to_string();
        rust_pixels_open_document(ctrl.clone());
        rust_pixels_init(ctrl.clone(), layer.clone(), 4, 4, vec![0; 4 * 4 * 4]).expect("init");
        rust_pixels_write_region(ctrl.clone(), layer.clone(), 0, 0, 4, 4, vec![7; 4 * 4 * 4])
            .expect("control write_region");
        let ctrl_tip = rust_pixels_history_tip(ctrl.clone()).expect("control tip");
        assert_eq!(ctrl_tip.undo_tip_kind, Some(PayloadKind::Pixel));
        let ctrl_res = rust_pixels_undo(ctrl.clone(), layer.clone()).expect("control undo");
        assert!(
            !ctrl_res.tiles.is_empty(),
            "a Pixel tip DOES yield tiles, so the empty yield above is the External arm"
        );
    }

    #[test]
    fn tip_probe_rejects_invalid_doc_ids_and_an_empty_cursor_reads_none() {
        let _g = TEST_REGISTRY_LOCK.lock().unwrap();
        *registry() = None;

        for bad in [
            String::new(),
            "   \t\n".to_string(),
            "no-such-document".to_string(),
        ] {
            assert!(
                rust_pixels_history_tip(bad.clone()).is_err(),
                "doc_id {bad:?} must reject rather than resolve a zero cursor"
            );
        }
        assert!(rust_pixels_history_tip("x".repeat(MAX_DOC_ID_BYTES + 1)).is_err());
        assert!(rust_pixels_history_tip("d".to_string()).is_err());

        // An OPEN document with no entries is a real cursor position, not an
        // error: depth 0 with no kind on either side is the honest answer.
        rust_pixels_open_document("tip-empty".to_string());
        let empty = rust_pixels_history_tip("tip-empty".to_string()).expect("open doc reads");
        assert_eq!(empty.total_depth, 0);
        assert_eq!(empty.undo_depth, 0);
        assert_eq!(empty.redo_depth, 0);
        assert_eq!(empty.undo_tip_kind, None);
        assert_eq!(empty.redo_tip_kind, None);
    }

    /// THE VERSION INVARIANT, executed rather than read off the source.
    ///
    /// `RustCursorStepper` (apps/desktop/src/engine/historyCursorStep.ts) infers "the
    /// Rust cursor step did not move the cursor" from the command `version` NOT
    /// advancing. That inference is sound only if `undo_pixel` / `redo_pixel` bump
    /// `version` EXACTLY when they move the cursor - never otherwise. The source says
    /// so (history.rs:220-223, :230-233, :258-261, :265-268), but a diagnostic that
    /// silently cannot fire is worse than none, so it is pinned against the real
    /// `PixelStoreRegistry` through the real commands here.
    ///
    /// Each case asserts the cursor and the version TOGETHER, so the two facts stay
    /// distinguishable rather than merely correlated, and the closing control compares
    /// the two DELTAS: a test that only asserted "a move increments" could not tell a
    /// real no-op from a no-op that started from a different base.
    #[test]
    fn cursor_version_advances_exactly_when_the_cursor_moves() {
        let _g = TEST_REGISTRY_LOCK.lock().unwrap();
        *registry() = None;

        // ---- CASE 1: undo over a PIXEL tip: cursor and version BOTH move ----
        let doc = "ver-pixel".to_string();
        let layer = "lp".to_string();
        rust_pixels_open_document(doc.clone());
        rust_pixels_init(doc.clone(), layer.clone(), 4, 4, vec![0; 4 * 4 * 4]).expect("init");
        apply_tile_patch(
            doc.clone(),
            layer.clone(),
            vec![wire(0, 0, 4, 4, vec![0; 4 * 4 * 4])],
            vec![wire(0, 0, 4, 4, vec![9; 4 * 4 * 4])],
        )
        .expect("patch");
        let (px_c0, px_v0) = (cursor(&doc), version(&doc));
        let px_undo = rust_pixels_undo(doc.clone(), layer.clone()).expect("undo a pixel tip");
        let (px_c1, px_v1) = (cursor(&doc), version(&doc));
        assert_eq!(px_c1, px_c0 - 1, "a Pixel tip: the cursor moves back one");
        assert_eq!(px_v1, px_v0 + 1, "a Pixel tip: version STRICTLY increments");
        assert_eq!(
            px_undo.version, px_v1,
            "the command reports the post-move version, which is what the host reads"
        );
        assert!(!px_undo.tiles.is_empty(), "a Pixel tip yields its tiles");

        // ---- redo mirrors undo, on the same document and the same invariant ----
        let px_redo = rust_pixels_redo(doc.clone(), layer.clone()).expect("redo");
        let (px_c2, px_v2) = (cursor(&doc), version(&doc));
        assert_eq!(px_c2, px_c1 + 1, "redo over a Pixel tip moves the cursor");
        assert_eq!(px_v2, px_v1 + 1, "redo STRICTLY increments version too");
        assert_eq!(px_redo.version, px_v2);

        // ---- CASE 2: undo at CURSOR 0: neither moves ----
        while cursor(&doc) > 0 {
            rust_pixels_undo(doc.clone(), layer.clone()).expect("walk to cursor 0");
        }
        let (z_c0, z_v0) = (cursor(&doc), version(&doc));
        let z_undo = rust_pixels_undo(doc.clone(), layer.clone()).expect("undo at cursor 0 is Ok");
        let (z_c1, z_v1) = (cursor(&doc), version(&doc));
        assert!(z_undo.tiles.is_empty(), "nothing to undo yields no tiles");
        assert_eq!(z_c1, z_c0, "cursor 0: the cursor does NOT move");
        assert_eq!(z_v1, z_v0, "cursor 0: version is UNCHANGED");

        // ---- CASE 3: undo over an EXTERNAL tip: the cursor moves, tiles do not ----
        // The case that made the original defect silent: an External step moves the
        // cursor and yields nothing, so the tile yield alone could never detect it.
        let ext = "ver-external".to_string();
        rust_pixels_open_document(ext.clone());
        rust_pixels_record_external(
            ext.clone(),
            "Add Layer".to_string(),
            vec![],
            "ts".to_string(),
            "Add Layer".to_string(),
            None,
            None,
            None,
        )
        .expect("record an External entry");
        let (e_c0, e_v0) = (cursor(&ext), version(&ext));
        let e_undo =
            rust_pixels_undo(ext.clone(), "no-such-layer".to_string()).expect("undo an External");
        let (e_c1, e_v1) = (cursor(&ext), version(&ext));
        assert!(
            e_undo.tiles.is_empty(),
            "an External step yields no tiles - the tile yield cannot detect the move"
        );
        assert_eq!(e_c1, e_c0 - 1, "an External tip: the cursor DOES move");
        assert_eq!(
            e_v1,
            e_v0 + 1,
            "an External tip: version STRICTLY increments - the only observable"
        );

        // ---- CASE 4: undo over a SNAPSHOT tip: neither moves ----
        let snap = "ver-snapshot".to_string();
        rust_pixels_open_document(snap.clone());
        let dto = DocumentSnapshotDto {
            doc_id: snap.clone(),
            version: 0,
            layers: vec![],
        };
        rust_pixels_record_snapshot(snap.clone(), dto.clone(), dto).expect("record a Snapshot");
        let (s_c0, s_v0) = (cursor(&snap), version(&snap));
        let s_undo = rust_pixels_undo(snap.clone(), "no-such-layer".to_string())
            .expect("undo over a Snapshot tip is Ok");
        let (s_c1, s_v1) = (cursor(&snap), version(&snap));
        assert!(s_undo.tiles.is_empty(), "a Snapshot tip yields no tiles");
        assert_eq!(s_c1, s_c0, "a Snapshot tip: the cursor does NOT move");
        assert_eq!(s_v1, s_v0, "a Snapshot tip: version is UNCHANGED");

        // ---- CONTROL: the two outcomes are DISTINGUISHABLE, not correlated ----
        // Deltas, not absolutes: the four documents start from different bases, so
        // only the change is comparable.
        let moved: u64 = e_v1 - e_v0;
        let not_moved: u64 = s_v1 - s_v0;
        assert_eq!(moved, 1, "a real move advances version by exactly one");
        assert_eq!(not_moved, 0, "a real no-op advances it by none");
        assert_ne!(
            moved,
            not_moved,
            "the host separates the two ONLY because these differ; equal deltas would make the did-not-move diagnostic unable to ever fire"
        );
        assert_eq!(
            px_v1 - px_v0,
            moved,
            "a Pixel move and an External move are indistinguishable by version alone, which is fine: both moved"
        );
    }
}
