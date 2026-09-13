// SPDX-License-Identifier: AGPL-3.0-or-later
// Reorder undo/redo walker pins. The undo/redo `diff` must restate layer
// order as an ordered Upsert delta (never an empty delta) so TS hosts restore
// stacking from the delta. The two extracted snapshot-order tests come from
// document_core_arm_tests.rs; the shape-pin tests assert the delta returned by
// undo/redo matches the fixed `ProtocolEngine::diff` contract in
// document_core.rs.
use super::arm_tests::env;
use crate::canonical_model::{
    BlendMode, CanonicalDocument, CanonicalLayer, LayerType, Transform2D,
};
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
    let same: Vec<Arc<_>> = a.0.as_ref().to_vec();
    let b = LayerSet(Arc::new(same));
    let changes = ProtocolEngine::diff(&a, &b);
    assert!(
        changes.is_empty(),
        "identical-order diff must stay empty (guard must not fire)"
    );
}

// A value change (SetOpacity) undo emits a FULL-ORDER restatement (every layer
// upserted in snapshot order), NOT a per-layer value Upsert. The walker delta is
// the authoritative final vector, so the host adopts it verbatim - the changed
// layer's pre-op value is carried by its Upsert in the restated sequence.
#[test]
fn set_opacity_undo_is_full_restatement() {
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
        2,
        "value-change undo restates every layer (full restatement)"
    );
    assert!(
        changes
            .iter()
            .all(|c| matches!(c, RenderLayerChange::Upsert { .. })),
        "value-change undo delta must be all upserts"
    );
    // Order must match the snapshot order; the changed layer carries its pre-op (1.0) value.
    let order: Vec<&str> = changes
        .iter()
        .map(|c| match c {
            RenderLayerChange::Upsert { layer } => layer.id.as_str(),
            _ => unreachable!(),
        })
        .collect();
    assert_eq!(order, vec!["L1", "L2"], "undo restates snapshot order");
    match &changes[0] {
        RenderLayerChange::Upsert { layer } => {
            assert_eq!(layer.id, "L1");
            assert_eq!(layer.opacity, 1.0, "undo restores pre-set opacity");
        }
        _ => panic!("expected upsert, got {:?}", changes[0]),
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

// ── seed_canonical up-projection + walker Remove-guard ──────────
// Self-contained helpers (the canonical-seed helpers live in a sibling module and
// are private, so we re-derive minimal ones here).

fn mk_layer(id: &str, name: &str, rid: u32) -> RenderLayer {
    RenderLayer {
        id: id.to_string(),
        name: name.to_string(),
        visible: true,
        opacity: 1.0,
        resource_id: rid,
        x: 0.0,
        y: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
        rotation: 0.0,
        dirty_rect: None,
        ..Default::default()
    }
}

fn canon_layer(id: &str) -> CanonicalLayer {
    CanonicalLayer {
        id: id.to_string(),
        name: id.to_string(),
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
        resource_id: None,
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
    }
}

fn canon_doc(id: &str, w: f64, h: f64, layers: &[&str]) -> CanonicalDocument {
    CanonicalDocument {
        id: id.to_string(),
        name: "n".to_string(),
        width: w,
        height: h,
        layers: layers.iter().map(|s| canon_layer(s)).collect(),
        selection: None,
    }
}

fn layer_ids(e: &ProtocolEngine) -> Vec<String> {
    e.snapshot().layers.iter().map(|l| l.id.clone()).collect()
}

fn rid_of(e: &ProtocolEngine, id: &str) -> u32 {
    e.snapshot()
        .layers
        .into_iter()
        .find(|l| l.id == id)
        .expect("layer present")
        .resource_id
}

/// Apply a layer-set delta to an id vector the way a host (TS) would, to observe
/// what the host's layer membership becomes after receiving the walker delta.
fn apply_delta_to_ids(ids: &[String], changes: &[RenderLayerChange]) -> Vec<String> {
    let mut set: Vec<String> = ids.to_vec();
    for c in changes {
        match c {
            RenderLayerChange::Upsert { layer } => {
                if let Some(pos) = set.iter().position(|i| i == &layer.id) {
                    set[pos] = layer.id.clone();
                } else {
                    set.push(layer.id.clone());
                }
            }
            RenderLayerChange::Remove { id, .. } => {
                set.retain(|i| i != id);
            }
        }
    }
    set
}

/// open-seed parity. Seeding layers and then a canonical re-push carrying
/// the SAME truth must be idempotent: no layer change, no resource-id change, no
/// document-version bump, and no history entry.
#[test]
fn open_seed_parity_is_idempotent() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(
        vec![mk_layer("bg", "Background", 1), mk_layer("A", "A", 2)],
        0,
    );
    let before_ids = layer_ids(&e);
    let before_rids: Vec<u32> = e.snapshot().layers.iter().map(|l| l.resource_id).collect();
    let before_version = e.version;
    // Re-push the same truth through the canonical channel (the mirrored-commit path).
    e.seed_canonical(canon_doc("doc", 100.0, 100.0, &["bg", "A"]));
    assert_eq!(
        layer_ids(&e),
        before_ids,
        "ids unchanged by idempotent re-push"
    );
    assert_eq!(
        e.snapshot()
            .layers
            .iter()
            .map(|l| l.resource_id)
            .collect::<Vec<u32>>(),
        before_rids,
        "resource ids preserved by idempotent re-push"
    );
    assert_eq!(
        e.version, before_version,
        "seed_canonical never bumps the DV"
    );
    assert_eq!(
        e.entries.len(),
        0,
        "seed_canonical records no history entry"
    );
}

