// SPDX-License-Identifier: AGPL-3.0-or-later
// External (host-handoff) metadata undo/redo: what the engine can restore on
// its own, and where the `after` slot comes from.
//
// THE HOST SEQUENCE A metadata commit runs, in this order:
//   1. pre-op canonical push   -> seed_canonical(pre-op vector)
//   2. record                  -> RecordExternalTransition (captures `before`)
//   3. post-op canonical push  -> seed_canonical(post-op vector) fills `after`
// then the host's undo drives Command::Undo. Steps 1 and 3 are the two
// `repushCanonical*` invokes in `recordExternalTransitionFor`
// (apps/desktop/src/lib/protocol/facadeRegistry.ts:1121 and :1180); step 2 is
// the `applyCommand` at :1126; the undo is `facade.undo()`.
//
// This is the DEFAULT configuration, not a gated one: `installFacadeCommitShim`
// is called unconditionally from EditorShell, it exits early only when
// `isFacadeEnabled()` is false, and that predicate is
// `localStorage["photrez.facade"] !== "0"` - so the mirror runs unless the flag
// is explicitly set to "0". `isNativeAuthority()` is likewise native by default,
// which is what makes the two canonical pushes reach `seed_canonical`.
//
// The claim this file settles: an External undo restores the captured `before`
// vector, and `restore_external_layers` gates that on `after` being present.
// So: for every shape where `after` is still None at the undo, does dropping
// that gate change ANY observable - the emitted delta, or the layer vector the
// engine ends up with? `differential_*` below answers that by running the
// early-return answer and the ungated answer side by side over the reachable
// shapes.
use super::arm_tests::env;
use crate::canonical_model::{
    BlendMode, CanonicalDocument, CanonicalLayer, LayerType, Transform2D,
};
use crate::command::*;
use crate::document_core::ProtocolEngine;
use crate::model::{LayerSet, RenderLayer, RenderLayerChange};

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

fn canon_doc(layers: &[&str]) -> CanonicalDocument {
    CanonicalDocument {
        id: "doc".to_string(),
        name: "n".to_string(),
        width: 100.0,
        height: 100.0,
        layers: layers.iter().map(|s| canon_layer(s)).collect(),
        selection: None,
    }
}

fn seed_layers(engine: &mut ProtocolEngine, ids: &[&str]) {
    engine.seed_layers(
        ids.iter()
            .enumerate()
            .map(|(i, id)| RenderLayer {
                id: (*id).to_string(),
                name: (*id).to_string(),
                resource_id: (i + 1) as u32,
                ..Default::default()
            })
            .collect(),
        0,
    );
}

fn record(
    engine: &mut ProtocolEngine,
    label: &str,
    affected: &[&str],
    token: &str,
    minted: &[&str],
) -> u64 {
    engine.register_adapter("ts-external");
    engine
        .apply(env(Command::RecordExternalTransition {
            label: label.to_string(),
            affected_layer_ids: affected.iter().map(|s| s.to_string()).collect(),
            adapter_id: "ts-external".to_string(),
            token: token.to_string(),
            memory_cost_bytes: 0,
            doc_size_before: None,
            doc_size_after: None,
            minted_layer_ids: minted.iter().map(|s| s.to_string()).collect(),
        }))
        .expect("record external");
    engine
        .history_query()
        .entries
        .last()
        .expect("entry recorded")
        .seq
}

fn layer_ids(engine: &ProtocolEngine) -> Vec<String> {
    engine
        .snapshot()
        .layers
        .iter()
        .map(|l| l.id.clone())
        .collect()
}

fn delta_ids(changes: &[RenderLayerChange]) -> Vec<String> {
    changes
        .iter()
        .map(|c| match c {
            RenderLayerChange::Remove { id, .. } => format!("-{id}"),
            RenderLayerChange::Upsert { layer } => format!("+{}", layer.id),
        })
        .collect()
}

