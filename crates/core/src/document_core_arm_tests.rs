// SPDX-License-Identifier: AGPL-3.0-or-later
// Metadata-arm unit tests: the 6 new Command variants + the TransformLayer
// flip extension. Each arm mutates the engine LayerSet via COW and projects the
// changed field onto the render layer so the delta/shadow carry it. The last test
// is the shadow-reconcile spot check (one representative is enough; the merge
// logic is shared across all extended fields).
use crate::canonical_model::{
    BlendMode, CanonicalDocument, CanonicalLayer, LayerType, Transform2D,
};
use crate::command::*;
use crate::document_core::ProtocolEngine;
use crate::model::RenderLayer;

fn env(cmd: Command) -> CommandEnvelope {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: None,
        command: cmd,
    }
}

fn layer() -> RenderLayer {
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
    assert_eq!(l.visible, false);
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
    assert_eq!(l.visible, true);
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
    e.apply(env(Command::Reorder {
        id: "L1".into(),
        to: 1,
    }))
    .unwrap();
    let snap = e.snapshot();
    let ids: Vec<&str> = snap.layers.iter().map(|l| l.id.as_str()).collect();
    assert_eq!(ids, vec!["L2", "L1"], "L1 must move after L2");
}

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
    assert_eq!(find(&e, "L1").visible, true);
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
