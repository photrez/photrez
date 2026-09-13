// SPDX-License-Identifier: AGPL-3.0-or-later
// Engine-level fuzz for ProtocolEngine: random commands drawn from the full
// routed set plus undo/redo interleaves. This drives the REAL Rust engine (the
// exact code the desktop app runs), not a TypeScript emulator, so a green run is
// evidence about the engine's own trust-boundary and history invariants.
//
// Test-only: this file does not touch production logic. A divergence found here
// is reported, never papered over by loosening the fuzz.
//
// Determinism: the same hand-rolled xorshift32 the pixel-store randomized test
// uses (no external RNG dependency). A fixed seed reproduces a run exactly; the
// seed sweep covers several streams.
//
// What is asserted per operation:
//   * no panic: apply() returns Ok or a clean Err
//   * document version: Ok bumps DV exactly once, Err leaves it unchanged, and
//     DV never regresses
//   * history cursor: stays within [0, entries], agrees with can_undo/can_redo,
//     and undo/redo move it by at most one
//   * layer set: no empty or duplicate ids, count stays within the bound implied
//     by the forward add/duplicate ops seen
//   * undo/redo round-trip: the state digest is identical before an undo and
//     after the matching redo, and the cursor is restored
//   * memory: a pure undo/redo storm neither grows nor shrinks the history
//     entries or their retained byte estimate

use super::arm_tests::{env, shape_params, text_data};
use crate::canonical_model::{
    BasicAdjustment, BlendMode, CanonicalDocument, LayerType, SelectionShape, SelectionState,
};
use crate::command::{Command, LockKind, TransformPatch};
use crate::document_core::ProtocolEngine;
use crate::model::RenderLayer;

const DEFAULT_OPS: usize = 1000;
const DEFAULT_SEED: u32 = 0xC0FFEE;

// -- seeding (mirrors the structural/canvas arm tests) ---------------------

/// Canonical shadow with known dims and an empty layer vector. Merge/flatten
/// read document dims from this shadow; select-all/invert read it too.
fn canonical_dim_doc() -> CanonicalDocument {
    CanonicalDocument {
        id: "fuzz-doc".into(),
        name: "Fuzz".into(),
        width: 100.0,
        height: 100.0,
        layers: vec![],
        selection: None,
    }
}

fn seed_layer(id: &str, resource_id: u32) -> RenderLayer {
    RenderLayer {
        id: id.into(),
        name: id.into(),
        visible: true,
        opacity: 1.0,
        resource_id,
        x: 0.0,
        y: 0.0,
        scale_x: 1.0,
        scale_y: 1.0,
        rotation: 0.0,
        dirty_rect: None,
        ..Default::default()
    }
}

/// Engine with a seeded canonical shadow (dims) and two engine layers. Order
/// matters: seed_canonical replaces the engine layer set from its push, so the
/// dims push comes first and the layer load second, exactly as the structural
/// and canvas arm tests do it.
fn seeded_engine() -> ProtocolEngine {
    let mut e = ProtocolEngine::new();
    e.seed_canonical(canonical_dim_doc());
    e.seed_layers(vec![seed_layer("L1", 1), seed_layer("L2", 2)], 0);
    e
}

// -- deterministic RNG (same xorshift32 shape as pixel_store/tests.rs) ------

fn next_u32(rng: &mut u32) -> u32 {
    let mut s = *rng;
    s ^= s << 13;
    s ^= s >> 17;
    s ^= s << 5;
    *rng = s;
    s
}

/// Mix the user-facing seed so a small sweep (0, 1, 2, ...) still yields
/// distinct streams, and never hand xorshift32 the fixed point 0.
fn seed_rng(seed: u32) -> u32 {
    let mixed = seed.wrapping_mul(0x9E37_79B9).wrapping_add(1);
    if mixed == 0 {
        1
    } else {
        mixed
    }
}

fn chance(rng: &mut u32, percent: u32) -> bool {
    next_u32(rng) % 100 < percent
}

