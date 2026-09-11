// SPDX-License-Identifier: AGPL-3.0-or-later
// Structural-command-arm parity tests: Duplicate / MergeDown / MergeSelected /
// Flatten / RasterizeLayer. These mirror the TS oracle (layerOps) and the document
// graph mirror (document.rs). Each arm is a native undoable graph transition
// (begin_forward / finish_forward) that emits Removes for vanished ids followed by
// an Upsert of every surviving / new layer in final engine order. The implementation
// lives in document_core_structural.rs; this file is the ratchet that pins the
// current contract so a regression in any arm is caught.
use super::arm_tests::{env, layer, shape_params, text_data};
use crate::canonical_model::{
    BlendMode, CanonicalDocument, CanonicalLayer, LayerType, Transform2D,
};
use crate::command::*;
use crate::document_core::ProtocolEngine;
use crate::model::{RenderLayer, RenderLayerChange};
use crate::pixel_store::registry as pixel_registry;
use crate::projection::RenderSnapshot;

/// Canonical shadow with known dims (10x10, empty layer set) so the merge/flatten
/// arms can read the canonical shadow for document width/height (they reject
/// without it).
fn canonical_dim_doc() -> CanonicalDocument {
    CanonicalDocument {
        id: "dim-doc".into(),
        name: "D".into(),
        width: 10.0,
        height: 10.0,
        layers: vec![],
        selection: None,
    }
}

/// Build a layer from the shared `layer()` helper with explicit id/name/resource id.
fn mk(id: &str, name: &str, rid: u32) -> RenderLayer {
    let mut l = layer();
    l.id = id.into();
    l.name = name.into();
    l.resource_id = rid;
    l
}

/// Stack order of the current snapshot (top first).
fn ids_of(e: &ProtocolEngine) -> Vec<String> {
    e.snapshot().layers.iter().map(|l| l.id.clone()).collect()
}

/// Fetch one layer from the current snapshot by id.
fn layer_by(e: &ProtocolEngine, id: &str) -> RenderLayer {
    e.snapshot()
        .layers
        .into_iter()
        .find(|l| l.id == id)
        .expect("layer present")
}

// -- DuplicateLayer --------------------------------------------------------

#[test]
fn duplicate_layer_clones_above_source_and_mints_resource() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk("A", "Layer 1", 10)], 0);
    let result = e
        .apply(env(Command::DuplicateLayer {
            id: "A".into(),
            new_id: "A2".into(),
        }))
        .unwrap();
    let changes = &result.delta.changes;
    // Clone is inserted directly above the source (index 0 = top of stack).
    assert_eq!(ids_of(&e), vec!["A2", "A"]);
    let clone = layer_by(&e, "A2");
    // resource id is freshly minted from next_resource (10 -> 11 after seeding).
    assert_eq!(clone.resource_id, 11);
    // name follows the numeric-suffix rule ("Layer 1" -> "Layer 2").
    assert_eq!(clone.name, "Layer 2");
    // The source keeps its original resource id.
    assert_eq!(layer_by(&e, "A").resource_id, 10);
    // No removes; every layer is upserted in final order.
    assert!(changes
        .iter()
        .all(|c| matches!(c, RenderLayerChange::Upsert { .. })));
    assert_eq!(changes.len(), 2);
}

#[test]
fn duplicate_layer_derives_sequence_across_existing_numbers() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk("A", "Layer 1", 10), mk("B", "Layer 2", 20)], 0);
    e.apply(env(Command::DuplicateLayer {
        id: "A".into(),
        new_id: "A3".into(),
    }))
    .unwrap();
    // With "Layer 1" and "Layer 2" present, the next duplicate is "Layer 3".
    assert_eq!(layer_by(&e, "A3").name, "Layer 3");
}

#[test]
fn duplicate_layer_does_not_inherit_background_or_locks() {
    let mut e = ProtocolEngine::new();
    let mut src = mk("A", "Layer 1", 10);
    src.is_background = Some(true);
    src.locked = Some(true);
    src.lock_position = Some(true);
    src.lock_rotation = Some(true);
    src.lock_transparency = Some(true);
    e.seed_layers(vec![src], 0);
    e.apply(env(Command::DuplicateLayer {
        id: "A".into(),
        new_id: "A2".into(),
    }))
    .unwrap();
    let clone = layer_by(&e, "A2");
    assert_eq!(clone.is_background, None);
    assert_eq!(clone.locked, Some(false));
    assert_eq!(clone.lock_position, None);
    assert_eq!(clone.lock_rotation, None);
    assert_eq!(clone.lock_transparency, None);
}

