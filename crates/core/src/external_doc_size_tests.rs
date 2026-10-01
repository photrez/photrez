// Host-owned document-size halves on an External entry, and the cursor-advance
// proof the 6th-press refusal depends on.
//
// WHY THIS FILE EXISTS
//
// A host crop changes the document size out-of-band: the host commits its own
// snapshot, then applies the crop, and never routes through a canvas command arm
// (the pixel-baking variants are refused by the routed native arm). Rust only
// ever saw an External transition carrying the layer set, so undoing the crop
// restored layer order but left the document size post-crop.
//
// WHY THE PAIR IS HOST-OWNED (the trap this file guards against)
//
// The obvious capture source - the engine's live `doc_size` - is WRONG, and
// silently so. `seed_canonical` sets `doc_size` only when it is `None`
// (baseline-only by construction, and asserted by the existing
// `repush_never_mutates_native_doc_size`). So after the first host crop that
// field holds a STALE size and no walker arm updates it. Deriving the "before"
// half from it means a SECOND crop captures the pre-FIRST-crop size, and undoing
// it resizes the document to a size the user was never in. Only the host knows
// both halves: it commits before it mutates, and computes the post size from its
// own parameters. So the caller supplies both together.
//
// This file drives the REAL command surface (`RecordExternalTransition`), the
// real two-phase cursor commit, and the real `history_query` the host refusal
// reads. It reimplements no production predicate.

use super::*;
use crate::canonical_model::CanonicalDocument;
use crate::document_core::ProtocolEngine;
use crate::state_node::{StateMeta, TileRef};
use std::sync::Arc;

fn env(cmd: Command) -> CommandEnvelope {
    CommandEnvelope {
        contract_version: CONTRACT_VERSION,
        expected_version: None,
        command: cmd,
    }
}

/// 1x1 `Arc<StateNode>` whose single RGBA pixel is filled with `v`.
fn sn(v: u8) -> Arc<StateNode> {
    let (w, h) = (1u32, 1u32);
    let data = vec![v; (w * h * 4) as usize];
    let tile = TileRef::owning(0, 0, w, h, &data);
    Arc::new(StateNode::new(0, vec![tile], StateMeta::new(w, h, 0, 0)))
}

fn add_layer(eng: &mut ProtocolEngine, id: &str) {
    eng.apply(env(Command::AddLayer {
        id: id.to_string(),
        name: id.into(),
        width: 100.0,
        height: 100.0,
        index: 0,
        layer_type: None,
        shape_params: None,
        text_data: None,
    }))
    .unwrap();
}

/// Open a document the way the host does: the canonical seed is the baseline
/// `doc_size` owner, plus one layer, giving the single open-baseline entry the
/// host refusal's floor accounts for.
fn seed_open_document(eng: &mut ProtocolEngine, w: f64, h: f64) {
    eng.seed_canonical(CanonicalDocument {
        id: "doc-1".into(),
        name: "doc".into(),
        width: w,
        height: h,
        layers: Vec::new(),
        selection: None,
    });
    add_layer(eng, "bg");
    assert_eq!(eng.doc_size(), Some((w, h)));
}

/// Record a host transition. `size` is the host's pair: `None` for a
/// size-neutral transition (a layer delete, a move, a reorder), and the
/// before/after halves for one that resizes the document.
fn record_host_transition(
    eng: &mut ProtocolEngine,
    label: &str,
    size: Option<((f64, f64), (f64, f64))>,
) {
    let (before, after) = match size {
        Some((b, a)) => (Some(b), Some(a)),
        None => (None, None),
    };
    eng.apply(env(Command::RecordExternalTransition {
        label: label.into(),
        affected_layer_ids: vec!["bg".into()],
        adapter_id: "ts".into(),
        token: format!("tok-{label}"),
        memory_cost_bytes: 8,
        doc_size_before: before,
        doc_size_after: after,
    }))
    .unwrap();
}

