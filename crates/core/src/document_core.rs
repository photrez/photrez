// SPDX-License-Identifier: AGPL-3.0-or-later
// Protocol engine composition root: ProtocolEngine type, the `apply()` dispatch
// entry, per-document engines, and the wasm-bindgen bridge. History/command/
// model/projection types live in their own modules (history.rs / command.rs /
// model.rs / projection.rs).

use crate::canonical_bridge::CanonicalShadow;
use crate::canonical_model::{CanonicalDocument, SelectionState};
use crate::command::*;
use crate::history::*;
use crate::model::*;
use crate::projection::*;
use crate::state_node::StateNode;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use wasm_bindgen::prelude::*;
// ── Engine ───────────────────────────────────────────────────────────────
// HISTORY BOUNDARY: history/future store Vec<RenderLayer> METADATA snapshots only
// (id/name/resourceId/transform/opacity). They do NOT store pixel buffers and must NOT be
// interpreted as pixel history. The underlying `HistoryEntry` schema {command, affectedResources,
// patch/inverse, memoryCost, transaction} is the target; this Vec is transitional and will be
// replaced before pixel ownership moves to Rust. Storing full pixel snapshots at 2048²/4K would be GBs.
#[derive(Debug)]
pub struct ProtocolEngine {
    pub(crate) version: DocumentVersion,
    next_resource: ResourceId,
    // Structural-sharing layer metadata set (Arc<Vec<Arc<LayerMeta>>>). Only the
    // changed layer gets a new Arc; unchanged layers SHARE Arc pointers across
    // history before/after. Undo/redo swaps this Arc in O(1).
    layers: LayerSet,
    // H0 (canonical stream): cursor = number of APPLIED entries
    // (0..=entries.len()). Independent from `version` (see C1).
    pub(crate) entries: Vec<HistoryEntry>,
    pub(crate) cursor: usize,
    pub(crate) next_seq: u64,
    max_depth: usize, // Bound the pixel-only history stream (FIFO eviction of the oldest entry).
    pub(crate) adapters: Vec<String>, // "native" is implicit and always available
    // External-pending barrier (H0 invariant): while Some, the host owes a
    // protocol_history_cursor_commit for (seq, direction). EVERY command
    // through apply() is rejected with E_EXTERNAL_PENDING until then — the
    // host must not stack a pending external transition with new work.
    pub(crate) pending_external: Option<(u64, String)>,
    // Engine-local selection state (selection is core document state). Selection is NOT an
    // undoable transition in the command stream; it rides Model-A snapshots and is
    // reconciled onto the canonical shadow when one is seeded.
    pub(crate) selection: Option<SelectionState>,
    // Engine document size (canvas width/height). The native ProtocolEngine keeps
    // its own document dims (the TS oracle reads model.width/height directly; the
    // structural arms instead read the seeded canonical shadow). `None` until a
    // canvas-size command (Crop Canvas / Apply Crop / Resize Canvas) sets it or a
    // canonical shadow is seeded. Unlike the LayerSet, this is undoable state for
    // the canvas arms, so each such entry snapshots it (see history.rs).
    pub(crate) doc_size: Option<(f64, f64)>,
    // Per-document canonical shadow, kept in sync with layer edits via
    // reconciliation (see `CanonicalShadow`). `None` until seeded; wasm engines
    // are never seeded and the native-authority path is OFF by default, so
    // production stays byte-identical. `incomplete` flags engine-minted layers
    // whose canonical-only fields cannot be reconstructed until the next push.
    pub(crate) canonical: Option<CanonicalShadow>,
}
impl Default for ProtocolEngine {
    fn default() -> Self {
        Self {
            version: 0,
            next_resource: 1,
            layers: LayerSet::empty(),
            entries: Vec::new(),
            cursor: 0,
            next_seq: 1,
            max_depth: 50,
            adapters: Vec::new(),
            pending_external: None,
            canonical: None,
            selection: None,
            doc_size: None,
        }
    }
}
impl ProtocolEngine {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn version(&self) -> DocumentVersion {
        self.version
    }
    pub fn snapshot(&self) -> RenderSnapshot {
        RenderSnapshot {
            version: self.version,
            layers: self.layers.iter().map(|a| a.as_ref().clone()).collect(),
            selection: self.selection.clone(),
            width: self.doc_size.map(|(w, _)| w),
            height: self.doc_size.map(|(_, h)| h),
        }
    }

