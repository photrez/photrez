// SPDX-License-Identifier: AGPL-3.0-or-later
pub mod engine;
pub mod export;
pub mod kernel;
// Technique A: Rust/WASM owning a WebGPU compute pipeline inside the webview.
pub mod webgpu_adjust;