/// The host's two-phase undo: dispatch, then confirm the cursor move. Returns
/// the delta the host projects. Mirrors the shipped handoff sequence
/// (`facadeHistoryHandoff` -> `history_cursor_commit`), not a reimplementation of
/// any production predicate.
fn host_undo(eng: &mut ProtocolEngine) -> CommandResult {
    let r = eng.apply(env(Command::Undo)).unwrap();
    // Only an External step is a two-phase handoff; a Pixel step advances the
    // cursor in the walker itself and needs no confirmation. Asserting the
    // handoff unconditionally would be testing nothing.
    match r.external_seq {
        Some(seq) => {
            assert_eq!(r.status.as_deref(), Some("external"), "two-phase handoff");
            eng.history_cursor_commit(seq, "undo").unwrap();
        }
        None => assert_ne!(
            r.status.as_deref(),
            Some("external"),
            "an external_seq is required to confirm an External handoff"
        ),
    }
    r
}

fn host_redo(eng: &mut ProtocolEngine) -> CommandResult {
    let r = eng.apply(env(Command::Redo)).unwrap();
    match r.external_seq {
        Some(seq) => {
            assert_eq!(r.status.as_deref(), Some("external"));
            eng.history_cursor_commit(seq, "redo").unwrap();
        }
        None => assert_ne!(
            r.status.as_deref(),
            Some("external"),
            "an external_seq is required to confirm an External handoff"
        ),
    }
    r
}

/// The document size the host would adopt from this delta - the same
/// `delta.width ?? carry-forward` rule `applyDeltaToSnapshot` uses
/// (`editorFacade.ts:530-531`).
fn adopted_size(r: &CommandResult) -> Option<(f64, f64)> {
    match (r.delta.width, r.delta.height) {
        (Some(w), Some(h)) => Some((w, h)),
        _ => None,
    }
}

// ── The hole the reverted commit shipped ──────────────────────────────────

/// TWO CONSECUTIVE CROPS. The regression this file exists for.
///
/// The reverted implementation derived the "before" half from the engine's live
/// `doc_size`, which `seed_canonical` only ever sets once. After crop 1 that
/// field is stale, so crop 2 recorded the PRE-FIRST-CROP size and its undo
/// resized the document from 13 back to 128 - a size the user was never in.
///
/// The assertion is written against the true intermediate value, so it cannot
/// pass while the capture source is stale.
#[test]
fn two_consecutive_crops_undo_to_the_true_intermediate_size() {
    let mut eng = ProtocolEngine::new();
    seed_open_document(&mut eng, 128.0, 128.0);
    eng.register_adapter("ts");

    // Crop 1: 128 -> 13. The host commits, then applies the crop out-of-band.
    record_host_transition(
        &mut eng,
        "Crop Canvas",
        Some(((128.0, 128.0), (13.0, 13.0))),
    );
    // The host pushes canonical state after each mutation. This does NOT move the
    // engine's doc_size (baseline-only), which is exactly why it cannot be the
    // capture source.
    eng.seed_canonical(CanonicalDocument {
        id: "doc-1".into(),
        name: "doc".into(),
        width: 13.0,
        height: 13.0,
        layers: Vec::new(),
        selection: None,
    });
    assert_eq!(
        eng.doc_size(),
        Some((128.0, 128.0)),
        "the engine's doc_size is stale after crop 1 - the host owns the halves"
    );

    // Crop 2: 13 -> 40. The host knows the document is 13 right now, because it
    // is the host that applied crop 1.
    record_host_transition(&mut eng, "Crop Canvas", Some(((13.0, 13.0), (40.0, 40.0))));
    eng.seed_canonical(CanonicalDocument {
        id: "doc-1".into(),
        name: "doc".into(),
        width: 40.0,
        height: 40.0,
        layers: Vec::new(),
        selection: None,
    });

    // Undo crop 2 -> the TRUE intermediate size, never the pre-first-crop size.
    let r = host_undo(&mut eng);
    assert_eq!(
        adopted_size(&r),
        Some((13.0, 13.0)),
        "undoing crop 2 returns to 13x13 (the size the user was actually in)"
    );
    assert_ne!(
        adopted_size(&r),
        Some((128.0, 128.0)),
        "must NEVER return to the pre-first-crop size"
    );

    // Undo crop 1 -> the true original size.
    let r = host_undo(&mut eng);
    assert_eq!(adopted_size(&r), Some((128.0, 128.0)));

    // Redo both, symmetrically.
    let r = host_redo(&mut eng);
    assert_eq!(adopted_size(&r), Some((13.0, 13.0)), "redo crop 1");
    let r = host_redo(&mut eng);
    assert_eq!(adopted_size(&r), Some((40.0, 40.0)), "redo crop 2");
}

