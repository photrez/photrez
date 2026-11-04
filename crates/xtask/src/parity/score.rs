//! The scored surface verdict: the percentage floor and the band ceiling together.
//!
//! The ratified rule (2026-11-04) is two assertions. A surface passes only when its
//! whole-image changed percentage sits inside its recorded allowance *and* no band
//! exceeds the band ceiling after alignment. Each ceiling is a number the caller or the
//! bundle states, so a future ratification changes a value, not Rust source. The
//! per-surface percentage allowance is read from the bundle's own
//! `thresholds.max_changed_pixels`, which is where the 12 % glyph allowance already
//! lives.
//!
//! The verdict value is the band residue: what a band still differs by once the
//! whole-image horizontal shift and that band's own shift are absorbed. A displaced band
//! is a layout fact, and the ratified census scores colour defects on the residue. The
//! as-measured worst band is recorded beside it so a reader sees both readings.
//!
//! The pair must reproduce the numbers the bundle already records before it is scored.
//! That guard is what stops a wrong crop from being read as a clean surface.

use std::path::Path;

use serde_json::{json, Value};

use super::bands::{self, BandReport};
use super::perceptual::{self, DEFAULT_THRESHOLD};
use super::pngio::RgbaImage;
use super::rawdiff;
use super::{round3, Anchor, RegionSpec};

/// Band height the ratified census measured with.
pub const DEFAULT_BAND_HEIGHT: u32 = 60;
/// Horizontal and per-band alignment search, in pixels, from the ratified rule.
pub const DEFAULT_MAX_SHIFT: i32 = 8;
/// Colour tiers kept per band, from the ratified census.
pub const DEFAULT_TIERS: usize = 3;
/// The band identity ceiling adopted 2026-11-04. A caller may state another.
pub const RATIFIED_MAX_BAND_PCT: f64 = 20.0;
/// Whole-surface perceptual floor for a bundle that records no allowance of its own.
pub const RATIFIED_MAX_CHANGED_PCT: f64 = 7.834;

/// The two ceilings one surface is scored against.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ceilings {
    pub max_changed_pct: f64,
    pub max_band_pct: f64,
}

/// The worst band of an aligned report.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WorstBand {
    pub y0: u32,
    pub y1: u32,
    /// Changed fraction of the band, measured on the horizontally aligned pair.
    pub changed_pct: f64,
    /// Residue fraction after this band's own shift: the value the ceiling is applied to.
    pub residual_pct: f64,
}

/// The two-assertion verdict plus the numbers behind it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Verdict {
    pub pass: bool,
    pub percent_pass: bool,
    pub band_pass: bool,
    pub changed_pct: f64,
    pub worst: Option<WorstBand>,
}

fn pct(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    round3(part as f64 / whole as f64 * 100.0)
}

impl WorstBand {
    fn from_band(band: &bands::Band) -> Self {
        Self {
            y0: band.y0,
            y1: band.y1,
            changed_pct: band.changed_pct,
            residual_pct: pct(band.residual_at_best_dy, band.total_pixels),
        }
    }
}

/// The band with the largest residue. Ties go to the lower band on screen.
pub fn worst_band(report: &BandReport) -> Option<WorstBand> {
    let mut worst: Option<WorstBand> = None;
    for band in &report.bands {
        let candidate = WorstBand::from_band(band);
        if worst
            .as_ref()
            .is_none_or(|current| candidate.residual_pct > current.residual_pct)
        {
            worst = Some(candidate);
        }
    }
    worst
}

/// A ceiling is met at equality and missed above it: the rule reads "exceeds".
pub fn decide(changed_pct: f64, report: &BandReport, ceilings: Ceilings) -> Verdict {
    let worst = worst_band(report);
    let band_pass = worst.is_none_or(|band| band.residual_pct <= ceilings.max_band_pct);
    let percent_pass = changed_pct <= ceilings.max_changed_pct;
    Verdict {
        pass: percent_pass && band_pass,
        percent_pass,
        band_pass,
        changed_pct: round3(changed_pct),
        worst,
    }
}

/// Which comparator the bundle's recorded `metrics` came from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RecordedRule {
    /// `changed_pixels` with anti-aliasing exclusion.
    Perceptual,
    /// `raw_changed_pixels` from the class map; the band measure still uses the
    /// perceptual rule, so a scored document labels which number is whose.
    ClassMap,
}

