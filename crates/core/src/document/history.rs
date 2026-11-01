// SPDX-License-Identifier: AGPL-3.0-or-later
// History-cursor methods of DocumentCore: record/undo/redo for pixel, snapshot,
// and external-handoff entries, plus the host cursor commit. The payload types
// live in `history/payload.rs`.

use super::DocumentCore;
use crate::document::snapshot::DocumentSnapshot;
use crate::pixel::state_node::StateNode;
use crate::pixel::tile::TilePatch;
use crate::wire::command::CommandResult;
use crate::wire::history::Origin;
use crate::wire::projection::{state_node_touched_patches, ProtocolError, RenderDelta};
use std::sync::Arc;

mod payload;

pub(crate) use payload::{EntryPayload, HistoryEntry, PixelDelta};

/// Pixel undo/redo transaction result: `(layer_id, delta TilePatches,
/// authoritative StateNode Arc)`. The delta is the dirty-region tile set, not the
/// full layer.
type PixelTxn = (
    Option<String>,
    Option<Vec<TilePatch>>,
    Option<Arc<StateNode>>,
);
impl DocumentCore {
    /// The pixel step for the `Pixel` entry at `idx`: undo replays `before`,
    /// redo replays `after`. Returns the affected layer, the dirty-region tiles
    /// to replay, and the canonical node to re-anchor.
    ///
    /// Pure: it does NOT move the cursor or bump the version, so every caller
    /// owns both. This is the single definition of "what a pixel step is" — the
    /// `Command::Undo`/`Command::Redo` walker and `undo_pixel`/`redo_pixel` must
    /// not each grow their own copy, or the two executors drift apart.
    pub(crate) fn pixel_step_at(
        &self,
        idx: usize,
        redo: bool,
    ) -> Option<(String, Vec<TilePatch>, Arc<StateNode>)> {
        let entry = self.entries.get(idx)?;
        match &entry.payload {
            EntryPayload::Pixel {
                layer_id,
                before,
                after,
            } => {
                let (node, other) = if redo {
                    (after, before)
                } else {
                    (before, after)
                };
                Some((
                    layer_id.clone(),
                    state_node_touched_patches(node, other),
                    node.clone(),
                ))
            }
            // Every other payload keeps the caller's existing behaviour.
            _ => None,
        }
    }
    /// Undo the entry just below the cursor. For `Pixel` entries returns the
    /// affected layer id, the materialized `before` tiles (to replay onto
    /// `PixelLayer.pixels`), and the `before` `Arc<StateNode>` (so the
    /// canonical layer state can be re-anchored). Non-pixel entries move the
    /// cursor (+1 version) but yield no tiles. Rejects while a
    /// `pending_external` barrier is set (E_EXTERNAL_PENDING).
    pub fn undo_pixel(&mut self) -> Result<PixelTxn, ProtocolError> {
        if self.pending_external.is_some() {
            return Err(ProtocolError {
                code: "E_EXTERNAL_PENDING".to_string(),
                message: "external history transition pending; commit cursor first".to_string(),
            });
        }
        if self.cursor == 0 {
            return Ok((None, None, None));
        }
        let idx = self.cursor - 1;
        if let Some(txn) = self.pixel_step_at(idx, false) {
            self.cursor -= 1;
            self.version += 1;
            let (layer_id, tiles, node) = txn;
            return Ok((Some(layer_id), Some(tiles), Some(node)));
        }
        match &self.entries[idx].payload {
            // External (host-handoff) entries still step the unified cursor
            // so the ordering stays unified (the host executes + commits later);
            // no tiles are produced.
            EntryPayload::External { .. } => {
                self.cursor -= 1;
                self.version += 1;
                Ok((None, None, None))
            }
            // Snapshot / Native (metadata) entries are NOT owned by `undo_pixel`
            // — consuming one here would silently EAT the atomic Snapshot/meta
            // step and break atomicity. Return a no-op WITHOUT moving the
            // cursor; the caller MUST route by `tip_payload_kind()` to
            // `undo_snapshot` / the metadata undo path.
            EntryPayload::Snapshot { .. } | EntryPayload::Native { .. } => Ok((None, None, None)),
            // Pixel is handled above.
            EntryPayload::Pixel { .. } => Ok((None, None, None)),
        }
    }

