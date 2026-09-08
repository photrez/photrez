// SPDX-License-Identifier: AGPL-3.0-or-later
// Command wire types (ADR 0008 C1: version-stable envelope). These cross the
// wasm boundary and must not change their serialized shape.

use crate::canonical_model::BlendMode;
use crate::projection::RenderDelta;
use serde::{Deserialize, Serialize};
/// Schema/protocol version. Bump on breaking envelope change.
pub const CONTRACT_VERSION: u32 = 2;

pub type DocumentVersion = u64;
pub type ResourceId = u32;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TransformPatch {
    pub x: f64,
    pub y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation: f64,
    // Additive flip fields (mirror of Transform2D.flipH/flipV). Optional so a
    // pre-flip transform envelope (no flip keys) still deserializes; when present
    // the TransformLayer arm projects them onto the render layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flip_h: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flip_v: Option<bool>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StrokePoint {
    pub x: f64,
    pub y: f64,
    pub pressure: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrushSettings {
    pub size: f64,
    pub hardness: f64,
    pub opacity: f64,
    pub flow: f64,
}
/// The four TS layer-lock kinds (document.ts setLayerLocked +
/// setLayerLock{Transparency,Position,Rotation}). `base` maps to
/// `RenderLayer.locked`; the other three map to their named lock fields.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum LockKind {
    Base,
    Transparency,
    Position,
    Rotation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Command {
    Noop,
    Ping {
        echo: String,
    },
    // Host owns identity + placement: active-layer is UI state the engine must
    // not assume, so the id (TS-minted) and insertion index (clamped to the
    // current layer count) travel with the command. `width`/`height` seed the
    // new layer's dimensions.
    AddLayer {
        id: String,
        name: String,
        width: f64,
        height: f64,
        index: usize,
    },
    DeleteLayer {
        id: String,
    },
    TransformLayer {
        id: String,
        transform: TransformPatch,
    },
    SetOpacity {
        id: String,
        opacity: f64,
    },
    SetVisible {
        id: String,
        visible: bool,
    },
    SetLocked {
        id: String,
        kind: LockKind,
        locked: bool,
    },
    Rename {
        id: String,
        name: String,
    },
    Reorder {
        id: String,
        to: usize,
    },
    SetBackgroundFlag {
        id: String,
    },
    SetBlendMode {
        id: String,
        mode: BlendMode,
    },
    BrushStroke {
        layer_id: String,
        points: Vec<StrokePoint>,
        settings: BrushSettings,
    },
    Undo,
    Redo,
    // H0: records a legacy TS transition into the canonical stream.
    // Advances DocumentVersion by exactly 1; payload stays behind the EXTERNAL
    // PayloadAdapter (token only) — never re-owned by Rust.
    RecordExternalTransition {
        label: String,
        affected_layer_ids: Vec<String>,
        adapter_id: String,
        token: String,
        memory_cost_bytes: u64,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommandEnvelope {
    pub contract_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<DocumentVersion>,
    pub command: Command,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CommandResult {
    pub document_version: DocumentVersion,
    pub delta: RenderDelta,
    // H0: "external" means the entry at the cursor is owned by an external
    // PayloadAdapter — the HOST executes it via its adapter and then calls
    // protocol_history_cursor_commit. Absent = applied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external_seq: Option<u64>,
}
