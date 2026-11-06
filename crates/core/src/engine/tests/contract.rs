// SPDX-License-Identifier: AGPL-3.0-or-later
// Wire-contract pins for the command envelope + layer delta (single-layer arms).
use super::*;

#[test]
fn contract_and_document_versions_are_distinct() {
    let mut eng = DocumentCore::new();
    assert_eq!(eng.version(), 0);
    let r = eng
        .apply(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: None,
            command: Command::Noop,
        })
        .unwrap();
    assert_eq!(r.document_version, 1);
    assert_eq!(r.delta.base_version, 0);
    assert_eq!(r.delta.version, 1);
}

#[test]
fn rejects_wrong_contract_version() {
    let mut eng = DocumentCore::new();
    let err = eng
        .apply(CommandEnvelope {
            contract_version: 999,
            expected_version: None,
            command: Command::Noop,
        })
        .unwrap_err();
    assert_eq!(err.code, "E_CONTRACT_VERSION");
}

#[test]
fn snapshot_vs_delta_semantics() {
    let mut eng = DocumentCore::new();
    let r1 = eng
        .apply(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: None,
            command: Command::Ping { echo: "a".into() },
        })
        .unwrap();
    assert!(r1.delta.is_applicable(0));
    assert!(!r1.delta.is_applicable(999));
    let snap = eng.snapshot();
    assert_eq!(snap.version, 1);
    assert_eq!(snap.layers.len(), 1);
}

#[test]
fn add_delete_transform_opacity_single_owner() {
    let mut eng = DocumentCore::new();
    let a = eng
        .apply(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: None,
            command: Command::AddLayer {
                id: "layer-L1".into(),
                name: "L1".into(),
                width: 100.0,
                height: 100.0,
                index: 999,
                x: None,
                y: None,
                layer_type: None,
                shape_params: None,
                text_data: None,
            },
        })
        .unwrap();
    assert_eq!(eng.snapshot().layers.len(), 1);
    let id = eng.snapshot().layers[0].id.clone();
    let res_id = eng.snapshot().layers[0].resource_id;
    let t = eng
        .apply(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: None,
            command: Command::TransformLayer {
                id: id.clone(),
                transform: TransformPatch {
                    x: 10.0,
                    y: 5.0,
                    scale_x: 2.0,
                    scale_y: 2.0,
                    rotation: 15.0,
                    flip_h: None,
                    flip_v: None,
                },
            },
        })
        .unwrap();
    assert_eq!(t.delta.base_version, a.delta.version);
    assert_eq!(eng.snapshot().layers[0].x, 10.0);
    assert_eq!(eng.snapshot().layers[0].resource_id, res_id); // stable
    let o = eng
        .apply(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: None,
            command: Command::SetOpacity {
                id: id.clone(),
                opacity: 0.5,
            },
        })
        .unwrap();
    assert_eq!(eng.snapshot().layers[0].opacity, 0.5);
    assert!(o.delta.changes[0].clone().is_upsert());
    let d = eng
        .apply(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: None,
            command: Command::DeleteLayer { id: id.clone() },
        })
        .unwrap();
    assert_eq!(eng.snapshot().layers.len(), 0);
    assert!(matches!(d.delta.changes[0], LayerChange::Remove { .. }));
}

trait IsUpsert {
    fn is_upsert(&self) -> bool;
}
impl IsUpsert for LayerChange {
    fn is_upsert(&self) -> bool {
        matches!(self, LayerChange::Upsert { .. })
    }
}

#[test]
fn undo_redo_via_delta() {
    let mut eng = DocumentCore::new();
    eng.apply(CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: None,
        command: Command::AddLayer {
            id: "layer-A".into(),
            name: "A".into(),
            width: 100.0,
            height: 100.0,
            index: 999,
            x: None,
            y: None,
            layer_type: None,
            shape_params: None,
            text_data: None,
        },
    })
    .unwrap();
    let id = eng.snapshot().layers[0].id.clone();
    eng.apply(CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: None,
        command: Command::SetOpacity {
            id: id.clone(),
            opacity: 0.3,
        },
    })
    .unwrap();
    assert_eq!(eng.snapshot().layers[0].opacity, 0.3);
    let u = eng
        .apply(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: None,
            command: Command::Undo,
        })
        .unwrap();
    assert_eq!(eng.snapshot().layers[0].opacity, 1.0);
    assert!(u.delta.changes.iter().any(|c| c.is_upsert()));
    let r = eng
        .apply(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: None,
            command: Command::Redo,
        })
        .unwrap();
    assert_eq!(eng.snapshot().layers[0].opacity, 0.3);
    assert!(r.delta.changes.iter().any(|c| c.is_upsert()));
}

#[test]
fn brush_stroke_is_single_command() {
    let mut eng = DocumentCore::new();
    eng.apply(CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: None,
        command: Command::AddLayer {
            id: "layer-L".into(),
            name: "L".into(),
            width: 100.0,
            height: 100.0,
            index: 999,
            x: None,
            y: None,
            layer_type: None,
            shape_params: None,
            text_data: None,
        },
    })
    .unwrap();
    let id = eng.snapshot().layers[0].id.clone();
    let v_before = eng.version();
    let s = eng
        .apply(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: None,
            command: Command::BrushStroke {
                layer_id: id.clone(),
                points: vec![
                    StrokePoint {
                        x: 0.0,
                        y: 0.0,
                        pressure: 0.5,
                    },
                    StrokePoint {
                        x: 10.0,
                        y: 10.0,
                        pressure: 0.8,
                    },
                ],
                settings: BrushSettings {
                    size: 20.0,
                    hardness: 0.5,
                    opacity: 1.0,
                    flow: 1.0,
                },
            },
        })
        .unwrap();
    assert_eq!(s.delta.base_version, v_before);
    assert_eq!(eng.version(), v_before + 1);
    assert_eq!(s.delta.changes.len(), 1);
    // dirtyRect must include footprint, not just point bbox
    if let LayerChange::Upsert { layer } = &s.delta.changes[0] {
        // points 0,0 to 10,10 with size 20 => width/height at least 30
        assert!(layer.dirty_rect.as_ref().unwrap().width >= 30);
    } else {
        panic!("expected upsert");
    }
}