/// Re-measure the pair and check it against the numbers the bundle recorded.
///
/// Fails when the totals disagree, because then this is not the mask pair the recorded
/// verdict was made on.
pub fn verify_recorded(
    recorded: &Value,
    a: &RgbaImage,
    b: &RgbaImage,
) -> Result<(RecordedRule, u64), String> {
    let metrics = recorded
        .get("metrics")
        .and_then(Value::as_object)
        .ok_or("diff.json `metrics` is missing or not an object")?;
    let number = |key: &str| metrics.get(key).and_then(Value::as_u64);
    let total = number("total_pixels").ok_or("diff.json `metrics.total_pixels` missing")?;
    let rule = if number("ignored_aa_pixels").is_some() {
        RecordedRule::Perceptual
    } else if number("raw_changed_pixels").is_some() {
        RecordedRule::ClassMap
    } else {
        return Err(
            "diff.json `metrics` records neither ignored_aa_pixels nor raw_changed_pixels, so \
             nothing here can be checked against it"
                .to_string(),
        );
    };
    let measured = perceptual::compare(a, b, DEFAULT_THRESHOLD, true)
        .map_err(|e| format!("perceptual measure: {e}"))?;
    if measured.total_pixels != total {
        return Err(format!(
            "pair is {} px, diff.json records total_pixels {total}",
            measured.total_pixels
        ));
    }
    let observed = match rule {
        RecordedRule::Perceptual => {
            let changed = number("changed_pixels")
                .ok_or("diff.json `metrics.changed_pixels` missing")?;
            let aa = number("ignored_aa_pixels")
                .ok_or("diff.json `metrics.ignored_aa_pixels` missing")?;
            if (measured.changed_pixels, measured.ignored_aa_pixels) != (changed, aa) {
                return Err(format!(
                    "pair reproduces {} changed px with {aa} AA excluded while diff.json records \
                     {changed} with {aa}",
                    measured.changed_pixels
                ));
            }
            changed
        }
        RecordedRule::ClassMap => {
            let raw = number("raw_changed_pixels").expect("rule chosen from this key");
            let region = RegionSpec {
                name: "full".to_string(),
                anchor: Anchor::Top,
                rows: a.height,
            };
            let reports = rawdiff::compare_anchored(a, b, &[region])?;
            let observed_raw = reports.first().map(|r| r.diff_pixels).unwrap_or(0);
            if observed_raw != raw {
                return Err(format!(
                    "pair reproduces {observed_raw} raw px while diff.json records {raw}"
                ));
            }
            raw
        }
    };
    Ok((rule, observed))
}

/// The `bands` block: the ceiling it was scored against, the worst band, and every band.
pub fn bands_block(report: &BandReport, verdict: &Verdict, ceilings: Ceilings) -> Value {
    let worst_changed = report
        .bands
        .iter()
        .map(WorstBand::from_band)
        .max_by(|left, right| left.changed_pct.total_cmp(&right.changed_pct));
    json!({
        "ceiling_pct": ceilings.max_band_pct,
        "pass": verdict.band_pass,
        "worst_band_y0": verdict.worst.map(|w| w.y0),
        "worst_band_y1": verdict.worst.map(|w| w.y1),
        "worst_band_pct": verdict.worst.map(|w| w.residual_pct),
        "worst_changed_band_y0": worst_changed.as_ref().map(|w| w.y0),
        "worst_changed_band_y1": worst_changed.as_ref().map(|w| w.y1),
        "worst_changed_pct": worst_changed.as_ref().map(|w| w.changed_pct),
        "horizontal_dx": report.horizontal.best_dx,
        "report": report,
        "rule": format!(
            "band identity ceiling {:.3} % of a band's pixels, on the residue left after the \
             whole-image horizontal alignment within +-{} px",
            ceilings.max_band_pct, report.max_shift
        ),
    })
}

