use super::*;
use crate::command::*;
use crate::document_core::ProtocolEngine;
use crate::history::*;
use std::sync::Arc;

#[test]
fn contract_and_document_versions_are_distinct() {
    let mut eng = ProtocolEngine::new();
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
    let mut eng = ProtocolEngine::new();
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
    let mut eng = ProtocolEngine::new();
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
    let mut eng = ProtocolEngine::new();
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
    assert!(matches!(
        d.delta.changes[0],
        RenderLayerChange::Remove { .. }
    ));
}

trait IsUpsert {
    fn is_upsert(&self) -> bool;
}
impl IsUpsert for RenderLayerChange {
    fn is_upsert(&self) -> bool {
        matches!(self, RenderLayerChange::Upsert { .. })
    }
}

#[test]
fn undo_redo_via_delta() {
    let mut eng = ProtocolEngine::new();
    eng.apply(CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: None,
        command: Command::AddLayer {
            id: "layer-A".into(),
            name: "A".into(),
            width: 100.0,
            height: 100.0,
            index: 999,
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
    let mut eng = ProtocolEngine::new();
    eng.apply(CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: None,
        command: Command::AddLayer {
            id: "layer-L".into(),
            name: "L".into(),
            width: 100.0,
            height: 100.0,
            index: 999,
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
    if let RenderLayerChange::Upsert { layer } = &s.delta.changes[0] {
        // points 0,0 to 10,10 with size 20 => width/height at least 30
        assert!(layer.dirty_rect.as_ref().unwrap().width >= 30);
    } else {
        panic!("expected upsert");
    }
}

#[test]
fn expected_version_matches_accepted() {
    let mut eng = ProtocolEngine::new();
    eng.apply(CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: None,
        command: Command::AddLayer {
            id: "layer-A".into(),
            name: "A".into(),
            width: 100.0,
            height: 100.0,
            index: 999,
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
            },
        })
        .unwrap();
    assert_eq!(r.delta.base_version, v);
    assert_eq!(eng.snapshot().layers.len(), 2);
}

#[test]
fn expected_version_stale_rejected_document_unchanged() {
    let mut eng = ProtocolEngine::new();
    eng.apply(CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: None,
        command: Command::AddLayer {
            id: "layer-A".into(),
            name: "A".into(),
            width: 100.0,
            height: 100.0,
            index: 999,
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
            },
        })
        .unwrap_err();
    assert_eq!(err.code, "E_VERSION_MISMATCH");
    assert_eq!(eng.version(), v);
    assert_eq!(eng.snapshot().layers, before_layers);
}

#[test]
fn two_concurrent_same_expected_version_exactly_one_accepted() {
    let mut eng = ProtocolEngine::new();
    eng.apply(CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: None,
        command: Command::AddLayer {
            id: "layer-A".into(),
            name: "A".into(),
            width: 100.0,
            height: 100.0,
            index: 999,
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
        },
    });
    assert!(r2.is_err());
    assert_eq!(r2.unwrap_err().code, "E_VERSION_MISMATCH");
    // only one of B/C added
    assert_eq!(eng.snapshot().layers.len(), 2);
}

#[test]
fn retry_after_snapshot_succeeds_against_new_version() {
    let mut eng = ProtocolEngine::new();
    eng.apply(CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: None,
        command: Command::AddLayer {
            id: "layer-A".into(),
            name: "A".into(),
            width: 100.0,
            height: 100.0,
            index: 999,
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
            },
        })
        .unwrap();
    assert_eq!(r.delta.base_version, v0);
    assert_eq!(eng.snapshot().layers.len(), 2);
}

// ── Structural-sharing / no-aliasing (reviewer finding) ──────────────
fn make_layer_meta(name: &str) -> LayerMeta {
    RenderLayer {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.to_string(),
        visible: true,
        opacity: 1.0,
        resource_id: 0,
        x: 0.0,
        y: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
        rotation: 0.0,
        dirty_rect: Some(Rect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        }),
        ..Default::default()
    }
}

fn env(cmd: Command) -> CommandEnvelope {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: None,
        command: cmd,
    }
}

