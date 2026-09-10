// SPDX-License-Identifier: AGPL-3.0-or-later
// Metadata-arm unit tests: the 6 new Command variants + the TransformLayer
// flip extension. Each arm mutates the engine LayerSet via COW and projects the
// changed field onto the render layer so the delta/shadow carry it. The last test
// is the shadow-reconcile spot check (one representative is enough; the merge
// logic is shared across all extended fields).
use crate::canonical_model::{
    BasicAdjustment, BlendMode, CanonicalDocument, CanonicalLayer, LayerType, SelectionShape,
    SelectionState, ShapeFill, ShapeFillKind, ShapeKind, ShapeParams, ShapeStroke, TextAlign,
    TextBoxMode, TextData, TextFontStyle, TextStroke, Transform2D,
};
use crate::command::*;
use crate::document_core::ProtocolEngine;
use crate::model::{RenderLayer, RenderLayerChange};

pub(crate) fn env(cmd: Command) -> CommandEnvelope {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: None,
        command: cmd,
    }
}

pub(crate) fn layer() -> RenderLayer {
    RenderLayer {
        id: "L1".into(),
        name: "A".into(),
        visible: true,
        opacity: 1.0,
        resource_id: 1,
        x: 0.0,
        y: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
        rotation: 0.0,
        dirty_rect: None,
        ..Default::default()
    }
}

fn engine() -> ProtocolEngine {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![layer()], 0);
    e
}

fn find(e: &ProtocolEngine, id: &str) -> RenderLayer {
    e.snapshot()
        .layers
        .into_iter()
        .find(|l| l.id == id)
        .expect("layer present in snapshot")
}

#[test]
fn set_visible_applies_and_preserves_other_fields() {
    let mut e = engine();
    e.apply(env(Command::SetVisible {
        id: "L1".into(),
        visible: false,
    }))
    .unwrap();
    let l = find(&e, "L1");
    assert!(!l.visible);
    // untouched fields unchanged
    assert_eq!(l.opacity, 1.0);
    assert_eq!(l.rotation, 0.0);
    assert_eq!(l.blend_mode, None);
}

#[test]
fn set_locked_base_maps_to_locked_field() {
    let mut e = engine();
    e.apply(env(Command::SetLocked {
        id: "L1".into(),
        kind: LockKind::Base,
        locked: true,
    }))
    .unwrap();
    let l = find(&e, "L1");
    assert_eq!(l.locked, Some(true));
    // the other lock fields stay unset
    assert_eq!(l.lock_position, None);
    assert_eq!(l.lock_rotation, None);
}

#[test]
fn set_locked_transparency_maps_to_named_field() {
    let mut e = engine();
    e.apply(env(Command::SetLocked {
        id: "L1".into(),
        kind: LockKind::Transparency,
        locked: true,
    }))
    .unwrap();
    let l = find(&e, "L1");
    assert_eq!(l.lock_transparency, Some(true));
    assert_eq!(l.locked, None);
}

#[test]
fn rename_changes_only_name() {
    let mut e = engine();
    e.apply(env(Command::Rename {
        id: "L1".into(),
        name: "Renamed".into(),
    }))
    .unwrap();
    let l = find(&e, "L1");
    assert_eq!(l.name, "Renamed");
    assert!(l.visible);
    assert_eq!(l.opacity, 1.0);
}