/// A number that is mostly in a small valid range but occasionally non-finite or
/// extreme, so the arms' trust-boundary guards are exercised. A clean Err is a
/// pass; a panic is a failure.
fn rand_f64(rng: &mut u32) -> f64 {
    match next_u32(rng) % 10 {
        0 => f64::NAN,
        1 => f64::INFINITY,
        2 => f64::NEG_INFINITY,
        3 => 0.0,
        4 => -1.0,
        5 => -0.5,
        6 => 1.0e12,
        7 => -1.0e12,
        _ => (next_u32(rng) % 400) as f64 / 100.0 - 2.0,
    }
}

fn rand_blend(rng: &mut u32) -> BlendMode {
    match next_u32(rng) % 12 {
        0 => BlendMode::Normal,
        1 => BlendMode::Multiply,
        2 => BlendMode::Screen,
        3 => BlendMode::Overlay,
        4 => BlendMode::Darken,
        5 => BlendMode::Lighten,
        6 => BlendMode::ColorDodge,
        7 => BlendMode::ColorBurn,
        8 => BlendMode::SoftLight,
        9 => BlendMode::HardLight,
        10 => BlendMode::Difference,
        _ => BlendMode::Exclusion,
    }
}

/// Host-minted id, like the real host supplies for structural ops.
fn fresh_id(next_id: &mut u32) -> String {
    let id = *next_id;
    *next_id += 1;
    format!("N{id}")
}

fn live_ids(e: &ProtocolEngine) -> Vec<String> {
    e.snapshot().layers.iter().map(|l| l.id.clone()).collect()
}

/// Mostly a live id, occasionally an unknown one, so the arms' silent no-op
/// paths (unknown id) are exercised alongside the happy path.
fn rand_existing_id(rng: &mut u32, ids: &[String]) -> String {
    if ids.is_empty() || chance(rng, 10) {
        let n = next_u32(rng) % 1000;
        format!("missing-{n}")
    } else {
        let i = (next_u32(rng) as usize) % ids.len();
        ids[i].clone()
    }
}

// -- digest / history stats -------------------------------------------------

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// FNV-1a digest of the reversible document state: layers + selection + doc
/// size. The monotonic `version` counter is deliberately excluded - every
/// accepted apply (including undo/redo and no-ops) bumps it, so it is a
/// transition counter, not state. Comparing it across an undo/redo round-trip
/// would be a false invariant; the meaningful round-trip check is that the
/// STATE and the cursor are restored.
fn state_digest(e: &ProtocolEngine) -> u64 {
    let s = e.snapshot();
    let json = serde_json::to_string(&(s.layers, s.selection, s.width, s.height))
        .expect("snapshot state serializes to JSON");
    fnv1a64(json.as_bytes())
}

/// (entry count, summed retained byte estimate) from the same history query the
/// TypeScript facade reads.
fn history_stats(e: &ProtocolEngine) -> (usize, u64) {
    let q = e.history_query();
    let bytes: u64 = q.entries.iter().map(|x| x.memory_cost_bytes).sum();
    (q.entries.len(), bytes)
}

// -- random command generation ---------------------------------------------