#[test]
fn structural_sharing_unchanged_layer_shares_arc_ptr_across_replace() {
    let l1 = make_layer_meta("L1");
    let l2 = make_layer_meta("L2");
    let set = LayerSet::empty().pushed(l1.clone()).pushed(l2.clone());

    // Build an "after" state that only changes L1 via COW replace.
    let mut edited_l1 = l1.clone();
    edited_l1.opacity = 0.5;
    let after = set.replaced(0, edited_l1);

    // Unchanged L2 keeps the SAME `Arc<LayerMeta>` allocation across before/after.
    assert!(
        Arc::ptr_eq(&set.0[1], &after.0[1]),
        "unchanged layer must share its Arc pointer across before/after",
    );
    // And its value is untouched by the L1 edit.
    assert_eq!(after.0[1].as_ref(), &l2);

    // The changed L1 got a NEW Arc allocation (COW), never an in-place mutation.
    assert!(
        !Arc::ptr_eq(&set.0[0], &after.0[0]),
        "changed layer must get a fresh Arc allocation",
    );
    assert_ne!(after.0[0].as_ref(), &l1, "edited value must differ");
}

#[test]
fn no_aliasing_editing_one_layer_does_not_mutate_shared_sibling() {
    let l1 = make_layer_meta("L1");
    let l2 = make_layer_meta("L2");
    let set = LayerSet::empty().pushed(l1.clone()).pushed(l2.clone());

    // COW-edit L1 (index 0); L2 (index 1) must be pointer- & value-identical.
    let mut edited_l1 = l1.clone();
    edited_l1.x = 42.0;
    let after = set.replaced(0, edited_l1);

    assert!(Arc::ptr_eq(&set.0[1], &after.0[1]));
    assert_eq!(after.0[1].as_ref(), &l2, "sibling value must be unchanged");
    assert_eq!(set.0[0].as_ref().x, 0.0, "original set's L1 untouched");
    assert_eq!(after.0[0].as_ref().x, 42.0, "edited set's L1 changed");
}

#[test]
fn cow_builders_share_untouched_arcs_and_only_changed_layer_is_new() {
    let base = LayerSet::empty()
        .pushed(make_layer_meta("L1"))
        .pushed(make_layer_meta("L2"));
    let base_l1 = &base.0[0];
    let base_l2 = &base.0[1];

    // pushed: appends a new layer; existing Arc pointers shared.
    let pushed = base.pushed(make_layer_meta("L3"));
    assert_eq!(pushed.0.len(), 3);
    assert!(Arc::ptr_eq(base_l1, &pushed.0[0]));
    assert!(Arc::ptr_eq(base_l2, &pushed.0[1]));
    assert!(!Arc::ptr_eq(base_l1, &pushed.0[2])); // new layer distinct

    // replaced: index 1 becomes a fresh Arc; index 0 shared.
    let replaced = base.replaced(1, make_layer_meta("L2b"));
    assert!(Arc::ptr_eq(base_l1, &replaced.0[0]));
    assert!(!Arc::ptr_eq(base_l2, &replaced.0[1]));
    assert_eq!(replaced.0[1].as_ref().name, "L2b");

    // removed: index 0 removed; the surviving layer keeps its Arc pointer.
    let removed = base.removed(0);
    assert_eq!(removed.0.len(), 1);
    assert!(Arc::ptr_eq(base_l2, &removed.0[0]));
}