// ── Size-neutral transitions must stay size-neutral ───────────────────────

/// A transition whose AFTER half is absent - the case the previous guard could
/// not detect, because it pushed equal halves so the absent case never arose.
///
/// Both a fully absent pair and a before-only pair must emit no size at all.
#[test]
fn external_with_absent_after_half_emits_no_size() {
    let mut eng = ProtocolEngine::new();
    seed_open_document(&mut eng, 128.0, 128.0);
    eng.register_adapter("ts");

    // Before half only: the host knows the size but not the result. This must NOT
    // be mistaken for "restore to this size".
    eng.apply(env(Command::RecordExternalTransition {
        label: "partial".into(),
        affected_layer_ids: vec!["bg".into()],
        adapter_id: "ts".into(),
        token: "tok-partial".into(),
        memory_cost_bytes: 8,
        doc_size_before: Some((128.0, 128.0)),
        doc_size_after: None,
    }))
    .unwrap();

    let r = host_undo(&mut eng);
    assert_eq!(
        r.delta.width, None,
        "a half-supplied pair must emit no width (after half absent)"
    );
    assert_eq!(r.delta.height, None);

    let r = host_redo(&mut eng);
    assert_eq!(r.delta.width, None, "symmetric on redo");
    assert_eq!(r.delta.height, None);
}

/// A pure metadata transition (layer delete / move / reorder): no pair at all.
/// Its undo must emit an empty layer delta with NO size, which is what keeps the
/// host's `lastHistoryDeltaWasEmpty` true and its fall-through routing intact.
#[test]
fn metadata_external_emits_empty_delta_and_no_size() {
    let mut eng = ProtocolEngine::new();
    seed_open_document(&mut eng, 128.0, 128.0);
    eng.register_adapter("ts");
    record_host_transition(&mut eng, "Delete Layer", None);

    let r = host_undo(&mut eng);
    assert_eq!(r.delta.width, None, "metadata external carries no size");
    assert_eq!(r.delta.height, None);
}

/// A host mutation that happens to be size- AND layer-neutral (equal halves)
/// also emits no size, so the host still reads it as an empty step.
#[test]
fn size_neutral_host_mutation_with_equal_halves_emits_no_size() {
    let mut eng = ProtocolEngine::new();
    seed_open_document(&mut eng, 128.0, 128.0);
    eng.register_adapter("ts");
    record_host_transition(
        &mut eng,
        "Toggle Visible",
        Some(((128.0, 128.0), (128.0, 128.0))),
    );

    let r = host_undo(&mut eng);
    assert_eq!(
        r.delta.width, None,
        "equal halves -> no size change -> no delta"
    );
    assert_eq!(r.delta.height, None);
}

// ── Cursor advance: the 6th-press refusal's input ─────────────────────────

