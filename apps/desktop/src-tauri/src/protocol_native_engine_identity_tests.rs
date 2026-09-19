// Engine-identity attestation for the native protocol authority.
//
// WHY THIS FILE EXISTS, AND WHAT IT DOES NOT CLAIM
//
// It is tempting to test that the native-authority path and the wasm-authority
// path produce identical state over a long scripted sequence, including
// undo/redo walk-backs. That test would prove nothing here, because the two
// paths are two transports over ONE engine implementation:
//
//   wasm bridge    crates/core/src/document_core.rs:702 `protocol_apply_command`
//                  -> the module `ENGINES` thread_local
//                  -> `ProtocolEngine::apply`
//   native surface apps/desktop/src-tauri/src/protocol_native_cmds.rs:42
//                  `protocol_apply_command_native`
//                  -> the process pixel-store registry
//                  -> `DocumentPixelStore::history` (a `ProtocolEngine`)
//                  -> `ProtocolEngine::apply`
//
// `ProtocolEngine::apply` is defined exactly once in the workspace
// (crates/core/src/document_core_apply.rs:24), so a digest comparison between
// the two transports cannot detect a divergence between two engine
// implementations: there is only one. Such a test would be `f(x) == f(x)` and
// would stay green no matter what either path does.
//
// What CAN be pinned, and what this file pins, is the property that makes the
// two-authority state claim hold by reduction: the native transport adds NO
// state of its own. Per step, over the routed command script with an undo/redo
// walk-back interleaved after every command, the digest read back through the
// native commands equals the digest of a bare `ProtocolEngine` fed the same
// envelopes. Combined with the wasm bridge calling the same `apply` on the same
// type, that is the whole of the native-vs-wasm state claim - double-driving one
// engine against itself would not add evidence.
//
// The compile-time half of the pin is the `const _` assertion below: if the
// native authority ever gains a second engine type, the crate stops compiling,
// which is the signal that a genuine two-implementation comparison has become
// possible and must be written.

use super::tests::{
    apply_ok, close_doc, command_digest, envelope_json, routed_command_script, seed_routed_doc,
    sequence_canonical_fixture, snapshot_digest, ROUTED_SEED_LAYERS,
};
use crate::paint_parity_cmds::TEST_REGISTRY_LOCK;
use photrez_core::canonical_model::CanonicalDocument;
use photrez_core::pixel_store::DocumentPixelStore;
use photrez_core::protocol::{CommandEnvelope, ProtocolEngine, RenderLayer};

// Compile-time pin: the native authority's per-document engine field is the
// core `ProtocolEngine` - the same type the wasm bridge drives. A different
// type here means the migration acquired a second engine and this file's
// premise (and the parity criterion) must be re-derived. Anonymous so it costs
// no runtime and cannot warn as dead code.
const _: fn(&DocumentPixelStore) -> &ProtocolEngine =
    |doc: &DocumentPixelStore| -> &ProtocolEngine { &doc.history };

/// The native surface seeds the engine at version 10 (`seed_routed_doc`).
const SEED_VERSION: u64 = 10;

const IDENTITY_DOC: &str = "engine-identity-native-test-doc";

const UNDO_CMD: &str = r#"{"type":"undo"}"#;
const REDO_CMD: &str = r#"{"type":"redo"}"#;

/// Labels for the observed sequence, in order: the seed, then for every routed
/// command the post-apply, post-undo and post-redo observation. Built once so
/// both sides are compared at the same index and a divergence names the same
/// operation.
fn step_labels() -> Vec<String> {
    let mut labels = vec!["seed".to_string()];
    for command in routed_command_script() {
        labels.push(format!("apply {command}"));
        labels.push("undo".to_string());
        labels.push("redo".to_string());
    }
    labels
}

/// Per-step canonical state digests observed through the native command
/// surface (the exact functions the desktop client reaches over Tauri IPC).
fn native_surface_digests(doc: &str) -> Vec<u64> {
    seed_routed_doc(doc);
    let mut digests = vec![command_digest(doc)];
    for command in routed_command_script() {
        apply_ok(doc, &command);
        digests.push(command_digest(doc));
        apply_ok(doc, UNDO_CMD);
        digests.push(command_digest(doc));
        apply_ok(doc, REDO_CMD);
        digests.push(command_digest(doc));
    }
    digests
}

/// Per-step canonical state digests of a bare `ProtocolEngine` seeded exactly
/// as `seed_routed_doc` seeds the native one and fed the same envelopes. Any
/// difference from `native_surface_digests` is state the native transport added
/// or dropped.
fn bare_engine_digests() -> Vec<u64> {
    let mut engine = ProtocolEngine::new();
    let layers: Vec<RenderLayer> =
        serde_json::from_str(ROUTED_SEED_LAYERS).expect("seed layer fixture parses");
    engine.seed_layers(layers, SEED_VERSION);
    let canonical: CanonicalDocument =
        serde_json::from_str(&sequence_canonical_fixture()).expect("canonical fixture parses");
    engine.seed_canonical(canonical);

    let mut digests = vec![snapshot_digest(&engine.snapshot())];
    for command in routed_command_script() {
        for cmd in [command.as_str(), UNDO_CMD, REDO_CMD] {
            let version = engine.version();
            let envelope: CommandEnvelope = serde_json::from_str(&envelope_json(cmd, version))
                .expect("reference envelope parses");
            engine
                .apply(envelope)
                .unwrap_or_else(|e| panic!("reference rejected {cmd}: {e:?}"));
            assert_eq!(
                engine.version(),
                version + 1,
                "reference {cmd} must bump the version by exactly one",
            );
            digests.push(snapshot_digest(&engine.snapshot()));
        }
    }
    digests
}

#[test]
fn native_transport_is_step_by_step_transparent_to_the_one_engine() {
    let _registry_guard = TEST_REGISTRY_LOCK.lock().unwrap();
    let labels = step_labels();
    let native = native_surface_digests(IDENTITY_DOC);
    let reference = bare_engine_digests();
    close_doc(IDENTITY_DOC);

    assert_eq!(native.len(), labels.len(), "native step count");
    assert_eq!(reference.len(), labels.len(), "reference step count");

    // Vacuity guards. Without these the comparison would pass on a sequence
    // that silently became a run of no-ops, or on a truncated script. Measured
    // at the time of writing: 81 of the 87 observed transitions move the digest.
    let moved = native.windows(2).filter(|w| w[0] != w[1]).count();
    let transitions = native.len() - 1;
    assert!(
        moved * 3 >= transitions * 2,
        "the routed script moved state only {moved} times out of {transitions} transitions - \
         the comparison would be vacuous",
    );

    // Every undo/redo walk-back must return to the post-apply state. Asserted
    // on the native side directly, so the walk-back contract is pinned even if
    // the reference side ever degenerates.
    for (i, chunk) in native[1..].chunks_exact(3).enumerate() {
        let (apply_digest, _undo, redo_digest) = (chunk[0], chunk[1], chunk[2]);
        assert_eq!(
            redo_digest,
            apply_digest,
            "walk-back {i} ({}): redo did not restore the post-apply state",
            labels[1 + i * 3],
        );
    }

    for (i, (native_digest, reference_digest)) in native.iter().zip(reference.iter()).enumerate() {
        if native_digest != reference_digest {
            panic!(
                "state diverged at step {i} of {} ({}) - native transport digest {native_digest}, \
                 bare ProtocolEngine digest {reference_digest}",
                labels.len(),
                labels[i],
            );
        }
    }
}
