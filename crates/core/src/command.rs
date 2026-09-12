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
    // Duplicate a layer verbatim, inserting the clone directly above the source.
    // Mirrors TS duplicateLayer / document.duplicate_layer on the happy path: clones
    // the source RenderLayer, derives a numeric-suffix name from the engine layer
    // set, mints a fresh resource id (the clone owns an independent bitmap), and
    // clears the background / bottom flags. Unknown id is a silent no-op (engine
    // no-ops like DeleteLayer); the host-side TS duplicateLayer THROWS on unknown id
    // - a documented divergence. The host owns identity checks before sending, so the
    // engine's no-op is safe. An empty or already present new_id is rejected with
    // E_INVALID before any mutation or history entry.
    DuplicateLayer {
        id: String,
        new_id: String,
    },
    // Merge a layer down into the one below it. The merged node inherits the bottom
    // layer's blend mode, is locked when either source is locked, and is named
    // "top + bottom". Document dims come from the seeded canonical shadow (absent ->
    // E_INVALID). Unknown id or a bottom-most layer is a silent no-op.
    MergeDown {
        id: String,
        merged_id: String,
    },
    // Merge multiple selected layers into one raster node at the highest stack
    // position among them. Blend mode is always Normal; locked when ANY selected
    // layer is locked. Fewer than two matched ids is a silent no-op.
    MergeSelected {
        ids: Vec<String>,
        merged_id: String,
    },
    // Flatten every layer into a single Background node (bottom flag + position/
    // rotation locks). Document dims come from the seeded canonical shadow (absent
    // -> E_INVALID). A single-layer document is a silent no-op.
    Flatten {
        merged_id: String,
    },
    // Rasterize a parametric (shape/text) layer to a plain raster layer: drop the
    // shape/text params, keep bitmap/dims/transform/adjustments. A non-parametric
    // layer is a silent no-op (mirrors shapeLayerToRaster/textLayerToRaster guard).
    RasterizeLayer {
        id: String,
    },
    // Crop the canvas: offset every unlocked layer by (-x, -y) and set the document
    // size to (width, height). Mirror of DocumentEngine.cropCanvas / performCropCanvas
    // (unlocked layers move, document resizes; locked layers are untouched, selection
    // is cleared). Non-positive width/height is a silent no-op; non-finite inputs
    // reject with E_INVALID (stricter than the host, which would let NaN slip
    // through its comparisons).
    //
    // Divergence (host-side guard must survive at routing time): the host oracle also
    // silently rejects dimensions above its device-adaptive effective maximum
    // (document.ts cropCanvas/applyCrop/resizeCanvas max-dim checks), whereas this arm
    // accepts any finite size. The host guard must remain in place before this command
    // reaches the arm. JSON-wire nuance: a non-finite value inside an OPTIONAL field
    // (rotation / target dims) arrives on the wire as null -> absent, so the arm treats
    // it as no rotation / no target; the arm's non-finite E_INVALID gate therefore
    // applies to the REQUIRED numeric fields (x/y/width/height). The host oracle would
    // instead perform NaN math on the optional field.
    CropCanvas {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
    // Apply a non-destructive crop: recenter every unlocked layer into the crop
    // region, rotate around the crop center, scale to the (optional) target size, and
    // re-derive its transform. Mirror of DocumentEngine.applyCrop's non-destructive
    // branch (the pixel-baking delete/fill variants remain host-side). `rotation`
    // defaults to 0; `target_width`/`target_height` are an optional pair (a half-pair
    // rejects with E_INVALID). Non-positive width/height is a silent no-op; a target
    // size that resolves to <= 0 is also a silent no-op; non-finite inputs reject
    // with E_INVALID.
    //
    // Divergence (host-side guard must survive at routing time): the host oracle also
    // silently rejects final dimensions above its device-adaptive effective maximum
    // (document.ts applyCrop max-dim check), whereas this arm accepts any finite size.
    // JSON-wire nuance: `rotation` and `target_width`/`target_height` are OPTIONAL; a
    // non-finite value inside either arrives on the wire as null -> absent, so the arm
    // treats it as no rotation / no target. The arm's non-finite E_INVALID gate
    // therefore applies to the REQUIRED fields (x/y/width/height/rotation when present);
    // the host oracle would perform NaN math on the absent-but-parsed optional field.
    ApplyCrop {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        #[serde(default)]
        rotation: Option<f64>,
        #[serde(default)]
        target_width: Option<f64>,
        #[serde(default)]
        target_height: Option<f64>,
    },
    // Resize the canvas to (width, height). Mirror of DocumentEngine.resizeCanvas:
    // only the document size changes, no layer is touched. Non-positive dims are a
    // silent no-op; non-finite inputs reject with E_INVALID. The document size is
    // undoable and rides the snapshot (no layer delta is emitted).
    //
    // Divergence (host-side guard must survive at routing time): the host
    // DocumentEngine.resizeCanvas THROWS E_RESOURCE_LIMIT when the new dimensions
    // exceed its memory-budget projection (document.ts resizeCanvas budget check),
    // whereas this arm has no notion of a pixel-memory budget and accepts any finite
    // size. The host guard must remain in place before this command reaches the arm.
    // JSON-wire nuance: width/height are REQUIRED fields, so a non-finite value on the
    // wire serializes to null and is rejected at envelope parse (E_ENVELOPE_PARSE)
    // before this arm's explicit E_INVALID gate runs.
    ResizeCanvas {
        width: f64,
        height: f64,
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