#[test]
fn reorder_moves_layer_to_clamped_target_index() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(
        vec![
            RenderLayer {
                id: "L1".into(),
                name: "A".into(),
                visible: true,
                opacity: 1.0,
                resource_id: 1,
                x: 0.0,
                y: 0.0,
                scale_x: 1.0,
                scale_y: 1.0,
                rotation: 0.0,
                dirty_rect: None,
                ..Default::default()
            },
            RenderLayer {
                id: "L2".into(),
                name: "B".into(),
                visible: true,
                opacity: 1.0,
                resource_id: 2,
                x: 0.0,
                y: 0.0,
                scale_x: 1.0,
                scale_y: 1.0,
                rotation: 0.0,
                dirty_rect: None,
                ..Default::default()
            },
        ],
        0,
    );
    // move L1 (index 0) to index 1
    let res = e
        .apply(env(Command::Reorder {
            id: "L1".into(),
            to: 1,
        }))
        .unwrap();
    let snap = e.snapshot();
    let ids: Vec<&str> = snap.layers.iter().map(|l| l.id.as_str()).collect();
    assert_eq!(ids, vec!["L2", "L1"], "L1 must move after L2");
    // The delta must carry every layer (not just the moved one) as an Upsert in
    // engine order, so the host reconciles the new stacking from one delta.
    let changes = res.delta.changes;
    assert_eq!(changes.len(), 2, "delta must upsert every layer");
    let order: Vec<&str> = changes
        .iter()
        .map(|c| match c {
            RenderLayerChange::Upsert { layer } => layer.id.as_str(),
            _ => panic!("reorder delta must contain only upserts"),
        })
        .collect();
    assert_eq!(
        order,
        vec!["L2", "L1"],
        "delta order must follow engine order"
    );
}

#[test]
fn reorder_rejects_out_of_range_target_index_with_e_invalid() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(
        vec![
            RenderLayer {
                id: "L1".into(),
                name: "A".into(),
                visible: true,
                opacity: 1.0,
                resource_id: 1,
                x: 0.0,
                y: 0.0,
                scale_x: 1.0,
                scale_y: 1.0,
                rotation: 0.0,
                dirty_rect: None,
                ..Default::default()
            },
            RenderLayer {
                id: "L2".into(),
                name: "B".into(),
                visible: true,
                opacity: 1.0,
                resource_id: 2,
                x: 0.0,
                y: 0.0,
                scale_x: 1.0,
                scale_y: 1.0,
                rotation: 0.0,
                dirty_rect: None,
                ..Default::default()
            },
        ],
        0,
    );
    let before_version = e.version;
    let before = e.snapshot();
    // to == len (2) is out of range; must reject before mutating state.
    let res = e.apply(env(Command::Reorder {
        id: "L1".into(),
        to: 2,
    }));
    assert!(res.is_err(), "out-of-range reorder target must be rejected");
    assert_eq!(res.unwrap_err().code, "E_INVALID");
    // State and document version (DV) must be unchanged.
    assert_eq!(e.snapshot(), before, "reorder must not mutate on rejection");
    assert_eq!(
        e.version, before_version,
        "reorder must not bump DV on rejection"
    );
}

// Pure-reorder undo/redo delta shape pins moved to document_core_reorder_tests.rs
// (keeps this module under the 1000-line guard).

#[test]
fn set_background_flag_sets_flag_and_locks_position_rotation() {
    let mut e = engine();
    e.apply(env(Command::SetBackgroundFlag { id: "L1".into() }))
        .unwrap();
    let l = find(&e, "L1");
    assert_eq!(l.is_background, Some(true));
    assert_eq!(l.lock_position, Some(true));
    assert_eq!(l.lock_rotation, Some(true));
}

#[test]
fn set_blend_mode_sets_field() {
    let mut e = engine();
    e.apply(env(Command::SetBlendMode {
        id: "L1".into(),
        mode: BlendMode::Multiply,
    }))
    .unwrap();
    let l = find(&e, "L1");
    assert_eq!(l.blend_mode, Some(BlendMode::Multiply));
}

#[test]
fn transform_layer_flip_projects_onto_render_layer() {
    let mut e = engine();
    e.apply(env(Command::TransformLayer {
        id: "L1".into(),
        transform: TransformPatch {
            x: 0.0,
            y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
            flip_h: Some(true),
            flip_v: Some(false),
        },
    }))
    .unwrap();
    let l = find(&e, "L1");
    assert_eq!(l.flip_h, Some(true));
    assert_eq!(l.flip_v, Some(false));
}

