// SPDX-License-Identifier: AGPL-3.0-or-later
// The read/parse half of the `.ptz` file envelope.

use super::PtzDocument;

impl PtzDocument {
    /// Parse a `document.json` payload into the typed file model.
    ///
    /// Unknown keys are ignored (a newer build's extra fields), and absent
    /// optional fields take their defaults, so a v1/v2/v3 file all parse.
    /// Malformed JSON or an out-of-contract field type returns `Err` -- the
    /// save then fails before any file is created rather than writing a
    /// document that cannot be read back.
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| e.to_string())
    }
}
