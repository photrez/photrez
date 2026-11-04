//! The scored run tested on fixture bundles: the band ceiling rejects a painted
//! control the percentage floor lets through.

use super::{
    bands, file_name, mask_candidates, mask_side, perceptual, png_size, read_json, report_batch,
    report_line, score_bundle, score_root, write_json, RecordedRule, ScoreOpts, Side,
};
use super::{Path, PathBuf, Rect, RgbaImage, DEFAULT_THRESHOLD};
use serde_json::{json, Value};

const DARK: [u8; 3] = [30, 30, 30];
const MAGENTA: [u8; 3] = [255, 0, 255];

/// A fixture image, so a scored run can be tested without a capture.
fn image(width: u32, height: u32, color_at: impl Fn(u32, u32) -> [u8; 3]) -> RgbaImage {
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let [r, g, b] = color_at(x, y);
            data.extend_from_slice(&[r, g, b, 255]);
        }
    }
    RgbaImage {
        width,
        height,
        data,
    }
}

fn flat(width: u32, height: u32) -> RgbaImage {
    image(width, height, |_, _| DARK)
}

/// The painted input field the percentage floor was blind to: 200x30 px of a 400x200
/// mask, which is 7.5 % of the surface and 25 % of the band it sits in.
fn painted_control() -> RgbaImage {
    image(400, 200, |x, y| {
        if (100..300).contains(&x) && (80..110).contains(&y) {
            MAGENTA
        } else {
            DARK
        }
    })
}

/// A directory under the workspace target dir, where a run keeps its artifact for the
/// reader without touching a baseline or a recorded bundle.
fn artifact_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/xtask-parity-score")
        .join(name);
    std::fs::create_dir_all(&dir).expect("artifact directory");
    dir
}

/// Write a fixture bundle whose `diff.json` records what the comparator really measures
/// on the pair, so the scored run has a true record to check itself against.
fn write_bundle(
    dir: &Path,
    surface: &str,
    oracle: &RgbaImage,
    native: &RgbaImage,
    thresholds: Value,
) {
    let measured = perceptual::compare(oracle, native, DEFAULT_THRESHOLD, true).expect("measure");
    let record = json!({
        "surface": surface,
        "pass": true,
        "thresholds": thresholds,
        "metrics": {
            "changed_pixels": measured.changed_pixels,
            "total_pixels": measured.total_pixels,
            "changed_pct": measured.changed_pct,
            "ignored_aa_pixels": measured.ignored_aa_pixels,
            "threshold": DEFAULT_THRESHOLD,
        },
        "inputs": { "oracle": "fnv64:oracle", "native": "fnv64:native" },
        "red": { "capture": "fixture-native-mask-red.png", "changed_pct": 66.0 },
        "states": [{ "id": "resting", "pass": true }],
    });
    std::fs::create_dir_all(dir).expect("fixture directory");
    oracle
        .save(&dir.join("fixture-oracle-mask.png"))
        .expect("write oracle mask");
    native
        .save(&dir.join("fixture-native-mask.png"))
        .expect("write native mask");
    std::fs::write(
        dir.join("diff.json"),
        serde_json::to_string_pretty(&record).expect("serialize fixture record"),
    )
    .expect("write fixture record");
}

