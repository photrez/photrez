// SPDX-License-Identifier: AGPL-3.0-or-later
// Command wire types (ADR 0008 C1: version-stable envelope). These cross the
// wasm boundary and must not change their serialized shape.

use crate::canonical_model::{
    BasicAdjustment, BlendMode, LayerType, SelectionState, ShapeParams, TextData,
};
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
    // new layer's dimensions. Typed adds carry `layer_type` + the nested payload
    // (shape_params / text_data); all three are serde-defaulted so a v2 envelope
    // without them still parses and falls back to the raster/normal behavior.
    AddLayer {
        id: String,
        name: String,
        width: f64,
        height: f64,
        index: usize,
        #[serde(default)]
        layer_type: Option<LayerType>,
        #[serde(default)]
        shape_params: Option<ShapeParams>,
        #[serde(default)]
        text_data: Option<TextData>,
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
    // Set the parametric source-of-truth for an existing layer (shape/text).
    // Mirrors TS updateShapeParams / updateTextData: whichever of `shape_params` /
    // `text_data` is Some is written; the other is left untouched. Both None is an
    // invalid no-op (rejected with E_INVALID before any mutation). An unknown id is
    // a silent no-op (mirrors DeleteLayer + the TS guarded apply ops).
    SetLayerParams {
        id: String,
        #[serde(default)]
        shape_params: Option<ShapeParams>,
        #[serde(default)]
        text_data: Option<TextData>,
    },
    // Set or clear the non-destructive basic adjustment for an existing layer.
    // `adjustment: Some(...)` sets it (and derives has_adjustments from the values,
    // mirroring TS applyBasicAdjustment); `None` clears it and sets has_adjustments
    // to false (mirroring TS clearBasicAdjustments). Unknown id is a silent no-op.
    SetAdjustment {
        id: String,
        #[serde(default)]
        adjustment: Option<BasicAdjustment>,
    },
    BrushStroke {
        layer_id: String,
        points: Vec<StrokePoint>,
        settings: BrushSettings,
    },
    Undo,
    Redo,
    // Selection arms: selection is engine-local UI state that rides Model-A
    // snapshots (snapshot() includes it; snapshot/restore stay host-side by
    // design). These arms mutate the engine selection but commit NO history entry
    // (no begin_forward/finish_forward) and produce an empty delta — selection is
    // not an undoable transition in the command stream.
    SetSelection {
        selection: SelectionState,
    },
    ClearSelection,
    SelectAll,
    // Invert toggles the inverted flag when a selection exists; with none it mirrors
    // the host op, which falls back to select-all (full canvas from seeded canonical
    // dims; canonical absent rejects with E_INVALID like SelectAll). No history entry.
    InvertSelection,
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