/// Merge a band result into the recorded bundle document.
///
/// Every legacy key keeps its meaning; `pass` becomes the two-assertion verdict, and
/// `thresholds.max_band_pct` records the ceiling this run applied.
pub fn score_document(
    recorded: &Value,
    report: &BandReport,
    verdict: &Verdict,
    ceilings: Ceilings,
    rule: RecordedRule,
) -> Value {
    let mut document = recorded.clone();
    let band_block = bands_block(report, verdict, ceilings);
    if let Some(map) = document.as_object_mut() {
        map.insert("bands".to_string(), band_block);
        map.insert("pass".to_string(), json!(verdict.pass));
        let thresholds = map
            .entry("thresholds")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("thresholds was written as an object");
        thresholds.insert("max_band_pct".to_string(), json!(ceilings.max_band_pct));
        thresholds.insert("band_verdict".to_string(), json!(verdict.band_pass));
        let metrics = map
            .entry("metrics")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("metrics was written as an object");
        metrics.insert("changed_pct".to_string(), json!(verdict.changed_pct));
        metrics.insert(
            "comparator".to_string(),
            json!(match rule {
                RecordedRule::Perceptual => "perceptual",
                RecordedRule::ClassMap => "class-map gate, perceptual bands",
            }),
        );
    }
    document
}

/// One scored surface.
pub struct Scored {
    pub document: Value,
    pub verdict: Verdict,
    pub ceilings: Ceilings,
    pub report: BandReport,
}

/// Score the mask pair of one evidence bundle against the two ceilings.
#[allow(clippy::too_many_arguments)]
pub fn score_bundle(
    recorded: &Value,
    a: &RgbaImage,
    b: &RgbaImage,
    max_band_pct: f64,
    band_height: u32,
    max_shift: i32,
    tiers: usize,
) -> Result<Scored, String> {
    let (rule, _) = verify_recorded(recorded, a, b)?;
    let measured = perceptual::compare(a, b, DEFAULT_THRESHOLD, true)?;
    let report = bands::report(a, b, DEFAULT_THRESHOLD, band_height, max_shift, tiers)?;
    let ceilings = Ceilings {
        max_changed_pct: recorded_allowance(recorded, measured.total_pixels),
        max_band_pct,
    };
    let verdict = decide(measured.changed_pct, &report, ceilings);
    let document = score_document(recorded, &report, &verdict, ceilings, rule);
    Ok(Scored {
        document,
        verdict,
        ceilings,
        report,
    })
}

/// The percentage allowance the bundle recorded for itself, as a percentage of the mask.
fn recorded_allowance(recorded: &Value, total_pixels: u64) -> f64 {
    let allowance = recorded
        .get("thresholds")
        .and_then(|t| t.get("max_changed_pixels"))
        .and_then(Value::as_u64);
    match allowance {
        Some(pixels) if total_pixels > 0 => pct(pixels, total_pixels),
        _ => RATIFIED_MAX_CHANGED_PCT,
    }
}