/// a legacy commit re-push carrying a TS-originated layer alongside a
/// known one must mint a fresh non-zero resource id for the unknown layer, preserve
/// the known layer's resource id, record no history entry, and not bump the DV.
#[test]
fn canonical_push_mints_unknown_and_preserves_known_rid() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk_layer("bg", "Background", 1)], 0);
    e.seed_canonical(canon_doc("doc", 100.0, 100.0, &["bg"])); // shadow baseline
    let before_version = e.version;
    // Legacy commit re-push carrying a TS-originated layer `d1` next to `bg`.
    e.seed_canonical(canon_doc("doc", 100.0, 100.0, &["bg", "d1"]));
    assert_eq!(
        layer_ids(&e),
        vec!["bg".to_string(), "d1".to_string()],
        "d1 added in push order"
    );
    assert_eq!(rid_of(&e, "bg"), 1, "known layer resource id preserved");
    let d1_rid = rid_of(&e, "d1");
    assert_ne!(d1_rid, 0, "minted resource id is never 0");
    assert_eq!(d1_rid, 2, "minted from the next_resource frontier");
    assert_eq!(e.version, before_version, "no document-version bump");
    assert_eq!(e.entries.len(), 0, "no history entry for a re-push");
    assert_eq!(
        e.canonical()
            .unwrap()
            .layers
            .iter()
            .map(|l| l.id.clone())
            .collect::<Vec<String>>(),
        vec!["bg".to_string(), "d1".to_string()],
        "shadow mirrors the current engine truth"
    );
}

/// a re-push missing an engine id must drop that id from the native set
/// (a re-push missing an id is a TS-originated delete). NOTE on pixel-store: the
/// engine LayerSet owns no pixel buffers - the host owns pixel lifecycle and orphans
/// buffers until document close (see `structural_arms_never_drop_pixel_store_buffers`),
/// so dropping from the LayerSet needs no additional engine-side cleanup; this is
/// consistent with how the DeleteLayer arm already behaves.
#[test]
fn canonical_push_drops_engine_id_absent_from_push() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk_layer("bg", "Background", 1)], 0);
    // Re-push that OMITS bg (a TS-originated delete mirrored through the channel).
    e.seed_canonical(canon_doc("doc", 100.0, 100.0, &[]));
    assert!(
        layer_ids(&e).is_empty(),
        "engine id absent from the push is dropped"
    );
}

