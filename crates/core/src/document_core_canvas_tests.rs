// SPDX-License-Identifier: AGPL-3.0-or-later
// Canvas-size command-arm tests: CropCanvas / ApplyCrop / ResizeCanvas. These pin
// the native-authority contract for the three document-size arms so a regression
// (silent no-op semantics, locked-layer skip, selection clear, shadow dim sync,
// non-finite rejection, and doc_size undo/redo restoration) is caught. The TS
// parity (Side A oracle vs Side B arm) lives in operationParity.matrix.test.ts.
use super::arm_tests::{env, layer};
use crate::canonical_model::{CanonicalDocument, SelectionShape, SelectionState};
use crate::command::*;
use crate::document_core::ProtocolEngine;
use crate::model::RenderLayer;

fn mk(id: &str, x: f64, y: f64, w: f64, h: f64) -> RenderLayer {
    let mut l = layer();
    l.id = id.into();
    l.name = id.into();
    l.x = x;
    l.y = y;
    l.width = Some(w);
    l.height = Some(h);
    l.scale_x = 1.0;
    l.scale_y = 1.0;
    l.rotation = 0.0;
    l
}

fn layer_by(e: &ProtocolEngine, id: &str) -> RenderLayer {
    e.snapshot()
        .layers
        .into_iter()
        .find(|l| l.id == id)
        .expect("layer present in snapshot")
}

fn canonical_dim_doc() -> CanonicalDocument {
    CanonicalDocument {
        id: "dim-doc".into(),
        name: "D".into(),
        width: 10.0,
        height: 10.0,
        layers: vec![],
        selection: None,
    }
}

const TOL: f64 = 1e-6;
fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < TOL
}

// Clean-room reference of the TS performApplyCrop non-destructive math, used to
// assert the Rust arm matches the oracle (independent of the arm's own code).
fn ref_apply_crop(
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    rotation: f64,
    target_w: f64,
    target_h: f64,
    lx: f64,
    ly: f64,
    lw: f64,
    lh: f64,
    lr: f64,
) -> (f64, f64, f64, f64, f64) {
    let crop_center_x = x + width / 2.0;
    let crop_center_y = y + height / 2.0;
    let rad = (-rotation * std::f64::consts::PI) / 180.0;
    let cos = rad.cos();
    let sin = rad.sin();
    let export_scale_x = target_w / width;
    let export_scale_y = target_h / height;
    let lcx = lx + (lw * 1.0) / 2.0;
    let lcy = ly + (lh * 1.0) / 2.0;
    let vx = lcx - crop_center_x;
    let vy = lcy - crop_center_y;
    let rvx = vx * cos - vy * sin;
    let rvy = vx * sin + vy * cos;
    let nlcx = width / 2.0 + rvx;
    let nlcy = height / 2.0 + rvy;
    let final_cx = nlcx * export_scale_x;
    let final_cy = nlcy * export_scale_y;
    let final_scale_x = 1.0 * export_scale_x;
    let final_scale_y = 1.0 * export_scale_y;
    let final_rotation = {
        let mut a = (lr - rotation) % 360.0;
        if a > 180.0 {
            a -= 360.0;
        }
        if a < -180.0 {
            a += 360.0;
        }
        a
    };
    let new_x = final_cx - (lw * final_scale_x.abs()) / 2.0;
    let new_y = final_cy - (lh * final_scale_y.abs()) / 2.0;
    (new_x, new_y, final_scale_x, final_scale_y, final_rotation)
}

#[test]
fn crop_canvas_offsets_unlocked_and_resizes_doc() {
    let mut e = ProtocolEngine::new();
    let mut locked = mk("B", 5.0, 6.0, 50.0, 50.0);
    locked.locked = Some(true);
    e.seed_layers(vec![mk("A", 100.0, 200.0, 50.0, 50.0), locked], 0);

    e.apply(env(Command::CropCanvas {
        x: 10.0,
        y: 20.0,
        width: 500.0,
        height: 400.0,
    }))
    .unwrap();

    // Unlocked A shifted by (-x, -y).
    let a = layer_by(&e, "A");
    assert!(close(a.x, 90.0));
    assert!(close(a.y, 180.0));
    // Locked B untouched.
    let b = layer_by(&e, "B");
    assert!(close(b.x, 5.0));
    assert!(close(b.y, 6.0));
    // Document size set.
    assert_eq!(e.doc_size(), Some((500.0, 400.0)));
    // One history entry, DV bumped once.
    assert_eq!(e.entries.len(), 1);
    assert_eq!(e.entries[0].label, "Crop Canvas");
    assert_eq!(e.version(), 1);
    // Delta is an ordered Upsert of every layer.
    let changes = e.snapshot();
    assert_eq!(changes.layers.len(), 2);
}

