// SPDX-License-Identifier: AGPL-3.0-or-later
//! Proof-of-convergence for the pixel-path adjustment-bake family.
//!
//! Under native authority the command engine IS `DocumentPixelStore.history`
//! (the shared `ProtocolEngine`). An adjustment bake converges as a COMPOSITION
//! on that engine, not as a new command variant:
//!   1. the host bakes pixels -> `write_region` replaces the whole layer (one
//!      `Pixel` entry, unified cursor),
//!   2. the existing `SetAdjustment { id, adjustment: None }` arm clears the
//!      metadata (one `Native` entry).
//!
//! A compute-arm cannot reach the registry from inside `apply()` (ownership
//! inversion), and a metadata-only `Bake` arm would merely duplicate
//! `SetAdjustment None`. This drives that two-step sequence end-to-end on the
//! SHARED registry instance and pins the engine-stream contract.

use super::*;
use crate::canonical_model::{
    BasicAdjustment, BlendMode, CanonicalDocument, CanonicalLayer, LayerType, Transform2D,
};
use crate::model::RenderLayer;
use crate::protocol::{Command, CommandEnvelope, CommandResult, PayloadKind};

// The global REGISTRY is process-global and tests run in parallel, so a
// collision-free key is required (never reuse a sibling test's key).
const BAKE_DOC: &str = "composition_bake_doc_9f3a";
const BAKE_LAYER: &str = "bake-layer-9f3a";

fn envelope(cmd: Command) -> CommandEnvelope {
    CommandEnvelope {
        contract_version: crate::protocol::CONTRACT_VERSION,
        expected_version: None,
        command: cmd,
    }
}

// Adjustment the seeded layer carries, so SetAdjustment None has something to clear.
fn seeded_adjustment() -> BasicAdjustment {
    BasicAdjustment {
        brightness: 12.0,
        contrast: -8.0,
        saturation: 4.0,
    }
}

fn engine_layer() -> RenderLayer {
    RenderLayer {
        id: BAKE_LAYER.to_string(),
        name: "Bake".to_string(),
        visible: true,
        opacity: 1.0,
        resource_id: 1,
        basic_adjustment: Some(seeded_adjustment()),
        has_adjustments: Some(true),
        ..Default::default()
    }
}

