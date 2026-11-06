// SPDX-License-Identifier: AGPL-3.0-or-later
// The persisted `.ptz` model: the document content, the file envelope, the
// nested per-layer payloads, the render-side projection bridge, the derived
// document replica, the seed-payload validation, and the shared test fixtures.
pub mod ptz;
// The document engine: the authoritative `DocumentCore` state, its history
// stream, command dispatch, the structural and canvas command families, the
// structural-sharing layer set, and the document snapshot DTO. Public so its
// engine types reach the facade.
pub mod engine;
// The document model, owned by `photrez-document` and re-exported so callers keep
// one entry point.
pub mod document;
pub mod geometry;
pub mod parallel;
// Pixel ownership: the document-namespaced registry, the store that binds
// buffers to a document's history, its byte accounting, and the image payload
// codecs. The buffers, tiles, and kernels themselves are the raster primitives
// in `raster`.
pub mod pixel;
pub mod protocol;
// The raster primitives: the canonical pixel buffer, tiles, the copy-on-write
// state-node seam, the compute kernels, and the paint and brush geometry that
// prepares the stamps those kernels apply. They hold pixels, not document state.
pub use photrez_raster as raster;
// Render-side compositing: the CPU compositor entry point the native host
// renders through, plus the crate-private shape and text rasterizers. The layer
// metadata it draws lives in `document`.
pub use photrez_render as render;
pub(crate) mod wire;
