//! Reader for the golden-master `manifest.json` (the shot index beside the
//! baseline PNGs) and for an evidence bundle's recorded `diff.json`. Only the
//! fields the parity runs need are typed; everything else stays unread.

use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use super::round3;

#[derive(Clone, Debug, Deserialize)]
pub struct Manifest {
    /// The locale the baselines were captured under; the frozen reference's
    /// manifest records `"en"`. Absent in manifests that predate the field.
    #[serde(default)]
    pub locale: Option<String>,
    /// The nondeterministic footer strip that `--mask-metrics` blanks. Absent in
    /// manifests that predate the field, which fall back to the built-in mask
    /// measured on the 1366x820 web baselines - and only at that size. The key
    /// is renamed explicitly because this struct, unlike `Shot`, carries no
    /// container-level `rename_all`.
    #[serde(default, rename = "metricsMask")]
    pub metrics_mask: Option<MetricsMask>,
    /// The commit the reference images were captured from. Absent in a manifest
    /// that predates the field, which the run then reports as unrecorded rather
    /// than staying silent, because a FAIL against an unknown baseline revision
    /// reads like a defect against the current one.
    #[serde(default, rename = "sourceCommit")]
    pub source_commit: Option<String>,
    /// When the reference images were captured, as recorded by the capture.
    #[serde(default, rename = "capturedAt")]
    pub captured_at: Option<String>,
    pub shots: Vec<Shot>,
}

/// A mask rect in viewport coordinates, plus the viewport size it was measured
/// on. The size travels with the rect because a mask is not portable: blanking
/// `y 801..=816` on a 1020px-tall capture hides a different part of the UI than
/// it did on the 820px-tall one it was measured on, and the diff still reads
/// clean. Declaring the calibration size turns that silent wrong answer into a
/// refusal.
#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsMask {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
    pub viewport_width: u32,
    pub viewport_height: u32,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // mirrors the published manifest schema; the pipeline reads id, file, box and camera
pub struct Shot {
    pub id: String,
    pub file: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub note: String,
    /// CSS selector the original capture used; null or missing for shots
    /// without one.
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub needs_doc: bool,
    #[serde(rename = "box")]
    pub bounds: ShotBox,
    #[serde(default)]
    pub reason: Option<String>,
    /// The camera the reference frame was taken at, when it differs from the
    /// live fit. Parity replays it so a zoomed or panned baseline is scored
    /// against the same page placement rather than against a scaled artifact.
    #[serde(default)]
    pub camera: Option<ShotCamera>,
}

/// A per-shot capture camera: a zoom percent and a page origin in element-local
/// screen pixels (`"41,73"`), in the same text form `--zoom` and `--pan` take.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShotCamera {
    #[serde(default)]
    pub zoom: Option<String>,
    #[serde(default)]
    pub pan: Option<String>,
}

/// The element box the shot was cropped from, in viewport coordinates.
/// Fractional by design (layout widths like 89.859) - consumers anchor at the
/// origin and take size from the PNG.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct ShotBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Load and parse the manifest.
pub fn load(path: &Path) -> Result<Manifest, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))
}

/// Fail fast when a manifest records a non-English capture locale: every
/// baseline in the golden master was taken with the English catalog, so a
/// diff under another locale is meaningless rather than merely different.
/// A manifest without the field predates it and is accepted.
pub fn require_english_locale(manifest: &Manifest) -> Result<(), String> {
    match manifest.locale.as_deref() {
        None => Ok(()),
        Some(tag) if tag.trim().is_empty() || tag.trim().eq_ignore_ascii_case("en") => Ok(()),
        Some(tag) => Err(format!(
            "manifest records locale `{tag}`; parity diffs are only valid against the English baselines"
        )),
    }
}