/// What the UNGATED restore would produce for the External entry at `idx`: the
/// `before`-only merge, which is what `restore_external_layers` computes when
/// `after` is present (and provably ignores it - see `undo_path_ignores_the_post_sync_side`).
fn ungated_undo(engine: &ProtocolEngine, idx: usize) -> (Vec<String>, Vec<String>) {
    let (before, minted) = match &engine.entries[idx].payload {
        crate::history::EntryPayload::External {
            before,
            minted_layer_ids,
            ..
        } => (before.clone(), minted_layer_ids.clone()),
        _ => panic!("not an external entry"),
    };
    let other = before.clone();
    let new_layers: LayerSet = ProtocolEngine::restore_with_foreign_excluding_minted(
        &engine.layers,
        &before,
        &other,
        false,
        &minted,
    );
    (
        delta_ids(&ProtocolEngine::diff_walker(&engine.layers, &new_layers)),
        new_layers.iter().map(|l| l.id.clone()).collect(),
    )
}

/// THE PREMISE UNDER TEST. An undo passes the post-sync vector as `other_side`
/// with `other_side_proves_creation = false`. In
/// `restore_with_foreign_excluding_minted` that flag makes the survivor test
/// `!other_ids.contains(id) || !other_side_proves_creation` unconditionally
/// true, so `other_side` cannot influence the result at all. If this holds, the
/// `after: None` early return in `restore_external_layers` suppresses nothing:
/// the merge it would skip is the merge it already computes.
///
/// Falsifiable both ways: the assert below fails if `other_side` ever starts
/// mattering, and the shapes below fail if the flag ever stops holding.
#[test]
fn undo_path_ignores_the_post_sync_side() {
    use std::sync::Arc;
    let captured = LayerSet(Arc::new(vec![Arc::new(mk("A")), Arc::new(mk("B"))]));
    let current = LayerSet(Arc::new(vec![
        Arc::new(mk("A")),
        Arc::new(mk("B")),
        Arc::new(mk("host")),
    ]));
    for other_side in [
        LayerSet(Arc::new(vec![Arc::new(mk("A")), Arc::new(mk("B"))])),
        LayerSet(Arc::new(vec![Arc::new(mk("A"))])),
        LayerSet(Arc::new(vec![
            Arc::new(mk("A")),
            Arc::new(mk("B")),
            Arc::new(mk("dest")),
        ])),
        LayerSet::empty(),
    ] {
        let merged = ProtocolEngine::restore_with_foreign_excluding_minted(
            &current,
            &captured,
            &other_side,
            false,
            &[],
        );
        assert_eq!(
            merged.iter().map(|l| l.id.clone()).collect::<Vec<_>>(),
            vec!["A", "B", "host"],
            "the undo merge must not depend on the post-sync side (other={:?})",
            other_side.iter().map(|l| l.id.clone()).collect::<Vec<_>>()
        );
    }
    // Control: the REDO direction passes proves_creation = true, where the same
    // `other_side` DOES change the answer. A redo target that dropped `B` must
    // re-remove it, and that removal is decided purely by the pre-sync side
    // naming it. Without this the assertion above would also pass if the
    // argument were simply ignored everywhere.
    let post_undo = LayerSet(Arc::new(vec![Arc::new(mk("A")), Arc::new(mk("B"))]));
    let redo_target = LayerSet(Arc::new(vec![Arc::new(mk("A"))]));
    let redo_merged = ProtocolEngine::restore_with_foreign_excluding_minted(
        &post_undo,
        &redo_target,
        &post_undo,
        true,
        &[],
    );
    assert_eq!(
        redo_merged.iter().map(|l| l.id.clone()).collect::<Vec<_>>(),
        vec!["A"],
        "the redo direction must still use the pre-sync side to re-remove ids"
    );
    let undo_merged = ProtocolEngine::restore_with_foreign_excluding_minted(
        &post_undo,
        &redo_target,
        &post_undo,
        false,
        &[],
    );
    assert_eq!(
        undo_merged.iter().map(|l| l.id.clone()).collect::<Vec<_>>(),
        vec!["A", "B"],
        "the same call on the undo direction keeps B - the flag is the difference"
    );
}

