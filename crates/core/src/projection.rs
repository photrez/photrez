// SPDX-License-Identifier: AGPL-3.0-or-later
// Render projection types (RenderSnapshot / RenderDelta) + the pixel-history
// delta helpers that translate Arc<StateNode> states to the TS-facing TilePatch
// form, and the history query/error DTOs.

use crate::command::DocumentVersion;
use crate::history::PayloadKind;
use crate::model::{RenderLayer, RenderLayerChange};
use crate::pixel_store::TilePatch;
use crate::state_node::{StateNode, TILE};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderSnapshot {
    pub version: DocumentVersion,
    pub layers: Vec<RenderLayer>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderDelta {
    pub base_version: DocumentVersion,
    pub version: DocumentVersion,
    pub changes: Vec<RenderLayerChange>,
}
impl RenderDelta {
    pub fn is_applicable(&self, rendered_version: DocumentVersion) -> bool {
        self.base_version == rendered_version
    }
}
/// The delta tile set between two states: the tiles in `src` whose `TileRef`
/// `Arc<[u8]>` (shared packed subarray) identity differs from the matching tile
/// in `oth`. This is EXACTLY the set `LayerState::cow_batch` re-tiled (untouched
/// tiles share the packed `Arc` identity, per I3/I4/I9), so emitting only these
/// reproduces the dirty-region delta geometry (clipped edge tiles
/// included) WITHOUT shipping the whole layer over IPC on every undo/redo.
pub(crate) fn state_node_touched_patches(
    src: &Arc<StateNode>,
    oth: &Arc<StateNode>,
) -> Vec<TilePatch> {
    let oth_ids: HashMap<(u32, u32), (usize, usize)> = oth
        .tiles
        .iter()
        .map(|t| ((t.grid_x, t.grid_y), t.identity_key()))
        .collect();
    src.tiles
        .iter()
        .filter(|t| {
            oth_ids
                .get(&(t.grid_x, t.grid_y))
                .map(|key| *key != t.identity_key())
                .unwrap_or(true)
        })
        .map(|t| TilePatch {
            x: (t.grid_x as i64) * (TILE as i64),
            y: (t.grid_y as i64) * (TILE as i64),
            w: t.w as usize,
            h: t.h as usize,
            data: t.bytes().to_vec(),
        })
        .collect()
}
/// Sum the byte cost of the delta tiles WITHOUT materializing the per-tile
/// `Vec<u8>` (`data: t.bytes().to_vec()`) that `state_node_touched_patches`
/// would copy just to sum `w*h*4`. Iterates the touched tiles' dims directly
/// (zero buffer copy), so the commit-path memory-cost estimate has no full-tile
/// memcpy overhead.
fn touched_delta_tiles_size(src: &Arc<StateNode>, oth: &Arc<StateNode>) -> u64 {
    let oth_ids: HashMap<(u32, u32), (usize, usize)> = oth
        .tiles
        .iter()
        .map(|t| ((t.grid_x, t.grid_y), t.identity_key()))
        .collect();
    src.tiles
        .iter()
        .filter(|t| {
            oth_ids
                .get(&(t.grid_x, t.grid_y))
                .map(|key| *key != t.identity_key())
                .unwrap_or(true)
        })
        .map(|t| (t.w * t.h * 4) as u64)
        .sum()
}
/// Rough memory-cost estimate for a pixel history entry. Untouched tiles between
/// before/after are SHARED via `Arc`, so counting every tile in both states
/// overstates retention; count only the delta (touched) tiles of the transition.
pub(crate) fn state_node_delta_memory_cost(before: &Arc<StateNode>, after: &Arc<StateNode>) -> u64 {
    touched_delta_tiles_size(before, after) + touched_delta_tiles_size(after, before)
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
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolError {
    pub code: String,
    pub message: String,
}
