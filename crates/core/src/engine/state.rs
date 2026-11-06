// SPDX-License-Identifier: AGPL-3.0-or-later
// DocumentCore state: the fields, construction, and the read/seed accessors. The
// behavior half (forward/backward history helpers, layer restore) lives in the
// parent `engine` module; both are sibling `impl DocumentCore` blocks.

use super::*;

// ── Engine ───────────────────────────────────────────────────────────────
// HISTORY BOUNDARY: history/future store Vec<LayerMeta> METADATA snapshots only
// (id/name/resource_id/transform/opacity). They do NOT store pixel buffers and must NOT be
// interpreted as pixel history. The underlying `HistoryEntry` schema {command, affected
// resources, patch/inverse, memory cost, transaction} is the target; this Vec is transitional and will be
// replaced before pixel ownership moves to Rust. Storing full pixel snapshots at 2048²/4K would be GBs.
#[derive(Debug)]
pub struct DocumentCore {
    pub(crate) version: DocumentVersion,
    pub(crate) next_resource: ResourceId,
    // Structural-sharing layer metadata set (Arc<Vec<Arc<LayerMeta>>>). Only the
    // changed layer gets a new Arc; unchanged layers SHARE Arc pointers across
    // history before/after. Undo/redo swaps this Arc in O(1).
    pub(crate) layers: LayerSet,
    // History stream: cursor = number of APPLIED entries
    // (0..=entries.len()). Independent from `version` (see C1).
    pub(crate) entries: Vec<HistoryEntry>,
    pub(crate) cursor: usize,
    pub(crate) next_seq: u64,
    pub(crate) max_depth: usize, // Bound the pixel-only history stream (FIFO eviction of the oldest entry).
    pub(crate) adapters: Vec<String>, // "native" is implicit and always available
    // External-pending barrier (history-stream invariant): while Some, the host owes a
    // protocol_history_cursor_commit for (seq, direction). EVERY command
    // through apply() is rejected with E_EXTERNAL_PENDING until then — the
    // host must not stack a pending external transition with new work.
    pub(crate) pending_external: Option<(u64, String)>,
    // Set by the Undo/Redo walker when it steps a `Pixel` entry: the layer id,
    // the tiles to replay, and the canonical node to re-anchor. The engine owns
    // NO pixel buffer, so it cannot write the tiles itself - the enclosing
    // `DocumentPixelStore` drains this right after `apply()` (see
    // `DocumentPixelStore::apply_command`). This is the seam that lets the Rust
    // stream own the pixel step without the engine owning the pixels.
    pub(crate) pending_pixels: Vec<(String, Vec<TilePatch>, Arc<StateNode>)>,
    /// Pixel half of a destructive crop, drained by `DocumentPixelStore`
    /// immediately after the command is applied. The engine owns no pixel
    /// buffer, so it can only describe what to bake.
    pub(crate) pending_crop_bake: Option<crate::engine::crop_bake::CropBakePlan>,
    /// Pixel half of a parametric rasterize, drained by `DocumentPixelStore`
    /// immediately after the command is applied (same seam as
    /// `pending_crop_bake`): the host-rendered bitmap that materializes the
    /// rasterized layer's canonical buffer.
    pub(crate) pending_rasterize: Option<crate::engine::rasterize_bake::RasterizePlan>,
    // Engine-local selection state (selection is core document state). Selection is NOT an
    // undoable transition in the command stream; it rides whole-document snapshots and is
    // reconciled onto the canonical shadow when one is seeded.
    pub(crate) selection: Option<SelectionState>,
    // Engine document size (canvas width/height). The native DocumentCore keeps
    // its own document dims, while the structural arms read the seeded canonical
    // shadow instead. `None` until a canvas-size command (Crop Canvas / Apply
    // Crop / Resize Canvas) sets it or a canonical shadow is seeded. Unlike the
    // LayerSet, this is undoable state for the canvas arms, so each such entry
    // snapshots it (see history.rs).
    pub(crate) doc_size: Option<(f64, f64)>,
    // Per-document replica of the persisted content, kept in sync with layer
    // edits via reconciliation (see `PtzReplica`). `None` until seeded; unseeded engines
    // leave the native-authority path OFF by default, so
    // production stays byte-identical. `incomplete` flags engine-minted layers
    // whose persisted-only fields cannot be reconstructed until the next push.
    pub(crate) replica: Option<PtzReplica>,
    // How many External REDO steps handed the host its handoff with no layer
    // restatement, because the entry's post-sync side was never captured and no
    // other source holds it (see `restore_external_layers`). Counted rather than
    // raised: an error here would strand the pending-external barrier mid-stream,
    // and a `status` change would alter the wire shape the host routes on. Read
    // it with `external_post_state_gaps`.
    pub(crate) external_post_state_gaps: u64,
}
impl Default for DocumentCore {
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
            pending_pixels: Vec::new(),
            pending_crop_bake: None,
            pending_rasterize: None,
            replica: None,
            selection: None,
            doc_size: None,
            external_post_state_gaps: 0,
        }
    }
}
impl DocumentCore {
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