#[test]
fn engine_native_entry_before_after_share_unchanged_layer_arcs() {
    let mut eng = ProtocolEngine::new();
    eng.apply(env(Command::AddLayer {
        id: "layer-A".into(),
        name: "A".into(),
        width: 100.0,
        height: 100.0,
        index: 999,
    }))
    .unwrap();
    eng.apply(env(Command::AddLayer {
        id: "layer-B".into(),
        name: "B".into(),
        width: 100.0,
        height: 100.0,
        index: 999,
    }))
    .unwrap();
    // Transform A (index 0): B must stay a shared Arc across the entry's
    // before/after LayerSets.
    let id_a = eng.snapshot().layers[0].id.clone();
    eng.apply(env(Command::TransformLayer {
        id: id_a,
        transform: TransformPatch {
            x: 5.0,
            y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
        },
    }))
    .unwrap();

    let entry = eng.entries.last().unwrap();
    match &entry.payload {
        EntryPayload::Native { before, after } => {
            assert_eq!(before.0.len(), 2);
            assert_eq!(after.0.len(), 2);
            // B (index 1) is unchanged -> same Arc pointer in before & after.
            assert!(Arc::ptr_eq(&before.0[1], &after.0[1]));
            // A (index 0) changed -> fresh Arc pointer.
            assert!(!Arc::ptr_eq(&before.0[0], &after.0[0]));
            // Undo swaps back via O(1) Arc/clone, so before is the ORIGINAL.
            assert_eq!(before.0[0].as_ref().x, 0.0);
            assert_eq!(after.0[0].as_ref().x, 5.0);
        }
        _ => panic!("expected Native payload"),
    }
}

#[test]
fn native_entry_memory_cost_does_not_double_count_shared_layers() {
    let mut eng = ProtocolEngine::new();
    eng.apply(env(Command::AddLayer {
        id: "layer-A".into(),
        name: "A".into(),
        width: 100.0,
        height: 100.0,
        index: 999,
    }))
    .unwrap();
    eng.apply(env(Command::AddLayer {
        id: "layer-B".into(),
        name: "B".into(),
        width: 100.0,
        height: 100.0,
        index: 999,
    }))
    .unwrap();
    let id_a = eng.snapshot().layers[0].id.clone();
    eng.apply(env(Command::TransformLayer {
        id: id_a,
        transform: TransformPatch {
            x: 5.0,
            y: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
        },
    }))
    .unwrap();

    let entry = eng.entries.last().unwrap();
    let cost = entry.memory_cost_bytes;
    // Unique-allocation cost must be strictly below the naive before+after
    // per-set sum, which double-counts the shared B layer.
    let naive_sum = match &entry.payload {
        EntryPayload::Native { before, after } => {
            let b: u64 = before
                .iter()
                .map(|a| ProtocolEngine::estimate_layer_meta_bytes(a.as_ref()))
                .sum();
            let a: u64 = after
                .iter()
                .map(|a| ProtocolEngine::estimate_layer_meta_bytes(a.as_ref()))
                .sum();
            b + a
        }
        _ => panic!("expected Native payload"),
    };
    assert!(cost > 0, "memory cost must be non-zero");
    assert!(
        cost < naive_sum,
        "unique-count cost must be below the double-counting before+after sum",
    );
}

// -- Wire-format contract (photrez-counter residual 1) ----------------
// `#[serde(rename_all = "camelCase", tag = "type")]` on the `Command` enum
// renames the VARIANT only - NOT the fields of a struct variant. So the
// `BrushStroke` variant carries `layer_id` (snake_case), not `layerId`. The
// TS sender in bridge.ts must emit `layer_id` on the wire. This pins that
// contract so a future `rename_all_fields` or a TS sender regression is
// caught here.
#[test]
fn brush_stroke_wire_format_expects_snake_case_layer_id() {
    let ok: CommandEnvelope = serde_json::from_str(
            r#"{"contractVersion":1,"command":{"type":"brushStroke","layer_id":"L1","points":[{"x":0,"y":0,"pressure":0.5}],"settings":{"size":20,"hardness":0.5,"opacity":1,"flow":1}}}"#,
        )
        .unwrap();
    match ok.command {
        Command::BrushStroke { layer_id, .. } => assert_eq!(layer_id, "L1"),
        _ => panic!("expected brushStroke"),
    }

    // The camelCase `layerId` sender (pre-fix bridge.ts) must be REJECTED.
    let err = serde_json::from_str::<CommandEnvelope>(
        r#"{"contractVersion":1,"command":{"type":"brushStroke","layerId":"L1","points":[{"x":0,"y":0,"pressure":0.5}],"settings":{"size":20,"hardness":0.5,"opacity":1,"flow":1}}}"#,
    );
    assert!(
        err.is_err(),
        "camelCase layerId must be rejected: the enum variant field is snake_case layer_id",
    );
}

