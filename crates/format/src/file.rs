// SPDX-License-Identifier: AGPL-3.0-or-later
// The `.ptz` `document.json` payload: the typed document Rust WRITES on save.
//
// `document::content` holds the document CONTENT model (layers, transforms,
// text, adjustments). This module holds everything the file needs on top of
// that: the `photrez-ptz` / version header, the session fields the loader
// restores, and the two per-layer bitmap keys that must stay null.
//
// PRODUCTION PATH: the app's document service (`crates/photrez/src/services/
// document.rs`) builds a `PtzDocument` from the engine content, serializes it
// HERE, and hands the bytes to the save stream
// (`crates/photrez/src/services/save_stream.rs`), which writes them into the
// ZIP. Nothing else assembles the save payload.
//
// WHY THE SESSION FIELDS LIVE HERE AND NOT IN `document::content`:
// `activeLayerId` / `viewport` / `dirty` are editor state, not document
// content, so they are deliberately not part of `PtzDocumentContent`. They are
// still part of the FILE, because opening a document restores the viewport
// from the saved file. Keeping them in a
// file-payload type (not the content model) is what lets the content model
// stay free of view state without the file losing it.
//
// BITMAP KEYS: the two per-layer bitmap-reference keys that `write.rs` appends
// are always written as
// `null`. Pixels live in the ZIP as `layers/<id>.png`, never in the JSON.
// They are emitted by the `write` half rather than modelled as fields because
// the engine holds live pixel resources in those slots, which are not JSON and
// must not be written into the document.

use photrez_document::content::{PtzLayer, SelectionState};
use serde::{Deserialize, Serialize};

// The read/parse half and the write/serialize half of the envelope.
mod read;
mod write;

#[cfg(test)]
use photrez_document::content::PtzDocumentContent;

/// Viewport state as persisted in `document.json` (camelCase on the wire).
///
/// Defaults to identity at 100% zoom so a hand-edited or pre-viewport file
/// loads at a usable zoom instead of an empty viewport, which the
/// renderer would turn into NaN geometry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewportState {
    pub pan_x: f64,
    pub pan_y: f64,
    pub zoom: f64,
    pub rotation: f64,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            pan_x: 0.0,
            pan_y: 0.0,
            zoom: 1.0,
            rotation: 0.0,
        }
    }
}

/// The `.ptz` `document.json` payload as written to disk.
///
/// `format` / `version` and the per-layer bitmap keys are not fields: they are
/// appended by `to_json`, which is the only place the on-disk header is built.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PtzDocument {
    pub id: String,
    pub name: String,
    pub width: f64,
    pub height: f64,
    pub layers: Vec<PtzLayer>,
    /// `None` (written `null`) when no layer is active. Restored by the loader.
    #[serde(default)]
    pub active_layer_id: Option<String>,
    #[serde(default)]
    pub selection: Option<SelectionState>,
    /// Restored by the loader (`restoreViewport: true` on project open).
    #[serde(default)]
    pub viewport: ViewportState,
    /// Save-state flag. The loader clears it after restore, so it is written
    /// for fidelity with the previous writer rather than load-bearing.
    #[serde(default)]
    pub dirty: bool,
}

impl PtzDocument {
    /// The document CONTENT, dropping the session/transport fields.
    #[cfg(test)]
    pub fn to_canonical(&self) -> PtzDocumentContent {
        PtzDocumentContent {
            id: self.id.clone(),
            name: self.name.clone(),
            width: self.width,
            height: self.height,
            layers: self.layers.clone(),
            selection: self.selection.clone(),
        }
    }
}