/// `xtask parity surfaces --score --bundle <dir> --oracle-mask <png> --native-mask <png>
///  [--max-band-pct <f>] [--band-height <px>] [--max-shift <px>] [--tiers <n>] [--out <json>]`
///
/// Re-measures the recorded mask pair, adds the band identity block to its `diff.json`,
/// and applies the band ceiling to the verdict. Exits 1 when the surface fails either
/// ceiling. Nothing is written unless `--out` names a path, so a recorded bundle is never
/// edited in place.
pub fn cmd(args: &[String]) -> i32 {
    let (Some(bundle), Some(oracle), Some(native)) = (
        super::surfaces::flag_path(args, "--bundle"),
        super::surfaces::flag_path(args, "--oracle-mask"),
        super::surfaces::flag_path(args, "--native-mask"),
    ) else {
        eprintln!(
            "xtask parity surfaces --score: --bundle <dir>, --oracle-mask <png> and \
             --native-mask <png> are required.\n\
             \x20 The bundle holds the recorded diff.json; the two masks are its measured pair."
        );
        return 2;
    };
    let number = |name: &str, default: f64| -> Result<f64, i32> {
        match super::surfaces::flag(args, name) {
            None => Ok(default),
            Some(v) => v.parse::<f64>().map_err(|_| {
                eprintln!("xtask parity surfaces --score: {name} must be a number, got `{v}`");
                2
            }),
        }
    };
    let max_band_pct = match number("--max-band-pct", RATIFIED_MAX_BAND_PCT) {
        Ok(v) if v >= 0.0 => v,
        Ok(v) => {
            eprintln!("xtask parity surfaces --score: --max-band-pct must not be {v}");
            return 2;
        }
        Err(code) => return code,
    };
    let band_height = match number("--band-height", f64::from(DEFAULT_BAND_HEIGHT)) {
        Ok(v) if v >= 1.0 => v as u32,
        Ok(v) => {
            eprintln!("xtask parity surfaces --score: --band-height must be at least 1, got {v}");
            return 2;
        }
        Err(code) => return code,
    };
    let max_shift = match number("--max-shift", f64::from(DEFAULT_MAX_SHIFT)) {
        Ok(v) if v >= 0.0 => v as i32,
        Ok(v) => {
            eprintln!("xtask parity surfaces --score: --max-shift must not be {v}");
            return 2;
        }
        Err(code) => return code,
    };
    let tiers = match number("--tiers", f64::from(DEFAULT_TIERS as u32)) {
        Ok(v) if v >= 1.0 => v as usize,
        Ok(v) => {
            eprintln!("xtask parity surfaces --score: --tiers must be at least 1, got {v}");
            return 2;
        }
        Err(code) => return code,
    };
    let diff_path = bundle.join("diff.json");
    let recorded = match read_json(&diff_path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("xtask parity surfaces --score: {e}");
            return 2;
        }
    };
    let images = |paths: [&Path; 2]| -> Option<[RgbaImage; 2]> {
        paths
            .iter()
            .map(|p| RgbaImage::load(p).ok())
            .collect::<Vec<Option<RgbaImage>>>()
            .try_into()
            .ok()
            .and_then(|v: [Option<RgbaImage>; 2]| {
                let [a, b] = v;
                match (a, b) {
                    (Some(a), Some(b)) => Some([a, b]),
                    _ => None,
                }
            })
    };
    let _ = images;
    let (a, b) = match (RgbaImage::load(&oracle), RgbaImage::load(&native)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("xtask parity surfaces --score: {e}");
            return 2;
        }
    };
    let scored = match score_bundle(&recorded, &a, &b, max_band_pct, band_height, max_shift, tiers)
    {
        Ok(s) => s,
        Err(e) => {
            eprintln!("xtask parity surfaces --score: {e}");
            return 1;
        }
    };
    let surface = recorded
        .get("surface")
        .and_then(Value::as_str)
        .unwrap_or("(unnamed)");
    println!(
        "{surface}: {} px changed {:.3}% of allowance {:.3}% | worst band {}..{} residue {:.3}% \
         of ceiling {:.3}% | horizontal dx {}",
        scored.report.horizontal.changed_pixels,
        scored.verdict.changed_pct,
        scored.ceilings.max_changed_pct,
        scored.verdict.worst.map_or(0, |w| w.y0),
        scored.verdict.worst.map_or(0, |w| w.y1),
        scored.verdict.worst.map_or(0.0, |w| w.residual_pct),
        scored.ceilings.max_band_pct,
        scored.report.horizontal.best_dx,
    );
    for band in &scored.report.bands {
        let residue = pct(band.residual_at_best_dy, band.total_pixels);
        println!(
            "  y {:>4}..{:<4} {:>6} px differ ({:.3}%), dy {:>3}, residue {residue:.3}%",
            band.y0, band.y1, band.changed_pixels, band.changed_pct, band.best_dy
        );
    }
    if let Some(out) = super::surfaces::flag(args, "--out").map(Path::new) {
        let text = serde_json::to_string_pretty(&scored.document)
            .map_err(|e| format!("serialize scored diff.json: {e}"))
            .unwrap_or_else(|e| {
                eprintln!("xtask parity surfaces --score: {e}");
                std::process::exit(2);
            });
        if let Err(e) = std::fs::write(out, text) {
            eprintln!("xtask parity surfaces --score: writing {}: {e}", out.display());
            return 2;
        }
    }
    if scored.verdict.pass {
        println!("{surface}: PASS");
        return 0;
    }
    let mut reasons = Vec::new();
    if !scored.verdict.percent_pass {
        reasons.push("percentage floor");
    }
    if !scored.verdict.band_pass {
        reasons.push("band identity ceiling");
    }
    eprintln!(
        "{surface}: FAIL ({}) - worst band residue {:.3}% over ceiling {:.3}%",
        reasons.join(" + "),
        scored.verdict.worst.map_or(0.0, |w| w.residual_pct),
        scored.ceilings.max_band_pct,
    );
    1
}