/// the walker guard soundness core. Route AddLayer X, then a canonical re-push
/// that ADDS a foreign layer `d1` (membership change), then undo X. X must be gone
/// but `d1` must SURVIVE: without the walker guard the undo delta would also carry
/// Remove{d1} and the host would lose the foreign layer (the data-loss class). The
/// native undo swaps the layer set to the entry's before (dropping d1 from the
/// engine internals); the guarded DELTA preserves d1, and the next sync re-projects
/// it - this is the disclosed (documented) limitation, healed at the next forward commit.
#[test]
fn undo_of_native_add_keeps_foreign_layer() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk_layer("bg", "Background", 1)], 0);
    // Route AddLayer X (native): before=[bg], after=[X, bg].
    e.apply(env(Command::AddLayer {
        id: "X".into(),
        name: "X".into(),
        width: 10.0,
        height: 10.0,
        index: 0,
        layer_type: None,
        shape_params: None,
        text_data: None,
    }))
    .unwrap();
    // Canonical re-push that adds a foreign layer d1 (membership change).
    e.seed_canonical(canon_doc("doc", 100.0, 100.0, &["X", "bg", "d1"]));
    assert_eq!(
        layer_ids(&e),
        vec!["X".to_string(), "bg".to_string(), "d1".to_string()],
        "push added d1"
    );

    let pre_undo = layer_ids(&e);
    let res = e.apply(env(Command::Undo)).unwrap();
    let changes = res.delta.changes;
    // X is removed (it was introduced by the entry under undo).
    assert!(
        changes
            .iter()
            .any(|c| matches!(c, RenderLayerChange::Remove { id, .. } if id == "X")),
        "undo removes the entry's own introduced layer X"
    );
    // d1 is NOT removed: the guard scopes removal to after \ before = {X} only.
    assert!(
        !changes
            .iter()
            .any(|c| matches!(c, RenderLayerChange::Remove { id, .. } if id == "d1")),
        "undo must NOT remove the foreign layer d1 (data-loss class dead)"
    );
    // Prove the host keeps d1 when it applies the delta.
    let host_after = apply_delta_to_ids(&pre_undo, &changes);
    assert!(!host_after.contains(&"X".to_string()), "host lost X");
    assert!(
        host_after.contains(&"d1".to_string()),
        "host keeps the foreign layer d1"
    );

    // Redo X: restore after=[X, bg]; the redo whitelist is before \ after = {} so d1
    // (already dropped from the engine set by the swap) is never re-removed and the
    // host still keeps it.
    let res_redo = e.apply(env(Command::Redo)).unwrap();
    let changes_redo = res_redo.delta.changes;
    assert!(
        changes_redo
            .iter()
            .any(|c| matches!(c, RenderLayerChange::Upsert { layer } if layer.id == "X")),
        "redo re-adds X"
    );
    assert!(
        !changes_redo
            .iter()
            .any(|c| matches!(c, RenderLayerChange::Remove { id, .. } if id == "d1")),
        "redo must NOT remove the foreign layer d1"
    );
    let host_after_redo = apply_delta_to_ids(&host_after, &changes_redo);
    assert!(
        host_after_redo.contains(&"d1".to_string()),
        "d1 intact after redo"
    );
}

