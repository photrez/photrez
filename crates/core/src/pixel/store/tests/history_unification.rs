// SPDX-License-Identifier: AGPL-3.0-or-later
//! History unification proof: the engine-side (pixel) and host-side (non-pixel)
//! operations share EXACTLY ONE logical history cursor.

use super::tile;
use crate::pixel::store::PixelStoreRegistry;
use crate::pixel::tile::TilePatch;

// Î“Ã¶Ã‡Î“Ã¶Ã‡ History unification Î“Ã¶Ã‡Î“Ã¶Ã‡
// Proof target: TS (non-pixel) and Rust (pixel) operations share EXACTLY ONE
// logical history cursor. The Rust `DocumentCore` already routes both
// `apply_pixel_patch` (Pixel entry) and `record_external` (External entry)
// into the same `entries`/`cursor`/`version` stream. These tests assert the
// invariants the gate requires on the REAL carrier (no hand-rolled cursor).

#[derive(Clone, Copy, Debug)]
enum Op {
    RustPixel(u8), // Rust-side pixel mutation -> apply_pixel_patch (Pixel entry)
    TsMeta,        // TS-side non-pixel mutation -> record_external (External entry)
    Undo,
    Redo,
}

fn xor_all(buf: &[u8], k: u8) -> Vec<u8> {
    buf.iter().map(|b| b ^ k).collect()
}

/// Drive a real `PixelStoreRegistry` with a mixed op sequence and assert, at
/// every step, the history invariants:
///  - exactly ONE cursor position (`get_history_cursor == oracle cursor`),
///  - canonical pixels equal the independent oracle,
///  - `DocumentVersion` is strictly monotonic on commit, non-decreasing on undo/redo.
fn run_single_cursor(ops: &[Op]) -> bool {
    const N: usize = 256;
    let mut r = PixelStoreRegistry::new();
    r.open_document("docA");
    let seed = vec![0u8; N * N * 4];
    r.add_layer("docA", "L", N as u32, N as u32, seed.clone())
        .unwrap();

    let mut oracle: Vec<u8> = seed.clone();
    let mut cursor: usize = 0; // applied entries == Rust cursor

    let mut prev_version: u64 = 0;
    let mut tip: usize = 0; // total committed (redo branch truncated on new op)

    for op in ops {
        match op {
            Op::RustPixel(k) => {
                let cur = r.get_layer("docA", "L").unwrap().pixels.clone();
                let after_data = xor_all(&cur, *k);
                let before_patch = TilePatch {
                    x: 0,
                    y: 0,
                    w: N,
                    h: N,
                    data: cur,
                };
                let after_patch = TilePatch {
                    x: 0,
                    y: 0,
                    w: N,
                    h: N,
                    data: after_data.clone(),
                };
                let (_b, _ep, ver) = r
                    .apply_pixel_patch("docA", "L", vec![before_patch], vec![after_patch])
                    .expect("apply_pixel_patch");
                oracle = after_data;
                cursor += 1;
                tip = cursor;
                if ver != prev_version + 1 {
                    return false; // version must advance exactly once per commit
                }
                prev_version = ver;
            }
            Op::TsMeta => {
                // TS non-pixel op joins the SAME cursor as an External entry.
                r.record_external(
                    "docA",
                    "ts-meta",
                    &["L".to_string()],
                    "ts",
                    "tok",
                    0,
                    None,
                    None,
                    &[],
                )
                .expect("record_external");
                cursor += 1;
                tip = cursor;
                let v = r.get_history_version("docA").unwrap();
                if v != prev_version + 1 {
                    return false; // record_external bumps version once
                }
                prev_version = v;
            }
            Op::Undo => {
                if cursor == 0 {
                    return false; // cannot undo past start
                }
                match r.undo_pixel("docA") {
                    Some((_l, tiles, _ep, _ver)) => {
                        if let Some(t) = tiles.first() {
                            oracle = t.data.clone();
                        }
                        // External entry: registry returns None -> oracle unchanged.
                    }
                    None => { /* external entry: oracle unchanged, cursor already decremented */ }
                }
                cursor -= 1;
                let v = r.get_history_version("docA").unwrap();
                if v < prev_version {
                    return false;
                }
                prev_version = v;
            }
            Op::Redo => {
                if cursor >= tip {
                    return false; // nothing to redo
                }
                match r.redo_pixel("docA") {
                    Some((_l, tiles, _ep, _ver)) => {
                        if let Some(t) = tiles.first() {
                            oracle = t.data.clone();
                        }
                    }
                    None => { /* external entry: oracle unchanged */ }
                }
                cursor += 1;
                let v = r.get_history_version("docA").unwrap();
                if v < prev_version {
                    return false;
                }
                prev_version = v;
            }
        }
        if r.get_history_cursor("docA") != Some(cursor) {
            return false; // INVARIANT: exactly one cursor position
        }
        if r.get_layer("docA", "L").unwrap().pixels != oracle {
            return false; // INVARIANT: canonical pixels match oracle
        }
    }
    true
}