/// The retained RED artifact of the adopted rule: a control painted the wrong colour
/// clears the ratified percentage floor and is refused by the band ceiling, and the
/// scored document names the band that tripped.
#[test]
fn a_painted_control_fails_the_band_while_the_percentage_passes() {
    let dir = artifact_dir("band-red-painted-control");
    // 6267 px is the ratified 7.834 % of this 80000 px mask; the control is 6000 px.
    write_bundle(
        &dir,
        "FIXTURE-RED",
        &flat(400, 200),
        &painted_control(),
        json!({ "max_changed_pixels": 6267, "mask": "400x200 fixture" }),
    );
    let scored = score_bundle(&dir, &ScoreOpts::default()).expect("scored fixture");
    assert_eq!(scored.rule, RecordedRule::Perceptual);
    assert!(
        scored.verdict.percent_pass,
        "the floor alone passes it: {} %",
        scored.verdict.changed_pct
    );
    assert!(
        !scored.verdict.band_pass,
        "the band the control sits in does not"
    );
    assert!(!scored.verdict.pass, "and a band fail is a surface fail");
    let worst = scored.verdict.worst.expect("a band trips the ceiling");
    assert_eq!(
        (worst.y0, worst.y1, worst.residual_pct),
        (60, 120, 25.0),
        "the reader can see which band tripped"
    );

    let out = dir.join("scored-diff.json");
    write_json(out.clone(), &scored.document).expect("write the RED artifact");
    println!("retained RED artifact: {}", out.display());

    let recorded = read_json(&dir.join("diff.json")).expect("recorded fixture");
    let document = &scored.document;
    for key in [
        "surface",
        "thresholds",
        "metrics",
        "inputs",
        "red",
        "states",
        "bands",
    ] {
        assert!(document.get(key).is_some(), "`{key}` is missing");
    }
    assert_eq!(
        document["metrics"], recorded["metrics"],
        "the gate number is not restated as a band number"
    );
    assert_eq!(document["inputs"], recorded["inputs"]);
    assert_eq!(document["red"], recorded["red"]);
    assert_eq!(document["states"], recorded["states"]);
    assert_eq!(
        document["pass"],
        json!(false),
        "the verdict is the two assertions"
    );
    assert_eq!(
        document["thresholds"]["max_changed_pixels"],
        recorded["thresholds"]["max_changed_pixels"]
    );
    let block = &document["bands"];
    assert_eq!(block["ceiling_pct"], json!(bands::RATIFIED_MAX_BAND_PCT));
    assert!(block["ceiling_source"]
        .as_str()
        .expect("source")
        .starts_with("the ratified ceiling"));
    assert_eq!(block["pass"], json!(false));
    assert_eq!(
        block["horizontal_dx"],
        json!(0),
        "this pair is not displaced"
    );
    assert_eq!(block["worst_band_pct"], json!(25.0));
    assert_eq!(block["worst_changed_band_pct"], json!(25.0));
    assert_eq!(block["report"]["bands"].as_array().expect("bands").len(), 4);
    assert!(block["rule"]
        .as_str()
        .expect("rule")
        .contains("band identity ceiling"));
}

/// Both ceilings are stated numbers: the bundle's own line first, then the caller's,
/// then the ratified value. The same fixture is a pass at 25 % and a fail at 5 %.
#[test]
fn a_ceiling_is_a_stated_number_not_a_value_buried_in_the_comparison() {
    let dir = artifact_dir("band-stated-ceilings");
    write_bundle(
        &dir,
        "FIXTURE-BUNDLE-CEILING",
        &flat(400, 200),
        &painted_control(),
        json!({ "max_changed_pixels": 6267, "max_band_pct": 25.0, "mask": "400x200 fixture" }),
    );
    let scored = score_bundle(&dir, &ScoreOpts::default()).expect("bundle ceiling");
    assert_eq!(scored.ceilings.max_band_pct, 25.0, "the bundle spoke");
    assert_eq!(
        scored.ceilings.max_changed_pct, 7.834,
        "from its own pixel allowance"
    );
    assert!(scored.verdict.pass, "25 % of the band is what it granted");
    assert_eq!(
        scored.document["bands"]["ceiling_source"],
        json!("stated by the bundle")
    );
    assert_eq!(
        scored.document["thresholds"]["max_band_pct"],
        json!(25.0),
        "and the ceiling applied is recorded"
    );

    let tighter = ScoreOpts {
        max_band_pct: Some(5.0),
        ..Default::default()
    };
    let overridden = score_bundle(&dir, &tighter).expect("caller ceiling");
    assert_eq!(
        overridden.ceilings.max_band_pct, 5.0,
        "the caller outranks the bundle"
    );
    assert!(!overridden.verdict.band_pass);
    assert_eq!(
        overridden.document["bands"]["ceiling_source"],
        json!("stated by the caller")
    );

    let unspoken = artifact_dir("band-ratified-ceiling");
    write_bundle(
        &unspoken,
        "FIXTURE-RATIFIED",
        &flat(400, 200),
        &painted_control(),
        json!({ "max_changed_pixels": 6267, "mask": "400x200 fixture" }),
    );
    let ratified = score_bundle(&unspoken, &ScoreOpts::default()).expect("ratified ceiling");
    assert_eq!(
        ratified.ceilings.max_band_pct,
        bands::RATIFIED_MAX_BAND_PCT,
        "with nothing stated, the adopted ceiling applies"
    );
}

