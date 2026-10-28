// SPDX-License-Identifier: AGPL-3.0-or-later
// History stream payload types: the per-entry payload and the entry record.
// The origin and the payload-kind discriminant live in `wire::history` (they
// are wire types, and the wire layer must not import the document engine).
// The cursor methods that build and walk these live in the parent
// `document::history` module.

use crate::document::layer_set::LayerSet;
use crate::document::snapshot::DocumentSnapshot;
use crate::pixel::state_node::StateNode;
use crate::wire::command::DocumentVersion;
use crate::wire::history::{Origin, PayloadKind};
use std::sync::Arc;

// `EntryPayload`/`HistoryEntry` intentionally do NOT derive Clone.
// `Pixel` payloads hold `Arc<StateNode>` (sharing is via `Arc`, never a value
// clone). They DO derive Debug (DocumentCore derives Debug, which requires it).
#[derive(Debug)]
pub(crate) enum EntryPayload {
    // Built-in native adapter: before/after metadata snapshots. Stored as
    // structural-sharing `LayerSet`s (Arc<Vec<Arc<LayerMeta>>>) so unchanged
    // layers SHARE pointers and undo/redo is an O(1) Arc swap (mirrors the
    // `Pixel` path's `Arc<StateNode>` COW).
    Native {
        before: LayerSet,
        after: LayerSet,
        // Document-size (canvas width/height) undo state for the Crop Canvas /
        // Apply Crop / Resize Canvas arms. The engine keeps `doc_size` as separate
        // state from the LayerSet, so a plain LayerSet swap does NOT restore it;
        // these capture the pre/post document size so undo/redo of a canvas arm
        // restores both the layer transforms AND the document dimensions. Core-
        // local (no serde) — never crosses the wire. All other arms leave both
        // `begin_forward` captures the then-current `doc_size` for EVERY `Native`
        // entry (not just the canvas arms), and the apply() walker unconditionally
        // writes it back on undo/redo. Non-canvas arms never mutate `doc_size`, so
        // restoring the captured (unchanged) value is a harmless no-op rather than
        // a special case.
        doc_size_before: Option<(f64, f64)>,
        doc_size_after: Option<(f64, f64)>,
    },
    /// TS (adapter) logical transition recorded as a host handoff. `token` is
    /// the opaque host payload reference. `before` is the engine's layer set as
    /// it stood when the transition was recorded - i.e. BEFORE the mirrored TS
    /// commit was up-projected into the engine, so it is exactly the state this
    /// entry undoes TO. It is an `Arc` clone of the structural-sharing
    /// `LayerSet` (O(1), no deep layer copy). `after` is the up-projected layer
    /// set, captured when the mirrored commit reaches the engine
    /// (`seed_ptz`); it is `None` until then and for handoffs that never
    /// pass through a canonical push.
    ///
    /// `doc_size_before` / `doc_size_after` are the document WIDTH/HEIGHT halves
    /// of the host's transition, mirroring the `Native` payload's pair above, and
    /// they are HOST-OWNED: the caller supplies both together at record time.
    ///
    /// Host ownership is load-bearing, not incidental. A host crop changes the
    /// document size without passing through a canvas command arm (the
    /// pixel-baking variants are refused by the routed native arm), so no engine
    /// command ever runs for it. The engine's own `doc_size` is baseline-only -
    /// `seed_ptz` sets it once, when it is `None` - so after the first crop
    /// it holds a STALE value. Deriving either half from that field therefore
    /// mis-describes every crop after the first: a second crop would capture the
    /// pre-first-crop size, and undoing it would resize the document to a size the
    /// user was never in. The host is the only party that knows both halves (it
    /// commits before it mutates, and computes the post size from its own
    /// parameters), so it supplies both and the engine never guesses.
    ///
    /// BOTH are `None` for a transition that changes no size (a layer delete, a
    /// move, a reorder) and `None` is a complete answer, not a pending state: the
    /// undo arm emits no size delta at all then, which keeps such a step a
    /// metadata no-op for the host.
    External {
        token: String,
        before: LayerSet,
        after: Option<LayerSet>,
        doc_size_before: Option<(f64, f64)>,
        doc_size_after: Option<(f64, f64)>,
        /// Ids THIS entry minted, supplied by the host that performed the op.
        ///
        /// The `after` slot is filled from a whole-document push, so "present only in
        /// `after`" cannot tell an id this entry created from a layer the host pushed
        /// outside it. A merge or flatten destination is the first case, and only the
        /// host knows it because the host minted the id. Recorded here so the undo
        /// REMOVES those ids instead of adopting them as survivors, without changing
        /// the survivor rule for ids nothing claims.
        ///
        /// Defaults to empty: an entry that recorded no mints behaves exactly as before.
        minted_layer_ids: Vec<String>,
    },
    /// Native pixel-history entry. Carries before/after pixel
    /// states as immutable, byte-free `Arc<StateNode>`s (COW tile blocks
    /// shared across states). The metadata-level `DocumentCore` is reused as
    /// the SINGLE authoritative history cursor. Not serialized — the
    /// `HistoryEntryView` exposes only a `payload_ref` token. On undo/redo the
    /// stored after/before `Arc<StateNode>` is materialized back into the
    /// tile-patch form the TS surface expects.
    Pixel {
        layer_id: String,
        before: Arc<StateNode>,
        after: Arc<StateNode>,
    },
    /// Atomic metadata+pixel snapshot entry. Carries the document metadata
    /// snapshot with per-layer opaque `bitmap_token` references. Rust stores NO
    /// ImageBitmap and NO pixel bytes here — TS owns the ImageBitmap and restores
    /// it by the token on undo/redo, so ONE entry restores BOTH metadata and
    /// pixel state atomically. Matches the `Pixel` pattern: BOTH `before` (the
    /// state being left, returned by `undo_snapshot`) and `after` (the new
    /// state, returned by `redo_snapshot`) are stored so each direction
    /// restores the CORRECT snapshot (undo != redo). Introduced flag-OFF
    /// (row-major + TS `CommandHistory` remain the ACTIVE default).
    Snapshot {
        before: DocumentSnapshot,
        after: DocumentSnapshot,
    },
}
#[derive(Debug)]
pub(crate) struct HistoryEntry {
    pub seq: u64,
    pub group_id: u64,
    pub origin: Origin,
    pub label: String,
    pub affected_layer_ids: Vec<String>,
    pub version_before: DocumentVersion,
    pub version_after: DocumentVersion,
    pub memory_cost_bytes: u64,
    pub payload: EntryPayload,
}
impl EntryPayload {
    /// The logical kind of this payload (used for unambiguous dispatch routing).
    pub fn kind(&self) -> PayloadKind {
        match self {
            EntryPayload::Native { .. } => PayloadKind::Metadata,
            EntryPayload::Pixel { .. } => PayloadKind::Pixel,
            EntryPayload::Snapshot { .. } => PayloadKind::Snapshot,
            EntryPayload::External { .. } => PayloadKind::External,
        }
    }
}