/// Shots whose id matches one of `patterns` (exact id, or prefix match -
/// `menu-` selects every menu), in manifest order. Unknown patterns are an
/// error: a typo must not silently compare nothing.
pub fn select<'a>(manifest: &'a Manifest, patterns: &[String]) -> Result<Vec<&'a Shot>, String> {
    let mut picked: Vec<&Shot> = Vec::new();
    for pattern in patterns {
        let mut matched = false;
        for shot in &manifest.shots {
            if shot.id == *pattern || shot.id.starts_with(pattern.as_str()) {
                matched = true;
                if !picked.iter().any(|s| s.id == shot.id) {
                    picked.push(shot);
                }
            }
        }
        if !matched {
            return Err(format!("no shot matches `{pattern}`"));
        }
    }
    if picked.is_empty() {
        return Err("no shots selected".into());
    }
    Ok(picked)
}

/// The pixel counts a bundle's `diff.json` recorded, with the comparator behind them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RecordedMetrics {
    /// The ratified perceptual rule: differing pixels, anti-aliasing set aside.
    Perceptual { changed: u64, ignored_aa: u64 },
    /// The class-map gate: pixels that differ at all. Its bands are still measured with
    /// the perceptual rule, so a scored bundle labels which number is whose.
    ClassMap { raw: u64 },
}

/// One evidence bundle's recorded verdict, as far as a re-score needs it.
#[derive(Clone, Debug, PartialEq)]
pub struct BundleRecord {
    pub surface: String,
    pub total_pixels: u64,
    pub recorded: RecordedMetrics,
    /// The pixels the recorded `changed_pct` was computed from. For a class-map bundle
    /// this is its own mismatch count, which is neither the raw count nor a perceptual
    /// measure, so the percentage assertion has to read it rather than re-derive it.
    pub gate_pixels: u64,
    /// The percentage allowance the bundle granted itself, as a share of its own mask.
    pub max_changed_pct: f64,
    /// The band ceiling the bundle states, when it states one.
    pub max_band_pct: Option<f64>,
}

/// Read the record. A document with no pixel counts is refused rather than scored,
/// because then nothing constrains which pair a re-measure may use.
pub fn bundle_record(diff: &Value) -> Result<BundleRecord, String> {
    let metrics = diff
        .get("metrics")
        .and_then(Value::as_object)
        .ok_or("diff.json `metrics` is missing or not an object")?;
    let number = |key: &str| metrics.get(key).and_then(Value::as_u64);
    let total_pixels = number("total_pixels").ok_or("diff.json `metrics.total_pixels` missing")?;
    let recorded =
        match (
            number("changed_pixels"),
            number("ignored_aa_pixels"),
            number("raw_changed_pixels"),
        ) {
            (Some(changed), Some(ignored_aa), _) => RecordedMetrics::Perceptual {
                changed,
                ignored_aa,
            },
            (_, None, Some(raw)) => RecordedMetrics::ClassMap { raw },
            _ => return Err(
                "diff.json `metrics` records neither changed_pixels with ignored_aa_pixels nor \
                 raw_changed_pixels, so no measurement can be checked against it"
                    .to_string(),
            ),
        };
    let gate_pixels = match recorded {
        RecordedMetrics::Perceptual { changed, .. } => changed,
        RecordedMetrics::ClassMap { raw } => number("changed_pixels").unwrap_or(raw),
    };
    Ok(BundleRecord {
        surface: diff
            .get("surface")
            .and_then(Value::as_str)
            .unwrap_or("(unnamed)")
            .to_string(),
        total_pixels,
        recorded,
        gate_pixels,
        max_changed_pct: percentage_allowance(diff, total_pixels),
        max_band_pct: stated_band_ceiling(diff),
    })
}

/// The percentage ceiling the bundle recorded for itself. `max_changed_pixels` is where
/// the ratified 12 % glyph allowance lives, so the floor travels with the bundle and the
/// ratified number is only the fallback for a bundle that states none.
fn percentage_allowance(diff: &Value, total_pixels: u64) -> f64 {
    let pixels = diff
        .get("thresholds")
        .and_then(|t| t.get("max_changed_pixels"))
        .and_then(Value::as_u64);
    match pixels {
        Some(pixels) if total_pixels > 0 => round3(pixels as f64 / total_pixels as f64 * 100.0),
        _ => super::RATIFIED_MAX_CHANGED_PCT,
    }
}