/// Cross-sync reorder-undo pin (honest shape, per audit): a pure reorder undone
/// AFTER a canonical push changed membership. Guarantees asserted DIRECTLY:
/// (1) MEMBERSHIP - d1 survives in the engine AND no Remove(d1) in the delta;
/// (2) ENGINE ORDER - the walker merged the captured entry vector with the
/// foreign survivor. Here the push placed d1 last, and the captured vector
/// [A, B] is restored ahead of it, so the survivor sits at its push position
/// (bounded: a survivor the entry's capture predates is placed by current
/// order, never dropped);
/// (3) DELTA SHAPE - the delta carries the FULL ordered restatement
/// (A, B, d1), so the host's order-aware consumer reconstructs the merged
/// order from the delta alone rather than tail-appending.
/// The counterpart for a host-handoff (external) entry - whose captured set
/// makes the order restorable natively - is covered by the `external_*` tests
/// below.
#[test]
fn reorder_undo_across_membership_change_restores_order_with_foreign_survivor() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk_layer("A", "A", 1), mk_layer("B", "B", 2)], 0);
    // Forward reorder: A <-> B (pure order change, no value change).
    e.apply(env(Command::Reorder {
        id: "A".into(),
        to: 1,
    }))
    .unwrap();
    // TS-sync re-push that ADDS a foreign layer `d1` (membership change).
    e.seed_canonical(canon_doc("doc", 100.0, 100.0, &["B", "A", "d1"]));
    // Undo the reorder.
    let res = e.apply(env(Command::Undo)).unwrap();
    let changes = res.delta.changes;
    // (1) Membership: no Remove(d1) in the delta, and d1 present in the engine.
    assert!(
        !changes
            .iter()
            .any(|c| matches!(c, RenderLayerChange::Remove { id, .. } if id == "d1")),
        "external layer must survive the reorder undo (no Remove in delta)"
    );
    // (2) Engine order: captured [A, B] first, foreign survivor after it at its
    // push position - asserted directly against the engine, independent of the delta.
    assert_eq!(
        layer_ids(&e),
        vec!["A".to_string(), "B".to_string(), "d1".to_string()],
        "engine restores captured order and keeps the foreign survivor at its push position"
    );
    // (3) Delta shape: the merged restate carries the FULL ordered
    // restatement - every merged layer upserted in merged order [A, B, d1].
    // The foreign survivor d1 follows the captured [A, B], which equals its
    // push position here (the push placed it last).
    let upserts: Vec<&str> = changes
        .iter()
        .filter_map(|c| match c {
            RenderLayerChange::Upsert { layer } => Some(layer.id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        upserts,
        vec!["A", "B", "d1"],
        "merged restate upserts every layer in merged order (got {upserts:?})"
    );
}

// -- external (host-handoff) entries capture the pre-sync layer vector --
// A mirrored legacy commit records an External entry. Capturing the engine's
// layer set at record time (an Arc clone under structural sharing) lets the
// walker restore that ORDER natively on undo/redo instead of leaving the
// engine on the post-sync order until the next re-push.

fn upsert_ids(changes: &[RenderLayerChange]) -> Vec<&str> {
    changes
        .iter()
        .filter_map(|c| match c {
            RenderLayerChange::Upsert { layer } => Some(layer.id.as_str()),
            _ => None,
        })
        .collect()
}

/// Mirror the host's two-pass order-aware consumer (`applyDeltaToSnapshot`):
/// apply removes first, then adopt the upsert sequence as the new order when
/// the upserts cover every remaining id. Used to prove a restored layer lands
/// at its snapshot position rather than being tail-appended.
fn apply_ordered_delta_to_ids(ids: &[String], changes: &[RenderLayerChange]) -> Vec<String> {
    let mut set: Vec<String> = ids.to_vec();
    for c in changes {
        if let RenderLayerChange::Remove { id, .. } = c {
            set.retain(|i| i != id);
        }
    }
    let restated: Vec<String> = changes
        .iter()
        .filter_map(|c| match c {
            RenderLayerChange::Upsert { layer } => Some(layer.id.clone()),
            _ => None,
        })
        .collect();
    let adopts = !restated.is_empty() && set.iter().all(|i| restated.iter().any(|r| r == i));
    if adopts {
        restated
    } else {
        set
    }
}

fn record_external(engine: &mut ProtocolEngine, label: &str, affected: &[&str], token: &str) {
    engine.register_adapter("ts-external");
    engine
        .apply(env(Command::RecordExternalTransition {
            label: label.to_string(),
            affected_layer_ids: affected.iter().map(|s| s.to_string()).collect(),
            adapter_id: "ts-external".to_string(),
            token: token.to_string(),
            memory_cost_bytes: 0,
        }))
        .unwrap();
}

/// Undo of an external delete restores the deleted layer at its ORIGINAL
/// mid-stack position (not tail-appended), and the delta is a full ordered
/// restatement so the host's order-aware consumer reconstructs the same order.
#[test]
fn external_delete_undo_restores_mid_stack_order() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(
        vec![
            mk_layer("A", "A", 1),
            mk_layer("B", "B", 2),
            mk_layer("C", "C", 3),
        ],
        0,
    );
    // Legacy delete of B: record the external entry (captures the pre-sync
    // [A, B, C]) then the mirrored re-push up-projects the TS truth [A, C].
    record_external(&mut e, "Delete Layer", &["B"], "tok-del");
    e.seed_canonical(canon_doc("doc", 100.0, 100.0, &["A", "C"]));
    assert_eq!(
        layer_ids(&e),
        vec!["A", "C"],
        "post-delete synced membership"
    );

    // Undo the external delete: B returns mid-stack, and the handoff contract
    // (status + barrier, cursor moved only on commit) is preserved.
    let res = e.apply(env(Command::Undo)).unwrap();
    assert_eq!(
        res.status.as_deref(),
        Some("external"),
        "external handoff status preserved"
    );
    assert_eq!(
        layer_ids(&e),
        vec!["A", "B", "C"],
        "B restored mid-stack, not tail-appended"
    );
    assert_eq!(
        upsert_ids(&res.delta.changes),
        vec!["A", "B", "C"],
        "undo delta carries the ordered full restatement"
    );
    // The host reconstructs [A, B, C] from the delta alone (no snapshot re-read).
    let host_after =
        apply_ordered_delta_to_ids(&["A".to_string(), "C".to_string()], &res.delta.changes);
    assert_eq!(
        host_after,
        vec!["A".to_string(), "B".to_string(), "C".to_string()],
        "host order-aware consumer restores mid-stack order"
    );

    // Clear the handoff barrier (moves the cursor below the entry), then redo.
    e.history_cursor_commit(1, "undo").unwrap();
    assert_eq!(e.cursor(), 0, "cursor committed below the external entry");
    let res_redo = e.apply(env(Command::Redo)).unwrap();
    assert_eq!(res_redo.status.as_deref(), Some("external"));
    assert_eq!(
        layer_ids(&e),
        vec!["A", "C"],
        "redo restores the post-sync membership"
    );
    assert_eq!(
        upsert_ids(&res_redo.delta.changes),
        vec!["A", "C"],
        "redo delta restates the post-sync order"
    );
}

