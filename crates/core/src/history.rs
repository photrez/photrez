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
#[path = "history_h0_tests.rs"]
mod h0_tests;
