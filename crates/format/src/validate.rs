// SPDX-License-Identifier: AGPL-3.0-or-later
// Validation surface for the native seed payload contract: confirms `json`
// parses into a `PtzDocumentContent` (the exact canonical document payload
// shape this build pushes on restore). Returns an empty string on success or
// the serde error message on failure. Pure parse-check only - it performs no
// persistence and stores nothing. Shared by native callers and the native test.

use photrez_document::content::PtzDocumentContent;

pub fn validate_payload(json: &str) -> String {
    match serde_json::from_str::<PtzDocumentContent>(json) {
        Ok(_) => String::new(),
        Err(e) => e.to_string(),
    }
}
