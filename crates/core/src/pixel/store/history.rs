// SPDX-License-Identifier: AGPL-3.0-or-later
//! History paths on the registry: the cursor, the depth/tip reports, and the
//! external / snapshot entries that share the document's unified cursor.

use super::PixelStoreRegistry;
use crate::pixel::document_store::DocumentSnapshot;
use crate::pixel::history::{HistoryDepth, HistoryTip};
use crate::wire::history::HistoryQuery;

impl PixelStoreRegistry {
    /// Current authoritative history cursor for the document.
    pub fn get_history_cursor(&self, doc_id: &str) -> Option<usize> {
        self.docs.get(doc_id).map(|d| d.history.cursor())
    }

    /// Current `DocumentVersion` for the document.
    pub fn get_history_version(&self, doc_id: &str) -> Option<u64> {
        self.docs.get(doc_id).map(|d| d.history.version())
    }

    /// Read-only view of the document's unified history stream.
    pub fn history_query(&self, doc_id: &str) -> Option<HistoryQuery> {
        self.docs.get(doc_id).map(|d| d.history.history_query())
    }

    /// Rename the history entry at the cursor tip (presentation metadata).
    pub fn set_history_label(&mut self, doc_id: &str, label: &str) -> bool {
        self.docs
            .get_mut(doc_id)
            .is_some_and(|d| d.history.set_history_label(label))
    }

    /// Read-only depth report for the document's unified history stream.
    ///
    /// `total_depth` is the stream length, `undo_depth` the applied entries
    /// behind the cursor, `redo_depth` the entries ahead of it (so
    /// `total_depth == undo_depth + redo_depth`), and `affected_layer_ids` the
    /// sorted, de-duplicated union of every entry's affected layers across the
    /// whole stream (both branches - a layer can be reachable again via redo).
    ///
    /// `&self` only: no cursor pop, tile write, `version()` bump, or
    /// `Arc<StateNode>` mutation, and no pixel bytes are returned, so two calls
    /// on an unchanged document are byte-identical.
    pub fn get_history_depth(&self, doc_id: &str) -> Option<HistoryDepth> {
        let doc = self.docs.get(doc_id)?;
        let history = &doc.history;
        let total_depth = history.entries.len();
        let undo_depth = history.cursor();
        let redo_depth = total_depth - undo_depth.min(total_depth);
        let mut affected_layer_ids: Vec<String> = history
            .entries
            .iter()
            .flat_map(|e| e.affected_layer_ids.iter().cloned())
            .collect();
        affected_layer_ids.sort();
        affected_layer_ids.dedup();
        Some(HistoryDepth {
            total_depth,
            undo_depth,
            redo_depth,
            affected_layer_ids,
        })
    }

    /// Read-only cursor position and per-direction tip kinds for the document's
    /// history stream, for comparing this stream's cursor against the host's own
    /// undo depth.
    ///
    /// `undo_tip_kind` is the kind of the entry the next undo would consume (the
    /// entry just below the cursor, so `None` at cursor 0); `redo_tip_kind` is
    /// the kind the next redo would consume (the entry just above the cursor, so
    /// `None` at the stream end). Both are the same rule
    /// `DocumentCore::tip_payload_kind` applies to the undo direction.
    ///
    /// `&self` only: no cursor pop, tile write, `version()` bump or
    /// `Arc<StateNode>` mutation, and no pixel bytes or payload data are
    /// returned, so two calls on an unchanged document are byte-identical.
    pub fn get_history_tip(&self, doc_id: &str) -> Option<HistoryTip> {
        let doc = self.docs.get(doc_id)?;
        let history = &doc.history;
        let total_depth = history.entries.len();
        let cursor = history.cursor().min(total_depth);
        Some(HistoryTip {
            total_depth,
            undo_depth: cursor,
            redo_depth: total_depth - cursor,
            undo_tip_kind: cursor
                .checked_sub(1)
                .and_then(|idx| history.entries.get(idx))
                .map(|e| e.payload.kind()),
            redo_tip_kind: history.entries.get(cursor).map(|e| e.payload.kind()),
        })
    }

