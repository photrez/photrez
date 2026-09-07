// SPDX-License-Identifier: AGPL-3.0-or-later
// Protocol engine composition root: ProtocolEngine type, the `apply()` dispatch
// entry, per-document engines, and the wasm-bindgen bridge. History/command/
// model/projection types live in their own modules (history.rs / command.rs /
// model.rs / projection.rs).

use crate::canonical_bridge::CanonicalShadow;
use crate::canonical_model::CanonicalDocument;
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
        }
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
    pub fn seed_canonical(&mut self, doc: CanonicalDocument) {
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
            if let EntryPayload::Native { after, .. } = &mut e.payload {
                // O(1): shares the post-change set (Arc bump), no layer clone.
                *after = self.layers.clone();
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
        changes
    }

    pub fn apply(&mut self, envelope: CommandEnvelope) -> Result<CommandResult, ProtocolError> {
        if envelope.contract_version != CONTRACT_VERSION {
            return Err(ProtocolError {
                code: "E_CONTRACT_VERSION".to_string(),
                message: format!(
                    "expected contractVersion {} got {}",
                    CONTRACT_VERSION, envelope.contract_version
                ),
            });
        }
        if let Some(expected) = envelope.expected_version {
            if expected != self.version {
                return Err(ProtocolError {
                    code: "E_VERSION_MISMATCH".to_string(),
                    message: format!("expected version {} got {}", expected, self.version),
                });
            }
        }
        // External-pending barrier (H0 invariant): a host handoff is
        // outstanding — no new command may enter until the cursor commit lands.
        if let Some((seq, dir)) = &self.pending_external {
            return Err(ProtocolError {
                code: "E_EXTERNAL_PENDING".to_string(),
                message: format!(
                    "external history transition pending: commit seq {} direction {} first",
                    seq, dir
                ),
            });
        }
        let base = self.version;
        // H0: external record is its own transition event.
        if let Command::RecordExternalTransition {
            label,
            affected_layer_ids,
            adapter_id,
            token,
            memory_cost_bytes,
        } = envelope.command
        {
            self.record_external(
                &label,
                &affected_layer_ids,
                &adapter_id,
                &token,
                memory_cost_bytes,
            )?;
            // Note: record_external already bumps version; do not bump again.
            // Layers unchanged here - reconcile intentionally skipped (host executes
            // external mutation out-of-band).
            return Ok(CommandResult {
                document_version: self.version,
                delta: RenderDelta {
                    base_version: base,
                    version: self.version,
                    changes: Vec::new(),
                },
                status: Some("external-recorded".to_string()),
                external_seq: None,
            });
        }
        // Walker handoff marker: set when an undo/redo step lands on an
        // EXTERNAL entry — host executes via its adapter then confirms with
        // protocol_history_cursor_commit (which performs the DV bump).
        let mut external_handoff: Option<u64> = None;
        let mut handoff_dir = "";
        let changes = match envelope.command {
            Command::Noop => Vec::new(),
            Command::Ping { echo } => {
                let id = format!("ping:{}", echo);
                let layer = RenderLayer {
                    id: id.clone(),
                    name: echo.clone(),
                    visible: true,
                    opacity: 1.0,
                    resource_id: self.next_resource,
                    x: 0.0,
                    y: 0.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation: 0.0,
                    dirty_rect: Some(Rect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    }),
                };
                if let Some(pos) = self.layers.position_by_id(&id) {
                    self.layers = self.layers.replaced(pos, layer.clone());
                } else {
                    self.next_resource += 1;
                    self.layers = self.layers.pushed(layer.clone());
                }
                vec![RenderLayerChange::Upsert { layer }]
            }
            Command::AddLayer { name } => {
                let _e = self.begin_forward("Add Layer", &[]);
                let id = uuid::Uuid::new_v4().to_string();
                let layer = RenderLayer {
                    id: id.clone(),
                    name: name.clone(),
                    visible: true,
                    opacity: 1.0,
                    resource_id: self.next_resource,
                    x: 0.0,
                    y: 0.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation: 0.0,
                    dirty_rect: Some(Rect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    }),
                };
                self.next_resource += 1;
                self.layers = self.layers.pushed(layer.clone());
                self.finish_forward(_e);
                vec![RenderLayerChange::Upsert { layer }]
            }
            Command::DeleteLayer { id } => {
                if let Some(pos) = self.layers.position_by_id(&id) {
                    let _e = self.begin_forward("Delete Layer", &[id.clone()]);
                    let resource_id = self.layers.get(pos).expect("layer present").resource_id;
                    self.layers = self.layers.removed(pos);
                    self.finish_forward(_e);
                    vec![RenderLayerChange::Remove {
                        id: id.clone(),
                        resource_id,
                    }]
                } else {
                    Vec::new()
                }
            }
            Command::TransformLayer { id, transform } => {
                if let Some(pos) = self.layers.position_by_id(&id) {
                    let _e = self.begin_forward("Transform Layer", &[id.clone()]);
                    let mut layer = self.layers.get(pos).expect("layer present").clone();
                    layer.x = transform.x;
                    layer.y = transform.y;
                    layer.scale_x = transform.scale_x;
                    layer.scale_y = transform.scale_y;
                    layer.rotation = transform.rotation;
                    layer.dirty_rect = Some(Rect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    });
                    self.layers = self.layers.replaced(pos, layer.clone());
                    self.finish_forward(_e);
                    vec![RenderLayerChange::Upsert { layer }]
                } else {
                    Vec::new()
                }
            }
            Command::SetOpacity { id, opacity } => {
                let clamped = opacity.clamp(0.0, 1.0);
                if let Some(pos) = self.layers.position_by_id(&id) {
                    let _e = self.begin_forward("Set Opacity", &[id.clone()]);
                    let mut layer = self.layers.get(pos).expect("layer present").clone();
                    layer.opacity = clamped;
                    layer.dirty_rect = Some(Rect {
                        x: 0,
                        y: 0,
                        width: 1,
                        height: 1,
                    });
                    self.layers = self.layers.replaced(pos, layer.clone());
                    self.finish_forward(_e);
                    vec![RenderLayerChange::Upsert { layer }]
                } else {
                    Vec::new()
                }
            }
            Command::BrushStroke {
                layer_id,
                points,
                settings,
            } => {
                if points.is_empty() {
                    Vec::new()
                } else if let Some(pos) = self.layers.position_by_id(&layer_id) {
                    let _e = self.begin_forward("Brush Stroke", &[layer_id.clone()]);
                    let mut layer = self.layers.get(pos).expect("layer present").clone();
                    // dirtyRect must account for brush footprint, not just point bbox
                    let mut min_x = f64::INFINITY;
                    let mut max_x = f64::NEG_INFINITY;
                    let mut min_y = f64::INFINITY;
                    let mut max_y = f64::NEG_INFINITY;
                    for p in &points {
                        if p.x < min_x {
                            min_x = p.x;
                        }
                        if p.x > max_x {
                            max_x = p.x;
                        }
                        if p.y < min_y {
                            min_y = p.y;
                        }
                        if p.y > max_y {
                            max_y = p.y;
                        }
                    }
                    let radius = (settings.size / 2.0).ceil() as i32;
                    let x = (min_x.floor() as i32) - radius;
                    let y = (min_y.floor() as i32) - radius;
                    let w =
                        ((max_x - min_x).ceil() as i32 + settings.size.ceil() as i32).max(1) as u32;
                    let h =
                        ((max_y - min_y).ceil() as i32 + settings.size.ceil() as i32).max(1) as u32;
                    layer.dirty_rect = Some(Rect {
                        x,
                        y,
                        width: w,
                        height: h,
                    });
                    self.layers = self.layers.replaced(pos, layer.clone());
                    self.finish_forward(_e);
                    vec![RenderLayerChange::Upsert { layer }]
                } else {
                    Vec::new()
                }
            }
            // Handled by the early-return above (kept for exhaustiveness).
            Command::RecordExternalTransition { .. } => Vec::new(),
            Command::Undo => {
                if self.cursor == 0 {
                    // No-op undo: DV still bumps (accepted transition event).
                    Vec::new()
                } else {
                    let e = &self.entries[self.cursor - 1];
                    match &e.payload {
                        EntryPayload::Native { before, .. } => {
                            let new_layers = before.clone();
                            let changes = Self::diff(&self.layers, &new_layers);
                            self.layers = new_layers;
                            self.cursor -= 1;
                            changes
                        }
                        EntryPayload::Pixel { .. } => {
                            // Pixel undo is handled by undo_pixel/redo_pixel
                            // (the separate authoritative path); the metadata
                            // apply walker leaves pixel entries untouched.
                            Vec::new()
                        }
                        EntryPayload::Snapshot { .. } => {
                            // Snapshot undo/redo is handled by the dedicated
                            // undo_snapshot/redo_snapshot path (it restores the
                            // metadata + bitmap-token reference); the metadata
                            // walker leaves Snapshot entries untouched.
                            Vec::new()
                        }
                        EntryPayload::External { .. } => {
                            // Host handoff: adapter executes, then
                            // protocol_history_cursor_commit moves the cursor
                            // and bumps DV. Nothing changes here.
                            external_handoff = Some(e.seq);
                            handoff_dir = "undo";
                            Vec::new()
                        }
                    }
                }
            }
            Command::Redo => {
                if self.cursor >= self.entries.len() {
                    Vec::new()
                } else {
                    let e = &self.entries[self.cursor];
                    match &e.payload {
                        EntryPayload::Native { after, .. } => {
                            let new_layers = after.clone();
                            let changes = Self::diff(&self.layers, &new_layers);
                            self.layers = new_layers;
                            self.cursor += 1;
                            changes
                        }
                        EntryPayload::Pixel { .. } => {
                            // See undo branch: pixel entries are owned by
                            // undo_pixel/redo_pixel, not the metadata walker.
                            Vec::new()
                        }
                        EntryPayload::Snapshot { .. } => {
                            // See undo branch: snapshot entries are owned by
                            // undo_snapshot/redo_snapshot, not the walker.
                            Vec::new()
                        }
                        EntryPayload::External { .. } => {
                            external_handoff = Some(e.seq);
                            handoff_dir = "redo";
                            Vec::new()
                        }
                    }
                }
            }
        };
        if let Some(seq) = external_handoff {
            self.pending_external = Some((seq, handoff_dir.to_string()));
            // Layers unchanged here - reconcile intentionally skipped (host executes
            // external mutation out-of-band); the cursor commit lands it separately.
            return Ok(CommandResult {
                document_version: self.version,
                delta: RenderDelta {
                    base_version: base,
                    version: self.version,
                    changes: Vec::new(),
                },
                status: Some("external".to_string()),
                external_seq: Some(seq),
            });
        }
        self.version += 1;
        self.reconcile_shadow();
        Ok(CommandResult {
            document_version: self.version,
            delta: RenderDelta {
                base_version: base,
                version: self.version,
                changes,
            },
            status: None,
            external_seq: None,
        })
    }
}
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
            })
    });
    serde_json::to_string(&snap).unwrap()
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
        };
        engine.seed_layers(vec![layer], 42);
        assert_eq!(engine.version(), 42);
    }
}

