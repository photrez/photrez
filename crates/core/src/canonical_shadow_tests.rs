use crate::canonical_model::*;
use crate::command::*;
use crate::document_core::ProtocolEngine;
use crate::model::RenderLayer;

// --- fixtures: three fully-distinct layers, overlap fields aligned between
// the CanonicalLayer (seeded doc) and the matching RenderLayer (seeded set) ---

fn canon_layer(i: usize) -> CanonicalLayer {
    let f = i as f64;
    CanonicalLayer {
        id: format!("L{}", i),
        name: format!("Name{}", i),
        layer_type: match i {
            1 => LayerType::Raster,
            2 => LayerType::Shape,
            _ => LayerType::Text,
        },
        visible: i % 2 == 1,
        opacity: 0.1 * f,
        locked: i % 2 == 0,
        is_background: Some(i == 1),
        lock_transparency: Some(i == 2),
        lock_position: Some(i == 3),
        lock_rotation: Some(i == 1),
        has_adjustments: Some(i == 2),
        basic_adjustment: Some(BasicAdjustment {
            brightness: f,
            contrast: f + 1.0,
            saturation: f + 2.0,
        }),
        resource_id: Some(i as u32 * 10),
        blend_mode: match i {
            1 => BlendMode::Normal,
            2 => BlendMode::Multiply,
            _ => BlendMode::Overlay,
        },
        transform: Transform2D {
            x: f * 10.0,
            y: f * 20.0,
            scale_x: f,
            scale_y: f + 0.5,
            rotation: f * 5.0,
            flip_h: i == 2,
            flip_v: i == 3,
        },
        width: f * 100.0,
        height: f * 200.0,
        shape_params: if i == 2 {
            Some(ShapeParams {
                kind: ShapeKind::Star,
                width: f * 30.0,
                height: f * 40.0,
                radius: f,
                fill: ShapeFill {
                    kind: ShapeFillKind::Solid,
                    color: format!("#{:02X}0000", i * 10),
                },
                stroke: ShapeStroke {
                    enabled: i == 2,
                    color: "#000000".to_string(),
                    width: f,
                },
                arrow_head: i == 2,
            })
        } else {
            None
        },
        text_data: if i == 3 {
            Some(TextData {
                content: format!("T{}", i),
                font_family: "Arial".to_string(),
                font_size: f * 10.0,
                font_weight: f * 100.0,
                font_style: TextFontStyle::Normal,
                color: "#123456".to_string(),
                align: TextAlign::Right,
                line_height: f,
                letter_spacing: f,
                box_mode: TextBoxMode::Point,
                box_width: f,
                box_height: f,
                stroke: TextStroke {
                    width: f,
                    color: "#654321".to_string(),
                    align: Some(TextStrokeAlign::Inside),
                },
                underline: Some(i == 3),
                strikethrough: Some(i != 3),
                uppercase: Some(i == 3),
            })
        } else {
            None
        },
    }
}

fn render_layer(i: usize) -> RenderLayer {
    let c = canon_layer(i);
    RenderLayer {
        id: c.id.clone(),
        name: c.name.clone(),
        visible: c.visible,
        opacity: c.opacity,
        resource_id: c.resource_id.unwrap_or(0),
        x: c.transform.x,
        y: c.transform.y,
        scale_x: c.transform.scale_x,
        scale_y: c.transform.scale_y,
        rotation: c.transform.rotation,
        dirty_rect: None,
        ..Default::default()
    }
}

fn doc_3() -> CanonicalDocument {
    CanonicalDocument {
        id: "doc-1".to_string(),
        name: "D".to_string(),
        width: 800.0,
        height: 600.0,
        layers: vec![canon_layer(1), canon_layer(2), canon_layer(3)],
        selection: None,
    }
}

fn seeded_engine() -> ProtocolEngine {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![render_layer(1), render_layer(2), render_layer(3)], 0);
    e.seed_canonical(doc_3());
    e
}

fn env(cmd: Command) -> CommandEnvelope {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: None,
        command: cmd,
    }
}

