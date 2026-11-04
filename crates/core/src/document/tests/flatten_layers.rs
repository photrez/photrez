// SPDX-License-Identifier: AGPL-3.0-or-later
// Flatten arms of the structural contract: the whole stack collapses into one background
// node in engine order with a fresh resource id, a single layer is a no-op, and an id that
// is empty or already taken is rejected. The helpers are shared with `structural.rs`, which
// pins the rest of the same contract.
use super::commands::env;
use super::structural::{canonical_dim_doc, ids_of, layer_by, mk};
use crate::document::DocumentCore;
use crate::ptz::content::LayerType;
use crate::wire::command::*;
use crate::wire::projection::RenderSnapshot;

#[test]
fn flatten_replaces_all_with_single_background_node() {
    let mut e = DocumentCore::new();
    e.seed_ptz(canonical_dim_doc());
    e.seed_layers(
        vec![
            mk("A", "Layer 1", 10),
            mk("B", "Layer 2", 20),
            mk("C", "Layer 3", 30),
        ],
        0,
    );
    e.apply(env(Command::Flatten {
        merged_id: "BG".into(),
    }))
    .unwrap();
    assert_eq!(ids_of(&e), vec!["BG"]);
    let bg = layer_by(&e, "BG");
    assert_eq!(bg.name, "Background");
    assert_eq!(bg.layer_type, Some(LayerType::Raster));
    assert_eq!(bg.is_background, Some(true));
    // The background node carries the position/rotation locks a real background has,
    // but is not itself locked.
    assert_eq!(bg.lock_position, Some(true));
    assert_eq!(bg.lock_rotation, Some(true));
    assert_eq!(bg.locked, Some(false));
    assert_eq!(bg.width, Some(10.0));
    assert_eq!(bg.height, Some(10.0));
}

#[test]
fn flatten_merged_node_mints_fresh_resource_id() {
    // Fresh-resource-id ratchet: flatten mints a fresh resource id for the background node.
    let mut e = DocumentCore::new();
    e.seed_ptz(canonical_dim_doc());
    e.seed_layers(
        vec![
            mk("A", "Layer 1", 10),
            mk("B", "Layer 2", 20),
            mk("C", "Layer 3", 30),
        ],
        0,
    );
    e.apply(env(Command::Flatten {
        merged_id: "BG".into(),
    }))
    .unwrap();
    assert_eq!(layer_by(&e, "BG").resource_id, 31);
}

#[test]
fn flatten_noop_when_single_layer() {
    let mut e = DocumentCore::new();
    e.seed_ptz(canonical_dim_doc());
    e.seed_layers(vec![mk("A", "Layer 1", 10)], 0);
    let result = e
        .apply(env(Command::Flatten {
            merged_id: "BG".into(),
        }))
        .unwrap();
    assert_eq!(ids_of(&e), vec!["A"]);
    assert!(result.delta.changes.is_empty());
}

#[test]
fn flatten_rejects_empty_and_present_merged_id() {
    let mut e = DocumentCore::new();
    e.seed_ptz(canonical_dim_doc());
    e.seed_layers(vec![mk("A", "Layer 1", 10), mk("B", "Layer 2", 20)], 0);
    let before: RenderSnapshot = e.snapshot();
    let r1 = e.apply(env(Command::Flatten {
        merged_id: "".into(),
    }));
    assert!(r1.is_err(), "empty merged_id must be rejected");
    assert_eq!(r1.unwrap_err().code, "E_INVALID");
    assert_eq!(
        e.snapshot(),
        before,
        "rejected flatten must not mutate engine"
    );
    let r2 = e.apply(env(Command::Flatten {
        merged_id: "A".into(),
    }));
    assert!(r2.is_err(), "present merged_id must be rejected");
    assert_eq!(r2.unwrap_err().code, "E_INVALID");
    assert_eq!(
        e.snapshot(),
        before,
        "rejected flatten must not mutate engine"
    );
}

#[test]
fn flatten_rejects_without_seeded_canonical_dims() {
    // The native arm reads the seeded canonical shadow for dims; without it the
    // command rejects rather than guessing.
    let mut e = DocumentCore::new();
    e.seed_layers(vec![mk("A", "Layer 1", 10), mk("B", "Layer 2", 20)], 0);
    let before: RenderSnapshot = e.snapshot();
    let r = e.apply(env(Command::Flatten {
        merged_id: "BG".into(),
    }));
    assert!(
        r.is_err(),
        "flatten without seeded canonical dims must be rejected"
    );
    assert_eq!(r.unwrap_err().code, "E_INVALID");
    assert_eq!(
        e.snapshot(),
        before,
        "rejected flatten must not mutate engine"
    );
}
