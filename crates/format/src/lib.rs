// SPDX-License-Identifier: AGPL-3.0-or-later
//! The `.ptz` file contract: the `document.json` envelope, its writer and
//! reader, the seed-payload validation, the shared test fixtures, and the image
//! payload codecs.
//!
//! The content model this format serializes is owned by `photrez-document`; the
//! envelope carries the format header, the session fields the loader restores,
//! and the per-layer bitmap keys that must stay null. Active document state,
//! history, and the loader workflow stay in `photrez-core` and the application:
//! this package turns bytes into a model and back, and nothing else.
//!
//! Version 3 is the production writer. A format change must be additive and
//! ship a migrator; see `docs/ptz-migration.md`.

pub mod export;
pub mod file;
pub mod validate;

#[cfg(any(test, feature = "ptz-test-fixtures"))]
pub mod fixtures;

pub use file::PtzDocument;

#[cfg(test)]
mod tests;
