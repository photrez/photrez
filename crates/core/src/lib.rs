// SPDX-License-Identifier: AGPL-3.0-or-later
pub mod engine;
pub mod export;
pub mod kernel;
// Rust/WASM owns a WebGPU compute pipeline inside the webview.
pub mod brush_engine;
// Typed canonical document model. On the save path via `ptz_document`, which
// wraps these types in the on-disk `.ptz` `document.json` payload.
pub mod canonical_model;
// The nested per-layer payloads of that model (adjustment / shape / text).
pub mod canonical_layer_params;
// The `.ptz` `document.json` payload Rust writes on save (the production save path).
pub mod ptz_document;
// Real `.ptz` `document.json` fixtures, shared by the core writer tests and the
// desktop command tests so the two round-trip proofs cannot drift apart.
// Test-only: compiled out of the shipped library unless the feature is on.
#[cfg(any(test, feature = "ptz-test-fixtures"))]
pub mod ptz_fixtures;
// RenderLayer <-> CanonicalLayer value bridge + seed-payload validation (additive, unwired).
pub mod canonical_bridge;
pub mod canonical_tip;
pub mod document;
pub(crate) mod document_dup;
pub mod geometry;
pub mod paint_bench;
pub mod paint_parity;
pub mod paint_parity_r15;
// Rust canonical pixel buffer + delta history (active layer only).
pub(crate) mod command;
pub(crate) mod document_core;
pub(crate) mod history;
pub(crate) mod model;
pub mod parallel;
pub mod pixel_store;
pub(crate) mod projection;
pub mod protocol;
pub mod rkyv_bench;
pub mod selection;
pub mod snapshot;
pub mod webgpu_adjust;

// Canonical persistent-pixel-ownership data model + tile-major store +
// independent-copy parity oracle. `state_node` and `tile_store` are always
// compiled in (no feature gate): the StateNode/TileRef/LayerState data model +
// the packed tile-major `TileStore` are production `pub(crate)` modules used by
// `protocol.rs` (Arc<StateNode> pixel history) and `pixel_store.rs` (the
// canonical packed copy-on-write seam). Their internal logical/parity tests
// remain `#[cfg(test)]`. The independent-copy `parity_oracle` stays test-only
// (referenced only by `#[cfg(test)]` helpers). History/command/model/projection
// types for the protocol engine live in `document_core.rs` / `history.rs` /
// `command.rs` / `model.rs` / `projection.rs` (the former `protocol.rs`).
#[cfg(test)]
mod canonical_shadow_tests;
#[cfg(test)]
mod parity_oracle;
#[cfg(test)]
mod ptz_save_perf;
#[cfg(test)]
mod ptz_writer_golden;
pub(crate) mod state_node;
pub(crate) mod tile_store;
