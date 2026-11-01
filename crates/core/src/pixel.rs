// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pixel ownership: the subsystem's public surface.
//!
//! The pixel subsystem owns the canonical per-layer pixel buffers, the
//! per-layer `StateNode` canon, the packed tile-major store, the independent
//! reference copy used by tests, and the native compute kernels over RGBA
//! planes.
//!
//! `pixel::store` is the document-namespaced registry the host drives through
//! its pixel lifecycle commands. `pixel::document_store` is the only place that
//! binds the pixel buffers to a document's history stream, so the composition
//! edge between the two subsystems stays one-way.

pub mod byte_accounting;
#[cfg(feature = "legacy-host")]
pub mod document_store;
#[cfg(not(feature = "legacy-host"))]
pub(crate) mod document_store;
pub mod engine;
pub mod export;
pub mod history;
pub mod kernel;
pub mod layer;
pub mod store;
pub mod tile;

// The per-layer canonical COW seam and the packed tile-major store are always
// compiled in (no feature gate): the StateNode/TileRef/LayerState data model
// and the packed `TileStore` are production modules used by the document
// engine (Arc<StateNode> pixel history) and by the canonical copy-on-write
// seam. The independent-copy reference module stays test-only.
pub(crate) mod state_node;
pub(crate) mod tile_store;

#[cfg(test)]
pub(crate) mod reference;

pub use byte_accounting::{PixelStoreBytes, TileByteReport};
#[cfg(feature = "legacy-host")]
pub use document_store::DocumentPixelStore;
#[cfg(not(feature = "legacy-host"))]
pub(crate) use document_store::DocumentPixelStore;
pub use history::{HistoryDepth, HistoryTip, PayloadKind};
pub use layer::PixelLayer;
#[cfg(feature = "legacy-host")]
pub use store::registry;
#[cfg(all(not(feature = "legacy-host"), test))]
pub(crate) use store::registry;
pub use store::CommitPhases;
pub use store::PixelStoreRegistry;
pub use tile::TilePatch;

#[cfg(test)]
mod tests;
