// SPDX-License-Identifier: AGPL-3.0-or-later
pub mod engine;
pub mod export;
pub mod kernel;
// Technique A: Rust/WASM owning a WebGPU compute pipeline inside the webview.
pub mod brush_engine;
pub mod canonical_tip;
pub mod document;
pub mod geometry;
pub mod history;
pub mod paint_bench;
pub mod paint_parity;
pub mod paint_parity_r15;
// C4 pilot: Rust canonical pixel buffer + Model B delta history (active layer only).
pub mod parallel;
pub mod pixel_store;
pub mod protocol;
pub mod render_worker;
pub mod rkyv_bench;
pub mod selection;
pub mod webgpu_adjust;

// Phase A0 (approved design RESPONSE.md §4-§6): canonical persistent-pixel-
// ownership data model + tile-major store + independent-copy parity oracle.
// Test-only: compiled ONLY during `cargo test`, NEVER in the production
// (wasm/native) build. Zero production path / wiring.
#[cfg(test)]
mod parity_oracle;
#[cfg(test)]
mod state_node;
#[cfg(test)]
mod tile_store;
