// SPDX-License-Identifier: AGPL-3.0-or-later
// DocumentCore behavior: forward/backward history helpers, the pixel-patch
// stream, the walker diff/restore pair, and the external handoff restore. The
// state fields + accessors live in `engine/state.rs`.

use super::*;

mod state;

pub use state::DocumentCore;

impl DocumentCore {
    // Opens a forward native entry at the cursor. Truncates the forward
    // region (invariant: new command truncates redo). Returns the entry index.
    pub(crate) fn begin_forward(&mut self, label: &str, affected: &[String]) -> usize {
        self.entries.truncate(self.cursor);
        let seq = self.next_seq;
        self.next_seq += 1;
        let entry = HistoryEntry {
            seq,
            group_id: seq, // History stream: singleton groups only
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
                pixel_deltas: Vec::new(),
            },
        };
        self.entries.push(entry);
        self.entries.len() - 1
    }

    pub(crate) fn finish_forward(&mut self, idx: usize) {
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
    // metadata path, so the `DocumentCore` is the single authoritative
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
    /// Fold a destructive step's pixel half into the metadata entry at `idx`.
    /// The engine owns no pixel buffer; `DocumentPixelStore` computes the
    /// before/after nodes while it bakes and hands them back here, so one
    /// command undoes metadata AND pixels as a single history step.
    pub(crate) fn attach_pixel_deltas(&mut self, idx: usize, deltas: Vec<PixelDelta>) {
        let Some(entry) = self.entries.get_mut(idx) else {
            return;
        };
        let EntryPayload::Native { pixel_deltas, .. } = &mut entry.payload else {
            return;
        };
        let cost: u64 = deltas
            .iter()
            .map(|d| state_node_delta_memory_cost(&d.before, &d.after))
            .sum();
        *pixel_deltas = deltas;
        entry.memory_cost_bytes = entry.memory_cost_bytes.saturating_add(cost);
    }

    pub fn invalidate_layer(&mut self, layer_id: &str) {
        // Barrier (defense-in-depth): no history mutation while a host handoff
        // (pending_external) is unconfirmed. Unreachable while pending in
        // production, but the guard makes the invariant locally enforced.
        if self.pending_external.is_some() {
            return;
        }
        // A layer resize/invalidate removes pixel entries from ANYWHERE
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

    /// How many External redo steps have run with no layer restatement to give
    /// the host, because the entry's post-sync side was never captured.
    ///
    /// Zero on every document whose External entries got their post-sync side
    /// from a canonical push (the host's external-transition record pushes
    /// one after the mutation it just committed), which is the shipping
    /// sequence. A non-zero value means some entry's post-sync side is missing
    /// and the engine has no other source for it - the redo moved the cursor and
    /// handed the host its token, but restored no layer state, so the host's own
    /// replay is the only thing that carried it. Monotonic per document.
    pub fn external_post_state_gaps(&self) -> u64 {
        self.external_post_state_gaps
    }

    pub fn register_adapter(&mut self, adapter_id: &str) {
        if adapter_id != "native" && !self.adapters.iter().any(|a| a == adapter_id) {
            self.adapters.push(adapter_id.to_string());
        }
    }

    /// Rename the entry at the cursor tip. Labels are presentation metadata;
    /// callers use it after a pixel commit to record the tool that produced it.
    pub fn set_history_label(&mut self, label: &str) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.entries[self.cursor - 1].label = label.to_string();
        true
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
                    EntryPayload::External { token, .. } => Some(token.clone()),
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
            degraded_hint: false, // engine-side always false; the host projection merges markers
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

    /// Walker-only delta used by the undo/redo arms. The undo/redo walker passes
    /// `new` as the MERGED vector produced by `restore_with_foreign` - i.e. the
    /// exact desired end state (captured entry vector + foreign survivors appended).
    /// The delta is therefore the authoritative final layer vector:
    ///
    /// - FAST PATH: identical membership AND order (pointer-identical Arcs) => empty
    ///   delta. Structural sharing makes this O(n) zip the cheap no-op check.
    /// - otherwise: Remove every layer present in `old` but absent from `new`
    ///   (these are the ids this entry itself introduced; foreign survivors are
    ///   already present in `new` via `restore_with_foreign`, so they are never
    ///   removed - membership safety), then Upsert EVERY layer of `new` in `new`'s
    ///   order. Because the upserts carry `new`'s exact sequence, the host
    ///   reconstructs order from the delta alone - no snapshot re-read, no
    ///   tail-append ambiguity, and a restored/merged layer lands at its SNAPSHOT
    ///   position (not the stack bottom).
    pub(crate) fn diff_walker(old: &LayerSet, new: &LayerSet) -> Vec<RenderLayerChange> {
        // FAST PATH: same membership + same order (Arc-pointer identical) => genuine
        // no-op. A value change (fresh Arc) or a move makes the zip differ, so the
        // full restatement below fires instead.
        if old.0.len() == new.0.len() && old.iter().zip(new.iter()).all(|(a, b)| Arc::ptr_eq(a, b))
        {
            return Vec::new();
        }
        // AUTHORITATIVE FULL-VECTOR DELTA. `new` is the merged end state, so it
        // lists every layer exactly once in final order.
        let new_ids: HashSet<&str> = new.iter().map(|a| a.id.as_str()).collect();
        let mut changes = Vec::new();
        // Remove layers the entry introduced: present in `old` (current) but gone
        // from the merged `new`. What lands in `new` is decided by
        // `restore_with_foreign`, which keeps every current layer the entry is
        // not proven to have created - so this loop only reaches ids the entry
        // itself brought in.
        for o in old.iter() {
            if !new_ids.contains(o.id.as_str()) {
                changes.push(RenderLayerChange::Remove {
                    id: o.id.clone(),
                    resource_id: o.resource_id,
                });
            }
        }
        // Upsert EVERY merged layer in `new`'s order. This restates the full
        // sequence: a reorder, a value undo, a delete-undo, and a merge all carry
        // an ordered upsert of the entire stack, so the host adopts it verbatim.
        for arc in new.iter() {
            changes.push(RenderLayerChange::Upsert {
                layer: arc.as_ref().clone(),
            });
        }
        changes
    }

    /// Diff two layer sets into the ordered change delta applied to the host.
    /// See `diff_walker` for the scoped (walker) variant. `#[cfg(test)]` because
    /// the non-walker contract is pinned only by a unit test in
    /// `document/tests/reorder_history.rs`; production uses `diff_walker` directly.
    #[cfg(test)]
    pub(crate) fn diff(old: &LayerSet, new: &LayerSet) -> Vec<RenderLayerChange> {
        Self::diff_walker(old, new)
    }

    /// Walker-state restore that PRESERVES layers this entry did not create.
    /// Undo/redo restores the entry's captured vector, but layers the host
    /// created since then (a canonical re-push the engine learned through the
    /// mirror) are not part of this entry's own transition - dropping them here
    /// would silently delete live layers from the native set. Result: the
    /// captured order first, survivors appended in current order (a bounded
    /// placement for layers the entry's capture predates). Shared by the native
    /// walker and the host-handoff (external) entry restore.
    ///
    /// `other_side_proves_creation` states whether `other_side` is
    /// authoritative evidence of which ids THIS entry's transition brought into
    /// existence: on undo that is the post-state, on redo it is the set the
    /// transition destroyed and the redo must destroy again. A Native entry
    /// captures both sides at commit time, so it is always `true`. An External
    /// entry records only its pre-sync vector at `record_external`; its
    /// post-sync slot is filled later by `seed_ptz` from a WHOLE-DOCUMENT
    /// push, so that side also carries layers the host created outside this
    /// entry and cannot prove the entry created them. Reading it as a creation
    /// set makes the undo emit `Remove` for a live host layer, and the host
    /// applies the delta as the authoritative whole layer list - the layer and
    /// its bitmap are deleted.
    pub(crate) fn restore_with_foreign(
        current: &LayerSet,
        captured: &LayerSet,
        other_side: &LayerSet,
        other_side_proves_creation: bool,
    ) -> LayerSet {
        Self::restore_with_foreign_excluding_minted(
            current,
            captured,
            other_side,
            other_side_proves_creation,
            &[],
        )
    }

    /// `restore_with_foreign`, plus the set of ids the ENTRY itself minted.
    ///
    /// A minted id is dropped from the survivor set because the entry SAYS it created
    /// it, not because it was inferred from `other_side`. That distinction is the
    /// point: the survivor rule exists to preserve a host-pushed layer the captured
    /// pre-sync vector predates, and it keeps doing exactly that. Only an id the host
    /// names is dropped, so a merge destination stops surviving its own undo while
    /// every unclaimed survivor is untouched. An empty set is the pre-existing
    /// behaviour, byte for byte.
    pub(crate) fn restore_with_foreign_excluding_minted(
        current: &LayerSet,
        captured: &LayerSet,
        other_side: &LayerSet,
        other_side_proves_creation: bool,
        minted: &[String],
    ) -> LayerSet {
        let captured_ids: HashSet<&str> = captured.iter().map(|a| a.id.as_str()).collect();
        let other_ids: HashSet<&str> = other_side.iter().map(|a| a.id.as_str()).collect();
        let minted_ids: HashSet<&str> = minted.iter().map(|s| s.as_str()).collect();
        let mut v: Vec<Arc<LayerMeta>> = captured.0.to_vec();
        for arc in current.iter() {
            let id = arc.id.as_str();
            if captured_ids.contains(id) {
                continue;
            }
            // Claimed by the entry that created it: never a survivor.
            if minted_ids.contains(id) {
                continue;
            }
            // Survivor unless the entry is proven to have brought it in.
            if !other_ids.contains(id) || !other_side_proves_creation {
                v.push(arc.clone());
            }
        }
        LayerSet(Arc::new(v))
    }

    /// Restore the native layer ORDER captured by an External (host-handoff)
    /// entry, and return the ordered restatement delta plus the entry seq.
    ///
    /// An External entry records a mirrored host commit. Its captured `before` is
    /// the engine's layer set at record time - the pre-sync state, i.e. exactly
    /// what an undo of this entry restores to. Its `after` slot is filled later,
    /// from the next canonical push (`seed_ptz`); undo restores `before`,
    /// redo restores `after`. Both are `Arc` clones under structural sharing.
    ///
    /// Because `after` is a whole-document observation rather than this entry's
    /// own creation set, the undo does not treat an id that appears only there
    /// as one this entry created: such a layer is kept as a survivor, so the
    /// delta carries no `Remove` for it and the host - which adopts the delta as
    /// the authoritative whole layer list - keeps the layer and its bitmap. The
    /// redo still restores the pre-sync vector as its removal source, so
    /// re-applying a mirrored delete destroys the layer again. Survivors are
    /// appended in current order - a bounded placement for layers the capture
    /// predates.
    ///
    /// Returns an empty delta (handoff-only, no state change) when the post-sync
    /// side was never captured (a handoff that did not pass through a canonical
    /// push), so the host keeps the pre-existing handoff behavior.
    ///
    /// The EMPTY RETURN is direction-blind; the counting is not. Both are measured
    /// rather than assumed.
    ///
    /// An UNDO does not need `after`: it passes the post-sync vector as
    /// `other_side` with `other_side_proves_creation = false`, and that flag
    /// makes the survivor test in `restore_with_foreign_excluding_minted`
    /// unconditionally true, so `other_side` cannot reach the result
    /// (`undo_path_ignores_the_post_sync_side` pins this across four different
    /// `other_side` values). Yet across the four reachable shapes where an undo
    /// still finds `after: None`, the before-only merge yields the CURRENT vector
    /// unchanged - the same empty delta this gate returns - and leaves the layer
    /// vector where the gate leaves it (`differential_*` in
    /// document/tests/external.rs runs both answers side by side).
    /// The reason is `seed_ptz`: it fills the newest entry's `after` on
    /// every push that lands while that entry is the tip, which is exactly when a
    /// push can carry that entry's post-state. An External entry that still has
    /// `after: None` at its own undo is therefore one whose layer vector the
    /// engine never saw change across it, so ungating the undo would buy nothing
    /// observable. The gate stays.
    ///
    /// A REDO is the direction that genuinely needs `after`, because `after` IS
    /// the state it re-establishes and nothing else holds it. With no captured
    /// post-sync side there is no target to restore, so the step runs its host
    /// handoff with no layer restatement and COUNTS itself (see
    /// `external_post_state_gaps`) rather than answering with an empty delta
    /// indistinguishable from "this step changed no layer order". An undo in the
    /// same state is NOT counted: it had a complete target (`before`) and simply
    /// found that restoring it changes nothing.
    pub(crate) fn restore_external_layers(
        &mut self,
        idx: usize,
        direction: &str,
    ) -> (u64, Vec<RenderLayerChange>) {
        let seq = self.entries[idx].seq;
        // Cheap Arc clones (structural sharing): no deep layer copy.
        let (before, after, minted) = match &self.entries[idx].payload {
            EntryPayload::External {
                before,
                after,
                minted_layer_ids,
                ..
            } => (before.clone(), after.clone(), minted_layer_ids.clone()),
            _ => return (seq, Vec::new()),
        };
        let after = match after {
            Some(a) => a,
            None => {
                if direction != "undo" {
                    // No redo target and no other source for one. The step still
                    // runs its host handoff (the cursor moves on the host's
                    // confirm), so the host keeps its replay; the gap is counted
                    // instead of surfacing as "nothing to restore".
                    self.external_post_state_gaps += 1;
                }
                return (seq, Vec::new());
            }
        };
        let (captured, other_side, other_side_proves_creation) = if direction == "undo" {
            // The post-sync side is a whole-document observation, not this
            // entry's own creation set (see `restore_with_foreign`).
            (&before, &after, false)
        } else {
            // The pre-sync side names the ids this entry destroyed; the redo
            // must destroy them again.
            (&after, &before, true)
        };
        let new_layers = Self::restore_with_foreign_excluding_minted(
            &self.layers,
            captured,
            other_side,
            other_side_proves_creation,
            &minted,
        );
        let changes = Self::diff_walker(&self.layers, &new_layers);
        self.layers = new_layers;
        // The layer vector changed natively, so keep the shadow consistent
        // (SelectAll/Invert read it). No-op when no shadow is seeded.
        self.reconcile_replica();
        (seq, changes)
    }
}

#[cfg(test)]
mod tests;