/// A pair that does not reproduce the recorded pixels is refused rather than scored.
/// This is what keeps a red control, or a pane cropped at another size, from being read
/// as the surface its record describes.
#[test]
fn a_pair_that_does_not_reproduce_the_record_is_refused_and_the_recorded_pair_is_found() {
    let dir = artifact_dir("band-record-refused");
    let oracle = flat(400, 200);
    let native = painted_control();
    write_bundle(
        &dir,
        "FIXTURE-REFUSED",
        &oracle,
        &native,
        json!({ "max_changed_pixels": 6267, "mask": "400x200 fixture" }),
    );
    // The control is on disk and is the same size as the recorded mask.
    let damaged = image(400, 200, |x, y| {
        if (100..300).contains(&x) && (20..180).contains(&y) {
            MAGENTA
        } else {
            DARK
        }
    });
    damaged
        .save(&dir.join("fixture-native-mask-red.png"))
        .expect("write control");
    let scored = score_bundle(&dir, &ScoreOpts::default()).expect("recorded pair");
    assert_eq!(
        file_name(&scored.pair.native),
        "fixture-native-mask.png",
        "the control is not the recorded pair"
    );
    assert_eq!(scored.report.horizontal.best_dx, 0);

    // Now move the record off the pair by one pixel and refuse the whole bundle.
    let mut record = read_json(&dir.join("diff.json")).expect("fixture record");
    let changed = record["metrics"]["changed_pixels"]
        .as_u64()
        .expect("changed");
    record["metrics"]["changed_pixels"] = json!(changed + 1);
    std::fs::write(dir.join("diff.json"), record.to_string()).expect("rewrite record");
    let err = score_bundle(&dir, &ScoreOpts::default()).expect_err("record no longer matches");
    assert!(err.contains("reproduces"), "names the mismatch: {err}");
    assert!(
        err.contains("AA excluded"),
        "and which comparator checked it: {err}"
    );
}

/// A bundle that stores no mask at the recorded size is scored from its own full
/// captures, cropped at the geometry its note records, and only once that crop
/// reproduces the recorded pixels.
#[test]
fn a_missing_mask_is_rebuilt_from_the_recorded_geometry() {
    let dir = artifact_dir("band-rebuilt-mask");
    let crop = Rect {
        x: 10,
        y: 20,
        width: 40,
        height: 60,
    };
    let oracle = flat(100, 100);
    let native = image(100, 100, |x, y| {
        if (20..40).contains(&x) && (30..50).contains(&y) {
            MAGENTA
        } else {
            DARK
        }
    });
    let measured = perceptual::compare(
        &oracle.crop(crop).expect("oracle crop"),
        &native.crop(crop).expect("native crop"),
        DEFAULT_THRESHOLD,
        true,
    )
    .expect("measure the crop");
    std::fs::create_dir_all(&dir).expect("fixture directory");
    oracle
        .save(&dir.join("S008-oracle-full.png"))
        .expect("write");
    native
        .save(&dir.join("S008-native-full.png"))
        .expect("write");
    let record = json!({
        "surface": "S008",
        "pass": true,
        "thresholds": {
            "max_changed_pixels": 600,
            "mask": "menu rect x=10 y=20 w=40 h=60 physical; no mask stored",
        },
        "metrics": {
            "changed_pixels": measured.changed_pixels,
            "total_pixels": 2400,
            "ignored_aa_pixels": measured.ignored_aa_pixels,
            "threshold": DEFAULT_THRESHOLD,
        },
    });
    std::fs::write(dir.join("diff.json"), record.to_string()).expect("write record");

    let scored = score_bundle(&dir, &ScoreOpts::default()).expect("rebuilt mask");
    assert_eq!(scored.size, (40, 60), "the crop is what was measured");
    assert_eq!(scored.pair.crop, Some(crop));
    assert!(
        scored.pair.note.contains("rebuilt"),
        "and the note says so: {}",
        scored.pair.note
    );
    assert_eq!(scored.verdict.changed_pct, measured.changed_pct);
    assert_eq!(
        scored.report.bands.len(),
        1,
        "one 60 px band covers the mask"
    );
}

