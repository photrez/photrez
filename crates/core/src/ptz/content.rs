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
// The v4 writer `to_ptz_document_json` below is fenced to `#[cfg(test)]`: the
// production writer is `PtzDocument::to_json`. See the note on that function.
//
// Every field/enum here is part of the persisted `.ptz` document schema:
//   - the layer node, Transform2D, BlendMode, ShapeKind, ShapeParams,
//     SelectionState, the document model, ViewportState and LayerId
//   - BasicAdjustment
//   - TextData, TextStroke and their unions
//
// Wire format: camelCase (matches the existing Rust protocol convention used by
// wire/command.rs / render/layer.rs / document/engine.rs, and the `.ptz` field
// names verbatim so saved documents stay readable). Enum string values are the
// exact strings already written into saved documents; changing one breaks them.
//
// EXCLUDED FIELDS (deliberate, with reason):
//   - LayerNode.bitmapEpoch: transient in-memory freshness epoch,
//     explicitly "not persisted to .ptz". Not document content.
//   - LayerNode.imageBitmap / baseImageBitmap: live pixel
//     references. Pixels are NEVER stored in the model.
//     Replaced by `resource_id: Option<ResourceId>` (a token handle mirroring
//     RenderLayer.resource_id in render/layer.rs).
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
// the unions they use live in `ptz/layer_params.rs`, re-exported here so the
// content model's imports stay in one place.
pub use crate::ptz::layer_params::{
    BasicAdjustment, ShapeFill, ShapeFillKind, ShapeKind, ShapeParams, ShapeStroke, TextAlign,
    TextBoxMode, TextData, TextFontStyle, TextStroke, TextStrokeAlign,
};

// The layer type and its unions live in `ptz/content/layers.rs`.
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
    /// `viewport`, `dirty`) and per-layer pixel references (`imageBitmap`,
    /// `baseImageBitmap`). The deserializer ignores these via its default
    /// field behavior. Optional layer/document fields that are absent from the
    /// file deserialize to `None`. Malformed JSON returns `Err` (never panics).
    pub fn from_ptz_document_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }

    /// Serialize the canonical document to a v4 `.ptz` `document.json` payload.
    ///
    /// NOT THE PRODUCTION WRITER. The production writer is `PtzDocument::to_json`
    /// in `ptz/file.rs`. This one is fenced to `#[cfg(test)]` on purpose: it
    /// emits `version: 4`, above the `version: 3` this build writes and reads
    /// back. It also
    /// cannot carry `viewport` or `activeLayerId`, which `PtzDocumentContent`
    /// deliberately excludes as editor state, so a file written by it would
    /// silently reset the viewport and active layer on open. Wiring this into a
    /// save path would ship both. Kept compiling under test only so the shape it
    /// describes stays exercised; see the pending-deletion note in
    /// `ptz/file.rs`.
    ///
    /// RESOURCE HANDLE STRIPPING: a `ResourceId` is a session-scoped token handle.
    /// Persisting it would be a lie the next session cannot honor (the handle is
    /// minted fresh per session), so every layer's `resource_id` is normalized to
    /// `None` before serialization.
    #[cfg(test)]
    pub fn to_ptz_document_json(&self) -> Result<String, String> {
        let mut normalized = self.clone();
        for layer in normalized.layers.iter_mut() {
            layer.resource_id = None;
        }
        let mut value = serde_json::to_value(&normalized).map_err(|e| e.to_string())?;
        if let serde_json::Value::Object(ref mut map) = value {
            map.insert(
                "format".to_string(),
                serde_json::Value::String("photrez-ptz".to_string()),
            );
            map.insert(
                "version".to_string(),
                serde_json::Value::Number(serde_json::Number::from(4u64)),
            );
        }
        serde_json::to_string(&value).map_err(|e| e.to_string())
    }
}