// -- N>=3-step undo/redo restores the EXACT layer-set (photrez-counter residual 3) --
// Applies a multi-step forward sequence then walks Undo xN / Redo xN and
// asserts the EXACT layer-set (resource_id/opacity/transform/name) is
// restored field-by-field at every step - proving the sequence restore is
// exact under structural sharing, not just a 1-step happy path.
#[test]
fn multi_step_undo_redo_restores_exact_field_state() {
    let mut eng = ProtocolEngine::new();
    let mut checkpoints: Vec<Vec<RenderLayer>> = Vec::new();

    eng.apply(env(Command::AddLayer {
        id: "layer-base".into(),
        name: "base".into(),
        width: 100.0,
        height: 100.0,
        index: 0,
    }))
    .unwrap();
    checkpoints.push(eng.snapshot().layers);
    eng.apply(env(Command::AddLayer {
        id: "layer-flip".into(),
        name: "flip".into(),
        width: 100.0,
        height: 100.0,
        index: 1,
    }))
    .unwrap();
    checkpoints.push(eng.snapshot().layers);

    let base_id = checkpoints[0][0].id.clone();
    let base_res = checkpoints[0][0].resource_id;
    let flip_id = checkpoints[1][1].id.clone();
    let flip_res = checkpoints[1][1].resource_id;
    let base_name = checkpoints[0][0].name.clone();
    let flip_name = checkpoints[1][1].name.clone();

    eng.apply(env(Command::TransformLayer {
        id: base_id.clone(),
        transform: TransformPatch {
            x: 10.0,
            y: 5.0,
            scale_x: 2.0,
            scale_y: 3.0,
            rotation: 45.0,
        },
    }))
    .unwrap();
    checkpoints.push(eng.snapshot().layers); // S3

    eng.apply(env(Command::SetOpacity {
        id: flip_id.clone(),
        opacity: 0.25,
    }))
    .unwrap();
    checkpoints.push(eng.snapshot().layers); // S4

    eng.apply(env(Command::DeleteLayer {
        id: flip_id.clone(),
    }))
    .unwrap();
    checkpoints.push(eng.snapshot().layers); // S5 (tip)

    // Forward tip is field-exact: transformed "base" survives, "flip" gone.
    assert_eq!(checkpoints.len(), 5);
    assert_eq!(eng.cursor(), 5);
    let tip = &checkpoints[4];
    assert_eq!(tip.len(), 1);
    assert_eq!(tip[0].id, base_id);
    assert_eq!(tip[0].name, base_name);
    assert_eq!(tip[0].resource_id, base_res);
    assert_eq!(tip[0].x, 10.0);
    assert_eq!(tip[0].scale_y, 3.0);
    assert_eq!(tip[0].rotation, 45.0);
    assert_eq!(tip[0].opacity, 1.0);

    // Undo x5 walks back through every forward state, restoring the EXACT
    // layer-set (all fields) at each step, ending at the empty start state.
    for k in (0..5).rev() {
        eng.apply(env(Command::Undo)).unwrap();
        let cur = eng.snapshot().layers;
        let expected: Vec<RenderLayer> = if k == 0 {
            Vec::new()
        } else {
            checkpoints[k - 1].clone()
        };
        assert_eq!(
            cur,
            expected,
            "undo step from S{} must restore the exact layer-set of S{}",
            k + 1,
            k,
        );
    }
    assert_eq!(eng.snapshot().layers.len(), 0);
    assert_eq!(eng.cursor(), 0);

    // Redo x5 walks forward, re-materializing the exact field state again.
    for k in 0..5 {
        eng.apply(env(Command::Redo)).unwrap();
        let cur = eng.snapshot().layers;
        assert_eq!(
            cur,
            checkpoints[k],
            "redo step to S{} must restore the exact layer-set",
            k + 1,
        );
    }
    assert_eq!(eng.cursor(), 5);
    // At the redo tip (S5 = deleteLayer) only transformed "base" survives -
    // flip was deleted by step 5, so field-exact identity of BOTH layers is
    // verified at the intermediate redo checkpoints (S1..S4) above rather
    // than at the tip.
    assert_eq!(eng.snapshot().layers.len(), 1);
    assert_eq!(eng.snapshot().layers[0].resource_id, base_res);
    assert_eq!(eng.snapshot().layers[0].name, base_name);
    assert_eq!(eng.snapshot().layers[0].x, 10.0);
    assert_eq!(eng.snapshot().layers[0].opacity, 1.0);
    // flip_res / flip_name / flip opacity 0.25 were each restored exactly at
    // the S2/S3/S4 redo checkpoints (compare against checkpoints[k]) - the
    // redo loop above asserts Vec<RenderLayer> equality for every step.
    assert_eq!(flip_res, checkpoints[1][1].resource_id);
    assert_eq!(flip_name, checkpoints[1][1].name);
    assert_eq!(checkpoints[3][1].opacity, 0.25);
}