/// Build one random command from the full routed set. Returns the command and
/// whether it can grow the layer count (used for a cheap layer-count bound).
fn random_command(rng: &mut u32, ids: &[String], next_id: &mut u32) -> (Command, bool) {
    let pick = next_u32(rng) % 24;
    let may_add = matches!(pick, 0 | 13);
    let cmd = match pick {
        0 => {
            let id = match next_u32(rng) % 8 {
                0 => String::new(), // E_INVALID: empty id
                1 => ids.first().cloned().unwrap_or_else(|| fresh_id(next_id)), // E_INVALID: duplicate id
                _ => fresh_id(next_id),
            };
            let (layer_type, shape, text) = match next_u32(rng) % 4 {
                0 => (Some(LayerType::Shape), Some(shape_params()), None),
                1 => (Some(LayerType::Text), None, Some(text_data())),
                2 => (Some(LayerType::Shape), None, None), // edge: typed add without payload
                _ => (None, None, None),
            };
            let n = next_u32(rng) % 100;
            Command::AddLayer {
                id,
                name: format!("Add{n}"),
                width: rand_f64(rng),
                height: rand_f64(rng),
                index: (next_u32(rng) % 6) as usize,
                layer_type,
                shape_params: shape,
                text_data: text,
            }
        }
        1 => Command::DeleteLayer {
            id: rand_existing_id(rng, ids),
        },
        2 => Command::TransformLayer {
            id: rand_existing_id(rng, ids),
            transform: TransformPatch {
                x: rand_f64(rng),
                y: rand_f64(rng),
                scale_x: rand_f64(rng),
                scale_y: rand_f64(rng),
                rotation: rand_f64(rng),
                flip_h: if chance(rng, 50) {
                    Some(next_u32(rng).is_multiple_of(2))
                } else {
                    None
                },
                flip_v: if chance(rng, 50) {
                    Some(next_u32(rng).is_multiple_of(2))
                } else {
                    None
                },
            },
        },
        3 => Command::SetOpacity {
            id: rand_existing_id(rng, ids),
            opacity: rand_f64(rng),
        },
        4 => Command::SetVisible {
            id: rand_existing_id(rng, ids),
            visible: next_u32(rng).is_multiple_of(2),
        },
        5 => Command::SetLocked {
            id: rand_existing_id(rng, ids),
            kind: match next_u32(rng) % 4 {
                0 => LockKind::Base,
                1 => LockKind::Transparency,
                2 => LockKind::Position,
                _ => LockKind::Rotation,
            },
            locked: next_u32(rng).is_multiple_of(2),
        },
        6 => {
            let n = next_u32(rng) % 100;
            Command::Rename {
                id: rand_existing_id(rng, ids),
                name: format!("R{n}"),
            }
        }
        7 => Command::Reorder {
            id: rand_existing_id(rng, ids),
            to: (next_u32(rng) % 8) as usize, // sometimes out of range -> clean Err
        },
        8 => Command::SetBackgroundFlag {
            id: rand_existing_id(rng, ids),
        },
        9 => Command::SetBlendMode {
            id: rand_existing_id(rng, ids),
            mode: rand_blend(rng),
        },
        10 => {
            let (shape, text) = match next_u32(rng) % 4 {
                0 => (Some(shape_params()), None),
                1 => (None, Some(text_data())),
                2 => (Some(shape_params()), Some(text_data())),
                _ => (None, None), // E_INVALID: both None
            };
            Command::SetLayerParams {
                id: rand_existing_id(rng, ids),
                shape_params: shape,
                text_data: text,
            }
        }
        11 => Command::SetAdjustment {
            id: rand_existing_id(rng, ids),
            adjustment: if chance(rng, 75) {
                Some(BasicAdjustment {
                    brightness: rand_f64(rng),
                    contrast: rand_f64(rng),
                    saturation: rand_f64(rng),
                })
            } else {
                None
            },
        },
        12 => Command::RasterizeLayer {
            id: rand_existing_id(rng, ids),
        },
        13 => Command::DuplicateLayer {
            id: rand_existing_id(rng, ids),
            new_id: fresh_id(next_id),
        },
        14 => Command::MergeDown {
            id: rand_existing_id(rng, ids),
            merged_id: fresh_id(next_id),
        },
        15 => {
            let n = 2 + (next_u32(rng) % 2) as usize; // 2..=3 ids (may repeat or be unknown)
            let selected: Vec<String> = (0..n).map(|_| rand_existing_id(rng, ids)).collect();
            Command::MergeSelected {
                ids: selected,
                merged_id: fresh_id(next_id),
            }
        }
        16 => Command::Flatten {
            merged_id: fresh_id(next_id),
        },
        17 => Command::CropCanvas {
            x: rand_f64(rng),
            y: rand_f64(rng),
            width: rand_f64(rng),
            height: rand_f64(rng),
        },
        18 => Command::ApplyCrop {
            x: rand_f64(rng),
            y: rand_f64(rng),
            width: rand_f64(rng),
            height: rand_f64(rng),
            rotation: if chance(rng, 60) {
                Some(rand_f64(rng))
            } else {
                None
            },
            target_width: if chance(rng, 40) {
                Some(rand_f64(rng))
            } else {
                None
            },
            target_height: if chance(rng, 40) {
                Some(rand_f64(rng))
            } else {
                None
            },
        },
        19 => Command::ResizeCanvas {
            width: rand_f64(rng),
            height: rand_f64(rng),
        },
        20 => Command::SetSelection {
            selection: SelectionState {
                x: rand_f64(rng),
                y: rand_f64(rng),
                width: rand_f64(rng),
                height: rand_f64(rng),
                angle: rand_f64(rng),
                shape: match next_u32(rng) % 3 {
                    0 => Some(SelectionShape::Rect),
                    1 => Some(SelectionShape::Ellipse),
                    _ => None,
                },
                inverted: if chance(rng, 50) {
                    Some(next_u32(rng).is_multiple_of(2))
                } else {
                    None
                },
            },
        },
        21 => Command::ClearSelection,
        22 => Command::SelectAll,
        _ => Command::InvertSelection,
    };
    (cmd, may_add)
}