#[test]
fn crop_canvas_clears_selection() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk("A", 0.0, 0.0, 50.0, 50.0)], 0);
    e.set_engine_selection(Some(SelectionState {
        x: 1.0,
        y: 2.0,
        width: 3.0,
        height: 4.0,
        angle: 0.0,
        shape: Some(SelectionShape::Rect),
        inverted: None,
    }));
    assert!(e.selection().is_some());

    e.apply(env(Command::CropCanvas {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
    }))
    .unwrap();

    assert!(e.selection().is_none());
}

#[test]
fn crop_canvas_no_seeded_shadow_still_sets_doc_size() {
    // Canvas arms do NOT require a seeded canonical shadow (unlike merge/flatten).
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk("A", 0.0, 0.0, 50.0, 50.0)], 0);
    assert!(e.canonical().is_none());

    e.apply(env(Command::CropCanvas {
        x: 10.0,
        y: 10.0,
        width: 800.0,
        height: 600.0,
    }))
    .unwrap();

    assert_eq!(e.doc_size(), Some((800.0, 600.0)));
    assert!(e.canonical().is_none());
}

#[test]
fn crop_canvas_syncs_shadow_dims_when_seeded() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk("A", 0.0, 0.0, 50.0, 50.0)], 0);
    e.seed_canonical(canonical_dim_doc());
    assert_eq!(e.canonical().unwrap().width, 10.0);

    e.apply(env(Command::CropCanvas {
        x: 0.0,
        y: 0.0,
        width: 320.0,
        height: 240.0,
    }))
    .unwrap();

    assert_eq!(e.canonical().unwrap().width, 320.0);
    assert_eq!(e.canonical().unwrap().height, 240.0);
}

#[test]
fn crop_canvas_non_positive_is_silent_noop() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk("A", 100.0, 200.0, 50.0, 50.0)], 0);
    let snap_before = e.snapshot();

    // width <= 0: no entry, snapshot unchanged, DV still bumps once.
    e.apply(env(Command::CropCanvas {
        x: -10.0,
        y: -10.0,
        width: -5.0,
        height: 400.0,
    }))
    .unwrap();

    assert_eq!(e.entries.len(), 0);
    assert_eq!(e.version(), 1);
    assert_eq!(e.doc_size(), None);
    assert_eq!(e.snapshot().layers, snap_before.layers);
}

#[test]
fn crop_canvas_non_finite_rejects_before_mutation() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk("A", 100.0, 200.0, 50.0, 50.0)], 0);
    let snap_before = e.snapshot();

    let res = e.apply(env(Command::CropCanvas {
        x: f64::NAN,
        y: 0.0,
        width: 100.0,
        height: 100.0,
    }));
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().code, "E_INVALID");
    // Nothing mutated: no entry, no DV bump, doc size unchanged.
    assert_eq!(e.entries.len(), 0);
    assert_eq!(e.version(), 0);
    assert_eq!(e.doc_size(), None);
    assert_eq!(e.snapshot().layers, snap_before.layers);
}

#[test]
fn apply_crop_plain_rect_recenters_layer() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk("A", 100.0, 100.0, 200.0, 100.0)], 0);

    e.apply(env(Command::ApplyCrop {
        x: 50.0,
        y: 50.0,
        width: 100.0,
        height: 100.0,
        rotation: None,
        target_width: None,
        target_height: None,
    }))
    .unwrap();

    // Reference: center (200,150) -> crop center (100,100) -> offset (100,50),
    // rotated 0 -> final center (150,100) -> new top-left (50,50).
    let a = layer_by(&e, "A");
    assert!(close(a.x, 50.0));
    assert!(close(a.y, 50.0));
    assert!(close(a.scale_x, 1.0));
    assert!(close(a.scale_y, 1.0));
    assert!(close(a.rotation, 0.0));
    assert_eq!(e.doc_size(), Some((100.0, 100.0)));
}

#[test]
fn apply_crop_with_rotation_and_target_matches_oracle() {
    let mut e = ProtocolEngine::new();
    // Layer at (100,100), 200x100, rotation 179; crop 50x50 at (10,10) rotated -5,
    // exported to 100x200 (forces a normalizeRotation wrap on layer.rotation - cropRotation).
    e.seed_layers(vec![mk("A", 100.0, 100.0, 200.0, 100.0)], 0);
    {
        let mut l = e.layers.get(0).expect("present").clone();
        l.rotation = 179.0;
        e.layers = e.layers.replaced(0, l);
    }

    e.apply(env(Command::ApplyCrop {
        x: 10.0,
        y: 10.0,
        width: 50.0,
        height: 50.0,
        rotation: Some(-5.0),
        target_width: Some(100.0),
        target_height: Some(200.0),
    }))
    .unwrap();

    let a = layer_by(&e, "A");
    let (ex, ey, esx, esy, erot) = ref_apply_crop(
        10.0, 10.0, 50.0, 50.0, -5.0, 100.0, 200.0, 100.0, 100.0, 200.0, 100.0, 179.0,
    );
    assert!(close(a.x, ex), "x {} vs {}", a.x, ex);
    assert!(close(a.y, ey), "y {} vs {}", a.y, ey);
    assert!(close(a.scale_x, esx));
    assert!(close(a.scale_y, esy));
    assert!(close(a.rotation, erot), "rot {} vs {}", a.rotation, erot);
    assert_eq!(e.doc_size(), Some((100.0, 200.0)));
}

