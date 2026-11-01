// SPDX-License-Identifier: AGPL-3.0-or-later
// Document-owned layer state: the per-layer metadata value and the
// structural-sharing `LayerSet` that holds it. The set is the undo/redo
// substrate: unchanged layers SHARE their `Arc` pointer across a before/after
// pair, so a history swap is an O(1) `Arc` clone.

use crate::render::layer::RenderLayer;
use std::sync::Arc;
/// The immutable per-layer metadata value held by the COW layer set. It is the
/// same data as `RenderLayer` (kept under a distinct name so the structural-
/// sharing set can carry `Arc<LayerMeta>` without conflating with the serde DTO
/// that crosses the engine/host boundary). Values are treated as immutable: editing a
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
    /// insertion position ("above the active layer") is honored.
    pub(crate) fn insert_at(&self, layer: LayerMeta, index: usize) -> Self {
        let mut v = self.0.as_ref().clone();
        let idx = index.min(v.len());
        v.insert(idx, Arc::new(layer));
        Self(Arc::new(v))
    }

    /// Build a `LayerSet` from an EXPLICIT list of layer metadata. Ids are taken
    /// verbatim from the input — the caller owns id assignment upstream (e.g. the
    /// host document model that already has canonical layer ids). This is the initial-layer-
    /// load path and must NOT mint fresh uuids, unlike the `AddLayer` command arm
    /// which generates ids. Every layer becomes its own `Arc<LayerMeta>` (COW-
    /// ready): later edits produce new `Arc`s, unchanged layers are shared.
    pub(crate) fn from_layers(layers: Vec<LayerMeta>) -> Self {
        Self(Arc::new(layers.into_iter().map(Arc::new).collect()))
    }
}
