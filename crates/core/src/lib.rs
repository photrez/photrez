// SPDX-License-Identifier: AGPL-3.0-or-later
pub mod engine;
pub mod export;
pub mod kernel;
// Rust/WASM owns a WebGPU compute pipeline inside the webview.
pub mod brush_engine;
pub mod canonical_tip;
pub mod document;
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
pub mod render_worker;
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
mod parity_oracle;
pub(crate) mod state_node;
pub(crate) mod tile_store;