    /// Borrow the current structural-sharing layer set. Used by the history
    /// module to capture the pre-transition vector for a host-handoff entry
    /// (an O(1) `Arc` clone under structural sharing).
    pub(crate) fn layers(&self) -> &LayerSet {
        &self.layers
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
    /// the canonical authority whose layer ids match the opened document's ids,
    /// letting later commands and their pixel resources line up) and aligns
    /// `version` to the supplied value so the caller's `expectedVersion` matches
    /// on the first real command.
    ///
    /// Id-preserved: the supplied `layers` keep their EXACT ids (no uuid minting —
    /// that happens only in the `AddLayer` command arm). Version-aligned: `version`
    /// is set to `version`. Silent: seeding is initialization, NOT a user edit, so
    /// it creates NO history entry and no undo step (snapshot seeding bypasses
    /// the monotonic guard rather than
    /// being a transition). Only-when-empty + idempotent: if the engine already
    /// has layers, the call is a silent no-op and never clobbers live state.
    pub fn seed_layers(&mut self, mut layers: Vec<LayerMeta>, version: DocumentVersion) {
        // Only-when-empty guard: never overwrite a populated engine. Seeding twice
        // yields the same result, and seeding onto live state is refused.
        if !self.layers.0.is_empty() {
            return;
        }
        // Resolve the adjustment flag at seed time. A host layer built from the
        // down-projection carries None ("no opinion"); leaving it None makes a
        // later SetAdjustment's pre-state ambiguous, and normalising it inside
        // that command would mutate state between history entries (breaking the
        // pure undo/redo stack). The canonical shadow knows which seeded layers
        // are adjusted, so the engine can carry an explicit, undoable flag now.
        for layer in &mut layers {
            if layer.has_adjustments.is_none() {
                let adjusted = self
                    .replica
                    .as_ref()
                    .and_then(|shadow| shadow.doc.layers.iter().find(|c| c.id == layer.id))
                    .is_some_and(|c| {
                        c.basic_adjustment.is_some() || c.has_adjustments == Some(true)
                    });
                layer.has_adjustments = Some(adjusted);
            }
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
        self.reconcile_replica();
    }

    /// Store a complete `PtzDocumentContent` and up-project its layer VECTOR into
    /// the native `LayerSet`. The canonical re-push channel carries
    /// the full layer truth at mirrored-commit moments; projecting it here makes
    /// the native engine authoritative for membership AND order (push order = engine
    /// order), so it is no longer blind to legacy structural mutations.
    ///
    /// Merge rules (all driven by the bridge's resource-preserving up-projection):
    ///   * id known to the engine -> value-merge preserving the engine's EXISTING
    ///     `resource_id` and `dirty_rect` (the renderer cannot own either); the
    ///     canonical 0-sentinel projection is deliberately NOT applied.
    ///   * id unknown -> mint a fresh, non-zero `resource_id` from `next_resource`
    ///     (bumped); never 0.
    ///   * engine id absent from the push -> dropped (a re-push missing an id is a
    ///     a delete made outside the engine, and mirroring it is the legitimate
    ///     reading).
    ///
    /// `doc_size` rule UNCHANGED: it has exactly three native owners (the baseline
    /// set here ONLY when `doc_size` is `None` at document open, the canvas arms,
    /// and the undo/redo walker) - this method still never mutates an existing
    /// `doc_size`. There is NO history entry: this is the host-authority assertion
    /// path (snapshot-seed semantics - an external step, not a
    /// native transition), so external steps stay host-executed and heal at the next
    /// forward commit. Single-owner contract still pinned by
    /// `repush_never_mutates_native_doc_size` / `audit_sequence_repush_preserves_history_and_dims`.
    pub fn seed_ptz(&mut self, doc: PtzDocumentContent) {
        let pushed_dims = (doc.width, doc.height);
        // Owner (1): baseline only at document open (engine has no dims yet).
        if self.doc_size.is_none() {
            self.doc_size = Some(pushed_dims);
        }

        // up-project the pushed layer vector into the native set.
        let mut new_layers: Vec<Arc<LayerMeta>> = Vec::with_capacity(doc.layers.len());
        for c in &doc.layers {
            if let Some(pos) = self.layers.position_by_id(&c.id) {
                let engine = self.layers.get(pos).expect("position valid").clone();
                new_layers.push(Arc::new(project_known_layer(c, &engine)));
            } else {
                let rid = self.next_resource;
                self.next_resource += 1;
                new_layers.push(Arc::new(project_new_layer(c, rid)));
            }
        }
        self.layers = LayerSet(Arc::new(new_layers));
        // Keep `next_resource` ahead of any id the push carried (mirrors seed_layers
        // :132-136), in case a preserved engine rid exceeds the minted frontier.
        if let Some(max_res) = self.layers.iter().map(|l| l.resource_id).max() {
            self.next_resource = self.next_resource.max(max_res.saturating_add(1));
        }

        // If the most recently applied history entry is a host-handoff
        // (external) record, this push is that entry's sync: capture the
        // up-projected vector as its post-sync side so an undo/redo of that
        // entry can restore layer order natively. Only an empty slot is filled,
        // so a later unrelated push never overwrites the captured side.
        if self.cursor > 0 {
            let post_sync = self.layers.clone();
            if let EntryPayload::External { after, .. } = &mut self.entries[self.cursor - 1].payload
            {
                if after.is_none() {
                    *after = Some(post_sync);
                }
            }
        }

        // Owner shadow: unconditional refresh so SelectAll / Invert read fresh dims.
        // The shadow mirrors the push exactly, so this reconcile is a near-no-op.
        self.replica = Some(PtzReplica::new(doc));
        self.reconcile_replica();
    }

    /// Read the seeded canonical document copy, if one has been stored.
    pub fn ptz_content(&self) -> Option<&PtzDocumentContent> {
        self.replica.as_ref().map(|s| &s.doc)
    }

    /// Whether the shadow is missing layers the engine has minted and cannot yet
    /// reconstruct (engine-side `AddLayer`). Sticky until the next full push.
    /// Test-only today; a future host-side consumer will read it.
    #[cfg(test)]
    pub(crate) fn replica_incomplete(&self) -> bool {
        self.replica.as_ref().map(|s| s.incomplete).unwrap_or(false)
    }

    /// Reconcile the shadow against the current engine layer set by reference
    /// (no per-layer clone). Zero-cost when no shadow is seeded.
    pub(crate) fn reconcile_replica(&mut self) {
        if let Some(shadow) = &mut self.replica {
            shadow.reconcile(self.layers.iter().map(|a| a.as_ref()));
        }
    }

    /// Mutable access to the shadow for tests that drive `reconcile` directly.
    #[cfg(test)]
    pub(crate) fn replica_mut(&mut self) -> Option<&mut PtzReplica> {
        self.replica.as_mut()
    }

    // ── History stream helpers ────────────────────────────────────────
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
    pub(crate) fn estimate_native_entry_cost(before: &LayerSet, after: &LayerSet) -> u64 {
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
}
