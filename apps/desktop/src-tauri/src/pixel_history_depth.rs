// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Read-only history-depth probe. Registered as `rust_pixels_history_depth`.
//
// Contract: returns exactly
// { total_depth, undo_depth, redo_depth, affected_layer_ids } for one document.
// The accessor behind it takes `&self` only - no cursor pop, tile write, version
// bump, Arc<StateNode> mutation, or byte return - so two calls on an unchanged
// document are byte-identical. Empty, whitespace-only, unknown and oversize
// doc_id reject with a bare string (Tauri surfaces a Rust Err(String) as a bare
// string); they never resolve to a zero-filled struct.

use photrez_core::pixel_store::{registry, HistoryDepth};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint_parity_cmds::{
        apply_tile_patch, rust_pixels_init, rust_pixels_open_document, rust_pixels_undo,
        TilePatchWire, TEST_REGISTRY_LOCK,
    };

    fn wire(x: i64, y: i64, w: usize, h: usize, data: Vec<u8>) -> TilePatchWire {
        TilePatchWire { x, y, w, h, data }
    }

    fn cursor_and_version(doc: &str) -> (Option<usize>, Option<u64>) {
        let reg = registry();
        let reg = reg.as_ref().expect("registry initialized");
        (reg.get_history_cursor(doc), reg.get_history_version(doc))
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
}
