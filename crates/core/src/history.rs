// SPDX-License-Identifier: AGPL-3.0-or-later
// History stream types + the history-cursor methods of ProtocolEngine
// (record/undo/redo for pixel, snapshot, and external-handoff entries).

use crate::command::*;
use crate::document_core::ProtocolEngine;
use crate::model::*;
use crate::pixel_store::TilePatch;
use crate::projection::*;
use crate::snapshot::DocumentSnapshot;
use crate::state_node::StateNode;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Origin {
    Native,
    External { adapter_id: String },
}
// `EntryPayload`/`HistoryEntry` intentionally do NOT derive Clone.
// `Pixel` payloads hold `Arc<StateNode>` (sharing is via `Arc`, never a value
// clone). They DO derive Debug (ProtocolEngine derives Debug, which requires it).
#[derive(Debug)]
pub(crate) enum EntryPayload {
    // Built-in native adapter: before/after metadata snapshots. Stored as
    // structural-sharing `LayerSet`s (Arc<Vec<Arc<LayerMeta>>>) so unchanged
    // layers SHARE pointers and undo/redo is an O(1) Arc swap (mirrors the
    // `Pixel` path's `Arc<StateNode>` COW).
    Native {
        before: LayerSet,
        after: LayerSet,
    },
    External {
        token: String,
    },
    /// Native pixel-history entry. Carries before/after pixel
    /// states as immutable, byte-free `Arc<StateNode>`s (COW tile blocks
    /// shared across states). The metadata-level `ProtocolEngine` is reused as
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
/// Typed discriminant of a payload — lets a dispatcher route `undo_pixel` vs
/// `undo_snapshot` vs a metadata/external undo WITHOUT string-matching labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PayloadKind {
    /// Row-major/storage pixel entry (handled by `undo_pixel`/`redo_pixel`).
    Pixel,
    /// Atomic metadata+pixel snapshot entry (handled by
    /// `undo_snapshot`/`redo_snapshot`).
    Snapshot,
    /// TS (adapter) logical transition (host-handoff: the host steps the cursor).
    External,
    /// Native metadata (`RenderLayer`) payload (handled by the walker).
    Metadata,
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
/// Pixel undo/redo transaction result: `(layer_id, delta TilePatches,
/// authoritative StateNode Arc)`. The delta is the dirty-region tile set, not the
/// full layer.
type PixelTxn = (
    Option<String>,
    Option<Vec<TilePatch>>,
    Option<Arc<StateNode>>,
);
impl ProtocolEngine {
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
        let entry = match self.entries.get(idx) {
            Some(e) => e,
            None => return Ok((None, None, None)),
        };
        match &entry.payload {
            EntryPayload::Pixel {
                layer_id,
                before,
                after,
            } => {
                let layer = Some(layer_id.clone());
                let tiles = Some(state_node_touched_patches(before, after));
                let node = Some(before.clone());
                self.cursor -= 1;
                self.version += 1;
                Ok((layer, tiles, node))
            }
            // External (TS host-handoff) entries still step the unified cursor
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
        let entry = match self.entries.get(idx) {
            Some(e) => e,
            None => return Ok((None, None, None)),
        };
        match &entry.payload {
            EntryPayload::Pixel {
                layer_id,
                before,
                after,
            } => {
                let layer = Some(layer_id.clone());
                let tiles = Some(state_node_touched_patches(after, before));
                let node = Some(after.clone());
                self.cursor += 1;
                self.version += 1;
                Ok((layer, tiles, node))
            }
            // External (TS host-handoff) entries still step the unified cursor.
            EntryPayload::External { .. } => {
                self.cursor += 1;
                self.version += 1;
                Ok((None, None, None))
            }
            // Snapshot / Native (metadata) entries are NOT owned by `redo_pixel`
            // — refuse (no-op, no cursor move) so a caller driving `redo_pixel`
            // alone cannot eat an atomic Snapshot/meta step. Route by
            // `tip_payload_kind()` instead.
            EntryPayload::Snapshot { .. } | EntryPayload::Native { .. } => Ok((None, None, None)),
        }
    }
    pub fn record_external(
        &mut self,
        label: &str,
        affected: &[String],
        adapter_id: &str,
        token: &str,
        memory_cost_bytes: u64,
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
            },
        });
        self.cursor = self.entries.len();
        // An external (TS) op is a committed logical mutation, so the
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
    /// enforced by the registry/TS caller; here the snapshots are stored as
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
            group_id: seq, // H0: singleton groups only
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
        self.reconcile_shadow();
        Ok(CommandResult {
            document_version: dv,
            delta: RenderDelta {
                base_version: dv - 1,
                version: dv,
                changes: Vec::new(),
            },
            status: Some("external-confirmed".to_string()),
            external_seq: Some(seq),
        })
    }
}
#[cfg(test)]
mod h0_tests {
    use super::*;
    use crate::document_core::ProtocolEngine;
    use crate::state_node::{StateMeta, TileRef};
    use std::sync::Arc;

