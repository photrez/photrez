// SPDX-License-Identifier: AGPL-3.0-or-later
// Reorder undo/redo walker pins. The undo/redo `diff` must restate layer
// order as an ordered Upsert delta (never an empty delta) so TS hosts restore
// stacking from the delta. The two extracted snapshot-order tests come from
// document_core_arm_tests.rs; the shape-pin tests assert the delta returned by
// undo/redo matches the fixed `ProtocolEngine::diff` contract in
// document_core.rs.
use super::arm_tests::env;
use crate::command::*;
use crate::document_core::ProtocolEngine;
use crate::model::{LayerSet, RenderLayer, RenderLayerChange};
use std::sync::Arc;

// ---- extracted from document_core_arm_tests.rs ----

#[test]
fn reorder_undo_restores_layer_order() {
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
    // move L1 (index 0) to index 1 -> [L2, L1]
    e.apply(env(Command::Reorder {
        id: "L1".into(),
        to: 1,
    }))
    .unwrap();
    let snap = e.snapshot();
    let after: Vec<&str> = snap.layers.iter().map(|l| l.id.as_str()).collect();
    assert_eq!(after, vec!["L2", "L1"], "forward reorder");
    // undo -> must restore [L1, L2]
    e.apply(env(Command::Undo)).unwrap();
    let snap2 = e.snapshot();
    let undone: Vec<&str> = snap2.layers.iter().map(|l| l.id.as_str()).collect();
    assert_eq!(undone, vec!["L1", "L2"], "undo must restore order");
}

#[test]
fn reorder_undo_after_addlayer_restores_order() {
    let mut e = ProtocolEngine::new();
    e.apply(env(Command::AddLayer {
        id: "L1".into(),
        name: "A".into(),
        width: 100.0,
        height: 100.0,
        index: 0,
        layer_type: None,
        shape_params: None,
        text_data: None,
    }))
    .unwrap();
    e.apply(env(Command::AddLayer {
        id: "L2".into(),
        name: "B".into(),
        width: 100.0,
        height: 100.0,
        index: 0,
        layer_type: None,
        shape_params: None,
        text_data: None,
    }))
    .unwrap();
    let sb = e.snapshot();
    let before: Vec<&str> = sb.layers.iter().map(|l| l.id.as_str()).collect();
    // move the top layer (index 0) to the bottom -> other layer on top
    let top = before[0].to_string();
    let bottom = before.len() - 1;
    e.apply(env(Command::Reorder {
        id: top.clone(),
        to: bottom,
    }))
    .unwrap();
    let sa = e.snapshot();
    let after: Vec<&str> = sa.layers.iter().map(|l| l.id.as_str()).collect();
    assert_ne!(after, before, "reorder must change order");
    // undo -> must restore the exact pre-reorder order
    e.apply(env(Command::Undo)).unwrap();
    let su = e.snapshot();
    let undone: Vec<&str> = su.layers.iter().map(|l| l.id.as_str()).collect();
    assert_eq!(undone, before, "undo after addlayer must restore order");
}

// ── shape pins: the undo/redo delta must carry the ordered restatement ──

fn two_layer_engine() -> ProtocolEngine {
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
    e
}

// After the engine fix, undo of a pure reorder emits a NON-EMPTY, all-Upsert
// delta whose sequence is the pre-reorder order (every layer restated).
#[test]
fn reorder_undo_delta_is_ordered_restatement() {
    let mut e = two_layer_engine();
    // forward: move L1 (index 0) -> index 1 => [L2, L1]
    e.apply(env(Command::Reorder {
        id: "L1".into(),
        to: 1,
    }))
    .unwrap();
    // undo: restore pre-reorder order [L1, L2]
    let res = e.apply(env(Command::Undo)).unwrap();
    let changes = res.delta.changes;
    assert!(!changes.is_empty(), "undo of reorder must not be empty");
    assert!(
        changes
            .iter()
            .all(|c| matches!(c, RenderLayerChange::Upsert { .. })),
        "undo delta must be all upserts"
    );
    assert_eq!(
        changes.len(),
        2,
        "restatement must upsert every layer, count == layer count"
    );
    let order: Vec<&str> = changes
        .iter()
        .map(|c| match c {
            RenderLayerChange::Upsert { layer } => layer.id.as_str(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(
        order,
        vec!["L1", "L2"],
        "undo delta order must be pre-reorder order"
    );
}

// Redo of a pure reorder emits the inverse restatement (post-reorder order).
#[test]
fn reorder_redo_delta_is_ordered_restatement() {
    let mut e = two_layer_engine();
    e.apply(env(Command::Reorder {
        id: "L1".into(),
        to: 1,
    }))
    .unwrap(); // [L2, L1]
    e.apply(env(Command::Undo)).unwrap(); // [L1, L2]
    let res = e.apply(env(Command::Redo)).unwrap(); // [L2, L1]
    let order: Vec<&str> = res
        .delta
        .changes
        .iter()
        .map(|c| match c {
            RenderLayerChange::Upsert { layer } => layer.id.as_str(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(
        order,
        vec!["L2", "L1"],
        "redo delta must restate post-reorder order"
    );
}

// Direct contract test of `diff`: identical order with identical Arc pointers
// (a genuine no-op) must stay EMPTY - the guard must NOT fire when order is
// identical. NOTE: a same-position Reorder command is NOT an Arc-level no-op
// (insert_at always allocates a fresh Arc), so this pins the guard's
// same_order branch directly rather than through the command API.
#[test]
fn diff_order_identical_is_empty() {
    let e = two_layer_engine();
    let a = e.layers.clone();
    // Rebuild an identical set reusing the SAME Arc<LayerMeta> pointers.
    let same: Vec<Arc<_>> = a.0.as_ref().iter().cloned().collect();
    let b = LayerSet(Arc::new(same));
    let changes = ProtocolEngine::diff(&a, &b);
    assert!(
        changes.is_empty(),
        "identical-order diff must stay empty (guard must not fire)"
    );
}

// A value change (SetOpacity) undo emits a per-layer value Upsert, NOT a
// full-order restatement: the guard must not fire when changes is non-empty.
#[test]
fn set_opacity_undo_is_value_upsert_not_restatement() {
    let mut e = two_layer_engine();
    e.apply(env(Command::SetOpacity {
        id: "L1".into(),
        opacity: 0.3,
    }))
    .unwrap();
    let res = e.apply(env(Command::Undo)).unwrap();
    let changes = res.delta.changes;
    assert_eq!(
        changes.len(),
        1,
        "value-change undo must upsert only the changed layer"
    );
    match &changes[0] {
        RenderLayerChange::Upsert { layer } => {
            assert_eq!(layer.id, "L1");
            assert_eq!(layer.opacity, 1.0, "undo restores pre-set opacity");
        }
        _ => panic!("expected single value upsert, got {:?}", changes[0]),
    }
}

// Undo with nothing on the stack (cursor at 0) returns an empty delta.
#[test]
fn undo_at_cursor_zero_is_empty() {
    let mut e = two_layer_engine();
    let res = e.apply(env(Command::Undo)).unwrap();
    assert!(
        res.delta.changes.is_empty(),
        "undo with nothing to undo is empty"
    );
}