    /// Read the engine document size, if a canvas-size command (or a canonical
    /// shadow seed) has set it.
    pub fn doc_size(&self) -> Option<(f64, f64)> {
        self.doc_size
    }

    /// Read the current engine selection state, if any.
    pub fn selection(&self) -> Option<&SelectionState> {
        self.selection.as_ref()
    }

    /// Seed the native document engine with an initial layer load.
    ///
    /// After the document is opened but before the first real command, the
    /// authoritative native engine starts with an EMPTY layer set and `version =
    /// 0`. This method loads an existing document's layers (so the engine becomes
    /// the canonical authority whose layer ids match the TS model's ids, letting
    /// later facade commands and TS bitmaps line up) and aligns `version` to the
    /// supplied value so the TS client's `expectedVersion` matches on the first
    /// real command.
    ///
    /// Id-preserved: the supplied `layers` keep their EXACT ids (no uuid minting —
    /// that happens only in the `AddLayer` command arm). Version-aligned: `version`
    /// is set to `version`. Silent: seeding is initialization, NOT a user edit, so
    /// it creates NO history entry and no undo step (mirrors the facade's
    /// snapshot-seed semantics, which bypass the monotonic guard rather than
    /// being a transition). Only-when-empty + idempotent: if the engine already
    /// has layers, the call is a silent no-op and never clobbers live state.
    pub fn seed_layers(&mut self, layers: Vec<RenderLayer>, version: DocumentVersion) {
        // Only-when-empty guard: never overwrite a populated engine. Seeding twice
        // yields the same result, and seeding onto live state is refused.
        if !self.layers.0.is_empty() {
            return;
        }
        self.layers = LayerSet::from_layers(layers);
        self.version = version;
        // Keep the engine-owned resource counter ahead of any resource ids the
        // seeded layers carried, so a later AddLayer never collides with them.
        if let Some(max_res) = self.layers.0.iter().map(|l| l.resource_id).max() {
            self.next_resource = self.next_resource.max(max_res.saturating_add(1));
        }
        // Keep a seeded canonical shadow consistent with the layer set (ordering
        // hazard if canonical was seeded before the layers). Zero-cost when no
        // shadow is seeded; idempotent.
        self.reconcile_shadow();
    }