#[test]
fn unknown_id_is_noop_for_metadata_arms() {
    // Mirrors DeleteLayer: a missing id is a silent no-op (no error), so the
    // native-authority path stays bug-compatible with the TS engine's guarded apply ops.
    let mut e = engine();
    let before = e.snapshot().layers.len();
    for cmd in [
        Command::SetVisible {
            id: "nope".into(),
            visible: false,
        },
        Command::SetLocked {
            id: "nope".into(),
            kind: LockKind::Base,
            locked: true,
        },
        Command::Rename {
            id: "nope".into(),
            name: "x".into(),
        },
        Command::Reorder {
            id: "nope".into(),
            to: 0,
        },
        Command::SetBackgroundFlag { id: "nope".into() },
        Command::SetBlendMode {
            id: "nope".into(),
            mode: BlendMode::Multiply,
        },
    ] {
        e.apply(env(cmd))
            .expect("unknown id is a no-op, not an error");
    }
    // No layer added/removed; the seeded layer is untouched.
    assert_eq!(e.snapshot().layers.len(), before);
    assert!(find(&e, "L1").visible);
}

#[test]
fn add_layer_rejects_empty_id_with_e_invalid() {
    let mut e = ProtocolEngine::new();
    let res = e.apply(env(Command::AddLayer {
        id: "".into(),
        name: "x".into(),
        width: 10.0,
        height: 10.0,
        index: 0,
        layer_type: None,
        shape_params: None,
        text_data: None,
    }));
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().code, "E_INVALID");
}

#[test]
fn add_layer_rejects_duplicate_id_with_e_invalid() {
    let mut e = engine();
    let res = e.apply(env(Command::AddLayer {
        id: "L1".into(), // already present from seed
        name: "dup".into(),
        width: 10.0,
        height: 10.0,
        index: 0,
        layer_type: None,
        shape_params: None,
        text_data: None,
    }));
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().code, "E_INVALID");
}

