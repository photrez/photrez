// SPDX-License-Identifier: AGPL-3.0-or-later
//! Baseline wall time of one full-layer canonical region commit
//! (`PixelStoreRegistry::write_region`) at 512/2048/4096 px square layers,
//! 5 timed runs per size, no warmup (the cold first run shows up in min/max).
//! Measurement harness only: it asserts commit validity and timer variance,
//! never a wall-time budget - budgets are derived from these numbers and
//! enforced by later tasks.
//!
//! Run for the recorded baseline (release profile, matches production):
//!   cargo test --release -p photrez-core --test pixel_store_commit_bench -- --nocapture
//!
//! Excluded from the measurement: IPC transport and serde response
//! serialization (they live in the desktop command layer, outside this crate).
//!
//! Falsifiability: `PHOTREZ_BENCH_STUB=1` skips the real store call so the
//! commits-landed and timer-variance pins must fail. Normal runs leave the
//! variable unset.

use photrez_core::pixel_store::PixelStoreRegistry;
use std::time::Instant;

const SIZES: [usize; 3] = [512, 2048, 4096];
const RUNS: usize = 5;
const STUB_ENV: &str = "PHOTREZ_BENCH_STUB";

fn percentile(sorted: &[f64], p: f64) -> f64 {
    let idx = ((p * sorted.len() as f64).ceil() as usize).saturating_sub(1);
    sorted[idx.min(sorted.len() - 1)]
}

#[test]
fn write_region_full_layer_wall_time_baseline() {
    let stub = std::env::var(STUB_ENV).is_ok_and(|v| v == "1");
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let mut failures: Vec<String> = Vec::new();

    for &n in &SIZES {
        let layer_id = format!("bench-{n}");
        let mut reg = PixelStoreRegistry::new();
        reg.open_document("bench-doc");
        reg.add_layer(
            "bench-doc",
            &layer_id,
            n as u32,
            n as u32,
            vec![0u8; n * n * 4],
        )
        .expect("bench layer must seed");
        let rgba = vec![0x40u8; n * n * 4];

        let mut samples: Vec<f64> = Vec::with_capacity(RUNS);
        let mut landed = 0usize;
        for _run in 0..RUNS {
            if stub {
                // Constant fake sample: the timer-variance pin must catch this.
                samples.push(0.5);
                continue;
            }
            // Payload clone is untimed: production hands write_region an owned
            // buffer that the IPC layer already deserialized.
            let payload = rgba.clone();
            let t0 = Instant::now();
            let res = reg.write_region("bench-doc", &layer_id, 0, 0, n, n, payload);
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            if res.is_some() {
                landed += 1;
            }
            samples.push(ms);
        }

        // Real commits bump the store epoch once per run; a stub run that
        // skipped write_region leaves the engine untouched and fails here.
        let epoch = reg.get_epoch("bench-doc", &layer_id).unwrap_or(0);
        if landed != RUNS || epoch != RUNS as u64 {
            failures.push(format!(
                "n={n}: timed commits landed={landed}, epoch={epoch}, expected {RUNS} each - real engine not exercised"
            ));
        }

        let mut sorted = samples.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut deduped = sorted.clone();
        deduped.dedup_by(|a, b| a == b);
        if deduped.len() < 2 {
            failures.push(format!(
                "n={n}: {} distinct timing sample(s) out of {RUNS} - timer looks stubbed",
                deduped.len()
            ));
        }

        eprintln!(
            "WRITE_REGION profile={profile} n={n} runs={RUNS} median_ms={:.3} p95_ms={:.3} min_ms={:.3} max_ms={:.3}",
            percentile(&sorted, 0.5),
            percentile(&sorted, 0.95),
            sorted[0],
            sorted[sorted.len() - 1],
        );
    }

    assert!(
        failures.is_empty(),
        "bench validity failed:\n{}",
        failures.join("\n")
    );
}