/// Names decide which candidate is tried first and which side of the pair it belongs to.
/// They never decide a verdict, which is what the recorded pixels do.
#[test]
fn bundle_names_decide_side_and_order_only() {
    assert_eq!(mask_side("S003-oracle-mask.png"), Some(Side::Oracle));
    assert_eq!(mask_side("S003-n-geometry.png"), Some(Side::Native));
    assert_eq!(mask_side("probe-mask-native.png"), Some(Side::Native));
    assert_eq!(
        mask_side("S039-native-shared.png"),
        Some(Side::Native),
        "'shared' is not a red control"
    );
    assert_eq!(
        mask_side("sweep--1-0.png"),
        None,
        "a sweep frame names no side"
    );

    let names: Vec<String> = [
        "S059-native-textmask-red.png",
        "S059-native-textmask.png",
        "S059-native-ring.png",
    ]
    .iter()
    .map(|name| name.to_string())
    .collect();
    assert_eq!(
        mask_candidates(&names),
        vec![
            "S059-native-textmask.png",
            "S059-native-ring.png",
            "S059-native-textmask-red.png"
        ],
        "the recorded mask first, the control last"
    );
}

/// Reading the size from the header is what lets a run size its candidates without
/// decoding every capture in the bundle.
#[test]
fn png_size_reads_the_header_and_refuses_anything_else() {
    let dir = artifact_dir("band-png-size");
    flat(37, 19)
        .save(&dir.join("probe.png"))
        .expect("write probe");
    assert_eq!(png_size(&dir.join("probe.png")), Some((37, 19)));
    std::fs::write(
        dir.join("not-a-png.png"),
        b"this is not a png but it is longer than the header",
    )
    .expect("write non-png");
    assert_eq!(png_size(&dir.join("not-a-png.png")), None);
}

/// The batch names its rejects and its refusals, and its exit code follows the rejects.
#[test]
fn a_batch_run_names_the_band_ceiling_rejects() {
    let root = artifact_dir("band-batch-root");
    for (surface, painted) in [("FIXTURE-A", true), ("FIXTURE-B", false)] {
        let bundle = root.join(surface);
        let native = if painted {
            painted_control()
        } else {
            flat(400, 200)
        };
        write_bundle(
            &bundle,
            surface,
            &flat(400, 200),
            &native,
            json!({ "max_changed_pixels": 6267, "mask": "400x200 fixture" }),
        );
    }
    // A directory with no recorded pixels is refused, not passed.
    let broken = root.join("FIXTURE-C");
    std::fs::create_dir_all(&broken).expect("broken bundle");
    std::fs::write(
        broken.join("diff.json"),
        json!({ "metrics": {} }).to_string(),
    )
    .expect("write empty record");

    let rows = score_root(&root, &ScoreOpts::default()).expect("root");
    let code = report_batch(&rows);
    assert_eq!(code, 1, "a band reject is a red batch");
    let names: Vec<&str> = rows.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        vec!["FIXTURE-A", "FIXTURE-B", "FIXTURE-C"],
        "in name order"
    );
    assert!(!rows[0].1.as_ref().expect("scored").verdict.band_pass);
    assert!(rows[1].1.as_ref().expect("scored").verdict.pass);
    assert!(
        rows[2].1.is_err(),
        "a record with no pixels cannot be scored"
    );
}

