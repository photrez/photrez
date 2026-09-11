use super::*;
use crate::canonical_model::CanonicalDocument;

/// Minimal `CanonicalDocument` (no layers) keyed by id, for shadow-copy tests.
fn empty_doc(id: &str) -> CanonicalDocument {
    CanonicalDocument {
        id: id.to_string(),
        name: "n".to_string(),
        width: 10.0,
        height: 10.0,
        layers: vec![],
        selection: None,
    }
}

#[test]
fn canonical_default_is_none() {
    let engine = ProtocolEngine::new();
    assert!(engine.canonical().is_none());
}

#[test]
fn seed_canonical_stores_copy() {
    let mut engine = ProtocolEngine::new();
    engine.seed_canonical(empty_doc("doc-A"));
    let c = engine.canonical().expect("canonical shadow stored");
    assert_eq!(c.id, "doc-A");
}

#[test]
fn seed_canonical_replaces_existing() {
    let mut engine = ProtocolEngine::new();
    engine.seed_canonical(empty_doc("doc-A"));
    engine.seed_canonical(empty_doc("doc-B"));
    let c = engine.canonical().expect("canonical shadow stored");
    assert_eq!(c.id, "doc-B", "second seed must replace the first shadow");
}

// ---- re-push / undo-redo safety (cutover correctness gate) -----------------
// These pin the REAL production call pattern: AddLayer / ResizeCanvas -> a
// canonical re-seed -> Undo / Redo. Under the single-owner rule seed_canonical()
// refreshes ONLY the shadow and NEVER mutates doc_size or touches the history
// stream, so undo/redo stays correct regardless of re-push order/dims. doc_size
// is owned solely by the canvas arms + the undo/redo walker (begin_forward
// captures the before/after pair); a re-push with divergent dims or divergent
// layer order is inert: it cannot wipe history. Tests below pin that contract.

use super::arm_tests::env;
use crate::canonical_model::{BlendMode, CanonicalLayer, LayerType, Transform2D};
use crate::document_core::ProtocolEngine;
use crate::history::EntryPayload;
use crate::model::RenderLayer;

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

fn shadow_dims(e: &ProtocolEngine) -> (f64, f64) {
    let c = e.canonical().expect("shadow present");
    (c.width, c.height)
}

fn layer_ids(e: &ProtocolEngine) -> Vec<String> {
    e.snapshot().layers.iter().map(|l| l.id.clone()).collect()
}

// t1: AddLayer -> re-seed (any re-push) -> Undo must restore the pre-add state.
// Under the single-owner rule seed_canonical NEVER touches the history stream, so
// ANY re-push (consistent OR order-divergent) leaves the AddLayer entry and its
// captured doc_size pairs verbatim; undo is therefore correct.
#[test]
fn reseed_after_addlayer_undo_restores_pre_add_state() {
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canon_doc("doc", 100.0, 100.0, &[]));
    e.seed_layers(vec![mk_layer("A", "Layer A", 10)], 0);

    // AddLayer is a non-canvas arm: it does not mutate doc_size itself.
    e.apply(env(Command::AddLayer {
        id: "B".into(),
        name: "Layer B".into(),
        width: 50.0,
        height: 50.0,
        index: 0,
        layer_type: None,
        shape_params: None,
        text_data: None,
    }))
    .unwrap();

    // Consistent re-push (TRUE post-add state: [B, A], 100x100).
    e.seed_canonical(canon_doc("doc", 100.0, 100.0, &["B", "A"]));
    assert_eq!(
        e.entries.len(),
        1,
        "re-seed must not alter the history stream"
    );
    // Order-divergent re-push (tail order [A, B] instead of native [B, A]) must be
    // equally inert: no wipe, the captured pairs survive verbatim.
    e.seed_canonical(canon_doc("doc", 100.0, 100.0, &["A", "B"]));
    assert_eq!(
        e.entries.len(),
        1,
        "ANY re-push must leave the history stream untouched"
    );
    match &e.entries[0].payload {
        EntryPayload::Native {
            doc_size_before,
            doc_size_after,
            ..
        } => {
            assert_eq!(*doc_size_before, Some((100.0, 100.0)));
            assert_eq!(*doc_size_after, Some((100.0, 100.0)));
        }
        _ => panic!("AddLayer must record a Native history entry"),
    }

    // Undo AddLayer.
    e.apply(env(Command::Undo)).unwrap();

    // Layer set + count restored to pre-add ([A]).
    assert_eq!(layer_ids(&e), vec!["A".to_string()]);
    assert_eq!(e.snapshot().layers.len(), 1);
    // doc_size restored to pre-add value.
    assert_eq!(e.doc_size, Some((100.0, 100.0)));
    // Shadow dims restored to pre-add value by the undo walker.
    assert_eq!(shadow_dims(&e), (100.0, 100.0));
}

