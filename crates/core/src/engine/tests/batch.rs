// SPDX-License-Identifier: AGPL-3.0-or-later
// Multi-layer command arms: one gesture commits ONE forward history entry,
// validation is all-or-nothing, and an all-no-op batch creates no entry.
// Each arm asserts the single-entry rule explicitly so a partial regression
// cannot hide behind a green redo walk.
use super::commands::{env, layer};
use crate::document::content::{BasicAdjustment, BlendMode};
use crate::document::layer::LayerMeta;
use crate::engine::DocumentCore;
use crate::wire::command::*;

fn engine_two() -> DocumentCore {
    let mut first = layer();
    first.id = "L1".into();
    first.name = "L1".into();
    first.resource_id = 1;
    let mut second = layer();
    second.id = "L2".into();
    second.name = "L2".into();
    second.resource_id = 2;
    let mut e = DocumentCore::new();
    e.seed_layers(vec![first, second], 0);
    e
}

fn find(e: &DocumentCore, id: &str) -> LayerMeta {
    e.snapshot()
        .layers
        .into_iter()
        .find(|l| l.id == id)
        .expect("layer present in snapshot")
}

#[test]
fn group_transform_commits_one_entry_and_undo_restores_every_member() {
    let mut e = engine_two();
    e.apply(env(Command::GroupTransformLayers {
        patches: vec![
            TransformLayerEntry {
                id: "L1".into(),
                transform: TransformPatch {
                    x: 10.0,
                    y: 3.0,
                    scale_x: 2.0,
                    scale_y: 2.0,
                    rotation: 0.0,
                    flip_h: None,
                    flip_v: None,
                },
            },
            TransformLayerEntry {
                id: "L2".into(),
                transform: TransformPatch {
                    x: 0.0,
                    y: 8.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation: 90.0,
                    flip_h: None,
                    flip_v: None,
                },
            },
        ],
    }))
    .unwrap();
    assert_eq!(e.cursor(), 1, "both members land in one history entry");
    assert_eq!(find(&e, "L1").x, 10.0);
    assert_eq!(find(&e, "L1").scale_x, 2.0);
    assert_eq!(find(&e, "L2").y, 8.0);
    assert_eq!(find(&e, "L2").rotation, 90.0);
    e.apply(env(Command::Undo)).unwrap();
    assert_eq!(find(&e, "L1").x, 0.0);
    assert_eq!(find(&e, "L2").y, 0.0);
    assert_eq!(e.cursor(), 0);
}

#[test]
fn group_transform_of_unchanged_values_creates_no_entry() {
    let mut e = engine_two();
    let cursor = e.cursor();
    let layers = e.snapshot().layers;
    let identical: Vec<TransformLayerEntry> = layers
        .iter()
        .map(|l| TransformLayerEntry {
            id: l.id.clone(),
            transform: TransformPatch {
                x: l.x,
                y: l.y,
                scale_x: l.scale_x,
                scale_y: l.scale_y,
                rotation: l.rotation,
                flip_h: l.flip_h,
                flip_v: l.flip_v,
            },
        })
        .collect();
    let result = e
        .apply(env(Command::GroupTransformLayers { patches: identical }))
        .unwrap();
    let _ = result;
    assert_eq!(e.cursor(), cursor, "all-no-op batch creates no entry");
}

#[test]
fn group_transform_with_an_unknown_id_rejects_before_any_mutation() {
    let mut e = engine_two();
    let before = e.snapshot().layers;
    let result = e.apply(env(Command::GroupTransformLayers {
        patches: vec![
            TransformLayerEntry {
                id: "L1".into(),
                transform: TransformPatch {
                    x: 50.0,
                    y: 0.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation: 0.0,
                    flip_h: None,
                    flip_v: None,
                },
            },
            TransformLayerEntry {
                id: "nope".into(),
                transform: TransformPatch {
                    x: 0.0,
                    y: 0.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation: 0.0,
                    flip_h: None,
                    flip_v: None,
                },
            },
        ],
    }));
    assert!(result.is_err(), "an unknown id rejects the whole batch");
    assert_eq!(
        e.snapshot().layers,
        before,
        "no member is mutated when any validation step fails"
    );
    assert_eq!(e.cursor(), 0);
}