fn read_json(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn img(width: u32, height: u32, color_at: impl Fn(u32, u32) -> [u8; 3]) -> RgbaImage {
        let mut data = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let [r, g, b] = color_at(x, y);
                data.extend_from_slice(&[r, g, b, 255]);
            }
        }
        RgbaImage::from_rgba(width, height, data).expect("fixture buffer")
    }

    fn flat(width: u32, height: u32, c: [u8; 3]) -> RgbaImage {
        img(width, height, |_, _| c)
    }

    const DARK: [u8; 3] = [30, 30, 30];
    const MAGENTA: [u8; 3] = [255, 0, 255];

    /// A band with no residue sits at zero.
    fn no_residue(width: u32, height: u32) -> BandReport {
        bands::report(
            &flat(width, height, DARK),
            &flat(width, height, DARK),
            DEFAULT_THRESHOLD,
            DEFAULT_BAND_HEIGHT,
            DEFAULT_MAX_SHIFT,
            DEFAULT_TIERS,
        )
        .expect("report")
    }

    /// The negative control that failed to fail: one input field painted magenta is a
    /// small share of its mask, so the percentage clears the floor while the band it sits
    /// in is almost entirely a different colour.
    #[test]
    fn a_wrong_coloured_band_fails_while_the_percentage_passes() {
        // A 400x200 mask; a 100x60 magenta block fills one band and 7.5 % of the image.
        let oracle = flat(400, 200, DARK);
        let native = img(400, 200, |x, y| {
            if (100..200).contains(&x) && (60..120).contains(&y) {
                MAGENTA
            } else {
                DARK
            }
        });
        let measured = perceptual::compare(&oracle, &native, DEFAULT_THRESHOLD, true).unwrap();
        let report =
            bands::report(&oracle, &native, DEFAULT_THRESHOLD, 60, 8, 3).expect("report");
        let ceilings = Ceilings {
            max_changed_pct: RATIFIED_MAX_CHANGED_PCT,
            max_band_pct: RATIFIED_MAX_BAND_PCT,
        };
        let verdict = decide(measured.changed_pct, &report, ceilings);
        assert!(
            measured.changed_pct < ceilings.max_changed_pct,
            "the old check must still pass: {} %",
            measured.changed_pct
        );
        assert!(verdict.percent_pass, "the floor is met on its own");
        assert!(!verdict.band_pass, "the recoloured band trips the ceiling");
        assert!(!verdict.pass, "a band fail is a surface fail");
        let worst = verdict.worst.expect("one band differs");
        assert_eq!((worst.y0, worst.y1), (60, 120), "the band that tripped");
        assert!(
            worst.residual_pct > ceilings.max_band_pct,
            "residue {} % over {}",
            worst.residual_pct,
            ceilings.max_band_pct
        );
    }

    /// Exactly at the ceiling is not over it. The rule reads "exceeds 20 %".
    #[test]
    fn a_band_at_the_ceiling_passes_and_one_above_fails() {
        let a = flat(100, 60, DARK);
        let painted = 20;
        let b = img(100, 60, |x, _| {
            if x < painted {
                MAGENTA
            } else {
                DARK
            }
        });
        let report = bands::report(&a, &b, DEFAULT_THRESHOLD, 60, 8, 3).expect("report");
        assert_eq!(
            worst_band(&report).expect("one band").residual_pct,
            RATIFIED_MAX_BAND_PCT,
            "20 of 100 pixels is exactly {} %",
            RATIFIED_MAX_BAND_PCT
        );
        let ceilings = Ceilings {
            max_changed_pct: RATIFIED_MAX_CHANGED_PCT,
            max_band_pct: RATIFIED_MAX_BAND_PCT,
        };
        assert!(
            decide(0.0, &report, ceilings).band_pass,
            "at the ceiling the band passes"
        );
        let b2 = img(100, 60, |x, _| {
            if x < painted + 1 {
                MAGENTA
            } else {
                DARK
            }
        });
        let r2 = bands::report(&a, &b2, DEFAULT_THRESHOLD, 60, 8, 3).expect("report");
        let v2 = decide(0.0, &r2, ceilings);
        assert_eq!(v2.worst.expect("one band").residual_pct, 21.0);
        assert!(!v2.band_pass, "one pixel above the ceiling fails");
    }

    /// A displaced band is a layout fact, not a colour defect: its residue is what counts.
    #[test]
    fn alignment_rescues_a_displaced_band() {
        let stripe = |y: u32| {
            if (60..72).contains(&y) {
                [242, 142, 66]
            } else {
                DARK
            }
        };
        let a = img(200, 180, |_, y| stripe(y));
        let b = img(200, 180, |_, y| stripe(y.saturating_sub(12)));
        let report = bands::report(&a, &b, DEFAULT_THRESHOLD, 60, 8, 3).expect("report");
        let band = &report.bands[1];
        assert!(band.changed_pct > RATIFIED_MAX_BAND_PCT);
        assert_eq!(band.best_dy, 12, "the band sits twelve rows lower");
        assert_eq!(band.residual_at_best_dy, 0, "the shift explains the whole band");
        let ceilings = Ceilings {
            max_changed_pct: RATIFIED_MAX_CHANGED_PCT,
            max_band_pct: RATIFIED_MAX_BAND_PCT,
        };
        let verdict = decide(5.0, &report, ceilings);
        assert!(
            verdict.band_pass,
            "a displaced band must not be read as a colour defect"
        );
        assert!(verdict.pass);
    }

    /// An identical pair clears both assertions and leaves the worst band undefined.
    #[test]
    fn a_clean_surface_passes_both_assertions() {
        let report = no_residue(120, 180);
        let ceilings = Ceilings {
            max_changed_pct: RATIFIED_MAX_CHANGED_PCT,
            max_band_pct: RATIFIED_MAX_BAND_PCT,
        };
        let verdict = decide(0.0, &report, ceilings);
        assert!(verdict.pass);
        assert_eq!(verdict.worst, Some(WorstBand::default_free(&report)));
    }

    /// A bundle whose recorded pair is not this pair must be refused, not scored.
    #[test]
    fn a_pair_that_does_not_reproduce_the_record_is_refused() {
        let recorded = json!({
            "surface": "S001",
            "pass": true,
            "thresholds": { "max_changed_pixels": 100, "mask": "100x60" },
            "metrics": {
                "changed_pixels": 999,
                "total_pixels": 6000,
                "ignored_aa_pixels": 0,
                "changed_pct": 16.65,
            },
        });
        let err = verify_recorded(&recorded, &flat(100, 60, DARK), &flat(100, 60, DARK))
            .expect_err("recorded count is wrong");
        assert!(err.contains("reproduces"), "got `{err}`");
    }

    /// A total that does not match the pair means the wrong crop was supplied.
    #[test]
    fn a_different_total_pixels_is_refused() {
        let recorded = json!({
            "metrics": { "changed_pixels": 0, "total_pixels": 12345, "ignored_aa_pixels": 0 },
        });
        let err = verify_recorded(&recorded, &flat(100, 60, DARK), &flat(100, 60, DARK))
            .expect_err("size does not match the record");
        assert!(err.contains("pair is 6000 px"), "got `{err}`");
    }

    /// A class-map bundle is verified on the raw comparator and labelled in the output,
    /// because its gate number and its band number come from different rules.
    #[test]
    fn a_class_map_bundle_is_verified_on_the_raw_count() {
        let a = flat(100, 60, DARK);
        let b = img(100, 60, |x, _| {
            if x < 10 {
                MAGENTA
            } else {
                DARK
            }
        });
        let recorded = json!({
            "surface": "S052",
            "pass": true,
            "thresholds": { "max_changed_pixels": 700, "mask": "100x60" },
            "metrics": {
                "changed_pixels": 600,
                "total_pixels": 6000,
                "changed_pct": 10.0,
                "raw_changed_pixels": 600,
            },
        });
        let (rule, observed) = verify_recorded(&recorded, &a, &b).expect("class-map pair");
        assert_eq!(rule, RecordedRule::ClassMap);
        assert_eq!(observed, 600);
        let scored = score_bundle(&recorded, &a, &b, RATIFIED_MAX_BAND_PCT, 60, 8, 3)
            .expect("scored class-map bundle");
        assert_eq!(
            scored.document["metrics"]["comparator"].as_str(),
            Some("class-map gate, perceptual bands")
        );
    }

    /// The written document keeps every legacy key and adds the band block beside them.
    #[test]
    fn the_scored_document_keeps_the_legacy_keys() {
        let a = flat(400, 200, DARK);
        let b = img(400, 200, |x, y| {
            if (100..200).contains(&x) && (60..120).contains(&y) {
                MAGENTA
            } else {
                DARK
            }
        });
        let measured = perceptual::compare(&a, &b, DEFAULT_THRESHOLD, true).unwrap();
        let recorded = json!({
            "surface": "S043",
            "pass": true,
            "thresholds": {
                "max_changed_pixels": 29238,
                "mask": "the PrintInspector pane alone: x=1131 y=143 w=479 h=779 physical",
                "rule": "owner-ratified 2026-10-30 text rule",
            },
            "metrics": {
                "changed_pixels": measured.changed_pixels,
                "total_pixels": measured.total_pixels,
                "changed_pct": 6.848,
                "ignored_aa_pixels": measured.ignored_aa_pixels,
                "threshold": 0.1,
            },
            "inputs": { "oracle": "fnv64:aaaa", "native": "fnv64:bbbb" },
            "red": { "capture": "S043-native-mask-red.png", "changed_pct": 66.0 },
            "states": [{ "id": "resting", "pass": true }],
        });
        let scored = score_bundle(&recorded, &a, &b, RATIFIED_MAX_BAND_PCT, 60, 8, 3)
            .expect("scored bundle");
        let document = &scored.document;
        for key in [
            "surface",
            "pass",
            "thresholds",
            "metrics",
            "inputs",
            "red",
            "states",
            "bands",
        ] {
            assert!(document.get(key).is_some(), "`{key}` is missing");
        }
        assert_eq!(document["surface"], "S043");
        assert_eq!(
            document["thresholds"]["mask"], recorded["thresholds"]["mask"],
            "the mask note is not rewritten"
        );
        assert_eq!(document["inputs"], recorded["inputs"]);
        assert_eq!(document["red"], recorded["red"]);
        assert_eq!(document["states"], recorded["states"]);
        assert_eq!(
            document["thresholds"]["max_changed_pixels"],
            recorded["thresholds"]["max_changed_pixels"]
        );
        assert_eq!(document["bands"]["ceiling_pct"], RATIFIED_MAX_BAND_PCT);
        assert_eq!(
            document["thresholds"]["max_band_pct"],
            json!(RATIFIED_MAX_BAND_PCT)
        );
        assert_eq!(document["pass"], false, "the band fail becomes the verdict");
        assert_eq!(
            document["metrics"]["changed_pixels"],
            recorded["metrics"]["changed_pixels"],
            "the legacy metric is not restated as a band number"
        );
        let worst_y0 = document["bands"]["worst_band_y0"].as_u64().expect("y0");
        assert_eq!(worst_y0, 60, "the reader can see which band tripped");
        assert_eq!(document["bands"]["horizontal_dx"], 0);
        assert_eq!(
            document["bands"]["report"]["bands"].as_array().expect("bands").len(),
            4
        );
    }

    /// A bundle that records no allowance is scored against the ratified floor, so the
    /// ceiling is never an unlabelled magic number in the comparison.
    #[test]
    fn a_bundle_without_an_allowance_uses_the_ratified_floor() {
        let recorded = json!({ "metrics": { "total_pixels": 6000 } });
        assert_eq!(
            recorded_allowance(&recorded, 6000),
            RATIFIED_MAX_CHANGED_PCT
        );
        let with = json!({ "thresholds": { "max_changed_pixels": 4992 }, "metrics": {} });
        assert_eq!(recorded_allowance(&with, 41600), 12.0);
    }

    /// Zero-size input cannot divide, and must not panic.
    #[test]
    fn an_empty_region_scores_as_zero() {
        assert_eq!(pct(0, 0), 0.0);
        assert_eq!(pct(1, 4), 25.0);
    }
}