// t2: ResizeCanvas arm sets doc_size + captured pair; a mid-session re-push with
// SAME dims leaves the history/stream untouched, and Undo/Redo walk the captured
// pair in both directions. (Divergent re-push is exercised inertly in t3/t4.)
#[test]
fn reseed_after_resize_undo_redo_follows_captured_dims() {
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canon_doc("doc", 100.0, 100.0, &[]));
    e.seed_layers(vec![mk_layer("A", "Layer A", 10)], 0);

    // Resize 1 -> (200,150): canvas arm sets doc_size AND captures the pair.
    e.apply(env(Command::ResizeCanvas {
        width: 200.0,
        height: 150.0,
    }))
    .unwrap();
    assert_eq!(e.doc_size, Some((200.0, 150.0)));
    // Re-push with SAME dims: inert w.r.t. history (no wipe, cursor unchanged).
    e.seed_canonical(canon_doc("doc", 200.0, 150.0, &["A"]));
    assert_eq!(
        e.entries.len(),
        1,
        "same-dims re-push leaves history intact"
    );
    assert_eq!(e.cursor, 1, "same-dims re-push does not reset the cursor");

    // Resize 2 -> (300,250), re-push the TRUE post-state.
    e.apply(env(Command::ResizeCanvas {
        width: 300.0,
        height: 250.0,
    }))
    .unwrap();
    e.seed_canonical(canon_doc("doc", 300.0, 250.0, &["A"]));

    // Undo both: dims walk back to pre-resize (100x100).
    e.apply(env(Command::Undo)).unwrap(); // undo resize 2
    assert_eq!(e.doc_size, Some((200.0, 150.0)));
    assert_eq!(shadow_dims(&e), (200.0, 150.0));

    e.apply(env(Command::Undo)).unwrap(); // undo resize 1
    assert_eq!(e.doc_size, Some((100.0, 100.0)));
    assert_eq!(shadow_dims(&e), (100.0, 100.0));

    // Redo both: dims walk forward to final (300x250).
    e.apply(env(Command::Redo)).unwrap(); // redo resize 1
    assert_eq!(e.doc_size, Some((200.0, 150.0)));
    assert_eq!(shadow_dims(&e), (200.0, 150.0));

    e.apply(env(Command::Redo)).unwrap(); // redo resize 2
    assert_eq!(e.doc_size, Some((300.0, 250.0)));
    assert_eq!(shadow_dims(&e), (300.0, 250.0));
}