    /// Store a complete `CanonicalDocument` copy and begin reconciling layer
    /// edits against it. A full push CLEARS tombstones and the `incomplete` flag,
    /// replacing the entire shadow (a re-open must refresh, not keep stale data).
    ///
    /// ADDITIVE / UNWIRED: only populated from the gated native-authority seed
    /// path. In production `canonical` is `None`, so no reconcile ever runs.
    ///
    /// Seed the canonical shadow copy for a document.
    ///
    /// This is the TS re-push path. The shadow dims are what the SelectAll /
    /// Invert arms read (apply.rs:529-530); the snapshot width/height has no
    /// production consumer today, and any stale-shadow dims heal on the next
    /// re-push, so the shadow is the ONLY thing a re-push is allowed to touch.
    ///
    /// `doc_size` has exactly three owners, all native:
    ///   1. the initial baseline set here, ONCE, only when the engine has no dims
    ///      yet (document open; the native engine is evicted on close so a reopen
    ///      starts at `doc_size: None` again - pixel_store.rs:332 removes the
    ///      `DocumentPixelStore` that owns the `ProtocolEngine`);
    ///   2. the canvas arms (crop at document_core_canvas.rs:90, apply-crop at
    ///      :207, resize at :246) which set `doc_size` AND capture its before /
    ///      after pair via `begin_forward`;
    ///   3. the undo/redo walker (apply.rs:717 / :785) which restores the
    ///      captured pair.
    ///
    /// Because only native arms ever produce a `doc_size` pair, the walker always
    /// restores a value consistent with the native dims timeline by construction.
    ///
    /// A re-push therefore NEVER mutates `doc_size` and NEVER invalidates history:
    /// order divergence between the shadow (TS order) and native is a non-event
    /// (SelectAll / Invert read the shadow only), and any stale shadow dims heal
    /// via a later re-push. There is intentionally NO truncation / cursor reset
    /// here - that was the old heuristic that wiped native history on a mere order
    /// mismatch, and it is removed. Single-owner contract pinned by
    /// `repush_never_mutates_native_doc_size` and `audit_sequence_repush_is_inert`
    /// in document_core_canonical_seed_tests.rs.
    pub fn seed_canonical(&mut self, doc: CanonicalDocument) {
        let pushed_dims = (doc.width, doc.height);
        // Owner (1): baseline only at document open (engine has no dims yet).
        if self.doc_size.is_none() {
            self.doc_size = Some(pushed_dims);
        }
        // Owner shadow: unconditional refresh so SelectAll / Invert read fresh dims.
        self.canonical = Some(CanonicalShadow::new(doc));
    }

    /// Read the seeded canonical document copy, if one has been stored.
    pub fn canonical(&self) -> Option<&CanonicalDocument> {
        self.canonical.as_ref().map(|s| &s.doc)
    }

    /// Whether the shadow is missing layers the engine has minted and cannot yet
    /// reconstruct (engine-side `AddLayer`). Sticky until the next full push.
    /// Test-only today; a future TS-side consumer will read it.
    #[cfg(test)]
    pub(crate) fn canonical_incomplete(&self) -> bool {
        self.canonical
            .as_ref()
            .map(|s| s.incomplete)
            .unwrap_or(false)
    }

    /// Reconcile the shadow against the current engine layer set by reference
    /// (no per-layer clone). Zero-cost when no shadow is seeded.
    pub(crate) fn reconcile_shadow(&mut self) {
        if let Some(shadow) = &mut self.canonical {
            shadow.reconcile(self.layers.iter().map(|a| a.as_ref()));
        }
    }

    /// Mutable access to the shadow for tests that drive `reconcile` directly.
    #[cfg(test)]
    pub(crate) fn canonical_shadow_mut(&mut self) -> Option<&mut CanonicalShadow> {
        self.canonical.as_mut()
    }

    // ── H0 stream helpers ────────────────────────────────────────
    /// Rough per-`Arc<LayerMeta>` heap-allocation slack folded into the cheap
    /// byte estimate (the Arc control block + allocator rounding). Kept as a
    /// named const so the magic number is documented rather than guessed at.
    const PER_LAYER_SLACK_BYTES: u64 = 64;

    /// Cheap per-layer byte estimate for memory-cost accounting. Replaces the
    /// old `estimate_layers_bytes` (which ran `serde_json::to_string` on the
    /// WHOLE layer set twice per forward command, i.e. O(total bytes) +
    /// allocation). This is O(1) per layer and never allocates/serializes.
    pub(crate) fn estimate_layer_meta_bytes(layer: &LayerMeta) -> u64 {
        // Fixed struct size (id/name Strings, resource id, bool, 6 f64s,
        // Option<Rect>) + the actual string byte lengths + per-arc slack. It
        // under-counts the exact RSS but is a stable, cheap, allocation-free
        // upper-ish bound for the history memory model.
        (std::mem::size_of::<LayerMeta>() as u64)
            + (layer.id.len() as u64)
            + (layer.name.len() as u64)
            + Self::PER_LAYER_SLACK_BYTES
    }