fn mk(id: &str) -> crate::model::LayerMeta {
    crate::model::LayerMeta {
        id: id.to_string(),
        resource_id: 1,
        ..Default::default()
    }
}

/// THE MEASUREMENT. The four reachable shapes in which an External entry reaches
/// its undo with `after` still None, each compared against the ungated answer.
/// `after_present` records whether the shipping post-op push actually filled
/// the slot, so a shape that silently starts arriving with `after` shows up
/// here instead of quietly proving nothing.
fn differential(name: &str, build: impl Fn(&mut ProtocolEngine)) {
    let mut e = ProtocolEngine::new();
    build(&mut e);
    let idx = e.cursor() - 1;
    let after_present = match &e.entries[idx].payload {
        crate::history::EntryPayload::External { after, .. } => after.is_some(),
        _ => panic!("{name}: tip is not External"),
    };
    let (gated_delta, gated_layers) = {
        let before_ids = layer_ids(&e);
        let res = e.apply(env(Command::Undo)).expect("undo");
        (delta_ids(&res.delta.changes), before_ids)
    };
    let _ = gated_layers;
    // Rebuild to read the ungated answer without the gate having consumed state.
    let mut e2 = ProtocolEngine::new();
    build(&mut e2);
    let (ungated_delta, ungated_layers) = ungated_undo(&e2, e2.cursor() - 1);

    println!(
        "DIFFERENTIAL {name}: after_present={after_present} gated_delta={gated_delta:?} ungated_delta={ungated_delta:?}"
    );
    assert!(
        !after_present,
        "{name}: this shape is only a differential if `after` really is None"
    );
    assert_eq!(
        gated_delta, ungated_delta,
        "{name}: the `after: None` early return must not suppress a delta"
    );
    assert_eq!(
        layer_ids(&e),
        ungated_layers,
        "{name}: the ungated restore must not move the layer vector either"
    );
}

/// Shape 1 - the host recorded the step and nothing ever told the engine about
/// the mutation (no canonical push at all). The engine's vector is still the
/// pre-op one, so there is nothing for the engine to restore.
#[test]
fn differential_record_only_never_pushed() {
    differential("record-only", |e| {
        seed_layers(e, &["A", "B"]);
        record(e, "Delete Layer", &["B"], "tok-del", &[]);
    });
}

/// Shape 2 - the pre-op push landed (so `before` is truthful) and the post-op
/// push did not. The engine never advanced past the pre-op vector.
#[test]
fn differential_pre_op_push_without_post_op_push() {
    differential("pre-op-push-only", |e| {
        seed_layers(e, &["A", "B"]);
        e.seed_canonical(canon_doc(&["A", "B"]));
        record(e, "Delete Layer", &["B"], "tok-del", &[]);
    });
}

/// Shape 3 - the post-op push landed while a LATER native entry was the tip, so
/// it moved the engine's vector without filling this entry's `after`. Undoing
/// the native entry first rolls the vector back to the captured `before`, so
/// the External undo is a no-op natively and the host replays its own snapshot.
#[test]
fn differential_push_lands_on_a_native_tip() {
    differential("push-on-native-tip", |e| {
        seed_layers(e, &["A", "B"]);
        record(e, "Delete Layer", &["B"], "tok-del", &[]);
        e.apply(env(Command::SetOpacity {
            id: "A".into(),
            opacity: 0.5,
        }))
        .expect("native edit");
        e.seed_canonical(canon_doc(&["A"]));
        // Undo the native edit, clearing the way to the External tip.
        e.apply(env(Command::Undo)).expect("undo native");
    });
}

/// Shape 4 - the merge case: a host-minted destination whose post-op push never
/// landed. `minted_layer_ids` is the one signal that can drop an id the engine
/// would otherwise keep as a survivor, so it is the shape where an ungated
/// restore could differ. It does not, because the engine never learned the
/// destination existed.
#[test]
fn differential_minted_destination_without_post_op_push() {
    differential("minted-no-post-op-push", |e| {
        seed_layers(e, &["A", "B"]);
        e.seed_canonical(canon_doc(&["A", "B"]));
        record(e, "Merge Down", &["A", "B"], "tok-merge", &["dest"]);
    });
}