fn env_ev(cmd: Command, ev: u64) -> CommandEnvelope {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: Some(ev),
        command: cmd,
    }
}

#[test]
fn set_opacity_updates_shadow_opacity_keeps_canonical_fields() {
    let mut e = seeded_engine();
    e.apply(env(Command::SetOpacity {
        id: "L2".to_string(),
        opacity: 0.33,
    }))
    .unwrap();
    let l2 = e
        .canonical()
        .unwrap()
        .layers
        .iter()
        .find(|l| l.id == "L2")
        .expect("present");
    assert!(
        (l2.opacity - 0.33).abs() < 1e-9,
        "opacity comes from engine"
    );
    let seed = canon_layer(2);
    assert_eq!(l2.layer_type, seed.layer_type);
    assert_eq!(l2.locked, seed.locked);
    assert_eq!(l2.is_background, seed.is_background);
    assert_eq!(l2.lock_transparency, seed.lock_transparency);
    assert_eq!(l2.lock_position, seed.lock_position);
    assert_eq!(l2.lock_rotation, seed.lock_rotation);
    assert_eq!(l2.has_adjustments, seed.has_adjustments);
    assert_eq!(l2.basic_adjustment, seed.basic_adjustment);
    assert_eq!(l2.blend_mode, seed.blend_mode);
    assert_eq!(l2.transform.flip_h, seed.transform.flip_h);
    assert_eq!(l2.transform.flip_v, seed.transform.flip_v);
    assert_eq!(l2.width, seed.width);
    assert_eq!(l2.height, seed.height);
    assert_eq!(l2.shape_params, seed.shape_params);
    assert_eq!(l2.text_data, seed.text_data);
    assert_eq!(l2.id, "L2");
    assert_eq!(l2.visible, seed.visible);
}

#[test]
fn transform_layer_updates_transform_keeps_other_fields() {
    let mut e = seeded_engine();
    e.apply(env(Command::TransformLayer {
        id: "L1".to_string(),
        transform: TransformPatch {
            x: 5.0,
            y: 6.0,
            scale_x: 2.0,
            scale_y: 3.0,
            rotation: 9.0,
            flip_h: None,
            flip_v: None,
        },
    }))
    .unwrap();
    let l1 = e
        .canonical()
        .unwrap()
        .layers
        .iter()
        .find(|l| l.id == "L1")
        .unwrap();
    assert_eq!(l1.transform.x, 5.0);
    assert_eq!(l1.transform.y, 6.0);
    assert_eq!(l1.transform.scale_x, 2.0);
    assert_eq!(l1.transform.scale_y, 3.0);
    assert_eq!(l1.transform.rotation, 9.0);
    let seed = canon_layer(1);
    assert_eq!(l1.transform.flip_h, seed.transform.flip_h);
    assert_eq!(l1.transform.flip_v, seed.transform.flip_v);
    assert_eq!(l1.resource_id, seed.resource_id);
    assert_eq!(l1.blend_mode, seed.blend_mode);
    assert_eq!(l1.width, seed.width);
    assert_eq!(l1.height, seed.height);
    assert_eq!(l1.layer_type, seed.layer_type);
    assert_eq!(l1.shape_params, seed.shape_params);
    assert_eq!(l1.text_data, seed.text_data);
}

#[test]
fn delete_layer_drops_from_shadow_and_tombstones() {
    let mut e = seeded_engine();
    e.apply(env(Command::DeleteLayer {
        id: "L2".to_string(),
    }))
    .unwrap();
    let shadow = e.canonical().unwrap();
    assert!(shadow.layers.iter().all(|l| l.id != "L2"), "L2 removed");
    assert!(shadow.layers.iter().any(|l| l.id == "L1"));
    assert!(shadow.layers.iter().any(|l| l.id == "L3"));
    let tomb = e
        .canonical_shadow_mut()
        .unwrap()
        .removed
        .get("L2")
        .expect("tombstoned");
    assert_eq!(tomb, &canon_layer(2));
}