    /// Redo the entry at the cursor. Symmetric to `undo_pixel` (returns `after`).
    pub fn redo_pixel(&mut self) -> Result<PixelTxn, ProtocolError> {
        if self.pending_external.is_some() {
            return Err(ProtocolError {
                code: "E_EXTERNAL_PENDING".to_string(),
                message: "external history transition pending; commit cursor first".to_string(),
            });
        }
        if self.cursor >= self.entries.len() {
            return Ok((None, None, None));
        }
        let idx = self.cursor;
        if let Some(txn) = self.pixel_step_at(idx, true) {
            self.cursor += 1;
            self.version += 1;
            let (layer_id, tiles, node) = txn;
            return Ok((Some(layer_id), Some(tiles), Some(node)));
        }
        match &self.entries[idx].payload {
            EntryPayload::External { .. } => {
                self.cursor += 1;
                self.version += 1;
                Ok((None, None, None))
            }
            EntryPayload::Snapshot { .. } | EntryPayload::Native { .. } => Ok((None, None, None)),
            EntryPayload::Pixel { .. } => Ok((None, None, None)),
        }
    }
    /// Record a host-handoff transition. `doc_size_before` / `doc_size_after` are
    /// the HOST's document-size halves, supplied together (see the `External`
    /// payload's contract); pass `None` for both on a transition that changes no
    /// size.
    // External-entry contract: the host size halves ship together (see the doc
    // above); allow the 9-arg bag rather than a struct every caller must build.
    #[allow(clippy::too_many_arguments)]
    pub fn record_external(
        &mut self,
        label: &str,
        affected: &[String],
        adapter_id: &str,
        token: &str,
        memory_cost_bytes: u64,
        doc_size_before: Option<(f64, f64)>,
        doc_size_after: Option<(f64, f64)>,
        minted_layer_ids: &[String],
    ) -> Result<(), ProtocolError> {
        // Barrier: no history mutation while a host handoff (pending_external)
        // is unconfirmed.
        self.external_barrier_check()?;
        if adapter_id != "native" && !self.adapters.iter().any(|a| a == adapter_id) {
            return Err(ProtocolError {
                code: "E_UNKNOWN_ADAPTER".to_string(),
                message: format!("adapter '{}' is not registered", adapter_id),
            });
        }
        // New forward transition: truncate redo region, append, advance DV once.
        self.entries.truncate(self.cursor);
        let seq = self.next_seq;
        self.next_seq += 1;
        self.entries.push(HistoryEntry {
            seq,
            group_id: seq,
            origin: Origin::External {
                adapter_id: adapter_id.to_string(),
            },
            label: label.to_string(),
            affected_layer_ids: affected.to_vec(),
            version_before: self.version,
            version_after: self.version + 1,
            memory_cost_bytes,
            payload: EntryPayload::External {
                token: token.to_string(),
                // Capture the pre-sync layer vector: the state this entry's undo
                // restores. Arc clone under structural sharing - O(1), no deep copy.
                before: self.layers().clone(),
                after: None,
                // Host-supplied, both halves together (see the payload's contract:
                // the engine's own doc_size is baseline-only and goes stale after
                // the first host crop, so it must never be the capture source).
                doc_size_before,
                doc_size_after,
                minted_layer_ids: minted_layer_ids.to_vec(),
            },
        });
        self.cursor = self.entries.len();
        // An external (host-executed) op is a committed logical mutation, so the
        // DocumentVersion must advance exactly once — matching apply_pixel_patch.
        self.version += 1;
        Ok(())
    }

    /// Record an atomic metadata+pixel `Snapshot` entry. Mirrors
    /// `record_external` (truncate redo, append, advance cursor + one Document
    /// Version) but stores BOTH the `before` and `after` snapshot so an undo
    /// restores the `before` and a redo re-applies the `after` — restoring BOTH
    /// the metadata and (via the per-layer opaque `bitmap_token`) the pixel
    /// state from ONE entry. `missing layers no-op` and `invalid doc Err` are
    /// enforced by the registry/host caller; here the snapshots are stored as
    /// given.
    pub fn record_snapshot(
        &mut self,
        before: DocumentSnapshot,
        after: DocumentSnapshot,
    ) -> Result<(), ProtocolError> {
        // Barrier: no history mutation while a host handoff (pending_external)
        // is unconfirmed.
        self.external_barrier_check()?;
        self.entries.truncate(self.cursor);
        let seq = self.next_seq;
        self.next_seq += 1;
        let affected: Vec<String> = after.layers.iter().map(|l| l.layer_id.clone()).collect();
        let cost = {
            let b = serde_json::to_string(&before)
                .map(|s| s.len() as u64)
                .unwrap_or(0);
            let a = serde_json::to_string(&after)
                .map(|s| s.len() as u64)
                .unwrap_or(0);
            b + a
        };
        self.entries.push(HistoryEntry {
            seq,
            group_id: seq, // History stream: singleton groups only
            origin: Origin::Native,
            label: "snapshot".to_string(),
            affected_layer_ids: affected,
            version_before: self.version,
            version_after: self.version + 1,
            memory_cost_bytes: cost,
            payload: EntryPayload::Snapshot { before, after },
        });
        self.cursor = self.entries.len();
        // A recorded snapshot is a committed logical mutation, so the
        // DocumentVersion advances exactly once — matching the other entries.
        self.version += 1;
        Ok(())
    }