// Shadow-reconcile spot check (D6 dependency): an extended Some field on the
// render layer must be merged onto the canonical shadow, so a future native-read
// path sees the metadata the arm set.
fn canonical_doc() -> CanonicalDocument {
    CanonicalDocument {
        id: "doc".into(),
        name: "n".into(),
        width: 10.0,
        height: 10.0,
        layers: vec![CanonicalLayer {
            id: "L1".into(),
            name: "A".into(),
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
            resource_id: Some(1),
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

#[test]
fn shadow_reconcile_picks_up_extended_field_from_arm() {
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canonical_doc());
    e.seed_layers(vec![layer()], 0);
    e.apply(env(Command::SetBlendMode {
        id: "L1".into(),
        mode: BlendMode::Multiply,
    }))
    .unwrap();
    let c = e.canonical().expect("shadow seeded");
    let layer = c
        .layers
        .iter()
        .find(|l| l.id == "L1")
        .expect("layer in shadow");
    assert_eq!(layer.blend_mode, BlendMode::Multiply);
}

// Second engine layer (id "L2") for the reorder-ordering shadow check.
fn layer_two() -> RenderLayer {
    let mut l = layer();
    l.id = "L2".into();
    l.name = "B".into();
    l.resource_id = 2;
    l
}

// Two-layer canonical shadow (L1 + L2) for the reorder-ordering check.
fn canonical_doc_two() -> CanonicalDocument {
    let mut doc = canonical_doc();
    let mut l2 = doc.layers[0].clone();
    l2.id = "L2".into();
    l2.name = "B".into();
    doc.layers.push(l2);
    doc
}

#[test]
fn shadow_reconcile_follows_engine_order_after_reorder() {
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canonical_doc_two());
    e.seed_layers(vec![layer(), layer_two()], 0);
    // Move L1 (index 0) after L2 (index 1): engine order becomes [L2, L1].
    e.apply(env(Command::Reorder {
        id: "L1".into(),
        to: 1,
    }))
    .unwrap();
    let c = e.canonical().expect("shadow seeded");
    let ids: Vec<&str> = c.layers.iter().map(|l| l.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["L2", "L1"],
        "canonical layer order must follow engine order after reorder"
    );
}

// typed-add / setLayerParams / SetAdjustment arms

pub(crate) fn shape_params() -> ShapeParams {
    ShapeParams {
        kind: ShapeKind::Star,
        width: 120.0,
        height: 80.0,
        radius: 6.0,
        fill: ShapeFill {
            kind: ShapeFillKind::Solid,
            color: "#E15A17".into(),
        },
        stroke: ShapeStroke {
            enabled: true,
            color: "#000000".into(),
            width: 2.0,
        },
        arrow_head: false,
    }
}

pub(crate) fn text_data() -> TextData {
    TextData {
        content: "Hi".into(),
        font_family: "Arial".into(),
        font_size: 32.0,
        font_weight: 400.0,
        font_style: TextFontStyle::Normal,
        color: "#000000".into(),
        align: TextAlign::Left,
        line_height: 1.2,
        letter_spacing: 0.0,
        box_mode: TextBoxMode::Point,
        box_width: 0.0,
        box_height: 0.0,
        stroke: TextStroke {
            width: 0.0,
            color: "#000000".into(),
            align: None,
        },
        underline: Some(false),
        strikethrough: Some(false),
        uppercase: Some(false),
    }
}

#[test]
fn add_layer_typed_shape_projects_type_and_params() {
    let mut e = ProtocolEngine::new();
    e.apply(env(Command::AddLayer {
        id: "S".into(),
        name: "Star".into(),
        width: 120.0,
        height: 80.0,
        index: 0,
        layer_type: Some(LayerType::Shape),
        shape_params: Some(shape_params()),
        text_data: None,
    }))
    .unwrap();
    let l = find(&e, "S");
    assert_eq!(l.layer_type, Some(LayerType::Shape));
    assert_eq!(l.shape_params, Some(shape_params()));
    // TS createShapeLayerNode keeps blendMode Normal, visible true, opacity 1.0.
    assert_eq!(l.blend_mode, Some(BlendMode::Normal));
    assert!(l.visible);
    assert_eq!(l.opacity, 1.0);
    assert!(l.text_data.is_none());
}

#[test]
fn add_layer_typed_text_projects_type_and_text_data() {
    let mut e = ProtocolEngine::new();
    e.apply(env(Command::AddLayer {
        id: "T".into(),
        name: "Text".into(),
        width: 100.0,
        height: 20.0,
        index: 0,
        layer_type: Some(LayerType::Text),
        shape_params: None,
        text_data: Some(text_data()),
    }))
    .unwrap();
    let l = find(&e, "T");
    assert_eq!(l.layer_type, Some(LayerType::Text));
    assert_eq!(l.text_data, Some(text_data()));
    assert!(l.shape_params.is_none());
}

#[test]
fn add_layer_without_type_stays_raster_backward_compatible() {
    // A v2 envelope missing the optional typed fields must still produce a raster
    // layer (the previous behavior) so existing envelopes keep working.
    let mut e = ProtocolEngine::new();
    e.apply(env(Command::AddLayer {
        id: "R".into(),
        name: "R".into(),
        width: 10.0,
        height: 10.0,
        index: 0,
        layer_type: None,
        shape_params: None,
        text_data: None,
    }))
    .unwrap();
    let l = find(&e, "R");
    assert_eq!(l.layer_type, Some(LayerType::Raster));
    assert_eq!(l.blend_mode, Some(BlendMode::Normal));
    assert!(l.shape_params.is_none());
    assert!(l.text_data.is_none());
}

#[test]
fn set_layer_params_sets_shape_params_only() {
    let mut e = engine();
    e.apply(env(Command::SetLayerParams {
        id: "L1".into(),
        shape_params: Some(shape_params()),
        text_data: None,
    }))
    .unwrap();
    let l = find(&e, "L1");
    assert_eq!(l.shape_params, Some(shape_params()));
    assert!(l.text_data.is_none());
}

#[test]
fn set_layer_params_sets_text_data_only() {
    let mut e = engine();
    e.apply(env(Command::SetLayerParams {
        id: "L1".into(),
        shape_params: None,
        text_data: Some(text_data()),
    }))
    .unwrap();
    let l = find(&e, "L1");
    assert_eq!(l.text_data, Some(text_data()));
    assert!(l.shape_params.is_none());
}

#[test]
fn set_layer_params_both_none_rejects_with_e_invalid() {
    let mut e = engine();
    let before = e.snapshot();
    let res = e.apply(env(Command::SetLayerParams {
        id: "L1".into(),
        shape_params: None,
        text_data: None,
    }));
    assert!(res.is_err(), "both-None params must be rejected");
    assert_eq!(res.unwrap_err().code, "E_INVALID");
    // No mutation, no DV bump, no history entry.
    assert_eq!(e.snapshot(), before);
}

#[test]
fn set_layer_params_unknown_id_is_noop() {
    let mut e = engine();
    let before = e.snapshot().layers.clone();
    e.apply(env(Command::SetLayerParams {
        id: "ghost".into(),
        shape_params: Some(shape_params()),
        text_data: None,
    }))
    .expect("unknown id is a silent no-op");
    // The layer set is untouched (mirrors DeleteLayer: unknown id is a no-op).
    // The document version still bumps (apply() walks the same path), so compare
    // layers, not the whole snapshot.
    assert_eq!(e.snapshot().layers, before);
}

#[test]
fn set_adjustment_sets_adjustment_and_derives_has_adjustments() {
    let mut e = engine();
    e.apply(env(Command::SetAdjustment {
        id: "L1".into(),
        adjustment: Some(BasicAdjustment {
            brightness: 10.0,
            contrast: 0.0,
            saturation: 0.0,
        }),
    }))
    .unwrap();
    let l = find(&e, "L1");
    assert_eq!(
        l.basic_adjustment,
        Some(BasicAdjustment {
            brightness: 10.0,
            contrast: 0.0,
            saturation: 0.0,
        })
    );
    // has_adjustments mirrors TS: true only when a channel is non-zero.
    assert_eq!(l.has_adjustments, Some(true));
}

#[test]
fn set_adjustment_all_zero_derives_has_adjustments_false() {
    let mut e = engine();
    e.apply(env(Command::SetAdjustment {
        id: "L1".into(),
        adjustment: Some(BasicAdjustment {
            brightness: 0.0,
            contrast: 0.0,
            saturation: 0.0,
        }),
    }))
    .unwrap();
    let l = find(&e, "L1");
    assert_eq!(
        l.basic_adjustment,
        Some(BasicAdjustment {
            brightness: 0.0,
            contrast: 0.0,
            saturation: 0.0,
        })
    );
    assert_eq!(l.has_adjustments, Some(false));
}

#[test]
fn set_adjustment_clamps_channels_to_range() {
    let mut e = engine();
    e.apply(env(Command::SetAdjustment {
        id: "L1".into(),
        adjustment: Some(BasicAdjustment {
            brightness: 500.0,
            contrast: -500.0,
            saturation: 50.0,
        }),
    }))
    .unwrap();
    let l = find(&e, "L1");
    assert_eq!(
        l.basic_adjustment,
        Some(BasicAdjustment {
            brightness: 100.0,
            contrast: -100.0,
            saturation: 50.0,
        })
    );
}

#[test]
fn set_adjustment_none_clears_and_sets_has_adjustments_false() {
    let mut e = engine();
    e.apply(env(Command::SetAdjustment {
        id: "L1".into(),
        adjustment: Some(BasicAdjustment {
            brightness: 10.0,
            contrast: 0.0,
            saturation: 0.0,
        }),
    }))
    .unwrap();
    e.apply(env(Command::SetAdjustment {
        id: "L1".into(),
        adjustment: None,
    }))
    .unwrap();
    let l = find(&e, "L1");
    assert!(
        l.basic_adjustment.is_none(),
        "None must clear basicAdjustment"
    );
    assert_eq!(l.has_adjustments, Some(false));
}

#[test]
fn set_adjustment_unknown_id_is_noop() {
    let mut e = engine();
    let before = e.snapshot().layers.clone();
    e.apply(env(Command::SetAdjustment {
        id: "ghost".into(),
        adjustment: Some(BasicAdjustment {
            brightness: 5.0,
            contrast: 0.0,
            saturation: 0.0,
        }),
    }))
    .expect("unknown id is a silent no-op");
    assert_eq!(e.snapshot().layers, before);
}

#[test]
fn shadow_reconcile_picks_up_typed_params_from_set_layer_params() {
    // D6 dependency: a SetLayerParams Some on the render layer must merge onto the
    // canonical shadow (Some takes), so a future native-read path sees the params.
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canonical_doc());
    e.seed_layers(vec![layer()], 0);
    e.apply(env(Command::SetLayerParams {
        id: "L1".into(),
        shape_params: Some(shape_params()),
        text_data: None,
    }))
    .unwrap();
    let c = e.canonical().expect("shadow seeded");
    let cl = c
        .layers
        .iter()
        .find(|l| l.id == "L1")
        .expect("layer in shadow");
    assert_eq!(cl.shape_params, Some(shape_params()));
}

// ── Selection-arm state contract ──────────────────────────────────
// Selection is NOT an undoable transition: every selection command must bump the
// document version (accepted transition) but leave the history entry count
// UNCHANGED, and the canonical shadow selection must track the engine selection.
fn sel_doc() -> CanonicalDocument {
    CanonicalDocument {
        id: "sel-doc".into(),
        name: "S".into(),
        width: 200.0,
        height: 150.0,
        layers: vec![],
        selection: None,
    }
}

#[test]
fn selection_arms_bump_version_without_history_entries_and_track_shadow() {
    let mut e = ProtocolEngine::new();
    e.seed_canonical(sel_doc());
    e.seed_layers(vec![layer()], 0);

    let entries0 = e.entries.len();
    let v0 = e.version();

    // SetSelection -> version +1, no new entry, engine + shadow selection set.
    e.apply(env(Command::SetSelection {
        selection: SelectionState {
            x: 10.0,
            y: 20.0,
            width: 30.0,
            height: 40.0,
            angle: 5.0,
            shape: Some(SelectionShape::Ellipse),
            inverted: Some(false),
        },
    }))
    .unwrap();
    assert_eq!(e.version(), v0 + 1, "setSelection bumps version");
    assert_eq!(
        e.entries.len(),
        entries0,
        "setSelection commits no history entry"
    );
    assert_eq!(e.selection().unwrap().x, 10.0);
    assert_eq!(
        e.canonical().unwrap().selection.as_ref().unwrap().x,
        10.0,
        "shadow selection tracks engine after set"
    );

    // ClearSelection -> version +1, no new entry, both cleared.
    e.apply(env(Command::ClearSelection)).unwrap();
    assert_eq!(e.version(), v0 + 2, "clearSelection bumps version");
    assert_eq!(
        e.entries.len(),
        entries0,
        "clearSelection commits no history entry"
    );
    assert!(e.selection().is_none());
    assert!(
        e.canonical().unwrap().selection.is_none(),
        "shadow cleared too"
    );

    // SelectAll -> full-canvas rect from seeded doc dims, no new entry.
    e.apply(env(Command::SelectAll)).unwrap();
    assert_eq!(e.version(), v0 + 3, "selectAll bumps version");
    assert_eq!(
        e.entries.len(),
        entries0,
        "selectAll commits no history entry"
    );
    let all = e.selection().unwrap();
    assert_eq!(all.x, 0.0);
    assert_eq!(all.y, 0.0);
    assert_eq!(all.width, 200.0);
    assert_eq!(all.height, 150.0);

    // InvertSelection (selection present) -> toggles inverted, no new entry.
    e.apply(env(Command::InvertSelection)).unwrap();
    assert_eq!(e.version(), v0 + 4, "invertSelection bumps version");
    assert_eq!(
        e.entries.len(),
        entries0,
        "invertSelection commits no history entry"
    );
    assert_eq!(e.selection().unwrap().inverted, Some(true));
}

#[test]
fn invert_without_selection_falls_back_to_full_canvas() {
    // No selection + seeded canonical -> InvertSelection falls back to the SAME
    // full-canvas rect the SelectAll arm builds, exactly (no-op fallback removed
    // to mirror the host op). Still no history entry, still version +1.
    let mut e = ProtocolEngine::new();
    e.seed_canonical(sel_doc());
    e.seed_layers(vec![layer()], 0);
    let entries0 = e.entries.len();
    let v0 = e.version();

    e.apply(env(Command::InvertSelection)).unwrap();
    let inv = e
        .selection()
        .expect("fallback produced a full-canvas selection");
    assert_eq!(
        e.entries.len(),
        entries0,
        "invert fallback commits no entry"
    );
    assert_eq!(e.version(), v0 + 1, "invert fallback bumps version");

    // Reference: SelectAll on the same seeded dims.
    let mut ref_e = ProtocolEngine::new();
    ref_e.seed_canonical(sel_doc());
    ref_e.seed_layers(vec![layer()], 0);
    ref_e.apply(env(Command::SelectAll)).unwrap();
    let all = ref_e
        .selection()
        .expect("selectAll produced a full-canvas selection");

    // InvertSelection-without-selection must equal SelectAll EXACTLY.
    assert_eq!(inv.x, all.x);
    assert_eq!(inv.y, all.y);
    assert_eq!(inv.width, all.width);
    assert_eq!(inv.height, all.height);
    assert_eq!(inv.angle, all.angle);
    assert_eq!(inv.shape, all.shape);
    assert_eq!(inv.inverted, all.inverted);
    assert_eq!(
        inv.inverted, None,
        "fallback matches SelectAll (inverted: None)"
    );

    // Existing selection -> toggles inverted (unchanged behavior).
    let mut t = ProtocolEngine::new();
    t.seed_canonical(sel_doc());
    t.seed_layers(vec![layer()], 0);
    t.apply(env(Command::SetSelection {
        selection: SelectionState {
            x: 1.0,
            y: 2.0,
            width: 3.0,
            height: 4.0,
            angle: 0.0,
            shape: None,
            inverted: Some(false),
        },
    }))
    .unwrap();
    t.apply(env(Command::InvertSelection)).unwrap();
    assert_eq!(
        t.selection().unwrap().inverted,
        Some(true),
        "existing selection toggles inverted"
    );

    // No canonical shadow -> falls back to select-all which requires seeded
    // canonical, so it rejects with the same E_INVALID-shaped error as SelectAll.
    let mut n = ProtocolEngine::new();
    n.seed_layers(vec![layer()], 0);
    let r = n.apply(env(Command::InvertSelection));
    assert!(r.is_err());
    assert_eq!(r.unwrap_err().code, "E_INVALID");
    assert!(n.selection().is_none(), "no mutation on rejection");
}

#[test]
fn select_all_without_seeded_canonical_rejects_e_invalid() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![layer()], 0);
    // No canonical shadow seeded -> host must seed before select-all.
    let res = e.apply(env(Command::SelectAll));
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().code, "E_INVALID");
    assert!(e.selection().is_none(), "no mutation on rejection");
}

#[test]
fn set_selection_rejects_nonfinite_or_negative_dims_with_e_invalid() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![layer()], 0);
    let bad = |sel: SelectionState| {
        let mut e2 = ProtocolEngine::new();
        e2.seed_layers(vec![layer()], 0);
        let r = e2.apply(env(Command::SetSelection { selection: sel }));
        assert!(r.is_err(), "non-finite/negative geometry must be rejected");
        assert_eq!(r.unwrap_err().code, "E_INVALID");
    };
    bad(SelectionState {
        x: 0.0,
        y: 0.0,
        width: f64::INFINITY,
        height: 1.0,
        angle: 0.0,
        shape: None,
        inverted: None,
    });
    bad(SelectionState {
        x: 0.0,
        y: 0.0,
        width: -1.0,
        height: 1.0,
        angle: 0.0,
        shape: None,
        inverted: None,
    });
    bad(SelectionState {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: f64::NAN,
        angle: 0.0,
        shape: None,
        inverted: None,
    });
    // The arm rejects NaN angle too - it is stricter than the TS host createSelection,
    // which validates none of the five numeric fields.
    bad(SelectionState {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
        angle: f64::NAN,
        shape: None,
        inverted: None,
    });
}
