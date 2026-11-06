// SPDX-License-Identifier: AGPL-3.0-or-later
//! Raster primitives: the canonical per-layer pixel buffer, the packed
//! tile-major store, the per-layer copy-on-write `StateNode` seam, the compute
//! kernels, the paint rasterizer and brush geometry that prepare a stamp, and
//! the independent reference copy the tests compare against.
//!
//! These types hold pixels and no document state. No command, history entry, or
//! document identity reaches this package: `photrez-core` owns the registry, the
//! document store, and the byte accounting that bind them, and core may use
//! `photrez-raster`, never the reverse. The application may not depend on this
//! package: `scripts/check-package-boundaries.sh` keeps `photrez` on `photrez-core`.

pub mod brush;
pub mod kernel;
pub mod layer;
pub mod paint;
pub mod tile;

pub(crate) mod tile_store;

// The per-layer canonical COW seam is always compiled in (no feature gate): the
// StateNode/TileRef/LayerState data model and the packed `TileStore` are
// production modules used by the document engine (Arc<StateNode> pixel history).
pub mod state_node;

// The independent-copy reference module exists for tests, not for a production
// path. `photrez-core` enables `test-reference` from its dev-dependencies to
// check the packed canon against a copy that shares no code with it; the
// shipped build compiles it out.
#[cfg(any(test, feature = "test-reference"))]
pub mod reference;

pub use layer::PixelLayer;

#[cfg(test)]
mod tests;