/// The shipping sequence, measured end to end: a metadata undo produces a real
/// delta and a real layer-vector change, and the redo is its mirror image.
/// This is the required end state - the native engine restores structural
/// document state on its own, instead of the host replaying its own snapshot -
/// and it is what production does today. Asserted so a regression in either
/// half is visible.
#[test]
fn shipping_sequence_undo_and_redo_are_symmetric_and_non_empty() {
    let mut e = ProtocolEngine::new();
    seed_layers(&mut e, &["A", "B"]);
    // 1. pre-op push, 2. record, 3. post-op push (the host already deleted B).
    e.seed_canonical(canon_doc(&["A", "B"]));
    let seq = record(&mut e, "Delete Layer", &["B"], "tok-del", &[]);
    e.seed_canonical(canon_doc(&["A"]));
    assert_eq!(
        layer_ids(&e),
        vec!["A"],
        "the post-op push synced the delete"
    );

    let undo = e.apply(env(Command::Undo)).expect("undo");
    assert_eq!(delta_ids(&undo.delta.changes), vec!["+A", "+B"]);
    assert_eq!(layer_ids(&e), vec!["A", "B"], "undo restored B");

    e.history_cursor_commit(seq, "undo")
        .expect("clear the barrier");
    let redo = e.apply(env(Command::Redo)).expect("redo");
    assert_eq!(
        delta_ids(&redo.delta.changes),
        vec!["-B", "+A"],
        "the redo restates the post-delete stack: B removed, the survivor restated"
    );
    assert_eq!(layer_ids(&e), vec!["A"], "redo re-applied the delete");
}

/// The zero-payload back-fill, measured. Filling the previous entry's `after`
/// from the layer vector captured at THIS record time is a contract-based
/// inference (the host commits before it mutates, so the vector now is the
/// previous entry's post-state) and costs one Arc clone. This test records what
/// it actually buys: nothing observable, because the vector it stores is the
/// same one `before` already captured, so both sides of the step are identical
/// and every restore is a no-op.
///
/// Asserted rather than left as a claim: if a future change makes the engine's
/// vector move across an External entry, the undo delta here becomes non-empty
/// and this test says so.
#[test]
fn the_record_time_backfill_is_a_no_op() {
    let mut e = ProtocolEngine::new();
    seed_layers(&mut e, &["A", "B"]);
    // Two host commits with no canonical push between them: the vector at the
    // second record is identical to the one at the first.
    record(&mut e, "Delete Layer", &["B"], "tok-1", &[]);
    let before_second = ProtocolEngine::layers(&e).clone();
    let seq_second = record(&mut e, "Delete Layer", &["B"], "tok-2", &[]);
    assert!(
        ProtocolEngine::layers(&e)
            .iter()
            .zip(before_second.iter())
            .all(|(a, b)| std::sync::Arc::ptr_eq(a, b)),
        "nothing moved the vector between the two records, so a back-fill from it is a no-op"
    );
    // Undo the newest entry, clear the barrier, then undo the first one.
    let undo_newest = e.apply(env(Command::Undo)).expect("undo the second entry");
    println!(
        "BACKFILL newest undo: delta={:?} layers={:?}",
        delta_ids(&undo_newest.delta.changes),
        layer_ids(&e)
    );
    e.history_cursor_commit(seq_second, "undo")
        .expect("clear barrier");
    let undo_first = e.apply(env(Command::Undo)).expect("undo the first entry");
    println!(
        "BACKFILL first undo: delta={:?} layers={:?}",
        delta_ids(&undo_first.delta.changes),
        layer_ids(&e)
    );
    assert!(
        undo_first.delta.changes.is_empty(),
        "an entry whose two sides are the same vector restores nothing"
    );
}