#[test]
fn undo_restores_deleted_layer_at_original_position() {
    let mut e = seeded_engine();
    e.apply(env(Command::DeleteLayer {
        id: "L2".to_string(),
    }))
    .unwrap();
    // Delete must drop L2 from the shadow AND tombstone it before the undo restores.
    let after_delete = e.canonical().unwrap();
    assert_eq!(
        after_delete.layers.len(),
        2,
        "L2 removed from shadow on delete"
    );
    assert!(
        after_delete.layers.iter().all(|l| l.id != "L2"),
        "L2 absent from shadow after delete"
    );
    assert!(
        e.canonical_shadow_mut().unwrap().removed.contains_key("L2"),
        "L2 tombstoned on delete"
    );
    e.apply(env(Command::Undo)).unwrap();
    let shadow = e.canonical().unwrap();
    assert_eq!(shadow.layers.len(), 3, "restored");
    assert_eq!(shadow.layers[0].id, "L1");
    assert_eq!(shadow.layers[1].id, "L2", "original position");
    assert_eq!(shadow.layers[2].id, "L3");
    assert_eq!(
        shadow.layers[1],
        canon_layer(2),
        "all fields intact via tombstone"
    );
    assert!(
        e.canonical_shadow_mut().unwrap().removed.is_empty(),
        "tombstone consumed on restore"
    );
}

#[test]
fn redo_removes_restored_layer_again() {
    let mut e = seeded_engine();
    e.apply(env(Command::DeleteLayer {
        id: "L2".to_string(),
    }))
    .unwrap();
    e.apply(env(Command::Undo)).unwrap();
    e.apply(env(Command::Redo)).unwrap();
    let shadow = e.canonical().unwrap();
    assert_eq!(shadow.layers.len(), 2);
    assert!(shadow.layers.iter().all(|l| l.id != "L2"));
    assert!(e.canonical_shadow_mut().unwrap().removed.contains_key("L2"));
}

#[test]
fn add_layer_marks_incomplete_and_does_not_insert() {
    let mut e = seeded_engine();
    e.apply(env(Command::AddLayer {
        id: "Minted-id".to_string(),
        name: "Minted".to_string(),
        width: 100.0,
        height: 100.0,
        index: 0,
    }))
    .unwrap();
    let shadow = e.canonical().unwrap();
    assert_eq!(shadow.layers.len(), 3, "engine-minted layer not inserted");
    assert!(e.canonical_incomplete(), "incomplete flag set");
}

#[test]
fn full_reseed_clears_incomplete_and_tombstones() {
    let mut e = seeded_engine();
    e.apply(env(Command::DeleteLayer {
        id: "L2".to_string(),
    }))
    .unwrap();
    e.apply(env(Command::AddLayer {
        id: "Minted-id".to_string(),
        name: "Minted".to_string(),
        width: 100.0,
        height: 100.0,
        index: 0,
    }))
    .unwrap();
    assert!(e.canonical_incomplete());

    // Rebuild a full document from the engine's CURRENT render layers (which now
    // include the minted one). A full push supplies real canonical data for it.
    let layers: Vec<CanonicalLayer> = e
        .snapshot()
        .layers
        .iter()
        .map(|r| {
            if let Some(seed) = (1..=3).find(|&i| canon_layer(i).id == r.id) {
                canon_layer(seed)
            } else {
                CanonicalLayer {
                    id: r.id.clone(),
                    name: r.name.clone(),
                    layer_type: LayerType::Raster,
                    visible: r.visible,
                    opacity: r.opacity,
                    locked: false,
                    is_background: None,
                    lock_transparency: None,
                    lock_position: None,
                    lock_rotation: None,
                    has_adjustments: None,
                    basic_adjustment: None,
                    resource_id: Some(r.resource_id),
                    blend_mode: BlendMode::Normal,
                    transform: Transform2D {
                        x: r.x,
                        y: r.y,
                        scale_x: r.scale_x,
                        scale_y: r.scale_y,
                        rotation: r.rotation,
                        flip_h: false,
                        flip_v: false,
                    },
                    width: 10.0,
                    height: 10.0,
                    shape_params: None,
                    text_data: None,
                }
            }
        })
        .collect();
    let full = CanonicalDocument {
        id: "doc-1".to_string(),
        name: "D".to_string(),
        width: 800.0,
        height: 600.0,
        layers,
        selection: None,
    };
    e.seed_canonical(full.clone());
    assert!(!e.canonical_incomplete(), "incomplete cleared by full push");
    assert!(
        e.canonical_shadow_mut().unwrap().removed.is_empty(),
        "tombstones cleared"
    );
    assert_eq!(
        e.canonical().unwrap().layers.len(),
        3,
        "full doc incl minted layer"
    );
}

