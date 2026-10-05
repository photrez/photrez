// SPDX-License-Identifier: AGPL-3.0-or-later
//! The composite-destination seed's THIRD step, on the real engine and the real
//! shared per-document cursor.
//!
//! WHY THIS IS A CORE TEST AND NOT A FRONTEND ONE. A composite op (merge down,
//! merge selected, flatten, stamp visible) installs a NEW layer whose pixels are
//! a CPU-canvas composite. The host seeds that destination in three steps
//! (`seedCompositeCanonicalPixels` in the desktop crate): probe, one
//! whole-layer `write_region`, then a re-assertion that replaces the store's
//! buffer from the raster. The third step exists for ONE reason - the
//! `write_region` opened a `Pixel` entry on the SAME cursor the op's structural
//! entry lives on, and a surviving `Pixel` entry is what an undo press steps, so
//! the structural restore never runs and the structural undo costs two presses.
//! `resize_layer` is the established way to say "this layer's raster is now
//! exactly the model raster, wholesale": it invalidates that layer's pixel
//! history entries (`DocumentPixelStore::resize_layer` ->
//! `History::invalidate_layer`).
//!
//! That is a claim about the CURSOR, so it can only be proven on the cursor. The
//! frontend suite emulates the registry, and the two emulators it composes - a
//! byte store and a history stream - cannot express "a `Pixel` entry appeared and
//! was then dropped" in one place: the byte store holds no entries, and the seed's
//! writes are not what drives the stream. Here both live in one real
//! `ProtocolEngine`.
//!
//! THE SEQUENCE, as the host issues it: a structural entry is already on the
//! stream (the op's own layer-graph record), then the seed writes the whole layer
//! (one `Pixel` entry ABOVE it), then the re-assertion resizes the layer (dropping
//! that `Pixel` entry). Afterwards the composite's own `Pixel` entry must be gone
//! and the structural one must still be there, so one undo press reaches the
//! STRUCTURAL restore rather than a pixel entry the gesture never earned.

use super::*;
use crate::model::RenderLayer;
use crate::protocol::{Command, CommandEnvelope, PayloadKind};

/// Collision-free namespace: the registry is process-global and tests run in
/// parallel, so a sibling test's doc key is never reused.
const SEED_DOC: &str = "composite_seed_doc_7c1e";
/// The layer the composite was installed on.
const SEED_LAYER: &str = "composite-dest-7c1e";
/// A second layer the structural op removes, so the structural entry is a real
/// layer-vector change rather than an add.
const SEED_VICTIM: &str = "composite-victim-7c1e";

fn envelope(cmd: Command) -> CommandEnvelope {
    CommandEnvelope {
        contract_version: crate::protocol::CONTRACT_VERSION,
        expected_version: None,
        command: cmd,
    }
}

fn engine_layer(id: &str) -> RenderLayer {
    RenderLayer {
        id: id.to_string(),
        name: id.to_string(),
        visible: true,
        opacity: 1.0,
        resource_id: 1,
        ..Default::default()
    }
}

fn whole_layer(w: usize, h: usize, value: u8) -> Vec<u8> {
    vec![value; w * h * 4]
}

/// Seed a two-layer document whose engine view knows both layers.
fn seed_document(reg: &mut PixelStoreRegistry, w: usize, h: usize) -> Vec<u8> {
    let initial = whole_layer(w, h, 0);
    reg.add_layer(SEED_DOC, SEED_LAYER, w as u32, h as u32, initial.clone())
        .expect("add_layer destination");
    reg.add_layer(SEED_DOC, SEED_VICTIM, w as u32, h as u32, initial.clone())
        .expect("add_layer victim");
    let doc = reg.docs.get_mut(SEED_DOC).expect("doc present");
    doc.history
        .seed_layers(vec![engine_layer(SEED_LAYER), engine_layer(SEED_VICTIM)], 0);
    initial
}

