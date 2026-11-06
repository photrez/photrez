// SPDX-License-Identifier: AGPL-3.0-or-later
//! The canonical Photrez document model: the persisted layer content, the
//! nested per-layer parameters, the layer metadata value the engine stores in
//! history, and the layer identity type.
//!
//! This crate owns no active document state. `photrez-core` owns the engine,
//! the history stream, the pixel registry, and command dispatch, and re-exports
//! these modules so callers keep one entry point. It has no dependency on any
//! other package in the workspace.
//!
//! The serialized field names here are the `.ptz` on-disk contract. A change
//! must be additive and ship a migrator; see `docs/ptz-migration.md`.

pub mod content;
pub mod identity;
pub mod layer;
pub mod layer_params;