// -- undo / redo helpers ----------------------------------------------------

fn apply_undo(e: &mut ProtocolEngine, version_before: u64, stats_before: (usize, u64)) {
    let cursor_before = e.cursor();
    let result = e.apply(env(Command::Undo)).expect("undo returns Ok");
    assert_eq!(
        result.document_version,
        version_before + 1,
        "undo bumps DV exactly once"
    );
    assert_eq!(e.version(), version_before + 1);
    let expected = cursor_before.saturating_sub(1);
    assert_eq!(
        e.cursor(),
        expected,
        "undo moves the cursor down by one (or stays at 0)"
    );
    assert_eq!(
        history_stats(e),
        stats_before,
        "undo must not add or evict history entries"
    );
}

fn apply_redo(e: &mut ProtocolEngine, version_before: u64, stats_before: (usize, u64)) {
    let cursor_before = e.cursor();
    let entries_before = e.history_query().entries.len();
    let result = e.apply(env(Command::Redo)).expect("redo returns Ok");
    assert_eq!(
        result.document_version,
        version_before + 1,
        "redo bumps DV exactly once"
    );
    assert_eq!(e.version(), version_before + 1);
    let expected = if cursor_before < entries_before {
        cursor_before + 1
    } else {
        cursor_before
    };
    assert_eq!(
        e.cursor(),
        expected,
        "redo moves the cursor up by one (or stays at the tip)"
    );
    assert_eq!(
        history_stats(e),
        stats_before,
        "redo must not add or evict history entries"
    );
}

/// Undo then immediately redo: the state digest and cursor must be restored.
/// Only the monotonic DV advances (by two), which is the documented transition
/// contract, not state.
fn undo_redo_round_trip(e: &mut ProtocolEngine, version_before: u64, stats_before: (usize, u64)) {
    if !e.can_undo() {
        // Nothing below the cursor: a clean no-op undo is still a DV transition.
        apply_undo(e, version_before, stats_before);
        return;
    }
    let cursor_at = e.cursor();
    let digest_at = state_digest(e);
    apply_undo(e, version_before, stats_before);
    assert_eq!(e.cursor(), cursor_at - 1, "round-trip undo lands one below");
    let undo_version = e.version();
    let result = e.apply(env(Command::Redo)).expect("redo returns Ok");
    assert_eq!(
        result.document_version,
        undo_version + 1,
        "round-trip redo bumps DV once"
    );
    assert_eq!(e.cursor(), cursor_at, "redo restores the cursor position");
    assert_eq!(
        state_digest(e),
        digest_at,
        "undo->redo restores the exact document state"
    );
    assert_eq!(
        history_stats(e),
        stats_before,
        "round-trip must not touch history entries"
    );
}

// -- the fuzz loop ----------------------------------------------------------

/// Op-mix counters, printed per run so the report can show the invalid-input
/// (clean Err) and undo/redo paths were actually exercised, not just the happy
/// path.
#[derive(Default)]
struct FuzzStats {
    ok: usize,
    err: usize,
    undo: usize,
    redo: usize,
    round_trip: usize,
}