/// Record the op's structural layer-vector change, as a routed composite op
/// commits it: one non-pixel entry on the shared stream.
fn record_structural_change(reg: &mut PixelStoreRegistry) -> Option<PayloadKind> {
    reg.docs
        .get_mut(SEED_DOC)
        .expect("doc present")
        .history
        .apply(envelope(Command::DeleteLayer {
            id: SEED_VICTIM.to_string(),
        }))
        .expect("apply DeleteLayer");
    let q = reg
        .docs
        .get(SEED_DOC)
        .expect("doc present")
        .history
        .history_query();
    assert_eq!(q.entries.len(), 1, "the structural op owns one entry");
    assert_eq!(q.cursor, 1, "the cursor sits above it");
    let kind = q.entries[0].payload_kind;
    assert_ne!(
        kind,
        Some(PayloadKind::Pixel),
        "the structural entry is not a pixel entry"
    );
    kind
}

/// The seed's probe plus its ONE canonical whole-layer write.
fn seed_composite(reg: &mut PixelStoreRegistry, w: usize, h: usize, composite: &[u8]) {
    // The probe RESOLVES for a layer the store already holds, which is the case
    // that makes the host skip its `rust_pixels_init` seed. Its epoch value is
    // not asserted: only the resolve/reject distinction is load-bearing here.
    reg.get_epoch(SEED_DOC, SEED_LAYER)
        .expect("probe resolves: the destination already has a store entry");
    reg.write_region(SEED_DOC, SEED_LAYER, 0, 0, w, h, composite.to_vec())
        .expect("write_region must succeed");
}

/// THE CONTRACT. After the seed's three steps: the destination holds the
/// composite bytes, the `Pixel` entry the write opened is GONE, the structural
/// entry survives and is still the undo tip, and one undo press therefore reaches
/// the structural restore.
#[test]
fn composite_seed_reassertion_drops_the_pixel_entry_it_just_opened() {
    let mut g = registry();
    let reg = g.get_or_insert_with(Default::default);
    reg.open_document(SEED_DOC);

    let w: usize = 8;
    let h: usize = 8;
    seed_document(reg, w, h);
    let composite = whole_layer(w, h, 137);

    // Step 0: the op's structural record.
    let structural_kind = record_structural_change(reg);

    // Steps 1 + 2: probe, then the one canonical whole-layer write.
    seed_composite(reg, w, h, &composite);

    // The write DID open a `Pixel` entry above the structural one. Asserted
    // present rather than assumed: without it the block below would pass on a
    // stream that never carried the hazard at all.
    let with_pixel = reg
        .docs
        .get(SEED_DOC)
        .expect("doc present")
        .history
        .history_query();
    assert_eq!(
        with_pixel.entries.len(),
        2,
        "the canonical write opened a second entry"
    );
    assert_eq!(
        with_pixel.entries[1].payload_kind,
        Some(PayloadKind::Pixel),
        "the entry above the structural one is the write's Pixel entry"
    );
    assert_eq!(with_pixel.cursor, 2, "both entries applied");

    // Step 3: the re-assertion. This is the host's
    // `syncLayerStoreToLayerRaster`, which reaches `resize_layer`.
    reg.resize_layer(SEED_DOC, SEED_LAYER, w as u32, h as u32, composite.clone())
        .expect("resize_layer re-asserts the store from the raster");

    // The re-assertion dropped an ENTRY, not data.
    let stored = reg
        .docs
        .get(SEED_DOC)
        .expect("doc present")
        .layers
        .get(SEED_LAYER)
        .expect("destination present")
        .pixels
        .clone();
    assert_eq!(
        stored, composite,
        "the destination holds the composite bytes after the re-assertion"
    );

    // THE CLAIM.
    let after = reg
        .docs
        .get(SEED_DOC)
        .expect("doc present")
        .history
        .history_query();
    assert_eq!(
        after.entries.len(),
        1,
        "the write's Pixel entry was dropped by the re-assertion"
    );
    assert_eq!(
        after.cursor, 1,
        "the cursor still sits above the structural entry"
    );
    assert_eq!(
        after.entries[0].payload_kind, structural_kind,
        "the surviving entry is the structural one, not a pixel entry"
    );

    // AND THE CONSEQUENCE: a pixel undo finds no `Pixel` entry to consume, so the
    // press is not spent on the composite's own write - it is free to reach the
    // structural restore. That is the observable form of "one gesture, one step".
    let undone = reg.undo_pixel(SEED_DOC);
    assert!(
        undone.is_none(),
        "no pixel entry is left for a pixel undo to consume, so the press is not spent on the composite's own write"
    );
    let after_undo = reg
        .docs
        .get(SEED_DOC)
        .expect("doc present")
        .history
        .history_query();
    assert_eq!(
        after_undo.cursor, 1,
        "the pixel undo moved nothing, leaving the structural entry as the tip the structural press reaches"
    );
    assert_eq!(
        after_undo.entries.len(),
        1,
        "the structural entry is still the only one on the stream"
    );

    reg.close_document(SEED_DOC);
}

