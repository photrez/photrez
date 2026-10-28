// SPDX-License-Identifier: AGPL-3.0-or-later
// Persisted `.ptz` model bounded context.
//
// `content` is the document CONTENT: layers, transforms, text, shape params, and
// adjustments. `file` is the `.ptz` `document.json` ENVELOPE: the format
// header, the session fields the loader restores, and the per-layer bitmap keys
// that must stay null. `layer_params` holds the nested per-layer payloads, and
// `fixtures` holds the shared test documents.
//
// `projection` bridges the persisted layer model to the render-side layer
// value, `replica` holds the derived per-document replica the engine
// reconciles against, and `validate` parses the native seed payload.
//
// The content model is on the save path: `file::PtzDocument` wraps it.

pub mod content;
pub mod file;
pub mod layer_params;
pub mod projection;
pub mod replica;
pub mod validate;
// Real `.ptz` `document.json` fixtures, shared by the core writer tests and the
// desktop command tests so the two round-trip proofs cannot drift apart.
// Test-only: compiled out of the shipped library unless the feature is on.
#[cfg(any(test, feature = "ptz-test-fixtures"))]
pub mod fixtures;

pub use content::*;
pub use file::PtzDocument;

#[cfg(test)]
mod tests;