#[test]
fn duplicate_layer_clones_full_surface_and_resets_locks() {
    // Divergence ratchet: the clone carries the verbatim parametric
    // surface (type/visible/opacity/shape/text) but mints a fresh resource id and
    // drops background + all locks.
    let mut e = ProtocolEngine::new();
    let mut src = mk("A", "Layer 1", 10);
    src.layer_type = Some(LayerType::Shape);
    src.shape_params = Some(shape_params());
    src.text_data = Some(text_data());
    src.visible = false;
    src.opacity = 0.5;
    src.is_background = Some(true);
    src.locked = Some(true);
    e.seed_layers(vec![src], 0);
    e.apply(env(Command::DuplicateLayer {
        id: "A".into(),
        new_id: "A2".into(),
    }))
    .unwrap();
    let clone = layer_by(&e, "A2");
    assert_eq!(clone.resource_id, 11);
    assert_eq!(clone.layer_type, Some(LayerType::Shape));
    assert_eq!(clone.shape_params, Some(shape_params()));
    assert_eq!(clone.text_data, Some(text_data()));
    assert!(!clone.visible);
    assert_eq!(clone.opacity, 0.5);
    // locks / background are reset
    assert_eq!(clone.is_background, None);
    assert_eq!(clone.locked, Some(false));
    assert_eq!(clone.lock_position, None);
    // source is untouched
    let s = layer_by(&e, "A");
    assert_eq!(s.resource_id, 10);
    assert_eq!(s.is_background, Some(true));
}

#[test]
fn duplicate_layer_rejects_empty_new_id_e_invalid() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk("A", "Layer 1", 10)], 0);
    let before: RenderSnapshot = e.snapshot();
    let r = e.apply(env(Command::DuplicateLayer {
        id: "A".into(),
        new_id: "".into(),
    }));
    assert!(r.is_err(), "empty new_id must be rejected");
    assert_eq!(r.unwrap_err().code, "E_INVALID");
    assert_eq!(
        e.snapshot(),
        before,
        "rejected command must not mutate engine"
    );
}

#[test]
fn duplicate_layer_rejects_present_new_id_e_invalid() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk("A", "Layer 1", 10), mk("B", "Layer 2", 20)], 0);
    let before: RenderSnapshot = e.snapshot();
    let r = e.apply(env(Command::DuplicateLayer {
        id: "A".into(),
        new_id: "B".into(),
    }));
    assert!(r.is_err(), "duplicate of present new_id must be rejected");
    assert_eq!(r.unwrap_err().code, "E_INVALID");
    assert_eq!(
        e.snapshot(),
        before,
        "rejected command must not mutate engine"
    );
}

#[test]
fn duplicate_layer_unknown_source_is_noop() {
    // Divergence: the host oracle throws on an unknown source id; the native arm
    // no-ops (the host owns identity checks). Recorded, not asserted equal.
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk("A", "Layer 1", 10)], 0);
    let result = e
        .apply(env(Command::DuplicateLayer {
            id: "ZZZ".into(),
            new_id: "X".into(),
        }))
        .unwrap();
    assert_eq!(ids_of(&e), vec!["A"]);
    assert!(result.delta.changes.is_empty());
}

// -- MergeDown -------------------------------------------------------------

#[test]
fn merge_down_combines_top_bottom_named_and_inherits_bottom_blend() {
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canonical_dim_doc());
    e.seed_layers(vec![mk("A", "Layer 1", 10), mk("B", "Layer 2", 20)], 0);
    e.apply(env(Command::MergeDown {
        id: "A".into(),
        merged_id: "M".into(),
    }))
    .unwrap();
    // Only the merged node survives.
    assert_eq!(ids_of(&e), vec!["M"]);
    let m = layer_by(&e, "M");
    assert_eq!(m.name, "Layer 1 + Layer 2");
    assert_eq!(m.layer_type, Some(LayerType::Raster));
    // blend mode inherits the bottom layer's (None -> Normal default).
    assert_eq!(m.blend_mode, Some(BlendMode::Normal));
    // rgba dims come from the seeded canonical shadow (10x10).
    assert_eq!(m.width, Some(10.0));
    assert_eq!(m.height, Some(10.0));
}