/// THE DEFEAT, kept so the case above cannot rot into a claim the engine happens
/// to satisfy. Same sequence with step 3 REMOVED: the `Pixel` entry survives,
/// sits above the structural one, and is what the next press consumes - exactly
/// the two-press structural undo step 3 exists to prevent. If this ever stops
/// reproducing the loss, the guard above is measuring nothing.
#[test]
fn defeat_without_the_reassertion_the_pixel_entry_shadows_the_structural_one() {
    let mut g = registry();
    let reg = g.get_or_insert_with(Default::default);
    reg.open_document(SEED_DOC);

    let w: usize = 8;
    let h: usize = 8;
    seed_document(reg, w, h);
    let composite = whole_layer(w, h, 137);

    record_structural_change(reg);
    seed_composite(reg, w, h, &composite);
    // NO step 3. INVERTED ASSERTION: the drop is what step 3 does, so removing
    // step 3 must make the entry SURVIVE. If this ever stopped holding, the
    // guard above would be asserting something the engine does for free.
    let stranded = reg
        .docs
        .get(SEED_DOC)
        .expect("doc present")
        .history
        .history_query();
    assert_eq!(
        stranded.entries.len(),
        2,
        "without the re-assertion both entries remain"
    );
    assert_eq!(
        stranded.entries[1].payload_kind,
        Some(PayloadKind::Pixel),
        "the pixel entry is above the structural one"
    );

    // THE LOSS. The first press is spent re-applying the composite's own pixel
    // write, so the layer vector the structural entry carries is still pending
    // behind it: the structural undo costs TWO presses.
    let undone = reg.undo_pixel(SEED_DOC);
    assert!(
        undone.is_some(),
        "the first press was consumed by the composite's own Pixel entry, not by the structural restore"
    );
    let after_first = reg
        .docs
        .get(SEED_DOC)
        .expect("doc present")
        .history
        .history_query();
    assert_eq!(
        after_first.cursor, 1,
        "the press moved the cursor down to the structural entry, which is now the SECOND press's work"
    );
    assert_eq!(
        after_first.entries.len(),
        2,
        "the structural entry is still on the stream, untouched by that press"
    );

    // The victim the structural record removed is therefore STILL removed: the
    // press reverted the composite's own pixels, not the layer vector.
    let survivors: Vec<String> = reg
        .docs
        .get(SEED_DOC)
        .expect("doc present")
        .history
        .snapshot()
        .layers
        .into_iter()
        .map(|l| l.id)
        .collect();
    assert!(
        !survivors.contains(&SEED_VICTIM.to_string()),
        "the layer the structural entry restores is still absent - that is the second press's work"
    );
    assert!(
        survivors.contains(&SEED_LAYER.to_string()),
        "the composite destination survives the first press"
    );

    reg.close_document(SEED_DOC);
}
