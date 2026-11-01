// SPDX-License-Identifier: AGPL-3.0-or-later
// The write/serialize half of the `.ptz` file envelope: the on-disk header and
// the per-layer bitmap keys that must stay null.

use super::PtzDocument;

/// The `.ptz` container format marker. `format: "photrez-ptz"` in `document.json`.
const PTZ_FORMAT: &str = "photrez-ptz";

/// The `document.json` schema version this build writes. v3 carries text and
/// shape layer metadata (they ride the layer spread). Raising it is a document
/// format change: the loader and the tracked `.ptz` fixtures have to move with
/// it in the same step, never silently.
///
/// There is a second `.ptz` writer in this crate:
/// `PtzDocumentContent::to_ptz_document_json` in `ptz/content.rs` emits
/// `version: 4` and carries neither `viewport` nor `activeLayerId`, so a file
/// it wrote would open with the view and active layer reset and outside the
/// version this build writes. It is fenced to
/// `#[cfg(test)]`. Deleting it is a PENDING DECISION, deliberately not taken
/// in the change that added this module. Do not wire it into a save path.
const PTZ_VERSION: u32 = 3;
impl PtzDocument {
    /// Serialize to the exact `document.json` bytes written into the ZIP.
    ///
    /// Appends the `photrez-ptz` / version header and the two per-layer
    /// bitmap keys. Both bitmap keys are written as a constant `null` and any
    /// value the host sent for them is discarded: the host holds live
    /// `ImageBitmap` objects there, and pixels belong in the ZIP
    /// (`layers/<id>.png`), never in the JSON. Writing the constant also stops
    /// a live bitmap from reaching `JSON.stringify`, which would serialize it
    /// to `{}` and make the loader treat it as a usable base image.
    pub fn to_json(&self) -> Result<String, String> {
        let mut value = serde_json::to_value(self).map_err(|e| e.to_string())?;
        if let serde_json::Value::Object(ref mut map) = value {
            map.insert(
                "format".to_string(),
                serde_json::Value::String(PTZ_FORMAT.to_string()),
            );
            map.insert(
                "version".to_string(),
                serde_json::Value::Number(serde_json::Number::from(PTZ_VERSION)),
            );
            if let Some(serde_json::Value::Array(ref mut layers)) = map.get_mut("layers") {
                for layer in layers.iter_mut() {
                    if let serde_json::Value::Object(ref mut layer_map) = layer {
                        layer_map.insert("imageBitmap".to_string(), serde_json::Value::Null);
                        layer_map.insert("baseImageBitmap".to_string(), serde_json::Value::Null);
                    }
                }
            }
        }
        value.sort_all_objects();
        serde_json::to_string(&value).map_err(|e| e.to_string())
    }
}