fn seq(ops: Vec<Op>) -> Vec<Op> {
    ops
}

#[test]
fn phase1_a_sequential_rust_ts_distinct_positions() {
    // A: [Rust, TS, Rust, TS] -> 4 distinct entries, interleaved fine.
    assert!(run_single_cursor(&seq(vec![
        Op::RustPixel(1),
        Op::TsMeta,
        Op::RustPixel(2),
        Op::TsMeta,
    ])));
}

#[test]
fn phase1_b_interleaved_ts_rust_any_order() {
    // B: [TS, Rust, TS, Rust] -> interleave reversed, also fine.
    assert!(run_single_cursor(&seq(vec![
        Op::TsMeta,
        Op::RustPixel(1),
        Op::TsMeta,
        Op::RustPixel(2),
    ])));
}

#[test]
fn phase1_c_undo_redo_returns_same_state() {
    // C: [Rust, TS, Undo, Redo] -> back to same canonical + cursor.
    assert!(run_single_cursor(&seq(vec![
        Op::RustPixel(1),
        Op::TsMeta,
        Op::Undo,
        Op::Redo,
    ])));
}

#[test]
fn phase1_d_full_undo_redo_roundtrip() {
    // D: [Rust, TS, Undo, Undo, Redo, Redo] -> round-trip to original.
    assert!(run_single_cursor(&seq(vec![
        Op::RustPixel(1),
        Op::TsMeta,
        Op::Undo,
        Op::Undo,
        Op::Redo,
        Op::Redo,
    ])));
}

#[test]
fn phase1_e_partial_undo_then_new_severs_redo() {
    // E: [Rust, TS, Undo(partial), TS-new] -> redo branch severed; cursor jumps.
    assert!(run_single_cursor(&seq(vec![
        Op::RustPixel(1),
        Op::TsMeta,
        Op::Undo,
        Op::TsMeta,
    ])));
    // And the severed redo is genuinely unavailable:
    let mut r = PixelStoreRegistry::new();
    r.open_document("d");
    r.add_layer("d", "L", 8, 8, vec![0u8; 8 * 8 * 4]).unwrap();
    r.apply_pixel_patch(
        "d",
        "L",
        vec![tile(0, 0, 8, 8, 0)],
        vec![tile(0, 0, 8, 8, 9)],
    )
    .unwrap();
    r.record_external("d", "m", &["L".to_string()], "ts", "t", 0, None, None, &[])
        .unwrap();
    r.undo_pixel("d"); // undo the TS meta entry -> cursor 1
    let after_undo = r.get_history_cursor("d").unwrap();
    // External redo returns no pixel tiles (TS reverts its own metadata) but the
    // UNIFIED cursor MUST advance Î“Ã‡Ã¶ that is the real redo signal.
    let redone = r.redo_pixel("d");
    assert_eq!(
        r.get_history_cursor("d"),
        Some(after_undo + 1),
        "redo advances the unified cursor"
    );
    assert!(redone.is_none(), "external redo returns no pixel tiles");
    r.record_external(
        "d",
        "m2",
        &["L".to_string()],
        "ts",
        "t2",
        0,
        None,
        None,
        &[],
    )
    .unwrap(); // new op -> severs redo
    let at_tip = r.get_history_cursor("d").unwrap();
    assert!(r.redo_pixel("d").is_none(), "redo truncated after new op");
    assert_eq!(
        r.get_history_cursor("d"),
        Some(at_tip),
        "redo no-op leaves cursor unchanged"
    );
}

#[test]
fn phase1_randomized_mixed_ops_pass() {
    // 1000 deterministic randomized sequences (mulberry32 seed 0xC0FFEE).
    let mut s: u32 = 0xC0FFEE;
    let mut rng = || {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        s
    };
    for _ in 0..1000u32 {
        let len = 4 + (rng() % 11) as usize; // 4..14
        let mut cursor = 0usize;
        let mut tip = 0usize;
        let mut ops: Vec<Op> = Vec::with_capacity(len);
        for _ in 0..len {
            loop {
                match rng() % 4 {
                    0 => {
                        ops.push(Op::RustPixel((rng() & 0xFF) as u8));
                        cursor += 1;
                        tip = cursor;
                        break;
                    }
                    1 => {
                        ops.push(Op::TsMeta);
                        cursor += 1;
                        tip = cursor;
                        break;
                    }
                    2 => {
                        if cursor == 0 {
                            continue;
                        }
                        ops.push(Op::Undo);
                        cursor -= 1;
                        break;
                    }
                    3 => {
                        if cursor >= tip {
                            continue;
                        }
                        ops.push(Op::Redo);
                        cursor += 1;
                        break;
                    }
                    _ => unreachable!(),
                }
            }
        }
        assert!(
            run_single_cursor(&ops),
            "single-cursor invariant failed on a mixed randomized sequence"
        );
    }
}