    /// Estimate the ACTUAL retained byte cost of a native history entry.
    ///
    /// `before`/`after` are structural-sharing `LayerSet`s whose unchanged
    /// layers are the SAME `Arc<LayerMeta>` allocation. Summing the two per-set
    /// estimates (`cost_before + cost_after`) would DOUBLE-COUNT those shared
    /// layers and therefore OVERSTATE the retained memory. Instead we count
    /// each UNIQUE `Arc<LayerMeta>` allocation (by pointer identity) exactly
    /// once, then add the two `LayerSet` structures (Arc wrapper + backing Vec
    /// pointer slot) that are genuinely separate per entry. This is a cheap
    /// O(n) approximation of the retained metadata — NOT a full RSS measurement
    /// and NOT a pretend-to-be-exact "retained bytes" figure.
    fn estimate_native_entry_cost(before: &LayerSet, after: &LayerSet) -> u64 {
        let mut seen: HashSet<usize> = HashSet::with_capacity(after.0.len());
        let mut layer_bytes: u64 = 0;
        let mut unique_count: u64 = 0;
        for set in [before, after] {
            for arc in set.iter() {
                // A shared (unchanged) layer appears in BOTH sets as the same
                // Arc pointer, so the pointer-identity insert counts it once.
                if seen.insert(Arc::as_ptr(arc) as usize) {
                    unique_count += 1;
                    layer_bytes += Self::estimate_layer_meta_bytes(arc.as_ref());
                }
            }
        }
        // Each unique Arc pointer also occupies a slot in a set's backing Vec
        // (up to 2 across before/after) plus the two Arc<Vec> wrappers.
        let ptr_slots = unique_count * 2 * (std::mem::size_of::<Arc<LayerMeta>>() as u64);
        let wrappers = 2 * (std::mem::size_of::<Arc<Vec<Arc<LayerMeta>>>>() as u64);
        layer_bytes + ptr_slots + wrappers
    }

    // Opens a forward native entry at the cursor. Truncates the forward
    // region (invariant: new command truncates redo). Returns the entry index.
    fn begin_forward(&mut self, label: &str, affected: &[String]) -> usize {
        self.entries.truncate(self.cursor);
        let seq = self.next_seq;
        self.next_seq += 1;
        let entry = HistoryEntry {
            seq,
            group_id: seq, // H0: singleton groups only
            origin: Origin::Native,
            label: label.to_string(),
            affected_layer_ids: affected.to_vec(),
            version_before: self.version,
            version_after: self.version + 1,
            memory_cost_bytes: 0, // filled by finish_forward
            payload: EntryPayload::Native {
                // O(1): `LayerSet` is an `Arc`, so this shares the pre-change
                // set (with its per-layer Arc pointers) instead of cloning it.
                before: self.layers.clone(),
                after: LayerSet::empty(),
                // Document size as observed BEFORE the forward command mutates it.
                // Canvas arms set `self.doc_size` after this, so `doc_size_after`
                // is filled in by finish_forward.
                doc_size_before: self.doc_size,
                doc_size_after: None,
            },
        };
        self.entries.push(entry);
        self.entries.len() - 1
    }

    fn finish_forward(&mut self, idx: usize) {
        // Unique-allocation cost, NOT `before + after` (which double-counts the
        // shared/unchanged layer Arc pointers across the two LayerSets).
        let cost = match self.entries.get(idx) {
            Some(e) => match &e.payload {
                EntryPayload::Native { before, .. } => {
                    Self::estimate_native_entry_cost(before, &self.layers)
                }
                _ => 0,
            },
            None => 0,
        };
        if let Some(e) = self.entries.get_mut(idx) {
            e.memory_cost_bytes = cost;
            if let EntryPayload::Native {
                after,
                doc_size_after,
                ..
            } = &mut e.payload
            {
                // O(1): shares the post-change set (Arc bump), no layer clone.
                *after = self.layers.clone();
                // Document size as observed AFTER the forward command mutates it.
                *doc_size_after = self.doc_size;
            }
        }
        self.cursor = self.entries.len();
    }

