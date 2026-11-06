// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pixel ownership: the document-namespaced registry and what binds pixels to a
//! document.
//!
//! `pixel::store` is the registry the host drives through its pixel lifecycle
//! commands. `pixel::document_store` is the only place that binds the pixel
//! buffers to a document's history stream, so the composition edge between the
//! two subsystems stays one-way. `pixel::byte_accounting` prices a store against
//! its history payloads, and `pixel::export` encodes and decodes the image
//! payloads the host saves.
//!
//! The buffer, tile, state-node, and kernel types these modules read are the
//! raster primitives in `crate::raster`.

pub mod byte_accounting;
#[cfg(feature = "legacy-host")]
pub mod document_store;
#[cfg(not(feature = "legacy-host"))]
pub(crate) mod document_store;
pub mod history;
pub mod store;

// The image payload codecs are owned by `photrez-format`; they work on byte
// slices and hold no document state.
pub use photrez_format::export;

pub use byte_accounting::{PixelStoreBytes, TileByteReport};
#[cfg(feature = "legacy-host")]
pub use document_store::DocumentPixelStore;
#[cfg(not(feature = "legacy-host"))]
pub(crate) use document_store::DocumentPixelStore;
pub use history::{HistoryDepth, HistoryTip, PayloadKind};
#[cfg(feature = "legacy-host")]
pub use store::registry;
#[cfg(all(not(feature = "legacy-host"), test))]
pub(crate) use store::registry;
pub use store::CommitPhases;
pub use store::PixelStoreRegistry;

#[cfg(test)]
mod tests;