#[test]
fn group_transform_with_a_repeated_id_rejects_before_any_mutation() {
    let mut e = engine_two();
    let before = e.snapshot().layers;
    let result = e.apply(env(Command::GroupTransformLayers {
        patches: vec![
            TransformLayerEntry {
                id: "L1".into(),
                transform: TransformPatch {
                    x: 10.0,
                    y: 0.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation: 0.0,
                    flip_h: None,
                    flip_v: None,
                },
            },
            TransformLayerEntry {
                id: "L1".into(),
                transform: TransformPatch {
                    x: 20.0,
                    y: 0.0,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    rotation: 0.0,
                    flip_h: None,
                    flip_v: None,
                },
            },
        ],
    }));
    assert!(result.is_err(), "a repeated id rejects the whole batch");
    assert_eq!(e.snapshot().layers, before);
    assert_eq!(e.cursor(), 0);
}

#[test]
fn set_layers_opacity_updates_every_member_in_one_entry() {
    let mut e = engine_two();
    e.apply(env(Command::SetLayersOpacity {
        ids: vec!["L1".into(), "L2".into()],
        opacity: 0.42,
    }))
    .unwrap();
    assert_eq!(e.cursor(), 1);
    assert_eq!(find(&e, "L1").opacity, 0.42);
    assert_eq!(find(&e, "L2").opacity, 0.42);
    e.apply(env(Command::Undo)).unwrap();
    assert_eq!(find(&e, "L1").opacity, 1.0);
    assert_eq!(find(&e, "L2").opacity, 1.0);
}

#[test]
fn set_layers_opacity_with_unchanged_values_creates_no_entry() {
    let mut e = engine_two();
    let cursor = e.cursor();
    e.apply(env(Command::SetLayersOpacity {
        ids: vec!["L1".into(), "L2".into()],
        opacity: 1.0,
    }))
    .unwrap();
    assert_eq!(e.cursor(), cursor);
}

#[test]
fn set_layers_visible_and_locked_share_the_one_entry_rule() {
    let mut e = engine_two();
    e.apply(env(Command::SetLayersVisible {
        ids: vec!["L1".into(), "L2".into()],
        visible: false,
    }))
    .unwrap();
    assert_eq!(e.cursor(), 1);
    assert!(!find(&e, "L1").visible);
    assert!(!find(&e, "L2").visible);
    e.apply(env(Command::SetLayersLocked {
        ids: vec!["L1".into(), "L2".into()],
        kind: LockKind::Position,
        locked: true,
    }))
    .unwrap();
    assert_eq!(e.cursor(), 2);
    assert_eq!(find(&e, "L1").lock_position, Some(true));
    assert_eq!(find(&e, "L2").lock_position, Some(true));
    e.apply(env(Command::Undo)).unwrap();
    assert_eq!(find(&e, "L1").lock_position, None);
}

#[test]
fn set_layers_blend_mode_applies_to_all_members() {
    let mut e = engine_two();
    e.apply(env(Command::SetLayersBlendMode {
        ids: vec!["L1".into(), "L2".into()],
        mode: BlendMode::Multiply,
    }))
    .unwrap();
    assert_eq!(e.cursor(), 1);
    assert_eq!(find(&e, "L1").blend_mode, Some(BlendMode::Multiply));
    assert_eq!(find(&e, "L2").blend_mode, Some(BlendMode::Multiply));
}

#[test]
fn apply_basic_adjustment_drops_zero_adjustment_in_one_entry() {
    let mut e = engine_two();
    e.apply(env(Command::SetAdjustment {
        id: "L1".into(),
        adjustment: Some(BasicAdjustment {
            brightness: 0.0,
            contrast: 0.0,
            saturation: 0.0,
        }),
    }))
    .unwrap();
    assert_eq!(e.cursor(), 1);
    e.apply(env(Command::ApplyBasicAdjustment {
        id: "L1".into(),
        pixels: None,
    }))
    .unwrap();
    assert_eq!(e.cursor(), 2);
    assert!(find(&e, "L1").basic_adjustment.is_none());
    assert_eq!(find(&e, "L1").has_adjustments, Some(false));
}

#[test]
fn apply_basic_adjustment_requires_the_baked_buffer_for_a_non_zero_bake() {
    let mut e = engine_two();
    e.apply(env(Command::SetAdjustment {
        id: "L1".into(),
        adjustment: Some(BasicAdjustment {
            brightness: 25.0,
            contrast: 0.0,
            saturation: 0.0,
        }),
    }))
    .unwrap();
    assert_eq!(e.cursor(), 1);
    let result = e.apply(env(Command::ApplyBasicAdjustment {
        id: "L1".into(),
        pixels: None,
    }));
    assert!(result.is_err(), "missing buffer rejects with E_INVALID");
    assert_eq!(e.cursor(), 1, "rejection adds no history");
    assert!(find(&e, "L1").basic_adjustment.is_some());
}