    // ── Native pixel history (unified cursor) ──────────────────────
    // These reuse the SAME `entries`/`cursor`/`next_seq`/`version` as the
    // metadata path, so the `ProtocolEngine` is the single authoritative
    // history for a document. The change is stored as immutable
    // `Arc<StateNode>` (before/after) — NOT tile deltas — so undo/redo can
    // reconstruct the full state cheaply from the shared tile blocks. The
    // legacy per-layer `undo_stack`/`redo_stack` were removed; the
    // only pixel history is now this stream, bounded to `max_depth`.
    pub fn apply_pixel_patch(
        &mut self,
        layer_id: &str,
        before: Arc<StateNode>,
        after: Arc<StateNode>,
    ) -> u64 {
        // Barrier (defense-in-depth): no history mutation while a host handoff
        // (pending_external) is unconfirmed. Unreachable while pending in
        // production, but the guard makes the invariant locally enforced.
        if self.pending_external.is_some() {
            return self.version;
        }
        self.entries.truncate(self.cursor);
        let seq = self.next_seq;
        self.next_seq += 1;
        let cost = state_node_delta_memory_cost(&before, &after);
        self.entries.push(HistoryEntry {
            seq,
            group_id: seq,
            origin: Origin::Native,
            label: "pixel".to_string(),
            affected_layer_ids: vec![layer_id.to_string()],
            version_before: self.version,
            version_after: self.version + 1,
            memory_cost_bytes: cost,
            payload: EntryPayload::Pixel {
                layer_id: layer_id.to_string(),
                before,
                after,
            },
        });
        self.cursor = self.entries.len();
        // Eviction: bound the pixel-only stream to `max_depth` entries.
        // FIFO remove of the oldest entry; at the tip, cursor tracks `entries.len()`.
        if self.entries.len() > self.max_depth {
            self.entries.remove(0);
            self.cursor = self.entries.len();
        }
        self.version += 1;
        self.version
    }
    pub fn invalidate_layer(&mut self, layer_id: &str) {
        // Barrier (defense-in-depth): no history mutation while a host handoff
        // (pending_external) is unconfirmed. Unreachable while pending in
        // production, but the guard makes the invariant locally enforced.
        if self.pending_external.is_some() {
            return;
        }
        // P0 (B2): a layer resize/invalidate removes pixel entries from ANYWHERE
        // in the shared stream. `cursor` counts APPLIED entries
        // (`entries[0..cursor]`), so any removed entry whose original index was
        // BELOW the cursor was an applied entry and MUST decrement the cursor.
        // Clamping alone would leave a stale cursor pointing into the survivor
        // region, mis-marking a never-applied entry as applied and letting the
        // next undo/redo replay a state that was never committed.
        let old_cursor = self.cursor;
        let old = std::mem::take(&mut self.entries);
        let mut removed_below_cursor = 0usize;
        let mut kept = Vec::with_capacity(old.len());
        for (idx, entry) in old.into_iter().enumerate() {
            let is_layer_pixel = matches!(
                &entry.payload,
                EntryPayload::Pixel { layer_id: l, .. } if l == layer_id
            );
            if is_layer_pixel {
                if idx < old_cursor {
                    removed_below_cursor += 1;
                }
            } else {
                kept.push(entry);
            }
        }
        self.entries = kept;
        self.cursor = old_cursor
            .saturating_sub(removed_below_cursor)
            .min(self.entries.len());
    }
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    pub fn can_redo(&self) -> bool {
        self.cursor < self.entries.len()
    }