// t3: a re-push with DIFFERENT dims (the old "divergent" push) NEVER mutates the
// native doc_size and NEVER wipes history. seed_canonical refreshes only the
// shadow; doc_size stays owned by the native resize arm + undo walker. Undo of
// the resize then restores the CAPTURED pair value, not the re-pushed dims - the
// baseline stability the single-owner rule guarantees.
#[test]
fn repush_never_mutates_native_doc_size() {
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canon_doc("doc", 100.0, 100.0, &[]));
    e.seed_layers(vec![mk_layer("A", "Layer A", 10)], 0);

    // Native resize arm: captures before=(100,100), after=(200,150).
    e.apply(env(Command::ResizeCanvas {
        width: 200.0,
        height: 150.0,
    }))
    .unwrap();
    assert_eq!(e.doc_size, Some((200.0, 150.0)));
    assert_eq!(e.entries.len(), 1);

    // Re-push with DIFFERENT dims (999x888): shadow updates, but doc_size and
    // history are UNTOUCHED (no wipe, no cursor reset).
    e.seed_canonical(canon_doc("doc", 999.0, 888.0, &["A"]));
    assert_eq!(
        e.doc_size,
        Some((200.0, 150.0)),
        "re-push must not mutate native doc_size"
    );
    assert_eq!(
        shadow_dims(&e),
        (999.0, 888.0),
        "re-push refreshes the shadow dims"
    );
    assert_eq!(e.entries.len(), 1, "re-push must not wipe history");
    assert_eq!(e.cursor, 1, "re-push must not reset the cursor");

    // Undo restores the CAPTURED pair (100x100), not the re-pushed 999x888: the
    // native baseline is stable regardless of TS re-push order/dims.
    e.apply(env(Command::Undo)).unwrap();
    assert_eq!(
        e.doc_size,
        Some((100.0, 100.0)),
        "undo restores the captured before-pair"
    );
    assert_eq!(shadow_dims(&e), (100.0, 100.0));
}

// The audit's reachable flipped-config sequence, reproduced natively under the
// single-owner rule. resize arm -> re-push with DIVERGENT dims (ignored for
// doc_size) -> undo restores the captured dims. A second re-push that is merely
// ORDER-divergent (different id vector, same dims) leaves HISTORY and doc_size
// untouched; its layer vector now drives the engine order by design (a push is
// the TS truth at mirrored moments).
#[test]
fn audit_sequence_repush_preserves_history_and_dims() {
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canon_doc("doc", 100.0, 100.0, &[]));
    e.seed_layers(vec![mk_layer("A", "Layer A", 10)], 0);

    // Native resize 100x100 -> 300x250.
    e.apply(env(Command::ResizeCanvas {
        width: 300.0,
        height: 250.0,
    }))
    .unwrap();
    assert_eq!(e.doc_size, Some((300.0, 250.0)));
    assert_eq!(e.entries.len(), 1);

    // Undo the resize: doc_size back to (100,100), entry kept (cursor at 0).
    e.apply(env(Command::Undo)).unwrap();
    assert_eq!(e.doc_size, Some((100.0, 100.0)));
    assert_eq!(e.entries.len(), 1, "undo keeps the resize entry");

    // Re-push with DIVERGENT dims (500x400): doc_size stays (100,100); history kept.
    e.seed_canonical(canon_doc("doc", 500.0, 400.0, &["A"]));
    assert_eq!(
        e.doc_size,
        Some((100.0, 100.0)),
        "divergent dims ignored for doc_size"
    );
    assert_eq!(e.entries.len(), 1, "re-push must not wipe history");
    assert_eq!(
        shadow_dims(&e),
        (500.0, 400.0),
        "re-push refreshes the shadow"
    );

    // Re-push merely ORDER-divergent (same dims 100x100, different id vector):
    // history and doc_size are untouched; the pushed vector drives engine order
    // by design (TS truth at mirrored moments), which is inert on those axes.
    e.seed_canonical(canon_doc("doc", 100.0, 100.0, &["B", "A", "X"]));
    assert_eq!(
        e.doc_size,
        Some((100.0, 100.0)),
        "order-divergent re-push leaves doc_size alone"
    );
    assert_eq!(
        e.entries.len(),
        1,
        "order-divergent re-push wipes no history"
    );
    assert_eq!(shadow_dims(&e), (100.0, 100.0));

    // Redo restores the captured resize after-pair (300x250) - the native baseline
    // is stable regardless of TS re-push order/dims.
    e.apply(env(Command::Redo)).unwrap();
    assert_eq!(
        e.doc_size,
        Some((300.0, 250.0)),
        "redo restores the captured after-pair"
    );
}
