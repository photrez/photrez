// SPDX-License-Identifier: AGPL-3.0-or-later
//! Single-sample companion for `apps/desktop/scripts/benchPixelPaired.ts`.
//!
//! The driver times one `readRect` in-process, then spawns THIS binary to time
//! the `write_region` half of the same run, so both halves of one run land in
//! a single paired JSONL record. Selection env vars: `PHOTREZ_BENCH_PAIR_N`
//! (square layer size) and `PHOTREZ_BENCH_PAIR_RUN` (run index echoed back for
//! pairing). With `PHOTREZ_BENCH_PAIR_N` unset the test takes no sample, so a
//! plain `cargo test` run costs nothing beyond one early return.
//!
//! Excluded from the timer: payload clone, IPC transport and serde response
//! serialization (they live outside this crate).
//!
//! Falsifiability: `PHOTREZ_BENCH_PAIR_STUB=1` skips the real store call and
//! returns a constant, which must trip the driver's "all timings identical"
//! guard; a normal run leaves the variable unset.

use photrez_core::pixel_store::PixelStoreRegistry;
use std::time::Instant;

#[test]
fn paired_single_write_region_sample() {
    let Ok(size_raw) = std::env::var("PHOTREZ_BENCH_PAIR_N") else {
        eprintln!("pair sample: PHOTREZ_BENCH_PAIR_N unset, no sample taken");
        return;
    };
    let n: usize = size_raw
        .parse()
        .expect("PHOTREZ_BENCH_PAIR_N must be an unsigned integer");
    assert!(n > 0, "PHOTREZ_BENCH_PAIR_N must be positive");
    let run: u64 = std::env::var("PHOTREZ_BENCH_PAIR_RUN")
        .map(|raw| {
            raw.parse::<u64>()
                .expect("PHOTREZ_BENCH_PAIR_RUN must be an unsigned integer")
        })
        .unwrap_or(0);
    let stub = std::env::var("PHOTREZ_BENCH_PAIR_STUB").is_ok_and(|v| v == "1");

    let layer_id = format!("pair-{n}");
    let mut registry = PixelStoreRegistry::new();
    registry.open_document("pair-doc");
    registry
        .add_layer(
            "pair-doc",
            &layer_id,
            n as u32,
            n as u32,
            vec![0u8; n * n * 4],
        )
        .expect("pair sample layer must seed");
    let rgba = vec![0x40u8; n * n * 4];

    let ms = if stub {
        0.5
    } else {
        // Payload clone is untimed: production hands write_region an owned
        // buffer the IPC layer already deserialized.
        let payload = rgba.clone();
        let started = Instant::now();
        let result = registry.write_region("pair-doc", &layer_id, 0, 0, n, n, payload);
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        assert!(result.is_some(), "write_region must land for size {n}");
        let epoch = registry.get_epoch("pair-doc", &layer_id).unwrap_or(0);
        assert_eq!(epoch, 1, "one real commit must bump the epoch for size {n}");
        elapsed
    };

    println!("PAIR_SAMPLE {{\"size\":{n},\"run\":{run},\"write_region_ms\":{ms:.6}}}");
}