/// A class-map bundle keeps its own gate number: the floor is applied to the mismatch
/// count it recorded, the pair is verified on the raw count, and the bands stay a
/// perceptual measure. Those are three different numbers, so the scored document labels
/// which is which.
#[test]
fn a_class_map_bundle_is_scored_on_its_recorded_gate_number() {
    let dir = artifact_dir("band-class-map");
    std::fs::create_dir_all(&dir).expect("bundle directory");
    let oracle = flat(100, 60);
    let native = image(100, 60, |x, _| if x < 10 { MAGENTA } else { DARK });
    oracle
        .save(&dir.join("fixture-oracle-mask.png"))
        .expect("write oracle mask");
    native
        .save(&dir.join("fixture-native-mask.png"))
        .expect("write native mask");
    let record = json!({
        "surface": "S052",
        "pass": true,
        "thresholds": { "max_changed_pixels": 600, "mask": "100x60 fixture" },
        "metrics": {
            "changed_pixels": 500,
            "total_pixels": 6000,
            "changed_pct": 8.333,
            "raw_changed_pixels": 600,
        },
    });
    std::fs::write(dir.join("diff.json"), record.to_string()).expect("write record");

    let scored = score_bundle(&dir, &ScoreOpts::default()).expect("class-map bundle");
    assert_eq!(scored.rule, RecordedRule::ClassMap);
    assert_eq!(
        scored.verdict.changed_pct, 8.333,
        "the recorded mismatch count, not the 600 raw px and not a perceptual measure"
    );
    assert!(
        scored.verdict.percent_pass,
        "8.333 % of the 10 % it granted"
    );
    assert!(scored.verdict.band_pass);
    assert_eq!(
        scored.document["bands"]["gate_rule"],
        json!("class-map gate, perceptual bands")
    );
    assert_eq!(
        scored.document["metrics"]["changed_pixels"],
        json!(500),
        "the legacy metric stays as recorded"
    );
    assert_eq!(scored.report.bands.len(), 1);
}

/// The census agreement. A census over the stored bundles predicted that a 20 %
/// worst-band ceiling rejects exactly S009, S043 and S059 after alignment, and that
/// 12 % rejects eight. This re-measures every bundle with the instrument, so it is slow
/// and runs only under `cargo test -p xtask -- --ignored --nocapture`. It is skipped
/// when the evidence tree is not on this machine.
#[test]
#[ignore]
fn the_census_reject_list_is_reproduced_on_the_stored_bundles() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.slim/deepwork/78-evidence");
    if !root.is_dir() {
        println!("skipped: no evidence tree at {}", root.display());
        return;
    }
    let opts = ScoreOpts::default();
    let rows = score_root(&root, &opts).expect("the evidence root is readable");
    let mut residues: Vec<(String, f64)> = Vec::new();
    let mut refused: Vec<String> = Vec::new();
    for (name, row) in &rows {
        match row {
            Ok(scored) => {
                let residue = scored
                    .report
                    .worst_band()
                    .map_or(0.0, |band| band.residual_pct);
                println!("{}", report_line(scored));
                residues.push((name.clone(), residue));
            }
            Err(reason) => {
                println!("{:<8} refused: {reason}", name);
                refused.push(name.clone());
            }
        }
    }
    // The residue does not depend on the ceiling, so one pass answers both questions.
    let rejects = |ceiling: f64| -> Vec<String> {
        residues
            .iter()
            .filter(|(_, residue)| *residue > ceiling)
            .map(|(name, _)| name.clone())
            .collect()
    };
    println!("measured {}, refused {:?}", residues.len(), refused);
    assert_eq!(
        refused,
        vec!["S053", "S055", "S056"],
        "the census refused these three"
    );
    assert_eq!(rejects(20.0), ["S009", "S043", "S059"]);
    assert_eq!(rejects(12.0).len(), 8, "{:?}", rejects(12.0));
}

/// A bundle is hand-edited evidence, so its `thresholds` block can be any JSON value.
/// The scored run has nowhere to write the ceiling it applied in that case, and it says
/// so instead of panicking inside the gate.
#[test]
fn a_bundle_that_records_thresholds_as_a_number_is_refused() {
    let dir = artifact_dir("band-thresholds-not-an-object");
    write_bundle(&dir, "S900", &flat(100, 60), &flat(100, 60), json!(5));
    let error = score_bundle(&dir, &ScoreOpts::default())
        .expect_err("the applied ceiling has nowhere to go");
    assert!(
        error.contains("thresholds is 5, not an object"),
        "names the recorded value, got {error}"
    );
}