/// A legacy move followed by a native edit: undoing and redoing each step
/// restores the correct ORDER natively (the move's order is not left stale).
#[test]
fn external_move_then_edit_undo_redo_restate_order() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk_layer("A", "A", 1), mk_layer("B", "B", 2)], 0);
    // Legacy reorder A to the bottom: record (captures [A, B]) then sync [B, A].
    record_external(&mut e, "Move Layer", &["A"], "tok-move");
    e.seed_canonical(canon_doc("doc", 100.0, 100.0, &["B", "A"]));
    assert_eq!(layer_ids(&e), vec!["B", "A"], "post-move synced order");
    // Native value edit on top (changes a value, not order).
    e.apply(env(Command::SetOpacity {
        id: "A".into(),
        opacity: 0.4,
    }))
    .unwrap();

    // Undo the edit: the synced order is kept.
    e.apply(env(Command::Undo)).unwrap();
    assert_eq!(
        layer_ids(&e),
        vec!["B", "A"],
        "edit undo keeps the synced order"
    );

    // Undo the external move: the captured pre-move order is restored natively
    // (the pre-fix behavior left the engine on the stale post-move [B, A]).
    let res = e.apply(env(Command::Undo)).unwrap();
    assert_eq!(res.status.as_deref(), Some("external"));
    assert_eq!(
        layer_ids(&e),
        vec!["A", "B"],
        "external undo restores the captured pre-move order"
    );
    assert_eq!(
        upsert_ids(&res.delta.changes),
        vec!["A", "B"],
        "external undo delta restates the captured order"
    );

    // Redo both steps: the external redo restores the post-sync order.
    e.history_cursor_commit(1, "undo").unwrap();
    let res_redo_ext = e.apply(env(Command::Redo)).unwrap();
    assert_eq!(res_redo_ext.status.as_deref(), Some("external"));
    assert_eq!(
        layer_ids(&e),
        vec!["B", "A"],
        "external redo restores the post-sync order"
    );
    assert_eq!(upsert_ids(&res_redo_ext.delta.changes), vec!["B", "A"]);
    e.history_cursor_commit(1, "redo").unwrap();
    e.apply(env(Command::Redo)).unwrap();
    assert_eq!(
        layer_ids(&e),
        vec!["B", "A"],
        "edit redo reapplies the value on the synced order"
    );
}