    /// Route a TS (non-pixel) logical mutation into the SAME unified
    /// `DocumentCore` cursor so mixed TS/Rust operations share one history
    /// position. Creates an `External` entry (no pixel delta) that advances the
    /// cursor + bumps `DocumentVersion` exactly once. The actual metadata revert
    /// on undo/redo remains TS-side; this entry only keeps the ordering unified.
    // External-entry contract: the host size halves ship together (see the doc
    // above); allow the 10-arg bag rather than a struct every caller must build.
    #[allow(clippy::too_many_arguments)]
    pub fn record_external(
        &mut self,
        doc_id: &str,
        label: &str,
        affected: &[String],
        adapter_id: &str,
        token: &str,
        memory_cost_bytes: u64,
        doc_size_before: Option<(f64, f64)>,
        doc_size_after: Option<(f64, f64)>,
        minted_layer_ids: &[String],
    ) -> Result<(), String> {
        let doc = self
            .docs
            .get_mut(doc_id)
            .ok_or_else(|| format!("document not open: {doc_id}"))?;
        // TS ops are external to the Rust engine; register the adapter once so
        // record_external's adapter check passes.
        doc.history.register_adapter(adapter_id);
        doc.history
            .record_external(
                label,
                affected,
                adapter_id,
                token,
                memory_cost_bytes,
                doc_size_before,
                doc_size_after,
                minted_layer_ids,
            )
            .map_err(|e| e.message)
    }

    /// Record an atomic metadata+pixel `Snapshot` entry into the document's
    /// unified `DocumentCore` cursor. Mirrors `record_external` (bumps
    /// `DocumentVersion` exactly once, truncates the redo branch) but stores BOTH
    /// the `before` and `after` snapshot so undo restores the `before` and redo
    /// re-applies the `after` — restoring the metadata AND (via the per-layer
    /// opaque `bitmap_token`) the pixel reference from ONE entry.
    ///
    /// Boundary rules: an unopen/unknown document -> `Err`; a `before`/`after`
    /// `doc_id` that mismatches `doc_id` -> `Err` (never mutate the wrong doc);
    /// layers that have no canonical buffer in the store are left untouched
    /// (no-op) — the snapshot's pixel restore happens TS-side via the token, so
    /// Rust does not need a PixelLayer for every referenced layer.
    pub fn record_snapshot(
        &mut self,
        doc_id: &str,
        before: DocumentSnapshot,
        after: DocumentSnapshot,
    ) -> Result<(), String> {
        let doc = self
            .docs
            .get_mut(doc_id)
            .ok_or_else(|| format!("document not open: {doc_id}"))?;
        if before.doc_id != doc_id {
            return Err(format!(
                "snapshot before.doc_id mismatch: {} != {doc_id}",
                before.doc_id
            ));
        }
        if after.doc_id != doc_id {
            return Err(format!(
                "snapshot after.doc_id mismatch: {} != {doc_id}",
                after.doc_id
            ));
        }
        doc.history
            .record_snapshot(before, after)
            .map_err(|e| e.message)
    }

    /// Undo the entry just below the cursor IF it is a `Snapshot` entry, returning
    /// the metadata snapshot (with per-layer bitmap tokens). Non-snapshot entries
    /// return `None` WITHOUT moving the cursor (so the caller dispatches the
    /// actual undo through `undo_pixel`/metadata paths).
    pub fn undo_snapshot(&mut self, doc_id: &str) -> Option<DocumentSnapshot> {
        self.docs
            .get_mut(doc_id)?
            .history
            .undo_snapshot()
            .ok()
            .flatten()
    }

    /// Redo the entry at the cursor IF it is a `Snapshot` entry. Symmetric to
    /// `undo_snapshot`.
    pub fn redo_snapshot(&mut self, doc_id: &str) -> Option<DocumentSnapshot> {
        self.docs
            .get_mut(doc_id)?
            .history
            .redo_snapshot()
            .ok()
            .flatten()
    }
}