#[test]
fn apply_crop_half_target_pair_rejects() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk("A", 0.0, 0.0, 50.0, 50.0)], 0);
    let res = e.apply(env(Command::ApplyCrop {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
        rotation: None,
        target_width: Some(200.0),
        target_height: None,
    }));
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().code, "E_INVALID");
    assert_eq!(e.doc_size(), None);
}

#[test]
fn apply_crop_non_finite_rotation_rejects() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk("A", 0.0, 0.0, 50.0, 50.0)], 0);
    let snap_before = e.snapshot();
    let res = e.apply(env(Command::ApplyCrop {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
        rotation: Some(f64::NAN),
        target_width: None,
        target_height: None,
    }));
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().code, "E_INVALID");
    // Nothing mutated: no entry, no DV bump, doc size + layers unchanged.
    assert_eq!(e.entries.len(), 0);
    assert_eq!(e.version(), 0);
    assert_eq!(e.doc_size(), None);
    assert_eq!(e.snapshot().layers, snap_before.layers);
}

#[test]
fn crop_then_rename_undo_undo_restores_transforms_doc_size_and_name_lifo() {
    // Mixed canvas + metadata sequence: a canvas arm (CropCanvas) followed by a
    // metadata arm (Rename), then two undos in LIFO order. Each undo must restore
    // the correct entry's payload (transform + doc size for the crop, name for the
    // rename), proving the mixed-stack walker restores both layer transforms AND
    // document size on the canvas entry.
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk("A", 100.0, 200.0, 50.0, 50.0)], 0);

    e.apply(env(Command::CropCanvas {
        x: 10.0,
        y: 20.0,
        width: 500.0,
        height: 400.0,
    }))
    .unwrap();
    assert_eq!(layer_by(&e, "A").x, 90.0);
    assert_eq!(e.doc_size(), Some((500.0, 400.0)));

    e.apply(env(Command::Rename {
        id: "A".into(),
        name: "Renamed".into(),
    }))
    .unwrap();
    assert_eq!(layer_by(&e, "A").name, "Renamed");

    // Undo #1 (Rename): name back to "A"; transform + doc size retained.
    e.apply(env(Command::Undo)).unwrap();
    assert_eq!(layer_by(&e, "A").name, "A");
    assert_eq!(layer_by(&e, "A").x, 90.0);
    assert_eq!(e.doc_size(), Some((500.0, 400.0)));

    // Undo #2 (CropCanvas): transform + doc size restored to pre-crop.
    e.apply(env(Command::Undo)).unwrap();
    assert_eq!(layer_by(&e, "A").x, 100.0);
    assert_eq!(layer_by(&e, "A").y, 200.0);
    assert_eq!(e.doc_size(), None, "second undo restores document size too");
}

#[test]
fn apply_crop_non_positive_is_silent_noop() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk("A", 100.0, 100.0, 200.0, 100.0)], 0);
    let snap_before = e.snapshot();
    e.apply(env(Command::ApplyCrop {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 100.0,
        rotation: None,
        target_width: None,
        target_height: None,
    }))
    .unwrap();
    assert_eq!(e.entries.len(), 0);
    assert_eq!(e.version(), 1);
    assert_eq!(e.doc_size(), None);
    assert_eq!(e.snapshot().layers, snap_before.layers);
}

#[test]
fn apply_crop_clears_selection() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk("A", 0.0, 0.0, 50.0, 50.0)], 0);
    e.set_engine_selection(Some(SelectionState {
        x: 1.0,
        y: 2.0,
        width: 3.0,
        height: 4.0,
        angle: 0.0,
        shape: None,
        inverted: None,
    }));
    e.apply(env(Command::ApplyCrop {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
        rotation: None,
        target_width: None,
        target_height: None,
    }))
    .unwrap();
    assert!(e.selection().is_none());
}