    /// The payload kind of the entry the next `undo_*` would consume (the entry
    /// just below the cursor). `None` when there is nothing undoable (empty
    /// history / cursor at 0). Lets a dispatcher route `undo_pixel` vs
    /// `undo_snapshot` vs a metadata/external undo unambiguously instead of
    /// string-matching labels. NOTE: a caller driving `undo_pixel` alone must
    /// consult this FIRST — `undo_pixel` refuses to consume a non-pixel tip.
    pub fn tip_payload_kind(&self) -> Option<PayloadKind> {
        if self.cursor == 0 {
            return None;
        }
        self.entries.get(self.cursor - 1).map(|e| e.payload.kind())
    }

    pub fn register_adapter(&mut self, adapter_id: &str) {
        if adapter_id != "native" && !self.adapters.iter().any(|a| a == adapter_id) {
            self.adapters.push(adapter_id.to_string());
        }
    }

    pub fn history_query(&self) -> HistoryQuery {
        let entries = self
            .entries
            .iter()
            .map(|e| HistoryEntryView {
                seq: e.seq,
                group_id: e.group_id,
                origin: match &e.origin {
                    Origin::Native => "native".to_string(),
                    Origin::External { adapter_id } => format!("external:{}", adapter_id),
                },
                label: e.label.clone(),
                affected_layer_ids: e.affected_layer_ids.clone(),
                version_before: e.version_before,
                version_after: e.version_after,
                memory_cost_bytes: e.memory_cost_bytes,
                payload_ref: match &e.payload {
                    EntryPayload::External { token } => Some(token.clone()),
                    EntryPayload::Native { .. } => None,
                    EntryPayload::Pixel { .. } => None,
                    EntryPayload::Snapshot { .. } => None,
                },
                payload_kind: Some(e.payload.kind()),
            })
            .collect();
        HistoryQuery {
            cursor: self.cursor as u64,
            last_seq: self.next_seq.saturating_sub(1),
            degraded_hint: false, // engine-side always false; TS merges markers
            pending_external: self
                .pending_external
                .as_ref()
                .map(|(s, d)| PendingExternalView {
                    seq: *s,
                    direction: d.clone(),
                }),
            entries,
        }
    }

    fn diff(old: &LayerSet, new: &LayerSet) -> Vec<RenderLayerChange> {
        let mut changes = Vec::new();
        // Index `old` by id once so the per-layer comparison is an O(1) lookup
        // instead of an O(n) linear scan per new layer (O(n²) total).
        let mut old_by_id: HashMap<&str, &Arc<LayerMeta>> = HashMap::with_capacity(old.0.len());
        for arc in old.iter() {
            old_by_id.insert(arc.id.as_str(), arc);
        }
        let new_ids: HashSet<&str> = new.iter().map(|a| a.id.as_str()).collect();

        for arc in new.iter() {
            let lr = arc.as_ref();
            match old_by_id.get(lr.id.as_str()) {
                // Pointer-identical to the before set: UNCHANGED layer (the
                // common case). The structural-sharing design guarantees this,
                // so we short-circuit with an O(1) Arc pointer compare instead
                // of a deep value compare.
                Some(o) if Arc::ptr_eq(o, arc) => {}
                // Same id, different allocation. Fall back to the deep value
                // compare so a re-built-but-value-equal layer does not emit a
                // spurious Upsert.
                Some(o) => {
                    if o.as_ref() != lr {
                        changes.push(RenderLayerChange::Upsert { layer: lr.clone() });
                    }
                }
                // No matching id: brand-new layer.
                None => changes.push(RenderLayerChange::Upsert { layer: lr.clone() }),
            }
        }
        for o in old.iter() {
            if !new_ids.contains(o.id.as_str()) {
                changes.push(RenderLayerChange::Remove {
                    id: o.id.clone(),
                    resource_id: o.resource_id,
                });
            }
        }
        // Pure-order changes (the Reorder command class) mutate no layer
        // VALUES: every Arc is pointer-identical across the transition, so the
        // loop above emits nothing even though the engine order moved. Hosts
        // that apply deltas reconstruct the sequence from the restatement, so
        // report every layer of `new` as an ordered Upsert. Value-change or
        // id-change diffs keep the existing behavior verbatim. The guard stays
        // empty for a true no-op (order identical) and only fires when
        // `changes` is still empty, so partial+move keeps current semantics
        // (documented as a pre-A0.5 gap for external entries).
        if changes.is_empty() && old.0.len() == new.0.len() {
            let same_order = old.iter().zip(new.iter()).all(|(a, b)| Arc::ptr_eq(a, b));
            if !same_order {
                for arc in new.iter() {
                    changes.push(RenderLayerChange::Upsert {
                        layer: arc.as_ref().clone(),
                    });
                }
            }
        }
        changes
    }
}
// `apply()` command-arm dispatch lives in a submodule to keep this module under
// the 1000-line guard; it is a sibling `impl ProtocolEngine` block.
#[path = "document_core_apply.rs"]
mod document_core_apply;
// Structural-command-arm bodies (duplicate / merge / flatten / rasterize) live in
// a sibling `impl ProtocolEngine` block to keep this module under the 1000-line guard.
#[path = "document_core_structural.rs"]
mod document_core_structural;
// Canvas-size command-arm bodies (Crop Canvas / Apply Crop / Resize Canvas) live in
// a sibling `impl ProtocolEngine` block to keep this module under the 1000-line guard.
#[path = "document_core_canvas.rs"]
mod document_core_canvas;