    /// Undo the entry just below the cursor IF it is a `Snapshot` entry.
    /// Returns the `before` metadata snapshot (carrying the per-layer bitmap
    /// token, i.e. the atomic metadata+pixel reference) and moves the cursor +
    /// bumps one version. Non-snapshot entries return `Ok(None)` WITHOUT moving
    /// the cursor, so a caller dispatches the actual undo (Pixel/metadata)
    /// through the existing `undo_pixel` path. Rejects while a
    /// `pending_external` barrier is set (E_EXTERNAL_PENDING).
    pub fn undo_snapshot(&mut self) -> Result<Option<DocumentSnapshot>, ProtocolError> {
        self.external_barrier_check()?;
        if self.cursor == 0 {
            return Ok(None);
        }
        let idx = self.cursor - 1;
        let ret = match self.entries.get(idx) {
            Some(HistoryEntry {
                payload: EntryPayload::Snapshot { before, .. },
                ..
            }) => {
                self.cursor -= 1;
                self.version += 1;
                Some(before.clone())
            }
            _ => None,
        };
        Ok(ret)
    }

    /// Redo the entry at the cursor IF it is a `Snapshot` entry. Symmetric to
    /// `undo_snapshot` (returns the `after` snapshot; moves cursor + bumps one
    /// version). Non-snapshot entries return `Ok(None)` WITHOUT moving the
    /// cursor.
    pub fn redo_snapshot(&mut self) -> Result<Option<DocumentSnapshot>, ProtocolError> {
        self.external_barrier_check()?;
        if self.cursor >= self.entries.len() {
            return Ok(None);
        }
        let idx = self.cursor;
        let ret = match self.entries.get(idx) {
            Some(HistoryEntry {
                payload: EntryPayload::Snapshot { after, .. },
                ..
            }) => {
                self.cursor += 1;
                self.version += 1;
                Some(after.clone())
            }
            _ => None,
        };
        Ok(ret)
    }

    fn external_barrier_check(&self) -> Result<(), ProtocolError> {
        if self.pending_external.is_some() {
            return Err(ProtocolError {
                code: "E_EXTERNAL_PENDING".to_string(),
                message: "external history transition pending; commit cursor first".to_string(),
            });
        }
        Ok(())
    }

    /// Host confirms an EXTERNAL step it executed via its adapter.
    /// Validates `(seq, direction)` against the pending-external barrier set by
    /// the walker handoff. ADR 0008 C1 forbids conflating HistorySeq (a monotonic
    /// entry id) with HistoryCursor (a position), so this uses NO index arithmetic:
    /// on any redo-truncated (non-dense) stream `entries[i].seq != i+1`, so
    /// `cursor == seq` is wrong. The cursor provably cannot have moved since the
    /// walker set the barrier - every cursor-moving path
    /// (apply_pixel_patch/invalidate_layer/record_external/record_snapshot/
    /// undo_pixel/redo_pixel/undo_snapshot/redo_snapshot) is barrier-gated - so the recorded
    /// barrier `(seq, direction)` is authoritative. A genuinely wrong
    /// `(seq, direction)` fails `pending_matches` and is rejected.
    /// Requires a matching pending_external barrier (set by the walker handoff).
    pub fn history_cursor_commit(
        &mut self,
        seq: u64,
        direction: &str,
    ) -> Result<CommandResult, ProtocolError> {
        let pending_matches = matches!(
            self.pending_external.as_ref(),
            Some((s, d)) if *s == seq && d == direction
        );
        if !pending_matches {
            return Err(ProtocolError {
                code: "E_CURSOR_MISMATCH".to_string(),
                message: format!(
                    "cursor {} pending_external {:?} incompatible with seq {} direction {}",
                    self.cursor, self.pending_external, seq, direction
                ),
            });
        }
        match direction {
            "undo" => self.cursor -= 1,
            _ => self.cursor += 1,
        }
        let dv = self.version + 1;
        self.version = dv;
        self.pending_external = None; // barrier cleared on success only
                                      // Cursor-commit does not swap the LayerSet (barrier-gated; the host executes
                                      // the external mutation out-of-band), so this reconcile is defensive/no-op today.
        self.reconcile_replica();
        Ok(CommandResult {
            document_version: dv,
            delta: RenderDelta {
                base_version: dv - 1,
                version: dv,
                changes: Vec::new(),
                width: None,
                height: None,
            },
            status: Some("external-confirmed".to_string()),
            external_seq: Some(seq),
            pixel_patches: None,
            external_token: None,
        })
    }
}