// ── Seed primitive (initial layer load) ───────────────────────────────
// Builds a full `RenderLayer` for seed tests with the supplied id/name. Ids are
// explicit (the whole point of seeding is id-preservation), everything else is
// default so the assertions focus on id + version + history behavior.
fn seed_layer(id: &str, name: &str) -> RenderLayer {
    RenderLayer {
        id: id.to_string(),
        name: name.to_string(),
        visible: true,
        opacity: 1.0,
        resource_id: 0,
        x: 0.0,
        y: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
        rotation: 0.0,
        dirty_rect: None,
        ..Default::default()
    }
}

#[test]
fn seed_loads_explicit_ids_no_uuid_minting() {
    let mut eng = ProtocolEngine::new();
    let layers = vec![seed_layer("L-fixed-1", "A"), seed_layer("L-fixed-2", "B")];
    eng.seed_layers(layers, 7);
    // (a) snapshot shows the EXACT supplied ids (no uuid minting).
    let snap = eng.snapshot();
    assert_eq!(snap.layers.len(), 2);
    assert_eq!(snap.layers[0].id, "L-fixed-1");
    assert_eq!(snap.layers[1].id, "L-fixed-2");
    // (b) version aligned to the supplied value.
    assert_eq!(eng.version(), 7);
    // (e) seed is silent: no history entry.
    assert_eq!(eng.entries.len(), 0);
    assert_eq!(eng.history_query().entries.len(), 0);
}

#[test]
fn seed_then_add_layer_uses_expected_version_without_mismatch() {
    let mut eng = ProtocolEngine::new();
    eng.seed_layers(vec![seed_layer("L-1", "A")], 7);
    // (c) a subsequent AddLayer with expected_version == seeded version is accepted.
    let r = eng
        .apply(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: Some(7),
            command: Command::AddLayer {
                id: "layer-B".into(),
                name: "B".into(),
                width: 100.0,
                height: 100.0,
                index: 999,
            },
        })
        .unwrap();
    assert_eq!(r.delta.base_version, 7);
    assert_eq!(eng.snapshot().layers.len(), 2);
    // And expected_version 0 (stale vs seeded 7) is rejected as before.
    let err = eng
        .apply(CommandEnvelope {
            contract_version: CONTRACT_VERSION,
            expected_version: Some(0),
            command: Command::AddLayer {
                id: "layer-C".into(),
                name: "C".into(),
                width: 100.0,
                height: 100.0,
                index: 999,
            },
        })
        .unwrap_err();
    assert_eq!(err.code, "E_VERSION_MISMATCH");
    assert_eq!(
        eng.snapshot().layers.len(),
        2,
        "rejected command is a no-op"
    );
}

