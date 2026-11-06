// SPDX-License-Identifier: AGPL-3.0-or-later
// The persisted document CONTENT model: layers, transforms, text, shape
// params, and adjustments.
//
// CONSUMED BY PRODUCTION through `ptz/file.rs`, which wraps these types in
// the on-disk `.ptz` `document.json` payload. So the document CONTENT is on
// the save path, while the transport/session envelope (header, viewport,
// active layer, per-layer bitmap keys) lives in `ptz/file.rs`; those are
// editor state, which the content model deliberately excludes.
//
// The model owns no writer. The production `.ptz` writer is
// `PtzDocument::to_json` in `ptz/file.rs`.
//
// Every field/enum here is part of the persisted `.ptz` document schema:
//   - the layer node, Transform2D, BlendMode, ShapeKind, ShapeParams,
//     SelectionState, the document model, ViewportState and LayerId
//   - BasicAdjustment
//   - TextData, TextStroke and their unions
//
// Wire format: camelCase (matches the existing Rust protocol convention used by
// identity.rs / layer.rs / the core engine, and the `.ptz` field
// names verbatim so saved documents stay readable). Enum string values are the
// exact strings already written into saved documents; changing one breaks them.
//
// EXCLUDED FIELDS (deliberate, with reason):
//   - LayerNode.bitmapEpoch: transient in-memory freshness epoch,
//     explicitly "not persisted to .ptz". Not document content.
//   - The two bitmap slots the previous host kept on each layer node: live pixel
//     references. Pixels are NEVER stored in the model.
//     Replaced by `resource_id: Option<ResourceId>` (a token handle mirroring
//     LayerMeta.resource_id in layer.rs).
//   - DocumentModel.activeLayerId: editor session state, not
//     document CONTENT. It IS persisted -- `PtzDocument` in `ptz/file.rs`
//     carries it on the file envelope, and the loader restores it.
//   - DocumentModel.viewport: same split. Persisted on the
//     envelope, not on the content model.
//   - DocumentModel.dirty: save-state flag, likewise persisted on
//     the envelope and cleared by the loader after restore.
// So these three are excluded from the CONTENT model and included in the FILE;
// which is why the exclusion list and the `.ptz` payload are not the same set.

use serde::{Deserialize, Serialize};

// The nested per-layer payloads (basic adjustment, shape params, text data) and
// the unions they use live in `ptz/layer_params.rs`, published again here so the
// content model's imports stay in one place.
pub use crate::layer_params::{
    BasicAdjustment, ShapeFill, ShapeFillKind, ShapeKind, ShapeParams, ShapeStroke, TextAlign,
    TextBoxMode, TextData, TextFontStyle, TextStroke, TextStrokeAlign,
};

// The layer type and its unions live in `content/layers.rs`.
mod layers;
pub use layers::{BlendMode, LayerType, PtzLayer};

/// SelectionState.shape union.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SelectionShape {
    Rect,
    Ellipse,
}

/// Transform2D (includes flipH / flipV).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transform2D {
    pub x: f64,
    pub y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation: f64,
    pub flip_h: bool,
    pub flip_v: bool,
}

/// The persisted selection rectangle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionState {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub angle: f64,
    pub shape: Option<SelectionShape>,
    pub inverted: Option<bool>,
}

/// The persisted document content: id, name, size, layers and selection.
/// EXCLUDES activeLayerId + viewport (UI/session state) and dirty
/// (save-state flag). Includes selection per the brief.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PtzDocumentContent {
    pub id: String,
    pub name: String,
    pub width: f64,
    pub height: f64,
    pub layers: Vec<PtzLayer>,
    #[serde(default)]
    pub selection: Option<SelectionState>,
}

impl PtzDocumentContent {
    /// Parse a v3 `document.json` (photrez-ptz) into the canonical model.
    ///
    /// Real saved files carry session/transport fields that are not part of the
    /// canonical document content (`format`, `version`, `activeLayerId`,
    /// `viewport`, `dirty`) and per-layer bitmap placeholder keys (the two
    /// null-emitting keys appended by `PtzDocument::to_json`). The deserializer
    /// ignores these via its default
    /// field behavior. Optional layer/document fields that are absent from the
    /// file deserialize to `None`. Malformed JSON returns `Err` (never panics).
    pub fn from_ptz_document_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }
}