#[test]
fn merge_down_merged_node_mints_fresh_resource_id() {
    // Fresh-resource-id ratchet: the merged node gets a fresh resource id distinct from both
    // sources (next_resource is minted, not copied).
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canonical_dim_doc());
    e.seed_layers(vec![mk("A", "Layer 1", 10), mk("B", "Layer 2", 20)], 0);
    e.apply(env(Command::MergeDown {
        id: "A".into(),
        merged_id: "M".into(),
    }))
    .unwrap();
    let m = layer_by(&e, "M");
    assert_eq!(m.resource_id, 21);
    assert_ne!(m.resource_id, 10);
    assert_ne!(m.resource_id, 20);
}

#[test]
fn merge_down_locked_derivation_either_source() {
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canonical_dim_doc());
    let mut a = mk("A", "Layer 1", 10);
    a.locked = Some(true);
    e.seed_layers(vec![a, mk("B", "Layer 2", 20)], 0);
    e.apply(env(Command::MergeDown {
        id: "A".into(),
        merged_id: "M".into(),
    }))
    .unwrap();
    assert_eq!(layer_by(&e, "M").locked, Some(true));
}

#[test]
fn merge_down_unknown_id_is_noop() {
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canonical_dim_doc());
    e.seed_layers(vec![mk("A", "Layer 1", 10), mk("B", "Layer 2", 20)], 0);
    e.apply(env(Command::MergeDown {
        id: "ZZZ".into(),
        merged_id: "M".into(),
    }))
    .unwrap();
    assert_eq!(ids_of(&e), vec!["A", "B"]);
}

#[test]
fn merge_down_bottommost_is_noop() {
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canonical_dim_doc());
    e.seed_layers(vec![mk("A", "Layer 1", 10), mk("B", "Layer 2", 20)], 0);
    // B is the bottom-most layer; nothing below it to merge into.
    e.apply(env(Command::MergeDown {
        id: "B".into(),
        merged_id: "M".into(),
    }))
    .unwrap();
    assert_eq!(ids_of(&e), vec!["A", "B"]);
}

#[test]
fn merge_down_with_bystanders_places_merged_between_x_and_y() {
    // Bystander-placement ratchet: a pair (A,B) inside a 4-layer stack merges into M at the pair's
    // original position, leaving the bystanders X (above) and Y (below) intact and
    // in place. The delta is a complete restatement: Remove(top), Remove(bottom),
    // then Upsert of every survivor in final stack order.
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canonical_dim_doc());
    e.seed_layers(
        vec![
            mk("X", "Layer X", 10),
            mk("A", "Layer 1", 20),
            mk("B", "Layer 2", 30),
            mk("Y", "Layer Y", 40),
        ],
        0,
    );
    let result = e
        .apply(env(Command::MergeDown {
            id: "A".into(),
            merged_id: "M".into(),
        }))
        .unwrap();
    let changes = &result.delta.changes;
    // Final stack: X, M, Y.
    assert_eq!(ids_of(&e), vec!["X", "M", "Y"]);
    let m = layer_by(&e, "M");
    assert_eq!(m.name, "Layer 1 + Layer 2");
    assert_eq!(m.resource_id, 41);
    // Delta restatement order: Remove(A), Remove(B), Upsert(X), Upsert(M), Upsert(Y).
    assert!(matches!(
        &changes[0],
        RenderLayerChange::Remove { id, .. } if id == "A"
    ));
    assert!(matches!(
        &changes[1],
        RenderLayerChange::Remove { id, .. } if id == "B"
    ));
    assert!(matches!(
        &changes[2],
        RenderLayerChange::Upsert { layer } if layer.id == "X"
    ));
    assert!(matches!(
        &changes[3],
        RenderLayerChange::Upsert { layer } if layer.id == "M"
    ));
    assert!(matches!(
        &changes[4],
        RenderLayerChange::Upsert { layer } if layer.id == "Y"
    ));
}