#[test]
fn adjustment_bake_composition_is_two_engine_entries_and_round_trips() {
    let mut g = registry();
    let reg = g.get_or_insert_with(Default::default);
    reg.open_document(BAKE_DOC);

    let w: usize = 8;
    let h: usize = 8;
    let initial: Vec<u8> = vec![0u8; w * h * 4];
    let baked: Vec<u8> = vec![200u8; w * h * 4];

    // Seed the canonical pixel buffer for the layer.
    reg.add_layer(BAKE_DOC, BAKE_LAYER, w as u32, h as u32, initial.clone())
        .expect("add_layer must succeed");

    // Seed the engine view: one layer carrying a basic adjustment + a canonical
    // shadow carrying the same adjustment.
    {
        let doc = reg.docs.get_mut(BAKE_DOC).unwrap();
        doc.history.seed_layers(vec![engine_layer()], 0);
        doc.history.seed_canonical(CanonicalDocument {
            id: BAKE_DOC.to_string(),
            name: "n".to_string(),
            width: w as f64,
            height: h as f64,
            layers: vec![CanonicalLayer {
                id: BAKE_LAYER.to_string(),
                name: "Bake".to_string(),
                layer_type: LayerType::Raster,
                visible: true,
                opacity: 1.0,
                locked: false,
                is_background: None,
                lock_transparency: None,
                lock_position: None,
                lock_rotation: None,
                has_adjustments: Some(true),
                basic_adjustment: Some(seeded_adjustment()),
                resource_id: Some(1),
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
                width: w as f64,
                height: h as f64,
                shape_params: None,
                text_data: None,
            }],
            selection: None,
        });
    }

    // (a) Host bake: write the whole layer with the baked bytes. One Pixel entry.
    let (_before, _after, _epoch, v1) = reg
        .write_region(BAKE_DOC, BAKE_LAYER, 0, 0, w, h, baked.clone())
        .expect("write_region must succeed");
    assert_eq!(v1, 1, "write_region advances version to v1");

    // (b) Apply a REAL serde-envelope SetAdjustment None through the shared engine.
    let res: CommandResult = reg
        .docs
        .get_mut(BAKE_DOC)
        .unwrap()
        .history
        .apply(envelope(Command::SetAdjustment {
            id: BAKE_LAYER.to_string(),
            adjustment: None,
        }))
        .expect("apply SetAdjustment None");
    assert_eq!(
        res.document_version, 2,
        "SetAdjustment advances version to v2"
    );

    // Engine stream must be exactly [Pixel, Native] in order.
    let q = reg.docs.get(BAKE_DOC).unwrap().history.history_query();
    assert_eq!(q.entries.len(), 2, "two entries");
    assert_eq!(q.entries[0].payload_kind, Some(PayloadKind::Pixel));
    assert_eq!(q.entries[1].payload_kind, Some(PayloadKind::Metadata));
    assert_eq!(q.entries[0].label, "pixel");
    assert_eq!(q.entries[1].label, "Set Adjustment");
    assert_eq!(q.cursor, 2);

    // Authoritative engine-layer metadata is cleared.
    let l = reg
        .docs
        .get(BAKE_DOC)
        .unwrap()
        .history
        .snapshot()
        .layers
        .into_iter()
        .find(|l| l.id == BAKE_LAYER)
        .expect("layer present");
    assert!(
        l.basic_adjustment.is_none(),
        "adjustment cleared on engine layer"
    );
    assert_eq!(l.has_adjustments, Some(false), "has_adjustments false");

    // Pixels remain the baked bytes (composition did not touch pixel content).
    let px = reg
        .docs
        .get(BAKE_DOC)
        .unwrap()
        .layers
        .get(BAKE_LAYER)
        .unwrap()
        .pixels
        .clone();
    assert_eq!(px, baked, "pixels are the baked region");

    // Canonical shadow contract (merge-or-fallback). The engine layer above was
    // cleared (authoritative user-visible state). The shadow preserves the base
    // adjustment via `basic_adjustment: render.or_else(base)`, but the engine set
    // `has_adjustments` to Some(false) explicitly, so the shadow flag follows it.
    let canon = reg
        .docs
        .get(BAKE_DOC)
        .unwrap()
        .history
        .canonical()
        .expect("canonical seeded");
    let cl = canon
        .layers
        .iter()
        .find(|l| l.id == BAKE_LAYER)
        .expect("canonical layer present");
    // Pin the documented retention: flipping the merge to overwrite-on-None would
    // drop this to None and fail.
    assert_eq!(
        cl.basic_adjustment,
        Some(seeded_adjustment()),
        "shadow retains basic_adjustment after None-arm"
    );
    // Honest pin of the flag: the engine set it explicitly, so the shadow is
    // Some(false), NOT the seeded Some(true).
    assert_eq!(
        cl.has_adjustments,
        Some(false),
        "shadow has_adjustments is Some(false) after None-arm (engine set it explicitly)"
    );

    // (c) Undo the Native entry (the metadata walker). Restores the adjustment
    // flag WITHOUT touching pixel bytes.
    let undo_native = reg
        .docs
        .get_mut(BAKE_DOC)
        .unwrap()
        .history
        .apply(envelope(Command::Undo))
        .expect("undo Native");
    assert_eq!(undo_native.document_version, 3, "undo Native bumps version");
    let l = reg
        .docs
        .get(BAKE_DOC)
        .unwrap()
        .history
        .snapshot()
        .layers
        .into_iter()
        .find(|l| l.id == BAKE_LAYER)
        .expect("layer present");
    assert_eq!(
        l.basic_adjustment,
        Some(seeded_adjustment()),
        "adjustment restored"
    );
    assert_eq!(l.has_adjustments, Some(true), "has_adjustments restored");
    let px = reg
        .docs
        .get(BAKE_DOC)
        .unwrap()
        .layers
        .get(BAKE_LAYER)
        .unwrap()
        .pixels
        .clone();
    assert_eq!(px, baked, "pixels untouched by Native undo");

    // (d) The host history presents ONE user step; the engine stream is two entries
    // by design. Undo the Pixel entry to restore the pre-bake tiles. The Native
    // entry sits above the Pixel entry, so it is undone first (in (c)) before
    // undo_pixel can reach the Pixel entry below the cursor.
    let (_lid, _tiles, _epoch, v4) = reg
        .undo_pixel(BAKE_DOC)
        .expect("undo_pixel restores pre-bake tiles");
    assert_eq!(v4, 4, "undo_pixel bumps version");
    let px = reg
        .docs
        .get(BAKE_DOC)
        .unwrap()
        .layers
        .get(BAKE_LAYER)
        .unwrap()
        .pixels
        .clone();
    assert_eq!(px, initial, "pre-bake tiles restored");

    // (e) Redo both entries, restoring the full (b) state.
    let (_lid, _tiles, _epoch, v5) = reg
        .redo_pixel(BAKE_DOC)
        .expect("redo_pixel restores baked tiles");
    assert_eq!(v5, 5, "redo_pixel bumps version");
    let redo_native = reg
        .docs
        .get_mut(BAKE_DOC)
        .unwrap()
        .history
        .apply(envelope(Command::Redo))
        .expect("redo Native");
    assert_eq!(redo_native.document_version, 6, "redo Native bumps version");

    // Final state equals (b).
    let l = reg
        .docs
        .get(BAKE_DOC)
        .unwrap()
        .history
        .snapshot()
        .layers
        .into_iter()
        .find(|l| l.id == BAKE_LAYER)
        .expect("layer present");
    assert!(
        l.basic_adjustment.is_none(),
        "adjustment cleared after redo"
    );
    assert_eq!(
        l.has_adjustments,
        Some(false),
        "has_adjustments false after redo"
    );
    let px = reg
        .docs
        .get(BAKE_DOC)
        .unwrap()
        .layers
        .get(BAKE_LAYER)
        .unwrap()
        .pixels
        .clone();
    assert_eq!(px, baked, "baked tiles restored after redo");
    let q = reg.docs.get(BAKE_DOC).unwrap().history.history_query();
    assert_eq!(q.cursor, 2, "cursor back at tip");

    // Cleanup: release the unique doc namespace so sibling tests are unaffected.
    reg.close_document(BAKE_DOC);
}
