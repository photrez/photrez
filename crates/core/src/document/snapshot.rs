// SPDX-License-Identifier: AGPL-3.0-or-later
//! Rust-side atomic document snapshot DTO for undo/redo.
//!
//! A snapshot is a METADATA + bitmap-token transport, NOT a pixel store:
//!   - It carries per-layer pixel metadata (width/height/epoch/pixel_version)
//!     plus an OPAQUE `bitmap_token` referencing the host-owned raster.
//!   - Rust NEVER stores a raster or pixel bytes here; the token lets the
//!     host restore the correct raster after an undo/redo step, so a
//!     single `EntryPayload::Snapshot` entry restores BOTH metadata (this DTO)
//!     and pixel state (via the token reference) atomically.
//!
//! This is a RUST-CORE capability introduced with the migration flag OFF: the
//! row-major `PixelLayer` + TS `CommandHistory` remain the ACTIVE default. The
//! full document metadata (visible/opacity/blend/transform/selection/viewport)
//! is a later increment; this carries the pixel-layer metadata + token only.

use serde::{Deserialize, Serialize};

/// One layer's pixel metadata + opaque bitmap reference in a document snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LayerSnapshot {
    pub layer_id: String,
    pub width: u32,
    pub height: u32,
    /// Opaque reference to the host-owned raster. `None` when the layer has no
    /// pixel store yet (e.g. an empty layer). Rust never dereferences the token.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bitmap_token: Option<String>,
    /// Layer mutation counter (mirrors the canonical store's epoch).
    pub epoch: u64,
    /// Pixel-mutation counter (mirrors the layer's `epoch`; the Rust store has no
    /// separate pixel_version field yet — kept as a distinct field for the later
    /// metadata increment).
    pub pixel_version: u64,
}

impl LayerSnapshot {
    /// Construct a layer snapshot with no bitmap token and zero counters.
    pub fn new(layer_id: &str, width: u32, height: u32) -> Self {
        Self {
            layer_id: layer_id.to_string(),
            width,
            height,
            bitmap_token: None,
            epoch: 0,
            pixel_version: 0,
        }
    }

    /// Builder-style attach of an opaque bitmap token (the pixel reference).
    pub fn with_bitmap_token(mut self, token: impl Into<String>) -> Self {
        self.bitmap_token = Some(token.into());
        self
    }
}

/// A document's atomic metadata + bitmap-token snapshot for undo/redo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSnapshot {
    pub doc_id: String,
    pub version: u64,
    pub layers: Vec<LayerSnapshot>,
}

impl DocumentSnapshot {
    /// Construct an empty snapshot for `doc_id` at `version`.
    pub fn new(doc_id: &str, version: u64) -> Self {
        Self {
            doc_id: doc_id.to_string(),
            version,
            layers: Vec::new(),
        }
    }

    /// Builder-style append of a layer snapshot.
    pub fn with_layer(mut self, layer: LayerSnapshot) -> Self {
        self.layers.push(layer);
        self
    }
}