#[test]
fn merge_down_rejects_empty_and_present_merged_id() {
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canonical_dim_doc());
    e.seed_layers(vec![mk("A", "Layer 1", 10), mk("B", "Layer 2", 20)], 0);
    let before: RenderSnapshot = e.snapshot();
    let r1 = e.apply(env(Command::MergeDown {
        id: "A".into(),
        merged_id: "".into(),
    }));
    assert!(r1.is_err(), "empty merged_id must be rejected");
    assert_eq!(r1.unwrap_err().code, "E_INVALID");
    assert_eq!(
        e.snapshot(),
        before,
        "rejected merge must not mutate engine"
    );
    let r2 = e.apply(env(Command::MergeDown {
        id: "A".into(),
        merged_id: "A".into(),
    }));
    assert!(r2.is_err(), "present merged_id must be rejected");
    assert_eq!(r2.unwrap_err().code, "E_INVALID");
    assert_eq!(
        e.snapshot(),
        before,
        "rejected merge must not mutate engine"
    );
}

// -- MergeSelected ----------------------------------------------------------

#[test]
fn merge_selected_combines_two_named_a_plus_b() {
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canonical_dim_doc());
    e.seed_layers(vec![mk("A", "Layer 1", 10), mk("B", "Layer 2", 20)], 0);
    e.apply(env(Command::MergeSelected {
        ids: vec!["A".into(), "B".into()],
        merged_id: "M".into(),
    }))
    .unwrap();
    assert_eq!(ids_of(&e), vec!["M"]);
    let m = layer_by(&e, "M");
    assert_eq!(m.name, "Layer 1 + Layer 2");
    assert_eq!(m.layer_type, Some(LayerType::Raster));
    assert_eq!(m.blend_mode, Some(BlendMode::Normal));
}

#[test]
fn merge_selected_merged_node_mints_fresh_resource_id() {
    // Fresh-resource-id ratchet: merge-selected also mints a fresh resource id.
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canonical_dim_doc());
    e.seed_layers(
        vec![
            mk("A", "Layer 1", 10),
            mk("B", "Layer 2", 20),
            mk("C", "Layer 3", 30),
        ],
        0,
    );
    e.apply(env(Command::MergeSelected {
        ids: vec!["A".into(), "B".into(), "C".into()],
        merged_id: "M".into(),
    }))
    .unwrap();
    let m = layer_by(&e, "M");
    assert_eq!(m.resource_id, 31);
    assert_ne!(m.resource_id, 10);
    assert_ne!(m.resource_id, 20);
    assert_ne!(m.resource_id, 30);
}

#[test]
fn merge_selected_three_uses_plus_n_merged_label() {
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canonical_dim_doc());
    e.seed_layers(
        vec![
            mk("A", "Layer 1", 10),
            mk("B", "Layer 2", 20),
            mk("C", "Layer 3", 30),
        ],
        0,
    );
    e.apply(env(Command::MergeSelected {
        ids: vec!["A".into(), "B".into(), "C".into()],
        merged_id: "M".into(),
    }))
    .unwrap();
    assert_eq!(layer_by(&e, "M").name, "Layer 1 (+2 merged)");
}

#[test]
fn merge_selected_places_merged_at_highest_selected_position() {
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canonical_dim_doc());
    e.seed_layers(
        vec![
            mk("A", "Layer 1", 10),
            mk("B", "Layer 2", 20),
            mk("C", "Layer 3", 30),
        ],
        0,
    );
    // Select B and C (skip A); merged lands at the highest (lowest-index) selected
    // position, which is B's index.
    e.apply(env(Command::MergeSelected {
        ids: vec!["B".into(), "C".into()],
        merged_id: "M".into(),
    }))
    .unwrap();
    assert_eq!(ids_of(&e), vec!["A", "M"]);
}

#[test]
fn merge_selected_locked_when_any_selected_locked() {
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canonical_dim_doc());
    let mut b = mk("B", "Layer 2", 20);
    b.locked = Some(true);
    e.seed_layers(vec![mk("A", "Layer 1", 10), b], 0);
    e.apply(env(Command::MergeSelected {
        ids: vec!["A".into(), "B".into()],
        merged_id: "M".into(),
    }))
    .unwrap();
    assert_eq!(layer_by(&e, "M").locked, Some(true));
}

