// SPDX-License-Identifier: AGPL-3.0-or-later
//! Read-only depth and tip reports for a document's unified pixel history.

// The typed payload discriminant of a history entry, re-exported so a
// read-only probe can name the kind of step the cursor points at without
// touching the stream. `history` itself is crate-private; `pixel` is not,
// and this is the field type on `HistoryTip`.
pub use crate::wire::history::PayloadKind;

/// Read-only depth report for one document's unified history stream, as returned
/// by `PixelStoreRegistry::get_history_depth`. Carries no pixel bytes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct HistoryDepth {
    /// Stream length (entries recorded, both branches included).
    pub total_depth: usize,
    /// Entries behind the cursor (undoable steps).
    pub undo_depth: usize,
    /// Entries ahead of the cursor (redoable steps).
    pub redo_depth: usize,
    /// Sorted, de-duplicated union of `affected_layer_ids` over the whole stream.
    pub affected_layer_ids: Vec<String>,
}

/// Read-only cursor position for one document's history stream, as returned by
/// `PixelStoreRegistry::get_history_tip`. Carries no pixel bytes and no payload
/// data — only the depth numbers and WHICH kind of entry each direction would
/// consume.
///
/// This exists to answer one question: does the caller's undo bookkeeping point
/// at the same history position as this stream? The two sides keep separate
/// depth counters and nothing in production compares them, so drift between
/// the two is invisible until it is measured.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct HistoryTip {
    /// Stream length (entries recorded, both branches included).
    pub total_depth: usize,
    /// Entries behind the cursor (undoable steps).
    pub undo_depth: usize,
    /// Entries ahead of the cursor (redoable steps).
    pub redo_depth: usize,
    /// Payload kind the next undo would consume (`None` at cursor 0).
    pub undo_tip_kind: Option<PayloadKind>,
    /// Payload kind the next redo would consume (`None` at the stream end).
    pub redo_tip_kind: Option<PayloadKind>,
}