#[test]
fn apply_basic_adjustment_rejects_a_length_mismatch_buffer() {
    let mut e = engine_two();
    e.apply(env(Command::SetAdjustment {
        id: "L1".into(),
        adjustment: Some(BasicAdjustment {
            brightness: 25.0,
            contrast: 0.0,
            saturation: 0.0,
        }),
    }))
    .unwrap();
    let result = e.apply(env(Command::ApplyBasicAdjustment {
        id: "L1".into(),
        pixels: Some(RasterizePixels {
            width: 4.0,
            height: 4.0,
            rgba: vec![255; 65],
        }),
    }));
    assert!(result.is_err(), "a wrong-length buffer rejects");
    assert_eq!(e.cursor(), 1);
    assert!(find(&e, "L1").basic_adjustment.is_some());
}

#[test]
fn apply_basic_adjustment_bakes_into_the_buffer_in_one_entry() {
    let mut e = engine_two();
    e.apply(env(Command::SetAdjustment {
        id: "L1".into(),
        adjustment: Some(BasicAdjustment {
            brightness: 25.0,
            contrast: 0.0,
            saturation: 0.0,
        }),
    }))
    .unwrap();
    assert_eq!(e.cursor(), 1);
    e.apply(env(Command::ApplyBasicAdjustment {
        id: "L1".into(),
        pixels: Some(RasterizePixels {
            width: 2.0,
            height: 2.0,
            rgba: vec![128; 16],
        }),
    }))
    .unwrap();
    assert_eq!(e.cursor(), 2, "the bake is one history entry");
    assert!(find(&e, "L1").basic_adjustment.is_none());
    assert_eq!(find(&e, "L1").has_adjustments, Some(false));
}

#[test]
fn apply_basic_adjustment_with_unknown_id_is_a_silent_no_op() {
    let mut e = engine_two();
    let cursor = e.cursor();
    let result = e
        .apply(env(Command::ApplyBasicAdjustment {
            id: "nope".into(),
            pixels: None,
        }))
        .unwrap();
    assert_eq!(result.delta.changes.len(), 0);
    assert_eq!(e.cursor(), cursor);
}

#[test]
fn delete_layers_removes_all_members_in_one_entry() {
    let mut e = engine_two();
    e.apply(env(Command::DeleteLayers {
        ids: vec!["L1".into(), "L2".into()],
    }))
    .unwrap();
    assert_eq!(e.cursor(), 1);
    assert_eq!(e.snapshot().layers.len(), 0);
    e.apply(env(Command::Undo)).unwrap();
    assert_eq!(e.snapshot().layers.len(), 2);
}

#[test]
fn delete_layers_with_an_unknown_id_rejects_before_any_mutation() {
    let mut e = engine_two();
    let before = e.snapshot().layers;
    let result = e.apply(env(Command::DeleteLayers {
        ids: vec!["L1".into(), "nope".into()],
    }));
    assert!(result.is_err());
    assert_eq!(e.snapshot().layers, before);
    assert_eq!(e.cursor(), 0);
}

#[test]
fn duplicate_layers_clones_sources_and_selects_them_as_one_entry() {
    let mut e = engine_two();
    e.apply(env(Command::DuplicateLayers {
        ids: vec!["L1".into(), "L2".into()],
        new_ids: vec!["N1".into(), "N2".into()],
    }))
    .unwrap();
    assert_eq!(e.cursor(), 1);
    let snapshot = e.snapshot();
    assert_eq!(snapshot.layers.len(), 4);
    // Clones sit above their sources: full stack top-to-bottom N1, L1, N2, L2.
    let ids: Vec<String> = snapshot.layers.iter().map(|l| l.id.clone()).collect();
    assert_eq!(ids, vec!["N1", "L1", "N2", "L2"]);
    assert_ne!(find(&e, "N1").resource_id, 1);
    assert!(!find(&e, "N1").locked.unwrap_or(true));
    assert!(find(&e, "N1").is_background.is_none());
    e.apply(env(Command::Undo)).unwrap();
    assert_eq!(e.snapshot().layers.len(), 2);
}

#[test]
fn duplicate_layers_with_a_present_new_id_rejects_before_any_mutation() {
    let mut e = engine_two();
    let before = e.snapshot().layers;
    let result = e.apply(env(Command::DuplicateLayers {
        ids: vec!["L1".into(), "L2".into()],
        new_ids: vec!["N1".into(), "L1".into()],
    }));
    assert!(result.is_err());
    assert_eq!(e.snapshot().layers, before);
    assert_eq!(e.cursor(), 0);
}