#[test]
fn merge_selected_noop_when_fewer_than_two_ids_or_matched() {
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canonical_dim_doc());
    e.seed_layers(vec![mk("A", "Layer 1", 10), mk("B", "Layer 2", 20)], 0);
    // Only one matched id -> silent no-op.
    e.apply(env(Command::MergeSelected {
        ids: vec!["A".into(), "ZZZ".into()],
        merged_id: "M".into(),
    }))
    .unwrap();
    assert_eq!(ids_of(&e), vec!["A", "B"]);
}

#[test]
fn merge_selected_rejects_empty_and_present_merged_id() {
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canonical_dim_doc());
    e.seed_layers(vec![mk("A", "Layer 1", 10), mk("B", "Layer 2", 20)], 0);
    let before: RenderSnapshot = e.snapshot();
    let r1 = e.apply(env(Command::MergeSelected {
        ids: vec!["A".into(), "B".into()],
        merged_id: "".into(),
    }));
    assert!(r1.is_err(), "empty merged_id must be rejected");
    assert_eq!(r1.unwrap_err().code, "E_INVALID");
    assert_eq!(
        e.snapshot(),
        before,
        "rejected merge must not mutate engine"
    );
    let r2 = e.apply(env(Command::MergeSelected {
        ids: vec!["A".into(), "B".into()],
        merged_id: "A".into(),
    }));
    assert!(r2.is_err(), "present merged_id must be rejected");
    assert_eq!(r2.unwrap_err().code, "E_INVALID");
    assert_eq!(
        e.snapshot(),
        before,
        "rejected merge must not mutate engine"
    );
}

// -- Flatten ----------------------------------------------------------------

#[test]
fn flatten_replaces_all_with_single_background_node() {
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canonical_dim_doc());
    e.seed_layers(
        vec![
            mk("A", "Layer 1", 10),
            mk("B", "Layer 2", 20),
            mk("C", "Layer 3", 30),
        ],
        0,
    );
    e.apply(env(Command::Flatten {
        merged_id: "BG".into(),
    }))
    .unwrap();
    assert_eq!(ids_of(&e), vec!["BG"]);
    let bg = layer_by(&e, "BG");
    assert_eq!(bg.name, "Background");
    assert_eq!(bg.layer_type, Some(LayerType::Raster));
    assert_eq!(bg.is_background, Some(true));
    // The background node carries the position/rotation locks a real background has,
    // but is not itself locked.
    assert_eq!(bg.lock_position, Some(true));
    assert_eq!(bg.lock_rotation, Some(true));
    assert_eq!(bg.locked, Some(false));
    assert_eq!(bg.width, Some(10.0));
    assert_eq!(bg.height, Some(10.0));
}

#[test]
fn flatten_merged_node_mints_fresh_resource_id() {
    // Fresh-resource-id ratchet: flatten mints a fresh resource id for the background node.
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canonical_dim_doc());
    e.seed_layers(
        vec![
            mk("A", "Layer 1", 10),
            mk("B", "Layer 2", 20),
            mk("C", "Layer 3", 30),
        ],
        0,
    );
    e.apply(env(Command::Flatten {
        merged_id: "BG".into(),
    }))
    .unwrap();
    assert_eq!(layer_by(&e, "BG").resource_id, 31);
}

#[test]
fn flatten_noop_when_single_layer() {
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canonical_dim_doc());
    e.seed_layers(vec![mk("A", "Layer 1", 10)], 0);
    let result = e
        .apply(env(Command::Flatten {
            merged_id: "BG".into(),
        }))
        .unwrap();
    assert_eq!(ids_of(&e), vec!["A"]);
    assert!(result.delta.changes.is_empty());
}

#[test]
fn flatten_rejects_empty_and_present_merged_id() {
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canonical_dim_doc());
    e.seed_layers(vec![mk("A", "Layer 1", 10), mk("B", "Layer 2", 20)], 0);
    let before: RenderSnapshot = e.snapshot();
    let r1 = e.apply(env(Command::Flatten {
        merged_id: "".into(),
    }));
    assert!(r1.is_err(), "empty merged_id must be rejected");
    assert_eq!(r1.unwrap_err().code, "E_INVALID");
    assert_eq!(
        e.snapshot(),
        before,
        "rejected flatten must not mutate engine"
    );
    let r2 = e.apply(env(Command::Flatten {
        merged_id: "A".into(),
    }));
    assert!(r2.is_err(), "present merged_id must be rejected");
    assert_eq!(r2.unwrap_err().code, "E_INVALID");
    assert_eq!(
        e.snapshot(),
        before,
        "rejected flatten must not mutate engine"
    );
}