/// Every entry kind must still advance the shared cursor, because the host
/// refusal reads it (`facadeHistoryHandoff.ts` -> `getHistoryQuery`) and the undo
/// button is enabled unconditionally under facade ownership: a frozen cursor
/// would not surface as a dead button, it would silently undo the wrong thing.
///
/// The engine has NO baseline-floor guard of its own - it will happily undo the
/// document-open entry - so the refusal has to be a host-side read of this
/// cursor. That is asserted here explicitly.
#[test]
fn every_entry_kind_still_advances_the_shared_cursor() {
    let mut eng = ProtocolEngine::new();
    seed_open_document(&mut eng, 128.0, 128.0);
    eng.register_adapter("ts");

    // Native (Metadata): moved by the walker itself.
    add_layer(&mut eng, "a");
    assert_eq!(eng.history_query().cursor, 2);
    eng.apply(env(Command::Undo)).unwrap();
    assert_eq!(eng.history_query().cursor, 1, "native undo");
    eng.apply(env(Command::Redo)).unwrap();
    assert_eq!(eng.history_query().cursor, 2, "native redo");

    // Pixel: moved by the walker's Pixel arm.
    eng.apply_pixel_patch("bg", sn(0), sn(7));
    assert_eq!(eng.history_query().cursor, 3);
    let (_, tiles, _) = eng.undo_pixel().unwrap();
    assert_eq!(tiles.unwrap()[0].data[0], 0);
    assert_eq!(eng.history_query().cursor, 2, "pixel undo");
    let (_, tiles, _) = eng.redo_pixel().unwrap();
    assert_eq!(tiles.unwrap()[0].data[0], 7);
    assert_eq!(eng.history_query().cursor, 3, "pixel redo");

    // External: moved ONLY on the cursor commit, never by the walker's
    // early return. Both phases asserted.
    record_host_transition(
        &mut eng,
        "Crop Canvas",
        Some(((128.0, 128.0), (13.0, 13.0))),
    );
    let c_before = eng.history_query().cursor;
    assert_eq!(c_before, 4);
    let r = eng.apply(env(Command::Undo)).unwrap();
    assert_eq!(
        eng.history_query().cursor,
        c_before,
        "the walker early-return must NOT move the cursor"
    );
    eng.history_cursor_commit(r.external_seq.unwrap(), "undo")
        .unwrap();
    assert_eq!(
        eng.history_query().cursor,
        c_before - 1,
        "the cursor commit advances it by exactly one"
    );
}

/// Snapshot entries move through `undo_snapshot`/`redo_snapshot`, NOT the
/// walker's Snapshot arm. Asserted so Phase 1 cannot have shifted it.
#[test]
fn snapshot_entry_cursor_contract_unchanged() {
    let mut eng = ProtocolEngine::new();
    seed_open_document(&mut eng, 128.0, 128.0);

    let snap = |v: u64, w: u32| {
        crate::snapshot::DocumentSnapshot::new("doc-1", v)
            .with_layer(crate::snapshot::LayerSnapshot::new("bg", w, w))
    };
    eng.record_snapshot(snap(0, 128), snap(1, 13)).unwrap();
    assert_eq!(eng.history_query().cursor, 2);

    eng.apply(env(Command::Undo)).unwrap();
    assert_eq!(
        eng.history_query().cursor,
        2,
        "the walker deliberately leaves the Snapshot cursor alone"
    );
    assert!(eng.undo_snapshot().unwrap().is_some());
    assert_eq!(eng.history_query().cursor, 1);
    assert!(eng.redo_snapshot().unwrap().is_some());
    assert_eq!(eng.history_query().cursor, 2);
}

/// The refusal's own relationship, walked press by press over the measured
/// stream `[baseline, pixel stroke, external crop]`. The refusal reads entries
/// below the cursor minus a one-entry open-baseline floor; this asserts each
/// press consumes exactly one entry and that the stream keeps answering.
#[test]
fn measured_stream_consumes_exactly_one_entry_per_press() {
    const FLOOR: u64 = 1; // the host proved one open-baseline entry exists
    let mut eng = ProtocolEngine::new();
    seed_open_document(&mut eng, 128.0, 128.0);
    eng.register_adapter("ts");

    eng.apply_pixel_patch("bg", sn(0), sn(9));
    record_host_transition(
        &mut eng,
        "Crop Canvas",
        Some(((128.0, 128.0), (13.0, 13.0))),
    );
    assert_eq!(eng.history_query().entries.len(), 3);
    assert_eq!(eng.history_query().cursor, 3);

    // Undo refusal: work exists iff cursor > floor.
    assert!(eng.history_query().cursor > FLOOR, "press 1 has work below");
    host_undo(&mut eng);
    assert_eq!(eng.history_query().cursor, 2, "press 1 consumed one entry");
    assert!(eng.history_query().cursor > FLOOR, "press 2 still has work");

    host_undo(&mut eng);
    assert_eq!(eng.history_query().cursor, 1, "press 2 consumed one entry");
    assert_eq!(
        eng.history_query().cursor,
        FLOOR,
        "at the floor the host refuses press 3"
    );

    // Redo refusal: work ahead iff entries.len() > cursor.
    assert!(eng.history_query().entries.len() as u64 > eng.history_query().cursor);
}