#[test]
fn expected_version_matches_accepted() {
    let mut eng = DocumentCore::new();
    eng.apply(CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: None,
        command: Command::AddLayer {
            id: "layer-A".into(),
            name: "A".into(),
            width: 100.0,
            height: 100.0,
            index: 999,
            x: None,
            y: None,
            layer_type: None,
            shape_params: None,
            text_data: None,
        },
    })
    .unwrap();
    let v = eng.version();
    let r = eng
        .apply(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: Some(v),
            command: Command::AddLayer {
                id: "layer-B".into(),
                name: "B".into(),
                width: 100.0,
                height: 100.0,
                index: 999,
                x: None,
                y: None,
                layer_type: None,
                shape_params: None,
                text_data: None,
            },
        })
        .unwrap();
    assert_eq!(r.delta.base_version, v);
    assert_eq!(eng.snapshot().layers.len(), 2);
}

#[test]
fn expected_version_stale_rejected_document_unchanged() {
    let mut eng = DocumentCore::new();
    eng.apply(CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: None,
        command: Command::AddLayer {
            id: "layer-A".into(),
            name: "A".into(),
            width: 100.0,
            height: 100.0,
            index: 999,
            x: None,
            y: None,
            layer_type: None,
            shape_params: None,
            text_data: None,
        },
    })
    .unwrap();
    let v = eng.version();
    let before_layers = eng.snapshot().layers.clone();
    let err = eng
        .apply(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: Some(999),
            command: Command::AddLayer {
                id: "layer-stale".into(),
                name: "stale".into(),
                width: 100.0,
                height: 100.0,
                index: 0,
                x: None,
                y: None,
                layer_type: None,
                shape_params: None,
                text_data: None,
            },
        })
        .unwrap_err();
    assert_eq!(err.code, "E_VERSION_MISMATCH");
    assert_eq!(eng.version(), v);
    assert_eq!(eng.snapshot().layers, before_layers);
}

#[test]
fn two_concurrent_same_expected_version_exactly_one_accepted() {
    let mut eng = DocumentCore::new();
    eng.apply(CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: None,
        command: Command::AddLayer {
            id: "layer-A".into(),
            name: "A".into(),
            width: 100.0,
            height: 100.0,
            index: 999,
            x: None,
            y: None,
            layer_type: None,
            shape_params: None,
            text_data: None,
        },
    })
    .unwrap();
    let v = eng.version();
    // two concurrent commands with same expected v
    let r1 = eng.apply(CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: Some(v),
        command: Command::AddLayer {
            id: "layer-B".into(),
            name: "B".into(),
            width: 100.0,
            height: 100.0,
            index: 999,
            x: None,
            y: None,
            layer_type: None,
            shape_params: None,
            text_data: None,
        },
    });
    assert!(r1.is_ok());
    let r2 = eng.apply(CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: Some(v),
        command: Command::AddLayer {
            id: "layer-C".into(),
            name: "C".into(),
            width: 100.0,
            height: 100.0,
            index: 999,
            x: None,
            y: None,
            layer_type: None,
            shape_params: None,
            text_data: None,
        },
    });
    assert!(r2.is_err());
    assert_eq!(r2.unwrap_err().code, "E_VERSION_MISMATCH");
    // only one of B/C added
    assert_eq!(eng.snapshot().layers.len(), 2);
}

#[test]
fn retry_after_snapshot_succeeds_against_new_version() {
    let mut eng = DocumentCore::new();
    eng.apply(CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: None,
        command: Command::AddLayer {
            id: "layer-A".into(),
            name: "A".into(),
            width: 100.0,
            height: 100.0,
            index: 999,
            x: None,
            y: None,
            layer_type: None,
            shape_params: None,
            text_data: None,
        },
    })
    .unwrap();
    let v0 = eng.version();
    // stale attempt
    let err = eng
        .apply(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: Some(999),
            command: Command::AddLayer {
                id: "layer-stale".into(),
                name: "stale".into(),
                width: 100.0,
                height: 100.0,
                index: 0,
                x: None,
                y: None,
                layer_type: None,
                shape_params: None,
                text_data: None,
            },
        })
        .unwrap_err();
    assert_eq!(err.code, "E_VERSION_MISMATCH");
    // retry with correct version (snapshot version)
    let r = eng
        .apply(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: Some(v0),
            command: Command::AddLayer {
                id: "layer-B".into(),
                name: "B".into(),
                width: 100.0,
                height: 100.0,
                index: 999,
                x: None,
                y: None,
                layer_type: None,
                shape_params: None,
                text_data: None,
            },
        })
        .unwrap();
    assert_eq!(r.delta.base_version, v0);
    assert_eq!(eng.snapshot().layers.len(), 2);
}