#[test]
fn seed_is_idempotent_and_only_when_empty() {
    let mut eng = ProtocolEngine::new();
    eng.seed_layers(vec![seed_layer("L-1", "A")], 5);
    // (d) second seed on the now-populated engine is a silent no-op.
    eng.seed_layers(vec![seed_layer("DIFFERENT", "Z")], 99);
    let snap = eng.snapshot();
    assert_eq!(snap.layers.len(), 1, "second seed must not clobber");
    assert_eq!(snap.layers[0].id, "L-1", "original seeded id preserved");
    assert_eq!(eng.version(), 5, "version unchanged by second seed");

    // (d) seed after a real command (engine non-empty) is also a no-op.
    eng.apply(env(Command::AddLayer {
        id: "layer-B".into(),
        name: "B".into(),
        width: 100.0,
        height: 100.0,
        index: 999,
    }))
    .unwrap();
    let before = eng.snapshot().layers.len();
    eng.seed_layers(vec![seed_layer("X", "Y")], 123);
    assert_eq!(
        eng.snapshot().layers.len(),
        before,
        "seed on non-empty engine is a no-op"
    );
    assert_eq!(eng.version(), 6, "version unchanged by seed on non-empty");
}

#[test]
fn seed_creates_no_history_entry_even_after_real_command() {
    let mut eng = ProtocolEngine::new();
    let n0 = eng.entries.len();
    // (e) seed pushes no history entry.
    eng.seed_layers(vec![seed_layer("L-1", "A")], 3);
    assert_eq!(eng.entries.len(), n0, "seed pushes no history entry");
    // A real command afterward creates exactly one entry.
    eng.apply(env(Command::AddLayer {
        id: "layer-B".into(),
        name: "B".into(),
        width: 100.0,
        height: 100.0,
        index: 999,
    }))
    .unwrap();
    assert_eq!(
        eng.entries.len(),
        n0 + 1,
        "only the real command is recorded"
    );
    // Undo removes JUST the AddLayer; the seeded layer (which had no entry)
    // remains, proving seed did not create an undo step.
    eng.apply(env(Command::Undo)).unwrap();
    let snap = eng.snapshot();
    assert_eq!(snap.layers.len(), 1, "undo reverts only the AddLayer");
    assert_eq!(snap.layers[0].id, "L-1", "seeded layer survives undo");
}

// Builds a seeded `RenderLayer` carrying an EXPLICIT resource_id. The default
// `seed_layer` helper always uses 0, so without this the next_resource bump
// (max seeded resource + 1) is never exercised by any test.
fn seed_layer_res(id: &str, name: &str, res: ResourceId) -> RenderLayer {
    let mut l = seed_layer(id, name);
    l.resource_id = res;
    l
}

#[test]
fn seed_bumps_next_resource_above_seeded_max() {
    let mut eng = ProtocolEngine::new();
    // Seed a layer that already owns resource_id 9.
    eng.seed_layers(vec![seed_layer_res("L-1", "A", 9)], 7);
    // A later AddLayer must take resource_id 10 (max+1), not collide with 9.
    eng.apply(CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: Some(7),
        command: Command::AddLayer {
            id: "layer-B".into(),
            name: "B".into(),
            width: 100.0,
            height: 100.0,
            index: 999,
        },
    })
    .unwrap();
    let snap = eng.snapshot();
    let added = snap
        .layers
        .iter()
        .find(|l| l.name == "B")
        .expect("added layer present");
    assert_eq!(
        added.resource_id, 10,
        "AddLayer must not collide with seeded resource_id 9"
    );
    assert_eq!(snap.layers[0].resource_id, 9, "seeded layer preserved at 9");
}

#[test]
fn seed_next_resource_is_max_plus_one_not_count() {
    let mut eng = ProtocolEngine::new();
    // Two seeded layers with a gap: 3 and 9 (not contiguous, not count-based).
    eng.seed_layers(
        vec![seed_layer_res("L-1", "A", 3), seed_layer_res("L-2", "B", 9)],
        7,
    );
    eng.apply(CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: Some(7),
        command: Command::AddLayer {
            id: "layer-C".into(),
            name: "C".into(),
            width: 100.0,
            height: 100.0,
            index: 999,
        },
    })
    .unwrap();
    let snap = eng.snapshot();
    let added = snap
        .layers
        .iter()
        .find(|l| l.name == "C")
        .expect("added layer present");
    assert_eq!(
        added.resource_id, 10,
        "must be max(3,9)+1 = 10, not count-based"
    );
    assert!(snap.layers.iter().any(|l| l.resource_id == 3));
    assert!(snap.layers.iter().any(|l| l.resource_id == 9));
}