#[cfg(test)]
mod canonical_seed_tests {
    use super::*;
    use crate::canonical_model::CanonicalDocument;

    /// Minimal `CanonicalDocument` (no layers) keyed by id, for shadow-copy tests.
    fn empty_doc(id: &str) -> CanonicalDocument {
        CanonicalDocument {
            id: id.to_string(),
            name: "n".to_string(),
            width: 10.0,
            height: 10.0,
            layers: vec![],
            selection: None,
        }
    }

    #[test]
    fn canonical_default_is_none() {
        let engine = ProtocolEngine::new();
        assert!(engine.canonical().is_none());
    }

    #[test]
    fn seed_canonical_stores_copy() {
        let mut engine = ProtocolEngine::new();
        engine.seed_canonical(empty_doc("doc-A"));
        let c = engine.canonical().expect("canonical shadow stored");
        assert_eq!(c.id, "doc-A");
    }

    #[test]
    fn seed_canonical_replaces_existing() {
        let mut engine = ProtocolEngine::new();
        engine.seed_canonical(empty_doc("doc-A"));
        engine.seed_canonical(empty_doc("doc-B"));
        let c = engine.canonical().expect("canonical shadow stored");
        assert_eq!(c.id, "doc-B", "second seed must replace the first shadow");
    }
}