/// A same-doc panel DnD reorder performed through the external/legacy path must
/// not be snapped back by a later routed reorder, and undoing back through the
/// stack must consume the right entry each step (routed entry, then external).
#[test]
fn external_dnd_reorder_then_routed_reorder_undo_consumes_right_entry() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(
        vec![
            mk_layer("A", "A", 1),
            mk_layer("B", "B", 2),
            mk_layer("C", "C", 3),
        ],
        0,
    );
    // Panel DnD reorder via the legacy/external path: C to the top.
    record_external(&mut e, "Reorder Layer", &["C"], "tok-dnd");
    e.seed_canonical(canon_doc("doc", 100.0, 100.0, &["C", "A", "B"]));
    assert_eq!(
        layer_ids(&e),
        vec!["C", "A", "B"],
        "DnD order synced natively"
    );

    // A routed reorder on top: B to the top. It must apply on the DnD order,
    // not snap back to the pre-DnD order.
    e.apply(env(Command::Reorder {
        id: "B".into(),
        to: 0,
    }))
    .unwrap();
    assert_eq!(
        layer_ids(&e),
        vec!["B", "C", "A"],
        "routed reorder applies on top of the DnD order"
    );

    // Undo the routed reorder -> back to the DnD order.
    e.apply(env(Command::Undo)).unwrap();
    assert_eq!(
        layer_ids(&e),
        vec!["C", "A", "B"],
        "undo routed reorder keeps the DnD order"
    );

    // Undo the DnD (external) reorder -> pre-DnD order, consumed as the external
    // entry (status + barrier), cursor moved on commit.
    let res = e.apply(env(Command::Undo)).unwrap();
    assert_eq!(
        res.status.as_deref(),
        Some("external"),
        "Ctrl+Z lands on the external DnD entry"
    );
    assert_eq!(
        layer_ids(&e),
        vec!["A", "B", "C"],
        "external undo restores the pre-DnD order"
    );
    e.history_cursor_commit(1, "undo").unwrap();
    assert_eq!(
        e.cursor(),
        0,
        "cursor consistent after the external handoff commit"
    );
}

/// An external entry crossed by a later membership-changing re-push: the
/// captured order is restored and the foreign survivor (added after the entry)
/// is retained, never dropped.
#[test]
fn external_undo_retains_foreign_survivor_and_restores_captured_order() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk_layer("A", "A", 1), mk_layer("B", "B", 2)], 0);
    // Legacy reorder A to the bottom: record (captures [A, B]) then sync [B, A].
    record_external(&mut e, "Move Layer", &["A"], "tok-move2");
    e.seed_canonical(canon_doc("doc", 100.0, 100.0, &["B", "A"]));
    // A later re-push adds a foreign layer d1 at the end.
    e.seed_canonical(canon_doc("doc", 100.0, 100.0, &["B", "A", "d1"]));
    assert_eq!(layer_ids(&e), vec!["B", "A", "d1"]);

    let res = e.apply(env(Command::Undo)).unwrap();
    assert_eq!(
        layer_ids(&e),
        vec!["A", "B", "d1"],
        "captured order restored with the foreign survivor retained"
    );
    assert!(
        !res.delta
            .changes
            .iter()
            .any(|c| matches!(c, RenderLayerChange::Remove { id, .. } if id == "d1")),
        "the foreign survivor is never removed (data-loss class dead)"
    );
    assert_eq!(
        upsert_ids(&res.delta.changes),
        vec!["A", "B", "d1"],
        "delta restates the merged order"
    );
}