// ── wasm bridge — per-document engines (module lifetime, survives location.reload() until WASM re-instantiated) ──
// Each document id owns its own ProtocolEngine so multi-document sessions are
// fully isolated: per-doc documentVersion, per-doc layer set, per-doc history.
// A reserved "default" key routes every caller that does not supply a document
// id (legacy / non-facade path) to a single shared engine — byte-identical to
// the previous module-global engine for those callers.
thread_local! {
    static ENGINES: std::cell::RefCell<std::collections::HashMap<String, ProtocolEngine>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}
// Normalizes a document id: empty/absent maps to the reserved "default" key so
// callers that do not thread a document id keep the shared-engine behavior.
fn resolve_doc_key(doc_id: &str) -> &str {
    if doc_id.is_empty() {
        "default"
    } else {
        doc_id
    }
}
#[wasm_bindgen]
pub fn protocol_contract_version() -> u32 {
    CONTRACT_VERSION
}

#[wasm_bindgen]
pub fn protocol_version(doc_id: &str) -> DocumentVersion {
    let key = resolve_doc_key(doc_id).to_string();
    ENGINES.with(|m| m.borrow().get(&key).map(|e| e.version()).unwrap_or(0))
}

#[wasm_bindgen]
pub fn protocol_reset(doc_id: &str) {
    let key = resolve_doc_key(doc_id).to_string();
    ENGINES.with(|m| {
        m.borrow_mut().insert(key, ProtocolEngine::new());
    });
}

#[wasm_bindgen]
pub fn protocol_apply_command(envelope_json: &str, doc_id: &str) -> Result<String, JsValue> {
    let env: CommandEnvelope = serde_json::from_str(envelope_json)
        .map_err(|e| JsValue::from_str(&format!("E_ENVELOPE_PARSE: {}", e)))?;
    let key = resolve_doc_key(doc_id).to_string();
    ENGINES.with(|m| {
        let mut map = m.borrow_mut();
        let eng = map.entry(key).or_default();
        eng.apply(env)
            .map(|r| serde_json::to_string(&r).unwrap())
            .map_err(|e| JsValue::from_str(&serde_json::to_string(&e).unwrap()))
    })
}

#[wasm_bindgen]
pub fn protocol_snapshot_json(doc_id: &str) -> String {
    let key = resolve_doc_key(doc_id).to_string();
    let snap = ENGINES.with(|m| {
        m.borrow()
            .get(&key)
            .map(|e| e.snapshot())
            .unwrap_or(RenderSnapshot {
                version: 0,
                layers: Vec::new(),
                selection: None,
                width: None,
                height: None,
            })
    });
    serde_json::to_string(&snap).unwrap()
}

