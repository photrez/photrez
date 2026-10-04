// SPDX-License-Identifier: AGPL-3.0-or-later
//! Golden file for the `.ptz` `document.json` writer.
//!
//! The TypeScript test suite needs to stand in for the Rust writer so it can
//! assert against a written document (`ptzWriterContract.ts`). That stand-in is
//! a second implementation, so on its own it would only prove itself. This
//! module closes that gap from the other direction: it writes the REAL bytes
//! `PtzDocument::to_json` produces, and the TS suite diffs its projection
//! against them.
//!
//! NOT PART OF THE NORMAL GATES. The generator is `#[ignore]`d, so
//! `cargo test -p photrez-core` never rewrites the golden and never fails on it.
//! Regenerate deliberately, and only when the writer's output is INTENTIONALLY
//! changing:
//!
//! ```text
//! cargo test -p photrez-core --lib -- --ignored ptz_writer_golden
//! ```
//!
//! The resulting diff is the record of what changed in the on-disk format. If
//! the diff is not something you meant, the writer is wrong.
//!
//! THE GOLDEN MUST BE COMMITTED. It is not gitignored, and
//! `ptz_writer_golden_is_up_to_date` — which runs in the normal gates — fails
//! with "golden file missing" if it is absent, so a fresh clone that never
//! stages it fails the build. Regenerate it and `git add` it together with the
//! writer change.
//!
//! The golden holds each case as `input` (the fixture the Rust writer was
//! given) plus `expected` (the bytes it wrote), so the TS side can feed the same
//! input to its projection and compare without duplicating the fixtures.

use crate::ptz_document::PtzDocument;

/// Where the golden is written, relative to the `crates/core` manifest dir.
const GOLDEN_REL_PATH: &str =
    "../../apps/desktop/src/components/editor/__tests__/ptzWriterGolden.json";

/// Build the golden payload: one entry per fixture.
fn golden_json() -> String {
    use crate::ptz_fixtures::{
        LEGACY_TEXT_LAYER_MODEL, MIXED_LAYER_TYPES_MODEL, REAL_DUMPED_MODEL,
    };

    let cases: [(&str, &str); 3] = [
        ("realDumpedModel", REAL_DUMPED_MODEL),
        ("mixedLayerTypes", MIXED_LAYER_TYPES_MODEL),
        ("legacyTextLayer", LEGACY_TEXT_LAYER_MODEL),
    ];

    let mut out = serde_json::Map::new();
    for (name, input) in cases {
        let expected = PtzDocument::from_json(input)
            .unwrap_or_else(|e| panic!("fixture {} must parse: {}", name, e))
            .to_json()
            .unwrap_or_else(|e| panic!("fixture {} must serialize: {}", name, e));
        out.insert(
            name.to_string(),
            serde_json::json!({ "input": input, "expected": expected }),
        );
    }
    serde_json::to_string_pretty(&serde_json::Value::Object(out)).expect("golden serializes")
}

#[test]
#[ignore = "regenerates a tracked golden file; run deliberately"]
fn ptz_writer_golden_regenerate() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_REL_PATH);
    std::fs::write(&path, golden_json()).expect("golden file is writable");
    eprintln!("wrote {}", path.display());
}

/// Guards that the tracked golden is current WITHOUT rewriting it: the normal
/// gates run this, so a writer change that was not regenerated fails here
/// rather than silently invalidating the TypeScript projection.
#[test]
fn ptz_writer_golden_is_up_to_date() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_REL_PATH);
    let on_disk = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "golden file missing at {} ({}). Regenerate with: cargo test -p photrez-core --lib -- --ignored ptz_writer_golden",
            path.display(),
            e
        )
    });
    assert_eq!(
        on_disk.trim(),
        golden_json().trim(),
        "the .ptz writer output changed but the golden was not regenerated. \
         Review the diff: it IS the on-disk format change. Then run \
         `cargo test -p photrez-core --lib -- --ignored ptz_writer_golden` \
         and update the TypeScript projection in ptzWriterContract.ts to match."
    );
}