/// The band ceiling the bundle states, if any.
pub fn stated_band_ceiling(diff: &Value) -> Option<f64> {
    diff.get("thresholds")
        .and_then(|t| t.get("max_band_pct"))
        .and_then(Value::as_f64)
}

/// The `x=.. y=.. w=.. h=..` regions a `thresholds.mask` note declares, in the order the
/// note gives them. A note may name several regions, so the caller matches one against
/// the recorded pixel count instead of taking the first.
pub fn mask_regions(diff: &Value) -> Vec<(u32, u32, u32, u32)> {
    let Some(note) = diff
        .get("thresholds")
        .and_then(|t| t.get("mask"))
        .and_then(Value::as_str)
    else {
        return Vec::new();
    };
    let bytes = note.as_bytes();
    let mut regions = Vec::new();
    let mut at = 0;
    while at + 1 < bytes.len() {
        if bytes[at] == b'x' && bytes[at + 1] == b'=' {
            if let Some((region, next)) = read_region(bytes, at + 2) {
                regions.push(region);
                at = next;
                continue;
            }
        }
        at += 1;
    }
    regions
}

/// Digits from `at`, and where the run ends. An empty run is not a number.
fn digits(bytes: &[u8], at: usize) -> Option<(u32, usize)> {
    let mut end = at;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end == at {
        return None;
    }
    let value = bytes[at..end].iter().try_fold(0u32, |n, d| {
        n.checked_mul(10)?.checked_add(u32::from(d - b'0'))
    })?;
    Some((value, end))
}

/// `x=.. y=.. w=.. h=..` from `at` (past the `x=`), and where the run ends.
fn read_region(bytes: &[u8], at: usize) -> Option<((u32, u32, u32, u32), usize)> {
    let (x, at) = digits(bytes, at)?;
    let (y, at) = keyed(bytes, at, b'y')?;
    let (w, at) = keyed(bytes, at, b'w')?;
    let (h, at) = keyed(bytes, at, b'h')?;
    Some(((x, y, w, h), at))
}

/// Optional spaces, then `key=`, then digits.
fn keyed(bytes: &[u8], at: usize, key: u8) -> Option<(u32, usize)> {
    let mut at = at;
    while at < bytes.len() && (bytes[at] == b' ' || bytes[at] == b'\t') {
        at += 1;
    }
    if at + 1 >= bytes.len() || bytes[at] != key || bytes[at + 1] != b'=' {
        return None;
    }
    digits(bytes, at + 2)
}

#[cfg(test)]
mod tests {
    use super::{bundle_record, mask_regions, require_english_locale, Manifest, RecordedMetrics};
    use serde_json::json;

    fn manifest_with(locale: Option<&str>) -> Manifest {
        Manifest {
            locale: locale.map(str::to_owned),
            metrics_mask: None,
            source_commit: None,
            captured_at: None,
            shots: Vec::new(),
        }
    }

    /// The manifest side of the locale fail-fast: English (any case) and a
    /// pre-field manifest pass; anything else is refused and named.
    #[test]
    fn english_locale_is_required() {
        assert!(require_english_locale(&manifest_with(None)).is_ok());
        assert!(require_english_locale(&manifest_with(Some(""))).is_ok());
        assert!(require_english_locale(&manifest_with(Some("en"))).is_ok());
        assert!(require_english_locale(&manifest_with(Some(" EN "))).is_ok());
        let err =
            require_english_locale(&manifest_with(Some("id"))).expect_err("id must be refused");
        assert!(err.contains("id"), "names the value: {err}");
    }