#[test]
fn noop_and_external_transition_leave_shadow_unchanged() {
    let mut e = seeded_engine();
    let before = e.canonical().unwrap().clone();
    e.apply(env(Command::Noop)).unwrap();
    assert_eq!(*e.canonical().unwrap(), before, "Noop unchanged");
    e.register_adapter("a");
    e.apply(env(Command::RecordExternalTransition {
        label: "x".to_string(),
        affected_layer_ids: vec![],
        adapter_id: "a".to_string(),
        token: "t".to_string(),
        memory_cost_bytes: 0,
    }))
    .unwrap();
    assert_eq!(
        *e.canonical().unwrap(),
        before,
        "RecordExternalTransition unchanged"
    );
    assert!(!e.canonical_incomplete());
}

#[test]
fn rejected_command_does_not_touch_shadow() {
    let mut e = seeded_engine();
    let before = e.canonical().unwrap().clone();
    let res = e.apply(env_ev(
        Command::SetOpacity {
            id: "L1".to_string(),
            opacity: 0.9,
        },
        999,
    ));
    assert!(res.is_err(), "version mismatch rejected");
    assert_eq!(
        *e.canonical().unwrap(),
        before,
        "shadow untouched on rejection"
    );
}

#[test]
fn no_shadow_no_reconcile_overhead() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![render_layer(1), render_layer(2)], 0);
    assert!(e.canonical().is_none());
    e.apply(env(Command::SetOpacity {
        id: "L1".to_string(),
        opacity: 0.5,
    }))
    .unwrap();
    e.apply(env(Command::DeleteLayer {
        id: "L2".to_string(),
    }))
    .unwrap();
    e.apply(env(Command::Undo)).unwrap();
    e.apply(env(Command::Redo)).unwrap();
    assert!(e.canonical().is_none(), "still none; zero-cost reconcile");
}

#[test]
fn reconcile_preserves_engine_order_via_apply() {
    let mut e = seeded_engine();
    // Seed canonical in a DIFFERENT order than the engine layer set.
    let mut doc = doc_3();
    doc.layers = vec![canon_layer(2), canon_layer(1), canon_layer(3)];
    e.seed_canonical(doc);
    e.apply(env(Command::Noop)).unwrap();
    let ids: Vec<&str> = e
        .canonical()
        .unwrap()
        .layers
        .iter()
        .map(|l| l.id.as_str())
        .collect();
    assert_eq!(
        ids,
        vec!["L1", "L2", "L3"],
        "engine order wins over seed order"
    );
}

#[test]
fn reconcile_direct_permuted_iterator_follows_order() {
    let mut e = seeded_engine();
    // Permute the engine's current render layers and drive reconcile directly.
    let mut rl: Vec<RenderLayer> = e.snapshot().layers;
    rl.rotate_left(1); // [L2, L3, L1]
    e.canonical_shadow_mut().unwrap().reconcile(rl.iter());
    let shadow = e.canonical().unwrap();
    let ids: Vec<&str> = shadow.layers.iter().map(|l| l.id.as_str()).collect();
    assert_eq!(ids, vec!["L2", "L3", "L1"], "follows the permuted iterator");
    assert_eq!(
        shadow.layers[0],
        canon_layer(2),
        "canonical-only fields preserved"
    );
}