fn fuzz_run(seed: u32, ops: usize) -> FuzzStats {
    let mut rng = seed_rng(seed);
    let mut e = seeded_engine();
    let mut next_id: u32 = 0;
    let mut forward_ops: usize = 0;
    let mut stats = FuzzStats::default();
    // Upper bound on the live layer count: the two seeded layers plus every
    // successful add/duplicate. Undo/redo never create a layer that never
    // existed, and merge/flatten only reduce, so this bound always holds.
    let mut layer_bound: usize = 2;
    let mut last_version = e.version();

    for _ in 0..ops {
        let version_before = e.version();
        let stats_before = history_stats(&e);

        if chance(&mut rng, 25) {
            match next_u32(&mut rng) % 3 {
                0 => {
                    apply_undo(&mut e, version_before, stats_before);
                    stats.undo += 1;
                }
                1 => {
                    apply_redo(&mut e, version_before, stats_before);
                    stats.redo += 1;
                }
                _ => {
                    undo_redo_round_trip(&mut e, version_before, stats_before);
                    stats.round_trip += 1;
                }
            }
        } else {
            let ids = live_ids(&e);
            let (cmd, may_add) = random_command(&mut rng, &ids, &mut next_id);
            match e.apply(env(cmd)) {
                Ok(result) => {
                    assert_eq!(
                        result.document_version,
                        version_before + 1,
                        "a successful apply bumps DV exactly once"
                    );
                    forward_ops += 1;
                    stats.ok += 1;
                    if may_add {
                        layer_bound += 1;
                    }
                }
                Err(_) => {
                    assert_eq!(
                        e.version(),
                        version_before,
                        "a rejected apply must not bump DV"
                    );
                    stats.err += 1;
                }
            }
        }

        // Invariants that must hold after every single operation.
        assert!(e.version() >= last_version, "document version regressed");
        last_version = e.version();

        let q = e.history_query();
        assert!(
            q.cursor <= q.entries.len() as u64,
            "history cursor {} out of range for {} entries",
            q.cursor,
            q.entries.len()
        );
        assert_eq!(
            e.cursor() as u64,
            q.cursor,
            "cursor() and history_query() disagree"
        );
        assert_eq!(
            e.can_undo(),
            q.cursor > 0,
            "can_undo disagrees with the cursor"
        );
        assert_eq!(
            e.can_redo(),
            (q.cursor as usize) < q.entries.len(),
            "can_redo disagrees with the cursor"
        );
        assert!(
            q.entries.len() <= forward_ops,
            "history entries ({}) exceed forward commits ({forward_ops})",
            q.entries.len()
        );

        let snapshot = e.snapshot();
        assert!(
            snapshot.layers.len() <= layer_bound,
            "layer count {} exceeds the op-derived bound {layer_bound}",
            snapshot.layers.len()
        );
        let mut seen = std::collections::HashSet::with_capacity(snapshot.layers.len());
        for layer in &snapshot.layers {
            assert!(!layer.id.is_empty(), "empty layer id in snapshot");
            assert!(
                seen.insert(layer.id.clone()),
                "duplicate layer id: {}",
                layer.id
            );
        }
    }

    // Full-history round-trip. First normalize to the tip (the main loop may have
    // left the cursor below it), then undo every entry to the bottom recording a
    // digest at each cursor, then redo back up comparing against the down pass.
    // The engine is a pure transition stack, so the two passes must agree exactly.
    while e.can_redo() {
        let v = e.version();
        let result = e.apply(env(Command::Redo)).expect("redo returns Ok");
        assert_eq!(result.document_version, v + 1);
    }
    let mut down: Vec<u64> = Vec::new();
    down.push(state_digest(&e)); // cursor at the tip
    while e.can_undo() {
        let v = e.version();
        let stats = history_stats(&e);
        apply_undo(&mut e, v, stats);
        down.push(state_digest(&e));
    }
    assert_eq!(e.cursor(), 0, "full undo reaches the bottom");
    let depth = e.history_query().entries.len();
    assert_eq!(down.len(), depth + 1, "digest recorded at every cursor");
    while e.can_redo() {
        let v = e.version();
        let result = e.apply(env(Command::Redo)).expect("redo returns Ok");
        assert_eq!(result.document_version, v + 1);
        let c = e.cursor();
        assert_eq!(
            state_digest(&e),
            down[depth - c],
            "redo digest diverged at cursor {c}"
        );
    }
    assert_eq!(e.cursor(), depth, "redo pass returns to the tip");
    stats
}