#[test]
fn flatten_rejects_without_seeded_canonical_dims() {
    // The native arm reads the seeded canonical shadow for dims; without it the
    // command rejects rather than guessing.
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk("A", "Layer 1", 10), mk("B", "Layer 2", 20)], 0);
    let before: RenderSnapshot = e.snapshot();
    let r = e.apply(env(Command::Flatten {
        merged_id: "BG".into(),
    }));
    assert!(
        r.is_err(),
        "flatten without seeded canonical dims must be rejected"
    );
    assert_eq!(r.unwrap_err().code, "E_INVALID");
    assert_eq!(
        e.snapshot(),
        before,
        "rejected flatten must not mutate engine"
    );
}

// -- RasterizeLayer ---------------------------------------------------------

#[test]
fn rasterize_shape_drops_params_and_becomes_raster() {
    let mut e = ProtocolEngine::new();
    let mut s = mk("A", "Shape", 10);
    s.layer_type = Some(LayerType::Shape);
    s.shape_params = Some(shape_params());
    e.seed_layers(vec![s], 0);
    e.apply(env(Command::RasterizeLayer { id: "A".into() }))
        .unwrap();
    let r = layer_by(&e, "A");
    assert_eq!(r.layer_type, Some(LayerType::Raster));
    assert_eq!(r.shape_params, None);
}

#[test]
fn rasterize_text_drops_text_data() {
    let mut e = ProtocolEngine::new();
    let mut t = mk("A", "Text", 10);
    t.layer_type = Some(LayerType::Text);
    t.text_data = Some(text_data());
    e.seed_layers(vec![t], 0);
    e.apply(env(Command::RasterizeLayer { id: "A".into() }))
        .unwrap();
    let r = layer_by(&e, "A");
    assert_eq!(r.layer_type, Some(LayerType::Raster));
    assert_eq!(r.text_data, None);
}

#[test]
fn rasterize_noop_for_non_parametric_layer() {
    // A raster (non-parametric) layer is untouched by rasterize; the arm returns an
    // empty change vector (silent no-op, mirroring the host type guard).
    let mut e = ProtocolEngine::new();
    let mut s = mk("A", "Raster", 10);
    s.layer_type = Some(LayerType::Raster);
    e.seed_layers(vec![s], 0);
    let result = e
        .apply(env(Command::RasterizeLayer { id: "A".into() }))
        .unwrap();
    assert_eq!(ids_of(&e), vec!["A"]);
    assert_eq!(layer_by(&e, "A").layer_type, Some(LayerType::Raster));
    assert!(result.delta.changes.is_empty());
}

// Canonical shadow with a single layer "A" so duplicate-reconciliation can be
// checked: after Duplicate{id:"A", new_id:"A2"} the engine mints "A2" (which has
// no canonical counterpart), flagging the shadow incomplete while A's canonical
// data must survive the reconcile.
fn canonical_doc_with_a() -> CanonicalDocument {
    CanonicalDocument {
        id: "dim-doc-a".into(),
        name: "A".into(),
        width: 10.0,
        height: 10.0,
        layers: vec![CanonicalLayer {
            id: "A".into(),
            name: "Layer 1".into(),
            layer_type: LayerType::Raster,
            visible: true,
            opacity: 1.0,
            locked: false,
            is_background: None,
            lock_transparency: None,
            lock_position: None,
            lock_rotation: None,
            has_adjustments: None,
            basic_adjustment: None,
            resource_id: Some(10),
            blend_mode: BlendMode::Normal,
            transform: Transform2D {
                x: 0.0,
                y: 0.0,
                scale_x: 1.0,
                scale_y: 1.0,
                rotation: 0.0,
                flip_h: false,
                flip_v: false,
            },
            width: 10.0,
            height: 10.0,
            shape_params: None,
            text_data: None,
        }],
        selection: None,
    }
}

