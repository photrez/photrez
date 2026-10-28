// SPDX-License-Identifier: AGPL-3.0-or-later
// Brush geometry: the stroke dab producer and the brush-tip builder.
pub mod brush;
// The persisted `.ptz` model: the document content, the file envelope, the
// nested per-layer payloads, the render-side projection bridge, the derived
// document replica, the seed-payload validation, and the shared test fixtures.
pub mod ptz;
// Runtime document bounded context: the `DocumentCore` engine, its history
// stream, and the document snapshot DTO. Public because `src-tauri` reads
// `photrez_core::document::{DocumentSnapshot, LayerSnapshot}`.
pub mod document;
pub mod geometry;
// The paint parity candidate rasterizer (the reference the host's paint path
// is compared against).
pub mod paint;
pub mod parallel;
// Pixel ownership: the authoritative per-layer buffers, the per-layer
// `StateNode` store, the packed tile-major store, the native compute kernels,
// and the document-namespaced registry the host drives.
pub mod pixel;
pub mod protocol;
// Render-side layer DTOs, the CPU compositor facade the native host renders
// through, and the crate-internal wire command/projection types. The wire
// module stays crate-private; `render::compositor` is the justified public
// document-to-pixels entry point.
pub mod render;
pub(crate) mod wire;