    /// `metricsMask` parses from camelCase JSON, and a manifest without it keeps
    /// the pre-field shape instead of failing to deserialize.
    #[test]
    fn metrics_mask_is_optional_and_camel_case() {
        let bare: Manifest = serde_json::from_str(r#"{"shots":[]}"#).expect("no mask parses");
        assert!(bare.metrics_mask.is_none());

        let with: Manifest = serde_json::from_str(
            r#"{"metricsMask":{"x0":10,"y0":20,"x1":30,"y1":40,
                "viewportWidth":1920,"viewportHeight":1020},"shots":[]}"#,
        )
        .expect("declared mask parses");
        let m = with.metrics_mask.expect("mask present");
        assert_eq!((m.x0, m.y0, m.x1, m.y1), (10, 20, 30, 40));
        assert_eq!((m.viewport_width, m.viewport_height), (1920, 1020));
    }

    /// A shot's camera parses, and a shot without one keeps the pre-camera shape.
    /// The field is what lets parity replay the placement a reference frame was
    /// taken at, so a wrong rename here would silently score at the live fit.
    #[test]
    fn shot_camera_is_optional() {
        let shot = r#""id":"dock-closed","file":"dock-closed.png","box":{"x":0,"y":0,"width":10,"height":10}"#;
        let bare: Manifest = serde_json::from_str(&format!(r#"{{"shots":[{{{shot}}}]}}"#))
            .expect("no camera parses");
        assert!(bare.shots[0].camera.is_none());

        let with: Manifest = serde_json::from_str(&format!(
            r#"{{"shots":[{{{shot},"camera":{{"zoom":"87.375","pan":"41,73"}}}}]}}"#
        ))
        .expect("declared camera parses");
        let c = with.shots[0].camera.as_ref().expect("camera present");
        assert_eq!(c.zoom.as_deref(), Some("87.375"));
        assert_eq!(c.pan.as_deref(), Some("41,73"));
    }

    /// The provenance fields bind to the camelCase keys the capture writes. This
    /// matters because the run prints either the recorded revision or a warning
    /// that none is recorded, so a wrong rename would print the warning about a
    /// manifest that does state its source.
    #[test]
    fn baseline_provenance_parses() {
        let bare: Manifest = serde_json::from_str(r#"{"shots":[]}"#).expect("no provenance parses");
        assert!(bare.source_commit.is_none());
        assert!(bare.captured_at.is_none());

        let with: Manifest = serde_json::from_str(
            r#"{"sourceCommit":"0c6e926dea8e","capturedAt":"2026-10-17T12:42:13.736Z",
               "shots":[]}"#,
        )
        .expect("declared provenance parses");
        assert_eq!(with.source_commit.as_deref(), Some("0c6e926dea8e"));
        assert_eq!(
            with.captured_at.as_deref(),
            Some("2026-10-17T12:42:13.736Z")
        );
    }

    /// The shipped baselines do state their source, so the run reports a revision
    /// rather than the unrecorded warning. Read through the loader the tool itself
    /// uses, not embedded at compile time: this crate's tests may not turn the
    /// application's test data into a build input.
    #[test]
    fn shipped_baselines_record_their_source() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../crates/photrez/tests/baseline/manifest.json");
        let manifest = super::load(&path).expect("the shipped manifest loads");
        assert_eq!(
            manifest.source_commit.as_deref(),
            Some("0c6e926dea8e37768dc7edd0e0f0924cc0e80b90"),
            "the baselines name the commit they were captured from"
        );
        assert!(
            manifest.captured_at.is_some(),
            "and when they were captured"
        );
    }

    /// A perceptual record and a class-map record are told apart by the keys they carry,
    /// because the two gates measure different pixels.
    #[test]
    fn the_bundle_record_names_its_comparator() {
        let perceptual = bundle_record(&json!({
            "surface": "S001",
            "thresholds": { "max_changed_pixels": 16492, "mask": "348x605" },
            "metrics": {
                "changed_pixels": 11765,
                "total_pixels": 210540,
                "ignored_aa_pixels": 5779,
            },
        }))
        .expect("perceptual record");
        assert_eq!(perceptual.surface, "S001");
        assert_eq!(perceptual.total_pixels, 210540);
        assert_eq!(
            perceptual.recorded,
            RecordedMetrics::Perceptual {
                changed: 11765,
                ignored_aa: 5779
            }
        );
        assert_eq!(
            perceptual.max_band_pct, None,
            "the bundle states no ceiling"
        );
        assert_eq!(
            perceptual.gate_pixels, 11765,
            "the percentage the record scored"
        );

        let class_map = bundle_record(&json!({
            "surface": "S052",
            "metrics": {
                "changed_pixels": 5737,
                "total_pixels": 90312,
                "raw_changed_pixels": 5404,
            },
        }))
        .expect("class-map record");
        assert_eq!(class_map.recorded, RecordedMetrics::ClassMap { raw: 5404 });
        assert_eq!(
            class_map.gate_pixels, 5737,
            "a class-map bundle gates on its own mismatch count, not on the raw count"
        );

        let err = bundle_record(&json!({ "metrics": { "total_pixels": 100 } }))
            .expect_err("no comparator can be identified");
        assert!(err.contains("raw_changed_pixels"), "names the gap: {err}");
    }

    /// The floor travels with the bundle: `max_changed_pixels` is where the ratified 12 %
    /// glyph allowance lives, and a bundle that states none gets the ratified number
    /// rather than a value invented at the comparison.
    #[test]
    fn the_ceilings_a_bundle_states_are_the_ceilings_it_is_scored_against() {
        let glyph = bundle_record(&json!({
            "thresholds": { "max_changed_pixels": 4992, "max_band_pct": 15.0 },
            "metrics": {
                "changed_pixels": 0,
                "total_pixels": 41600,
                "ignored_aa_pixels": 0,
            },
        }))
        .expect("glyph record");
        assert_eq!(glyph.max_changed_pct, 12.0, "4992 of 41600 px");
        assert_eq!(glyph.max_band_pct, Some(15.0));

        let bare = bundle_record(&json!({
            "metrics": { "changed_pixels": 0, "total_pixels": 6000, "ignored_aa_pixels": 0 },
        }))
        .expect("bare record");
        assert_eq!(bare.max_changed_pct, super::super::RATIFIED_MAX_CHANGED_PCT);
        assert_eq!(bare.max_band_pct, None);
    }

    /// A mask note may name several regions, so every `x=.. y=.. w=.. h=..` is returned and
    /// the caller matches the recorded pixel count instead of guessing which region the
    /// mask was. A note in another shape yields nothing, which refuses the rebuild.
    #[test]
    fn mask_regions_reads_every_region_a_note_declares() {
        let several = json!({
            "thresholds": {
                "mask": "overlay region x=790 y=585 w=320 h=130 (41600 px). Chrome masks: box \
                         x=860 y=585 w=180 h=90 with the glyph rect blanked.",
            },
        });
        assert_eq!(
            mask_regions(&several),
            vec![(790, 585, 320, 130), (860, 585, 180, 90)]
        );

        let menu = json!({
            "thresholds": { "mask": "measured menu+shadow rect x=551 y=577 w=265 h=352 physical \
                                     (menu panel 552..814 x 578..927 native)" },
        });
        assert_eq!(mask_regions(&menu), vec![(551, 577, 265, 352)]);

        assert!(
            mask_regions(
                &json!({ "thresholds": { "mask": "full client area 1920x1020 physical" } })
            )
            .is_empty(),
            "a size is not a region"
        );
        assert!(mask_regions(&json!({})).is_empty());
    }

    /// A digit run that would overflow is not a region, and must not wrap into one.
    #[test]
    fn an_absurd_coordinate_is_not_a_region() {
        let diff = json!({ "thresholds": { "mask": "x=99999999999999 y=1 w=2 h=3" } });
        assert!(mask_regions(&diff).is_empty());
    }
}