// M2: duplicate-reconciliation. The original survives; the engine-minted
// duplicate id is absent and flags the shadow incomplete.
#[test]
fn duplicate_mints_engine_layer_flagging_shadow_incomplete() {
    let mut e = ProtocolEngine::new();
    // seed_layers FIRST: the only-when-empty guard means a later seed_canonical
    // (which now up-projects the pushed layer vector) must find engine layer A
    // already present so it preserves the seeded resource_id 10 via up-projection.
    e.seed_layers(vec![mk("A", "Layer 1", 10)], 0);
    e.seed_canonical(canonical_doc_with_a());
    e.apply(env(Command::DuplicateLayer {
        id: "A".into(),
        new_id: "A2".into(),
    }))
    .unwrap();
    // Engine-minted duplicate id has no canonical counterpart -> shadow incomplete.
    assert!(
        e.canonical_incomplete(),
        "engine-minted duplicate flags shadow incomplete"
    );
    // Original layer A's canonical data survives the reconcile.
    let c = e.canonical().expect("shadow seeded");
    let a = c
        .layers
        .iter()
        .find(|l| l.id == "A")
        .expect("layer A survives in canonical shadow");
    assert_eq!(a.name, "Layer 1");
    assert_eq!(a.resource_id, Some(10));
    assert_eq!(a.width, 10.0);
    assert_eq!(a.height, 10.0);
}

// -- Pixel-store registry pin -------------------------------------------------
// Structural arms mutate only the engine's RenderLayer graph; they never reach
// into the pixel store. Removed source-layer buffers must therefore survive a
// merge/flatten until the host releases them on document close. The host owns
// pixel lifecycle, so orphan cleanup is a host-side concern, not the arm's.
#[test]
fn structural_arms_never_drop_pixel_store_buffers() {
    let doc_merge = "structural_pixel_pin_test_doc_merge";
    let doc_flatten = "structural_pixel_pin_test_doc_flatten";
    // Open both documents and seed two tiny 2x2 RGBA buffers each.
    {
        let mut g = pixel_registry();
        let reg = g.get_or_insert_with(Default::default);
        reg.open_document(doc_merge);
        reg.open_document(doc_flatten);
        reg.add_layer(doc_merge, "A", 2, 2, vec![0u8; 2 * 2 * 4])
            .unwrap();
        reg.add_layer(doc_merge, "B", 2, 2, vec![0u8; 2 * 2 * 4])
            .unwrap();
        reg.add_layer(doc_flatten, "A", 2, 2, vec![0u8; 2 * 2 * 4])
            .unwrap();
        reg.add_layer(doc_flatten, "B", 2, 2, vec![0u8; 2 * 2 * 4])
            .unwrap();
    }

    // MergeDown removes source layers A and B from the engine graph.
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canonical_dim_doc());
    e.seed_layers(vec![mk("A", "Layer 1", 10), mk("B", "Layer 2", 20)], 0);
    e.apply(env(Command::MergeDown {
        id: "A".into(),
        merged_id: "M".into(),
    }))
    .unwrap();

    // Flatten (separate arm) removes source layers A and B from its engine graph.
    let mut e2 = ProtocolEngine::new();
    e2.seed_canonical(canonical_dim_doc());
    e2.seed_layers(vec![mk("A", "Layer 1", 10), mk("B", "Layer 2", 20)], 0);
    e2.apply(env(Command::Flatten {
        merged_id: "BG".into(),
    }))
    .unwrap();

    // Both source buffers must still exist in the registry: the arms never touched
    // it (structural arms never touch the pixel store).
    {
        let mut g = pixel_registry();
        let reg = g.get_or_insert_with(Default::default);
        assert!(
            reg.get_layer(doc_merge, "A").is_some(),
            "source A buffer survives merge"
        );
        assert!(
            reg.get_layer(doc_merge, "B").is_some(),
            "source B buffer survives merge"
        );
        assert!(
            reg.get_layer(doc_flatten, "A").is_some(),
            "source A buffer survives flatten"
        );
        assert!(
            reg.get_layer(doc_flatten, "B").is_some(),
            "source B buffer survives flatten"
        );
    }

    // Cleanup mirrors sibling global-registry tests: close releases the storage.
    {
        let mut g = pixel_registry();
        let reg = g.get_or_insert_with(Default::default);
        reg.close_document(doc_merge);
        reg.close_document(doc_flatten);
    }
}