fn run_and_time(seed: u32, ops: usize) {
    let start = std::time::Instant::now();
    let stats = fuzz_run(seed, ops);
    let elapsed = start.elapsed();
    eprintln!(
        "[fuzz] seed={seed} ops={ops} elapsed={elapsed:?} ok={} err={} undo={} redo={} round_trip={}",
        stats.ok, stats.err, stats.undo, stats.redo, stats.round_trip
    );
}

// -- tests ------------------------------------------------------------------

#[test]
fn fuzz_1000_random_routed_ops_pass() {
    run_and_time(DEFAULT_SEED, DEFAULT_OPS);
}

/// Extra-seed sweep (10 seeds x 1000 ops = 10000 ops). Kept as a normal test
/// because a metadata-only run is cheap; it broadens coverage beyond the fixed
/// seed without slowing the suite meaningfully.
#[test]
fn fuzz_seed_sweep_10000_ops_pass() {
    for seed in 0..10u32 {
        run_and_time(seed, DEFAULT_OPS);
    }
}

/// A pure undo/redo storm must not grow or shrink the retained history: no new
/// entries, no eviction, and no change to the retained byte estimate. This is
/// the engine-level memory-stability check; the facade's 512-entry dropped-node
/// cache lives in the TypeScript layer and is out of scope for a Rust unit test.
#[test]
fn undo_redo_storm_memory_stable() {
    let mut rng = seed_rng(0x5EED);
    let mut e = seeded_engine();
    let mut next_id: u32 = 0;
    for _ in 0..12 {
        let ids = live_ids(&e);
        let (cmd, _) = random_command(&mut rng, &ids, &mut next_id);
        let _ = e.apply(env(cmd));
    }
    let (entries, bytes) = history_stats(&e);
    assert!(entries > 0, "storm test needs a non-empty history");

    let mut last_version = e.version();
    for i in 0..2000u32 {
        let cmd = if e.can_undo() && (i.is_multiple_of(2) || !e.can_redo()) {
            Command::Undo
        } else {
            Command::Redo
        };
        let version_before = e.version();
        let result = e.apply(env(cmd)).expect("undo/redo returns Ok");
        assert_eq!(result.document_version, version_before + 1);
        assert!(
            e.version() >= last_version,
            "version regressed during storm"
        );
        last_version = e.version();

        let q = e.history_query();
        assert!(
            q.cursor <= q.entries.len() as u64,
            "cursor left range in storm"
        );
        let sum: u64 = q.entries.iter().map(|x| x.memory_cost_bytes).sum();
        assert_eq!(
            (q.entries.len(), sum),
            (entries, bytes),
            "pure undo/redo must not grow or shrink retained history"
        );
    }
}

/// Boundary behavior pinned directly: undo past the bottom and redo past the top
/// are clean no-op transitions (DV bumps, cursor and state unchanged), never a
/// panic.
#[test]
fn undo_past_bottom_and_redo_past_top_are_clean_noops() {
    let mut e = seeded_engine();
    let digest = state_digest(&e);

    let v0 = e.version();
    let undo = e
        .apply(env(Command::Undo))
        .expect("undo at bottom returns Ok");
    assert_eq!(undo.document_version, v0 + 1);
    assert_eq!(e.cursor(), 0);
    assert_eq!(
        state_digest(&e),
        digest,
        "no-op undo leaves state unchanged"
    );

    let v1 = e.version();
    let redo = e.apply(env(Command::Redo)).expect("redo at top returns Ok");
    assert_eq!(redo.document_version, v1 + 1);
    assert_eq!(e.cursor(), 0, "redo at top is a no-op");
    assert_eq!(
        state_digest(&e),
        digest,
        "no-op redo leaves state unchanged"
    );
}
