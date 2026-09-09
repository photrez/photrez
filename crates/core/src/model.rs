// SPDX-License-Identifier: AGPL-3.0-or-later
// Layer metadata model: RenderLayer / LayerMeta / LayerSet (a structural-sharing
// snapshot of the layer-metadata set) + RenderLayerChange.

use crate::canonical_model::{BasicAdjustment, BlendMode, LayerType, ShapeParams, TextData};
use crate::command::ResourceId;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RenderLayer {
    pub id: String,
    pub name: String,
    pub visible: bool,
    pub opacity: f64,
    pub resource_id: ResourceId,
    pub x: f64,
    pub y: f64,
    pub scale_x: f64,
    pub scale_y: f64,
    pub rotation: f64,
    pub dirty_rect: Option<Rect>,
    // Canonical metadata subset (Option so old-shape envelopes still parse and
    // so the engine can carry only the fields an add/merge op sets). These
    // mirror the canonical model's string-enum + bool + f64 fields verbatim;
    // `width`/`height` are the new layer's dimensions when set by an add op.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer_type: Option<LayerType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blend_mode: Option<BlendMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_transparency: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_position: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lock_rotation: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_background: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_adjustments: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    // Horizontal/vertical flip flags (mirror of Transform2D.flipH/flipV). Optional
    // so an add/upsert envelope without them parses; when set by the TransformLayer
    // arm they project onto the shadow (D-b: Some(v) overrides the canonical base).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flip_h: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flip_v: Option<bool>,
    // Nested parametric payloads (mirror of CanonicalLayer.shapeParams/textData/
    // basicAdjustment). Optional so v2 envelopes without them still parse. The
    // typed-add / SetLayerParams / SetAdjustment arms set these; the merge bridge
    // takes a Some (Some(v) overrides the canonical base, None preserves it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape_params: Option<ShapeParams>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_data: Option<TextData>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub basic_adjustment: Option<BasicAdjustment>,
}
/// The immutable per-layer metadata value held by the COW layer set. It is the
/// same data as `RenderLayer` (kept under a distinct name so the structural-
/// sharing set can carry `Arc<LayerMeta>` without conflating with the serde DTO
/// that crosses the wasm front). Values are treated as immutable: editing a
/// layer produces a NEW `LayerMeta` via COW, never an in-place mutation of a
/// shared one (this mirrors the `StateNode`/`TileBlock` COW convention).
pub type LayerMeta = RenderLayer;
/// Structural-sharing snapshot of the layer metadata set. The backing `Vec` is
/// heap-allocated and only ever replaced wholesale (never mutated in place);
/// each element is an `Arc<LayerMeta>` so layers that did NOT change between a
/// before/after transition SHARE the same `Arc<LayerMeta>` pointer. Building a
/// new `LayerSet` after a change is O(changed) deep work + O(n) cheap pointer
/// copies; undo/redo swap `self.layers = before.clone()` in O(1) because a
/// `LayerSet` is itself an `Arc` (clone bumps the refcount only).
#[derive(Debug, Clone)]
pub(crate) struct LayerSet(pub(crate) Arc<Vec<Arc<LayerMeta>>>);

impl LayerSet {
    pub(crate) fn empty() -> Self {
        Self(Arc::new(Vec::new()))
    }

    /// Immutable access to the layer at `i` (derefs the shared `Arc<LayerMeta>`).
    pub(crate) fn get(&self, i: usize) -> Option<&LayerMeta> {
        self.0.get(i).map(|a| a.as_ref())
    }

    /// Iterate the shared `Arc<LayerMeta>` pointers (unchanged layers are
    /// identified by pointer identity across two sets).
    pub(crate) fn iter(&self) -> std::slice::Iter<'_, Arc<LayerMeta>> {
        self.0.iter()
    }

    pub(crate) fn position_by_id(&self, id: &str) -> Option<usize> {
        self.0.iter().position(|l| l.id == id)
    }

    // ── COW builders ─────────────────────────────────────────────
    // Each returns a NEW set sharing the untouched `Arc<LayerMeta>` pointers;
    // only the vector of pointers is copied (O(n) refcount bumps, no deep clone
    // of unchanged layer data).
    pub(crate) fn pushed(&self, layer: LayerMeta) -> Self {
        let mut v = self.0.as_ref().clone();
        v.push(Arc::new(layer));
        Self(Arc::new(v))
    }

    pub(crate) fn replaced(&self, i: usize, layer: LayerMeta) -> Self {
        let mut v = self.0.as_ref().clone();
        v[i] = Arc::new(layer);
        Self(Arc::new(v))
    }

    pub(crate) fn removed(&self, i: usize) -> Self {
        let mut v = self.0.as_ref().clone();
        v.remove(i);
        Self(Arc::new(v))
    }

    /// Insert at `index` (clamped to `0..=len`), preserving the shared
    /// `Arc<LayerMeta>` pointers of every untouched layer (only the inserted
    /// layer is a fresh Arc). Used by the `AddLayer` arm so the host-supplied
    /// insertion position (TS "above active") is honored.
    pub(crate) fn insert_at(&self, layer: LayerMeta, index: usize) -> Self {
        let mut v = self.0.as_ref().clone();
        let idx = index.min(v.len());
        v.insert(idx, Arc::new(layer));
        Self(Arc::new(v))
    }

    /// Build a `LayerSet` from an EXPLICIT list of layer metadata. Ids are taken
    /// verbatim from the input — the caller owns id assignment upstream (e.g. the
    /// TS model that already has canonical layer ids). This is the initial-layer-
    /// load path and must NOT mint fresh uuids, unlike the `AddLayer` command arm
    /// which generates ids. Every layer becomes its own `Arc<LayerMeta>` (COW-
    /// ready): later edits produce new `Arc`s, unchanged layers are shared.
    pub(crate) fn from_layers(layers: Vec<LayerMeta>) -> Self {
        Self(Arc::new(layers.into_iter().map(Arc::new).collect()))
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[allow(clippy::large_enum_variant)] // Upsert carries the full RenderLayer by value for O(1) history swap
pub enum RenderLayerChange {
    Upsert {
        layer: RenderLayer,
    },
    // DeleteLayer: carries resourceId so a future Resource
    // Registry can drive lifecycle (release/retain) WITHOUT re-owning pixels.
    #[serde(rename_all = "camelCase")]
    Remove {
        id: String,
        resource_id: ResourceId,
    },
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod wire_tests;
