// SPDX-License-Identifier: AGPL-3.0-or-later
// The `.ptz` surface this crate exposes. The envelope, its writer and reader, the
// seed-payload validation, and the shared test fixtures are owned by
// `photrez-format` and re-exported here, so callers keep one entry point.
//
// This module holds no production code. The projection and replica that used to sit
// here bind file bytes to live engine state, so they live in `engine`.

pub use photrez_format::file::PtzDocument;
pub use photrez_format::{file, validate};
// Kept for a host that enabled this feature while the fixtures lived here. No
// test in this crate uses them: they moved with the writer to `photrez-format`.
#[cfg(feature = "ptz-test-fixtures")]
pub use photrez_format::fixtures;