// ADDITIVE: seed the canonical shadow into a per-document wasm ProtocolEngine so
// the selection arms (selectAll reads doc dims from the shadow) can be exercised
// through the production bridge. Production defaults to `None` canonical, so this
// is never called in the default path; it exists for the parity matrix harness.
#[wasm_bindgen]
pub fn protocol_seed_canonical(canonical_json: &str, doc_id: &str) -> Result<String, JsValue> {
    let doc: CanonicalDocument = serde_json::from_str(canonical_json)
        .map_err(|e| JsValue::from_str(&format!("E_CANONICAL_PARSE: {}", e)))?;
    let key = resolve_doc_key(doc_id).to_string();
    ENGINES.with(|m| {
        let mut map = m.borrow_mut();
        let eng = map.entry(key).or_default();
        eng.seed_canonical(doc);
        Ok(serde_json::to_string(&()).unwrap())
    })
}

// ── H0: history stream exports ─────────────────────────────────
#[wasm_bindgen]
pub fn protocol_register_payload_adapter(adapter_id: &str, doc_id: &str) {
    let key = resolve_doc_key(doc_id).to_string();
    ENGINES.with(|m| {
        m.borrow_mut()
            .entry(key)
            .or_default()
            .register_adapter(adapter_id);
    });
}

#[wasm_bindgen]
pub fn protocol_history_query_json(doc_id: &str) -> String {
    let key = resolve_doc_key(doc_id).to_string();
    let q = ENGINES.with(|m| {
        m.borrow()
            .get(&key)
            .map(|e| e.history_query())
            .unwrap_or(HistoryQuery {
                cursor: 0,
                last_seq: 0,
                degraded_hint: false,
                pending_external: None,
                entries: Vec::new(),
            })
    });
    serde_json::to_string(&q).unwrap()
}

#[wasm_bindgen]
pub fn protocol_history_cursor_commit(json: &str, doc_id: &str) -> Result<String, JsValue> {
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Req {
        seq: u64,
        direction: String,
    }
    let req: Req = serde_json::from_str(json)
        .map_err(|e| JsValue::from_str(&format!("E_ENVELOPE_PARSE: {}", e)))?;
    let key = resolve_doc_key(doc_id).to_string();
    ENGINES.with(|m| {
        let mut map = m.borrow_mut();
        let eng = map.entry(key).or_default();
        eng.history_cursor_commit(req.seq, &req.direction)
            .map(|r| serde_json::to_string(&r).unwrap())
            .map_err(|e| JsValue::from_str(&serde_json::to_string(&e).unwrap()))
    })
}

#[cfg(test)]
mod version_tests {
    use super::*;

    // Cheap version getter: returns the seeded engine version without building
    // the full snapshot. Mirrors the native-version read optimization in the
    // facade syncFromEngine path.
    #[test]
    fn version_returns_seeded_engine_version() {
        let mut engine = ProtocolEngine::new();
        let layer = RenderLayer {
            id: "L1".to_string(),
            name: "Base".to_string(),
            visible: true,
            opacity: 1.0,
            resource_id: 1,
            x: 0.0,
            y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
            dirty_rect: None,
            ..Default::default()
        };
        engine.seed_layers(vec![layer], 42);
        assert_eq!(engine.version(), 42);
    }
}

#[cfg(test)]
#[path = "document_core_canonical_seed_tests.rs"]
mod canonical_seed_tests;

#[cfg(test)]
#[path = "document_core_arm_tests.rs"]
mod arm_tests;

#[cfg(test)]
#[path = "document_core_reorder_tests.rs"]
mod reorder_tests;

#[cfg(test)]
#[path = "document_core_arm_structural_tests.rs"]
mod arm_structural_tests;

#[cfg(test)]
#[path = "document_core_canvas_tests.rs"]
mod canvas_tests;