#[test]
fn resize_canvas_sets_dims_and_empty_delta() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk("A", 10.0, 20.0, 50.0, 50.0)], 0);

    let res = e
        .apply(env(Command::ResizeCanvas {
            width: 800.0,
            height: 600.0,
        }))
        .unwrap();

    assert_eq!(e.doc_size(), Some((800.0, 600.0)));
    // Layers untouched.
    let a = layer_by(&e, "A");
    assert!(close(a.x, 10.0));
    assert!(close(a.y, 20.0));
    // Empty layer delta, but an entry exists (doc effect rides the snapshot).
    assert_eq!(res.delta.changes.len(), 0);
    assert_eq!(e.entries.len(), 1);
    assert_eq!(e.entries[0].label, "Resize Canvas");
    assert_eq!(e.entries[0].affected_layer_ids.len(), 0);
}

#[test]
fn resize_canvas_non_positive_is_silent_noop() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk("A", 10.0, 20.0, 50.0, 50.0)], 0);
    let snap_before = e.snapshot();
    e.apply(env(Command::ResizeCanvas {
        width: -1.0,
        height: 600.0,
    }))
    .unwrap();
    assert_eq!(e.entries.len(), 0);
    assert_eq!(e.version(), 1);
    assert_eq!(e.doc_size(), None);
    assert_eq!(e.snapshot().layers, snap_before.layers);
}

#[test]
fn resize_canvas_non_finite_rejects() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk("A", 10.0, 20.0, 50.0, 50.0)], 0);
    let res = e.apply(env(Command::ResizeCanvas {
        width: f64::INFINITY,
        height: 600.0,
    }));
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().code, "E_INVALID");
    assert_eq!(e.doc_size(), None);
}

#[test]
fn crop_canvas_undo_redo_restores_transforms_and_doc_size() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk("A", 100.0, 200.0, 50.0, 50.0)], 0);

    e.apply(env(Command::CropCanvas {
        x: 10.0,
        y: 20.0,
        width: 500.0,
        height: 400.0,
    }))
    .unwrap();
    assert_eq!(layer_by(&e, "A").x, 90.0);
    assert_eq!(e.doc_size(), Some((500.0, 400.0)));

    e.apply(env(Command::Undo)).unwrap();
    assert!(close(layer_by(&e, "A").x, 100.0));
    assert!(close(layer_by(&e, "A").y, 200.0));
    assert_eq!(e.doc_size(), None, "undo must restore document size too");

    e.apply(env(Command::Redo)).unwrap();
    assert_eq!(layer_by(&e, "A").x, 90.0);
    assert_eq!(
        e.doc_size(),
        Some((500.0, 400.0)),
        "redo must restore document size too"
    );
}

#[test]
fn resize_canvas_undo_redo_restores_doc_size() {
    let mut e = ProtocolEngine::new();
    e.seed_layers(vec![mk("A", 10.0, 20.0, 50.0, 50.0)], 0);

    e.apply(env(Command::ResizeCanvas {
        width: 800.0,
        height: 600.0,
    }))
    .unwrap();
    assert_eq!(e.doc_size(), Some((800.0, 600.0)));

    e.apply(env(Command::Undo)).unwrap();
    assert_eq!(e.doc_size(), None, "undo must restore document size");

    e.apply(env(Command::Redo)).unwrap();
    assert_eq!(
        e.doc_size(),
        Some((800.0, 600.0)),
        "redo must restore document size"
    );
}

#[test]
fn flatten_then_crop_keeps_shadow_dims_coherent() {
    // Flatten then crop: the crop arm syncs the shadow dims to the crop rect even
    // though the shadow layer set is otherwise governed by reconcile. A Flatten-
    // minted layer is engine-only and unknown to the seeded shadow, so reconcile
    // correctly marks the shadow incomplete; that is expected, not an arm defect.
    let mut e = ProtocolEngine::new();
    e.seed_layers(
        vec![mk("A", 0.0, 0.0, 50.0, 50.0), mk("B", 5.0, 5.0, 40.0, 40.0)],
        0,
    );
    e.seed_canonical(canonical_dim_doc());
    e.apply(env(Command::Flatten {
        merged_id: "BG".into(),
    }))
    .unwrap();
    assert_eq!(e.snapshot().layers.len(), 1);

    e.apply(env(Command::CropCanvas {
        x: 0.0,
        y: 0.0,
        width: 300.0,
        height: 200.0,
    }))
    .unwrap();

    let sh = e.canonical().unwrap();
    // Dim sync is what the arm guarantees.
    assert_eq!(sh.width, 300.0);
    assert_eq!(sh.height, 200.0);
    // The engine-minted BG layer is not in the seeded shadow, so the shadow is
    // correctly flagged incomplete (reconcile cannot reconstruct it). Layer-set
    // sync is reconcile's concern, not the crop arm's.
    assert!(e.canonical_incomplete());
}