    fn env(cmd: Command) -> CommandEnvelope {
        CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: None,
            command: cmd,
        }
    }
    fn env_ev(cmd: Command, ev: u64) -> CommandEnvelope {
        CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: Some(ev),
            command: cmd,
        }
    }

    /// Test helper: build a 1x1 `Arc<StateNode>` for the single-tile pixel
    /// history tests. The payload value `v` fills the single RGBA pixel.
    fn sn(v: u8) -> Arc<StateNode> {
        let w = 1u32;
        let h = 1u32;
        let data = vec![v; (w * h * 4) as usize];
        let tile = TileRef::owning(0, 0, w, h, &data);
        Arc::new(StateNode::new(0, vec![tile], StateMeta::new(w, h, 0, 0)))
    }

    #[test]
    fn native_append_records_seq_and_cursor_advances() {
        let mut eng = ProtocolEngine::new();
        eng.apply(env(Command::AddLayer { name: "A".into() }))
            .unwrap();
        eng.apply(env(Command::AddLayer { name: "B".into() }))
            .unwrap();
        let q = eng.history_query();
        assert_eq!(q.cursor, 2);
        assert_eq!(q.last_seq, 2);
        assert_eq!(q.entries.len(), 2);
        assert_eq!(q.entries[0].seq, 1);
        assert_eq!(q.entries[1].seq, 2); // seq uniqueness/monotonicity
        assert_eq!(q.entries[0].origin, "native");
    }

    #[test]
    fn cursor_walks_back_and_forward_with_native_payload() {
        let mut eng = ProtocolEngine::new();
        eng.apply(env(Command::AddLayer { name: "A".into() }))
            .unwrap();
        let dv_after_add = eng.version();
        let r = eng.apply(env(Command::Undo)).unwrap();
        assert_eq!(r.status, None);
        assert_eq!(eng.history_query().cursor, 0);
        assert_eq!(eng.version(), dv_after_add + 1); // undo is itself a transition
        let r2 = eng.apply(env(Command::Redo)).unwrap();
        assert_eq!(r2.status, None);
        assert_eq!(eng.history_query().cursor, 1);
        assert!(r2
            .delta
            .changes
            .iter()
            .any(|c| matches!(c, RenderLayerChange::Upsert { .. })));
    }

    #[test]
    fn document_version_independent_from_cursor_on_noop_undo() {
        let mut eng = ProtocolEngine::new();
        let v0 = eng.version(); // cursor already 0
        let r = eng.apply(env(Command::Undo)).unwrap();
        assert_eq!(eng.history_query().cursor, 0); // cursor unchanged
        assert_eq!(r.document_version, v0 + 1); // DV advanced anyway
        assert!(r.delta.changes.is_empty());
    }

    #[test]
    fn redo_region_truncated_by_new_forward_command() {
        let mut eng = ProtocolEngine::new();
        eng.apply(env(Command::AddLayer { name: "A".into() }))
            .unwrap();
        eng.apply(env(Command::Undo)).unwrap(); // cursor=0
        eng.apply(env(Command::AddLayer { name: "B".into() }))
            .unwrap(); // truncates forward
        let q = eng.history_query();
        assert_eq!(q.cursor, 1);
        assert_eq!(q.entries.len(), 1);
        assert_eq!(q.entries[0].label, "Add Layer");
        assert_eq!(q.last_seq, 2); // seq NOT reused (next_seq monotonic)
        let r = eng.apply(env(Command::Redo)).unwrap();
        assert!(r.delta.changes.is_empty()); // nothing to redo
    }

    #[test]
    fn external_record_requires_registered_adapter_and_advances_dv_once() {
        let mut eng = ProtocolEngine::new();
        let err = eng
            .apply(env(Command::RecordExternalTransition {
                label: "L".into(),
                affected_layer_ids: vec![],
                adapter_id: "ts-external".into(),
                token: "t1".into(),
                memory_cost_bytes: 10,
            }))
            .unwrap_err();
        assert_eq!(err.code, "E_UNKNOWN_ADAPTER");

        eng.register_adapter("ts-external");
        let v0 = eng.version();
        let r = eng
            .apply(env(Command::RecordExternalTransition {
                label: "Move Layer".into(),
                affected_layer_ids: vec!["bg".into()],
                adapter_id: "ts-external".into(),
                token: "tok-1".into(),
                memory_cost_bytes: 128,
            }))
            .unwrap();
        assert_eq!(r.document_version, v0 + 1); // exactly once
        assert_eq!(r.status.as_deref(), Some("external-recorded"));
        let q = eng.history_query();
        assert_eq!(q.cursor, 1);
        assert_eq!(q.entries[0].origin, "external:ts-external");
        assert_eq!(q.entries[0].payload_ref.as_deref(), Some("tok-1"));
    }

    #[test]
    fn external_handoff_status_then_cursor_commit_bumps_dv_once() {
        let mut eng = ProtocolEngine::new();
        eng.register_adapter("ts-external");
        eng.apply(env(Command::RecordExternalTransition {
            label: "legacy op".into(),
            affected_layer_ids: vec![],
            adapter_id: "ts-external".into(),
            token: "t".into(),
            memory_cost_bytes: 1,
        }))
        .unwrap();
        let dv_before = eng.version();
        let r = eng.apply(env(Command::Undo)).unwrap();
        assert_eq!(r.status.as_deref(), Some("external"));
        assert_eq!(r.external_seq, Some(1));
        assert_eq!(r.document_version, dv_before); // NO bump at handoff time
        assert_eq!(eng.history_query().cursor, 1); // cursor untouched yet

        let c = eng.history_cursor_commit(1, "undo").unwrap();
        assert_eq!(c.document_version, dv_before + 1); // exactly one bump
        assert_eq!(eng.history_query().cursor, 0);

        // mismatched commit rejected
        let err = eng.history_cursor_commit(5, "undo").unwrap_err();
        assert_eq!(err.code, "E_CURSOR_MISMATCH");
    }

    #[test]
    fn history_cursor_commit_succeeds_on_non_dense_gapped_stream() {
        // ADR 0008 C1: HistorySeq (monotonic entry id) != HistoryCursor (position).
        // After a redo-truncation the stream is non-dense (entries[i].seq != i+1).
        // The old index arithmetic `cursor == seq` evaluated FALSE on such a stream
        // and retained the pending barrier forever (sticky historyDegraded). The fix
        // validates only the walker-recorded barrier, so the commit succeeds and the
        // wedge is cleared. This is the test that would have caught the bug.
        let mut eng = ProtocolEngine::new();
        eng.register_adapter("ts-external");
        // Dense region.
        eng.apply(env(Command::AddLayer { name: "A".into() }))
            .unwrap(); // seq 1
        eng.apply(env(Command::AddLayer { name: "B".into() }))
            .unwrap(); // seq 2
        eng.apply(env(Command::Undo)).unwrap(); // cursor=1 (B dropped from redo intent)
                                                // record_external truncates the redo region (removes seq 2) and appends the
                                                // next monotonic seq at index 1 -> entries[1].seq == 3 (a GAP).
        eng.apply(env(Command::RecordExternalTransition {
            label: "legacy op".into(),
            affected_layer_ids: vec![],
            adapter_id: "ts-external".into(),
            token: "t".into(),
            memory_cost_bytes: 1,
        }))
        .unwrap();
        let q = eng.history_query();
        assert_eq!(q.entries.len(), 2);
        assert_eq!(q.entries[1].seq, 3); // non-dense: position 2 carries seq 3

        // Undo lands on the gapped External entry without moving the cursor.
        let hand = eng.apply(env(Command::Undo)).unwrap();
        assert_eq!(hand.status.as_deref(), Some("external"));
        assert_eq!(hand.external_seq, Some(3));
        assert_eq!(eng.history_query().cursor, 2); // cursor untouched at handoff

        // Barrier validation (seq, direction) succeeds on the gapped stream.
        let c = eng.history_cursor_commit(3, "undo").unwrap();
        assert_eq!(c.document_version, eng.version());
        assert_eq!(eng.history_query().cursor, 1); // undo from cursor 2 -> cursor 1
        assert!(eng.pending_external.is_none()); // wedge cleared
                                                 // A following command is no longer rejected with E_EXTERNAL_PENDING.
        eng.apply(env(Command::AddLayer { name: "C".into() }))
            .unwrap();
    }

    #[test]
    fn history_cursor_commit_rejects_wrong_seq_on_non_dense_stream() {
        // Same gapped stream: a wrong seq (or direction) must still be rejected.
        let mut eng = ProtocolEngine::new();
        eng.register_adapter("ts-external");
        eng.apply(env(Command::AddLayer { name: "A".into() }))
            .unwrap();
        eng.apply(env(Command::AddLayer { name: "B".into() }))
            .unwrap();
        eng.apply(env(Command::Undo)).unwrap();
        eng.apply(env(Command::RecordExternalTransition {
            label: "legacy op".into(),
            affected_layer_ids: vec![],
            adapter_id: "ts-external".into(),
            token: "t".into(),
            memory_cost_bytes: 1,
        }))
        .unwrap();
        let _hand = eng.apply(env(Command::Undo)).unwrap();
        assert_eq!(eng.history_query().cursor, 2);

        // Wrong seq (the position looks plausible but the id does not match).
        let e1 = eng.history_cursor_commit(2, "undo").unwrap_err();
        assert_eq!(e1.code, "E_CURSOR_MISMATCH");
        assert!(eng.pending_external.is_some()); // barrier retained on failure

        // Wrong direction.
        let e2 = eng.history_cursor_commit(3, "redo").unwrap_err();
        assert_eq!(e2.code, "E_CURSOR_MISMATCH");
        assert!(eng.pending_external.is_some());
    }

    #[test]
    fn external_pending_barrier_blocks_forward_and_history_commands() {
        let mut eng = ProtocolEngine::new();
        eng.register_adapter("ts-external");
        eng.apply(env(Command::RecordExternalTransition {
            label: "legacy op".into(),
            affected_layer_ids: vec![],
            adapter_id: "ts-external".into(),
            token: "t".into(),
            memory_cost_bytes: 1,
        }))
        .unwrap();
        let hand = eng.apply(env(Command::Undo)).unwrap();
        assert_eq!(hand.status.as_deref(), Some("external"));
        assert!(eng.pending_external.is_some());

        // mutating command during pending -> E_EXTERNAL_PENDING
        let e1 = eng
            .apply(env(Command::AddLayer { name: "X".into() }))
            .unwrap_err();
        assert_eq!(e1.code, "E_EXTERNAL_PENDING");

        // another history command during pending -> E_EXTERNAL_PENDING
        let e2 = eng.apply(env(Command::Undo)).unwrap_err();
        assert_eq!(e2.code, "E_EXTERNAL_PENDING");

        // mismatched commit during pending -> E_CURSOR_MISMATCH, barrier retained
        let e3 = eng.history_cursor_commit(99, "undo").unwrap_err();
        assert_eq!(e3.code, "E_CURSOR_MISMATCH");
        assert!(eng.pending_external.is_some());

        // correct commit clears the barrier; forward command now accepted
        eng.history_cursor_commit(1, "undo").unwrap();
        assert!(eng.pending_external.is_none());
        eng.apply(env(Command::AddLayer { name: "Y".into() }))
            .unwrap(); // must not throw
    }

    #[test]
    fn query_exposes_pending_external_state() {
        let mut eng = ProtocolEngine::new();
        eng.register_adapter("ts-external");
        // no pending initially
        assert!(eng.history_query().pending_external.is_none());
        // record + undo creates a real handoff -> pending exposed via query
        eng.apply(env(Command::RecordExternalTransition {
            label: "L2".into(),
            affected_layer_ids: vec![],
            adapter_id: "ts-external".into(),
            token: "t2".into(),
            memory_cost_bytes: 0,
        }))
        .unwrap();
        let h = eng.apply(env(Command::Undo)).unwrap();
        assert_eq!(h.status.as_deref(), Some("external"));
        let pend = eng
            .history_query()
            .pending_external
            .expect("pending exposed in query");
        assert_eq!(pend.seq, 1);
        assert_eq!(pend.direction, "undo");
    }

    #[test]
    fn cursor_commit_without_pending_rejected() {
        let mut eng = ProtocolEngine::new();
        let err = eng.history_cursor_commit(1, "undo").unwrap_err();
        assert_eq!(err.code, "E_CURSOR_MISMATCH");
    }
    #[test]
    fn stale_expected_version_still_rejected_on_stream_path() {
        let mut eng = ProtocolEngine::new();
        eng.apply(env(Command::AddLayer { name: "A".into() }))
            .unwrap();
        let err = eng
            .apply(env_ev(
                Command::TransformLayer {
                    id: "x".into(),
                    transform: TransformPatch {
                        x: 1.0,
                        y: 0.0,
                        scale_x: 1.0,
                        scale_y: 1.0,
                        rotation: 0.0,
                    },
                },
                999,
            ))
            .unwrap_err();
        assert_eq!(err.code, "E_VERSION_MISMATCH");
    }

    #[test]
    fn pixel_undo_redo_rejected_while_external_pending() {
        let mut eng = ProtocolEngine::new();
        eng.register_adapter("ts-external");
        eng.apply(env(Command::RecordExternalTransition {
            label: "legacy op".into(),
            affected_layer_ids: vec![],
            adapter_id: "ts-external".into(),
            token: "t".into(),
            memory_cost_bytes: 1,
        }))
        .unwrap();
        let hand = eng.apply(env(Command::Undo)).unwrap();
        assert_eq!(hand.status.as_deref(), Some("external"));
        assert!(eng.pending_external.is_some());
        // The SAME barrier must block the unified pixel history path.
        let u = eng.undo_pixel();
        assert!(u.is_err(), "pixel undo must reject while external pending");
        assert_eq!(u.unwrap_err().code, "E_EXTERNAL_PENDING");
        let r = eng.redo_pixel();
        assert!(r.is_err(), "pixel redo must reject while external pending");
        assert_eq!(r.unwrap_err().code, "E_EXTERNAL_PENDING");
    }

    // C3: `undo_snapshot`/`redo_snapshot` under a pending_external barrier are
    // REJECTED with E_EXTERNAL_PENDING (no cursor move) — consistent with the
    // pixel path, so a snapshot entry cannot be consumed mid-host-handoff.
    #[test]
    fn snapshot_undo_redo_rejected_while_external_pending() {
        let mut eng = ProtocolEngine::new();
        eng.register_adapter("ts-external");
        eng.apply(env(Command::RecordExternalTransition {
            label: "legacy op".into(),
            affected_layer_ids: vec![],
            adapter_id: "ts-external".into(),
            token: "t".into(),
            memory_cost_bytes: 1,
        }))
        .unwrap();
        let hand = eng.apply(env(Command::Undo)).unwrap();
        assert_eq!(hand.status.as_deref(), Some("external"));
        assert!(eng.pending_external.is_some());
        assert_eq!(eng.cursor(), 1, "cursor untracked while external pending");

        let u = eng.undo_snapshot();
        assert!(
            u.is_err(),
            "snapshot undo must reject while external pending"
        );
        assert_eq!(u.unwrap_err().code, "E_EXTERNAL_PENDING");
        let r = eng.redo_snapshot();
        assert!(
            r.is_err(),
            "snapshot redo must reject while external pending"
        );
        assert_eq!(r.unwrap_err().code, "E_EXTERNAL_PENDING");
        assert_eq!(eng.cursor(), 1, "snapshot ops must not move the cursor");
    }

    // C4: the metadata walker's `Command::Undo` on a Snapshot tip must NOT
    // consume the atomic snapshot entry (no cursor move + empty changes) —
    // snapshot undo/redo is owned by `undo_snapshot`/`redo_snapshot`; the typed
    // `tip_payload_kind()` exposes the routing discriminant.
    #[test]
    fn walker_undo_on_snapshot_tip_no_cursor_move_empty_changes() {
        let mut eng = ProtocolEngine::new();
        let before = crate::snapshot::DocumentSnapshot::new("d", 0);
        let after = crate::snapshot::DocumentSnapshot::new("d", 1)
            .with_layer(crate::snapshot::LayerSnapshot::new("L", 8, 8));
        eng.record_snapshot(before, after).unwrap();
        assert_eq!(eng.cursor(), 1);
        assert_eq!(eng.tip_payload_kind(), Some(PayloadKind::Snapshot));

        let r = eng.apply(env(Command::Undo)).unwrap();
        assert!(
            r.delta.changes.is_empty(),
            "walker yields no changes for a snapshot tip"
        );
        assert_eq!(
            eng.cursor(),
            1,
            "walker must not consume the atomic snapshot tip"
        );
    }

    // Eviction: the pixel-only stream is bounded to `max_depth` (50)
    // entries. FIFO eviction of the oldest entry; cursor tracks the retained
    // stream; seq/version stay monotonic; canonical pixels are never touched.
    #[test]
    fn pixel_history_eviction_bounds_at_50() {
        let mut e = ProtocolEngine::new();

        // case 1: single entry
        e.apply_pixel_patch("L", sn(0), sn(1));
        assert_eq!(e.entries.len(), 1);
        assert_eq!(e.cursor(), 1);
        assert_eq!(e.version(), 1);

        // case 2: 50 entries, all retained
        for i in 2..=50 {
            e.apply_pixel_patch("L", sn(i - 1), sn(i));
        }
        assert_eq!(e.entries.len(), 50);
        assert_eq!(e.cursor(), 50);
        assert_eq!(e.version(), 50);
        let seqs: Vec<u64> = e.entries.iter().map(|x| x.seq).collect();
        assert!(seqs.windows(2).all(|w| w[0] < w[1]), "seq monotonic");

        // case 3: 51st entry evicts the oldest (seq 1); stream stays bounded
        e.apply_pixel_patch("L", sn(50), sn(51));
        assert_eq!(e.entries.len(), 50, "bounded at 50");
        assert_eq!(e.cursor(), 50, "cursor tracks retained stream");
        assert_eq!(e.version(), 51, "version monotonic");
        assert_eq!(e.entries[0].seq, 2, "oldest entry (seq 1) evicted");
        // retained payloads are intact (not corrupted by eviction)
        match &e.entries[49].payload {
            EntryPayload::Pixel { before, after, .. } => {
                assert_eq!(state_node_touched_patches(after, before)[0].data[0], 51)
            }
            _ => panic!("expected Pixel payload"),
        }

        // case 4: undo after eviction
        let (layer, tiles, _node) = e.undo_pixel().unwrap();
        assert_eq!(layer, Some("L".to_string()));
        assert_eq!(tiles.unwrap()[0].data[0], 50);
        assert_eq!(e.cursor(), 49);
        assert_eq!(e.version(), 52);

        // case 5: redo after eviction
        let (_, tiles, _node) = e.redo_pixel().unwrap();
        assert_eq!(tiles.unwrap()[0].data[0], 51);
        assert_eq!(e.cursor(), 50);

        // case 6: undo to the eviction boundary, then a new entry truncates redo
        while e.can_undo() {
            e.undo_pixel().unwrap();
        }
        assert_eq!(e.cursor(), 0);
        let (a, b, c) = e.undo_pixel().unwrap();
        assert!(
            a.is_none() && b.is_none() && c.is_none(),
            "cannot undo past eviction boundary"
        );
        e.apply_pixel_patch("L", sn(1), sn(99));
        assert_eq!(e.cursor(), 1);
        assert!(e.entries.len() <= 50, "still bounded");
    }

    // B2 (P0): a layer invalidate that removes a pixel entry BELOW the cursor
    // (with a surviving non-pixel entry above) must DECREMENT the cursor, not
    // just clamp it. Before the fix the cursor stayed at the pre-removal value,
    // mis-marking the surviving metadata entry as "applied" and letting a later
    // undo/redo replay a state that was never committed.
    #[test]
    fn invalidate_layer_mid_cursor_decrements_no_stale_resurrect() {
        let mut e = ProtocolEngine::new();

        // paint entry at index 0 (cursor 1) -> native metadata op at index 1
        // (cursor 2). A NATIVE non-pixel op is used (not external) so Undo moves
        // the cursor directly without creating an external-handoff barrier.
        e.apply_pixel_patch("L", sn(0), sn(1));
        e.apply(env(Command::AddLayer {
            name: "meta".into(),
        }))
        .unwrap();
        assert_eq!(e.cursor(), 2);
        assert_eq!(e.entries.len(), 2);

        // undo the metadata op -> cursor 1; now index 0 (pixel) is applied,
        // index 1 (metadata) is the forward/redo region.
        e.apply(env(Command::Undo)).unwrap();
        assert_eq!(e.cursor(), 1);

        // invalidate the layer: removes the pixel entry at index 0, which is
        // BELOW the old cursor (1) -> cursor must drop to 0 (clamping alone
        // would leave it at 1).
        e.invalidate_layer("L");
        assert_eq!(
            e.cursor(),
            0,
            "removed pixel entry below cursor must decrement the cursor"
        );
        assert_eq!(e.entries.len(), 1, "surviving metadata entry retained");
        assert_eq!(
            e.entries[0].origin,
            Origin::Native,
            "non-pixel metadata entry survives the invalidate"
        );

        // undo/redo must NOT revert a pixel op TS never applied after the resize.
        let (layer, tiles, _node) = e.undo_pixel().unwrap();
        assert!(
            layer.is_none() && tiles.is_none(),
            "no pixel undo after invalidate"
        );
        let (layer2, tiles2, _node2) = e.redo_pixel().unwrap();
        assert!(
            layer2.is_none() && tiles2.is_none(),
            "no pixel redo after invalidate"
        );
    }
}
