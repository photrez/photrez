// SPDX-License-Identifier: AGPL-3.0-or-later
// The document model this crate exposes. Every type here is owned by the
// `photrez-document` package; the engine, its history stream, and the command
// dispatch that change them live in `engine`. Re-exported so `photrez-core` stays
// the single entry point the application imports.

pub use photrez_document::{content, identity, layer, layer_params};
