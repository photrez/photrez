// SPDX-License-Identifier: AGPL-3.0-or-later
// History wire types: the entry origin, the typed payload discriminant, and
// the history view DTOs the host reads. They live in `wire` so the wire layer
// stays independent of the document engine (no `wire -> document` edge).

use crate::wire::command::DocumentVersion;
use serde::{Deserialize, Serialize};

/// Where a history entry came from: the engine's own arm, or a host adapter
/// that executed the step out of band.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Origin {
    Native,
    External { adapter_id: String },
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntryView {
    pub seq: u64,
    pub group_id: u64,
    pub origin: String, // "native" | "external:<adapterId>"
    pub label: String,
    pub affected_layer_ids: Vec<String>,
    pub version_before: DocumentVersion,
    pub version_after: DocumentVersion,
    pub memory_cost_bytes: u64,
    pub payload_ref: Option<String>, // Some(token) for external entries
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_kind: Option<PayloadKind>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryQuery {
    pub cursor: u64,
    pub last_seq: u64,
    pub degraded_hint: bool, // always false engine-side; TS projection merges markers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_external: Option<PendingExternalView>,
    pub entries: Vec<HistoryEntryView>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PendingExternalView {
    pub seq: u64,
    pub direction: String,
}
