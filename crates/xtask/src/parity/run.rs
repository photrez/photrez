//! Parity orchestrator: select shots from the manifest, capture (or reuse)
//! candidates, compare, report, and return the exit code.
//!
//! Verdict rule (strict by design): a shot is RED when any compared region has
//! `hardGeometryMismatchPct > 0.000`. Raw `diff%` and the ink ratio are
//! reported, never verdicts on their own. `--mask-metrics` blanks the
//! documented nondeterministic footer segment (viewport `x 620..=994`,
//! `y 801..=816`, per the baseline README's measured run-to-run variance) in
//! both images before comparison.
//!
//! The scored run (`cmd_score`) is a different gate: it re-measures an evidence
//! bundle's own mask pair and applies the two ratified assertions - the
//! whole-surface percentage floor and the band identity ceiling from
//! [`super::bands`]. A percentage inside the floor is not colour evidence, so a
//! surface whose worst aligned band exceeds the ceiling is a FAIL there even
//! when its percentage passes.
//!
//! Outputs into `--report` (default `target/xtask-parity/`):
//! `shape-report.json` (array over compared shots, the shape verdict surface),
//! `raw-report.json`, `ink-report.json`, and a stdout table. The baseline
//! directory is never written to.
//!
//! Candidate resolution per shot: `--candidate <dir>` -> `<dir>/<id>.png`;
//! `--candidate <file.png>` -> that file (only valid with exactly one
//! selected shot); otherwise a fresh capture via `capture::capture`.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::bands::{self, BandReport, Ceilings, Verdict};
use super::manifest::{self, Shot};
use super::perceptual::{self, DEFAULT_THRESHOLD};
use super::pngio::RgbaImage;
use super::rawdiff;
use super::shapediff;
use super::zoom;
use super::{Anchor, Rect, RegionSpec};
use super::{InkRegionReport, RawRegionReport, ShapeRegionReport};
use crate::capture;

/// Everything the `parity` command needs, parsed by `main.rs`.
pub struct ParityOpts {
    pub baseline: PathBuf,
    pub shots: Vec<String>,
    pub candidate: Option<PathBuf>,
    pub report_dir: PathBuf,
    pub mask_metrics: bool,
    pub accept_ink_residual: bool,
}

/// The nondeterministic footer metrics strip, in viewport coordinates, as
/// measured on the 1366x820 web baselines (x 620..=994, y 801..=816; run-to-run
/// variance per the baseline README). Used only when the manifest declares no
/// mask of its own *and* the candidate viewport is the size these numbers were
/// measured on - see `resolve_mask`.
const LEGACY_METRICS_MASK: manifest::MetricsMask = manifest::MetricsMask {
    x0: 620,
    y0: 801,
    x1: 994,
    y1: 816,
    viewport_width: 1366,
    viewport_height: 820,
};

/// Pick the footer mask for a candidate of this size, or refuse.
///
/// A mask rect is not portable between viewport sizes: blanking `y 801..=816` on
/// a 1020px-tall capture hides a different band of the UI than it did on the
/// 820px-tall capture the numbers came from, and the resulting diff still reads
/// clean. Failing closed here is the only way that trap stays visible.
fn resolve_mask(
    declared: Option<manifest::MetricsMask>,
    width: u32,
    height: u32,
) -> Result<manifest::MetricsMask, String> {
    let mask = declared.unwrap_or(LEGACY_METRICS_MASK);
    if mask.viewport_width == width && mask.viewport_height == height {
        return Ok(mask);
    }
    let source = if declared.is_some() {
        "manifest metricsMask is"
    } else {
        "the built-in mask is"
    };
    Err(format!(
        "--mask-metrics refused: {source} calibrated for {}x{} but the candidate is {}x{}; \
         declare a metricsMask in manifest.json for this size",
        mask.viewport_width, mask.viewport_height, width, height
    ))
}

/// Per-shot accumulated result for report assembly.
struct ShotResult {
    shot: String,
    file: String,
    bounds: Rect,
    candidate: String,
    shape_regions: Vec<ShapeRegionReport>,
    raw_regions: Vec<RawRegionReport>,
    ink_regions: Vec<InkRegionReport>,
    pass: bool,
    capture_error: Option<String>,
}

/// Run the comparison loop. `Ok(exit_code)`: 0 = all shots green, 1 = at
/// least one red (the report is written either way).
pub fn run_parity(opts: &ParityOpts) -> Result<i32, String> {
    // The baselines were captured with the English catalog: refuse a
    // non-English request (environment or manifest) before any capture.
    capture::require_english_locale()?;
    let manifest_path = opts.baseline.join("manifest.json");
    let manifest = manifest::load(&manifest_path)?;
    manifest::require_english_locale(&manifest)?;
    let selected = manifest::select(&manifest, &opts.shots)?;

    std::fs::create_dir_all(&opts.report_dir)
        .map_err(|e| format!("create {}: {e}", opts.report_dir.display()))?;
    let cand_dir = opts.report_dir.join("candidates");
    let sbs_dir = opts.report_dir.join("sbs");
    std::fs::create_dir_all(&cand_dir)
        .map_err(|e| format!("create {}: {e}", cand_dir.display()))?;
    std::fs::create_dir_all(&sbs_dir).map_err(|e| format!("create {}: {e}", sbs_dir.display()))?;

    let candidate_is_dir = opts.candidate.as_ref().map(|c| c.is_dir()).unwrap_or(false);

    let mut results: Vec<ShotResult> = Vec::new();

    for shot in selected {
        let res = process_shot(
            shot,
            opts,
            manifest.metrics_mask,
            &cand_dir,
            &sbs_dir,
            candidate_is_dir,
        );
        results.push(res);
    }

    write_reports(&opts.report_dir, opts, &results)?;

    let mut all_pass = true;
    let mut fail_count = 0usize;
    for r in &results {
        if r.capture_error.is_some() || !r.pass {
            all_pass = false;
            fail_count += 1;
        }
    }

    for r in &results {
        let hard_pct = r
            .shape_regions
            .first()
            .map(|rr| rr.hard_geometry_mismatch_pct)
            .unwrap_or(0.0);
        let diff_pct = r.raw_regions.first().map(|rr| rr.diff_pct).unwrap_or(0.0);
        let ink_ratio = r.ink_regions.first().map(|ir| ir.ratio).unwrap_or(0.0);
        let status = if r.pass { "PASS" } else { "FAIL" };
        println!(
            "{}  hard={:.3}%  diff={:.3}%  ink={:.3}  {}",
            r.shot, hard_pct, diff_pct, ink_ratio, status
        );
    }
    println!(
        "summary: {} shot(s), {} pass, {} fail",
        results.len(),
        results.len() - fail_count,
        fail_count
    );

    Ok(if all_pass { 0 } else { 1 })
}

/// Process a single shot: resolve candidate, crop, compare, assemble.
fn process_shot(
    shot: &Shot,
    opts: &ParityOpts,
    mask: Option<manifest::MetricsMask>,
    cand_dir: &Path,
    sbs_dir: &Path,
    candidate_is_dir: bool,
) -> ShotResult {
    let bounds = Rect {
        x: shot.bounds.x as u32,
        y: shot.bounds.y as u32,
        width: shot.bounds.width as u32,
        height: shot.bounds.height as u32,
    };

    // --- Candidate resolution ---
    let candidate: Result<RgbaImage, String> = resolve_candidate(
        shot,
        opts,
        cand_dir.join(format!("{}.png", shot.id)),
        candidate_is_dir,
    );

    let mut result = ShotResult {
        shot: shot.id.clone(),
        file: shot.file.clone(),
        bounds,
        candidate: String::new(),
        shape_regions: Vec::new(),
        raw_regions: Vec::new(),
        ink_regions: Vec::new(),
        pass: true,
        capture_error: None,
    };

    let candidate_path = match &opts.candidate {
        Some(p) if candidate_is_dir => cand_dir.join(format!("{}.png", shot.id)),
        Some(p) if p.is_file() => p.clone(),
        _ => cand_dir.join(format!("{}.png", shot.id)),
    };
    result.candidate = candidate_path.display().to_string();

    let candidate_img = match candidate {
        Ok(img) => img,
        Err(e) => {
            result.capture_error = Some(e.clone());
            result.pass = false;
            println!(
                "{}  hard=N/A  diff=N/A  ink=N/A  FAIL (capture error: {})",
                shot.id, e
            );
            result.candidate = candidate_path.display().to_string();
            return result;
        }
    };

    // --- Baseline load + crop ---
    let baseline_path = opts.baseline.join(&shot.file);
    let baseline_img = match RgbaImage::load(&baseline_path) {
        Ok(img) => img,
        Err(e) => {
            result.capture_error = Some(format!("baseline load: {e}"));
            result.pass = false;
            return result;
        }
    };

    // Crop rect: origin from manifest box (truncated toward zero, matching
    // shell-diff.ts's use of the f64 in `<< 2` indexing), size from baseline PNG.
    let rect = Rect {
        x: bounds.x,
        y: bounds.y,
        width: baseline_img.width,
        height: baseline_img.height,
    };

    if rect.x + rect.width > candidate_img.width || rect.y + rect.height > candidate_img.height {
        result.capture_error = Some(format!(
            "crop {}x{}@{},{} does not fit candidate {}x{}",
            rect.width, rect.height, rect.x, rect.y, candidate_img.width, candidate_img.height
        ));
        result.pass = false;
        return result;
    }

    let mut candidate_crop = match candidate_img.crop(rect) {
        Ok(c) => c,
        Err(e) => {
            result.capture_error = Some(format!("candidate crop: {e}"));
            result.pass = false;
            return result;
        }
    };

    let mut baseline_crop = baseline_img.clone();

    // --- Mask metrics (candidate coords: viewport; baseline coords: viewport - box origin) ---
    if opts.mask_metrics {
        match resolve_mask(mask, candidate_img.width, candidate_img.height) {
            Ok(m) => {
                mask_footer(&mut candidate_crop, m, 0, 0);
                let bx = bounds.x;
                let by = bounds.y;
                mask_footer_baseline(&mut baseline_crop, m, bx, by);
            }
            Err(e) => {
                result.capture_error = Some(e.clone());
                result.pass = false;
                println!("{}  hard=N/A  diff=N/A  ink=N/A  FAIL ({})", shot.id, e);
                return result;
            }
        }
    }

    // --- Compare ---
    let shape_report = match shapediff::compare_crop_in(&baseline_crop, &candidate_img, rect) {
        Ok(mut r) => {
            r.name = shot.id.clone();
            r
        }
        Err(e) => {
            result.capture_error = Some(e);
            result.pass = false;
            return result;
        }
    };

    let raw_report = match rawdiff::compare_crop_in(&baseline_crop, &candidate_img, rect) {
        Ok(mut r) => {
            r.name = shot.id.clone();
            r
        }
        Err(e) => {
            result.capture_error = Some(e);
            result.pass = false;
            return result;
        }
    };

    // --- Ink ---
    let (ink_orig, peak_orig) =
        zoom::region_ink(&baseline_crop, 0, rect.height, zoom::DEFAULT_BG_LUMA);
    let (ink_port, peak_port) =
        zoom::region_ink(&candidate_crop, 0, rect.height, zoom::DEFAULT_BG_LUMA);
    let ink_report = zoom::ink_report(
        shot.id.clone(),
        "crop",
        ink_orig,
        ink_port,
        peak_orig,
        peak_port,
    );

    // --- Side-by-side panel ---
    let sbs_path = sbs_dir.join(format!("{}.png", shot.id));
    let _ = rawdiff::write_sbs_panel(
        &baseline_crop,
        0,
        &candidate_crop,
        0,
        rect.height,
        &sbs_path,
    );

    // --- Pass/fail verdict ---
    if shape_report.hard_geometry_mismatch_pct > 0.0 {
        result.pass = false;
    }

    result.shape_regions.push(shape_report);
    result.raw_regions.push(raw_report);
    result.ink_regions.push(ink_report);
    result
}

/// Resolve the candidate image for a shot.
fn resolve_candidate(
    shot: &Shot,
    opts: &ParityOpts,
    out_path: PathBuf,
    candidate_is_dir: bool,
) -> Result<RgbaImage, String> {
    if let Some(candidate) = &opts.candidate {
        if candidate_is_dir {
            let cand_file = candidate.join(format!("{}.png", shot.id));
            return if cand_file.exists() {
                RgbaImage::load(&cand_file)
            } else {
                Err(format!("candidate file missing: {}", cand_file.display()))
            };
        }
        // File candidate: only valid with exactly one shot.
        return RgbaImage::load(candidate);
    }

    // No candidate: capture fresh.
    let spec = capture::CaptureSpec {
        shot: Some(shot.id.clone()),
        exe: None,
        out: out_path,
        wait_ms: 60_000,
    };
    capture::capture(&spec)?;
    RgbaImage::load(&spec.out)
}

/// Zero the footer metrics strip in viewport coordinates.
fn mask_footer(img: &mut RgbaImage, mask: manifest::MetricsMask, ox: u32, oy: u32) {
    let (x0, x1) = (mask.x0, mask.x1);
    let (y0, y1) = (mask.y0, mask.y1);
    for y in y0..=y1.min(img.height - 1) {
        for x in x0..=x1 {
            if x + ox < img.width && y + oy < img.height {
                let idx =
                    ((y as usize + oy as usize) * img.width as usize + x as usize + ox as usize)
                        * 4;
                img.data[idx] = 0;
                img.data[idx + 1] = 0;
                img.data[idx + 2] = 0;
                img.data[idx + 3] = 255;
            }
        }
    }
}

/// Zero the footer metrics strip in baseline crop coordinates (viewport offset
/// by the box origin, clamped to crop bounds).
fn mask_footer_baseline(img: &mut RgbaImage, mask: manifest::MetricsMask, box_x: u32, box_y: u32) {
    let (x0, x1) = (mask.x0, mask.x1);
    let (y0, y1) = (mask.y0, mask.y1);
    let cx0 = x0.saturating_sub(box_x);
    let cx1 = x1.saturating_sub(box_x);
    let cy0 = y0.saturating_sub(box_y);
    let cy1 = y1.saturating_sub(box_y);
    for y in cy0..=cy1.min(img.height - 1) {
        for x in cx0..=cx1.min(img.width - 1) {
            let idx = ((y as usize) * img.width as usize + x as usize) * 4;
            img.data[idx] = 0;
            img.data[idx + 1] = 0;
            img.data[idx + 2] = 0;
            img.data[idx + 3] = 255;
        }
    }
}

/// Write all three report JSON files.
fn write_reports(dir: &Path, opts: &ParityOpts, results: &[ShotResult]) -> Result<(), String> {
    let shape_report: Vec<ShapeReportEntry> = results
        .iter()
        .map(|r| ShapeReportEntry {
            shot: r.shot.clone(),
            file: r.file.clone(),
            bounds: r.bounds,
            candidate: r.candidate.clone(),
            regions: r.shape_regions.clone(),
            pass: r.pass && r.capture_error.is_none(),
        })
        .collect();

    let raw_report: Vec<RawReportEntry> = results
        .iter()
        .map(|r| RawReportEntry {
            shot: r.shot.clone(),
            file: r.file.clone(),
            bounds: r.bounds,
            candidate: r.candidate.clone(),
            regions: r.raw_regions.clone(),
            pass: r.pass && r.capture_error.is_none(),
        })
        .collect();

    let ink_report: Vec<InkReportEntry> = results
        .iter()
        .map(|r| InkReportEntry {
            shot: r.shot.clone(),
            entries: r.ink_regions.clone(),
        })
        .collect();

    write_json(dir.join("shape-report.json"), &shape_report)?;
    write_json(dir.join("raw-report.json"), &raw_report)?;
    write_json(dir.join("ink-report.json"), &ink_report)?;

    if opts.accept_ink_residual {
        let acceptance: InkAcceptanceEntry = InkAcceptanceEntry {
            accepted: true,
            shots: results
                .iter()
                .map(|r| InkAcceptanceShot {
                    shot: r.shot.clone(),
                    ratio: r.ink_regions.first().map(|e| e.ratio).unwrap_or(0.0),
                })
                .collect(),
        };
        write_json(dir.join("ink-acceptance.json"), &acceptance)?;
    }

    Ok(())
}

/// JSON entry for `shape-report.json`.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ShapeReportEntry {
    shot: String,
    file: String,
    bounds: Rect,
    candidate: String,
    regions: Vec<ShapeRegionReport>,
    pass: bool,
}

/// JSON entry for `raw-report.json`.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RawReportEntry {
    shot: String,
    file: String,
    bounds: Rect,
    candidate: String,
    regions: Vec<RawRegionReport>,
    pass: bool,
}

/// JSON entry for `ink-report.json`.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct InkReportEntry {
    shot: String,
    entries: Vec<InkRegionReport>,
}

/// JSON entry for `ink-acceptance.json` (`--accept-ink-residual`).
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct InkAcceptanceEntry {
    accepted: bool,
    shots: Vec<InkAcceptanceShot>,
}

/// One shot's ink ratio in the acceptance log.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct InkAcceptanceShot {
    shot: String,
    ratio: f64,
}

fn write_json<T: serde::Serialize>(path: PathBuf, value: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| format!("serialize {}: {e}", path.display()))?;
    std::fs::write(&path, json).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(())
}

/// Which comparator produced the gate number the bundle recorded.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RecordedRule {
    /// `changed_pixels` with anti-aliasing excluded.
    Perceptual,
    /// `raw_changed_pixels` from the class map. Its bands are still measured with the
    /// perceptual rule, so the scored document labels which number is whose.
    ClassMap,
}

impl RecordedRule {
    /// The label a reader of the scored document needs beside the band numbers.
    fn label(self) -> &'static str {
        match self {
            RecordedRule::Perceptual => "perceptual",
            RecordedRule::ClassMap => "class-map gate, perceptual bands",
        }
    }
}

/// One side of a recorded mask pair.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Side {
    Oracle,
    Native,
}

/// The pair a scored run measures, and how it found it.
#[derive(Clone, Debug)]
pub struct MaskPair {
    pub oracle: PathBuf,
    pub native: PathBuf,
    /// The recorded region inside each named capture, for a bundle whose stored file
    /// holds more than the mask its record was made on.
    pub crop: Option<Rect>,
    /// How the pair was obtained, for the reader of the scored document.
    pub note: String,
}

/// What a scored run takes from the command line.
#[derive(Clone, Debug)]
pub struct ScoreOpts {
    /// One evidence bundle to score.
    pub bundle: Option<PathBuf>,
    /// Or every bundle under this directory.
    pub root: Option<PathBuf>,
    /// Masks named by the caller instead of found in the bundle.
    pub oracle: Option<PathBuf>,
    pub native: Option<PathBuf>,
    /// Where to write the scored document. Nothing is written without it, so a recorded
    /// bundle is never edited in place.
    pub out: Option<PathBuf>,
    /// The band ceiling the caller states. `None` takes the bundle's own, then the
    /// ratified one, so no comparison carries a literal.
    pub max_band_pct: Option<f64>,
    pub band_height: u32,
    pub max_shift: i32,
    pub tiers: usize,
}

impl Default for ScoreOpts {
    fn default() -> Self {
        Self {
            bundle: None,
            root: None,
            oracle: None,
            native: None,
            out: None,
            max_band_pct: None,
            band_height: bands::DEFAULT_BAND_HEIGHT,
            max_shift: bands::DEFAULT_MAX_SHIFT,
            tiers: bands::DEFAULT_TIERS,
        }
    }
}

/// One bundle after a scored run.
#[derive(Debug)]
pub struct Scored {
    pub surface: String,
    pub size: (u32, u32),
    pub pair: MaskPair,
    pub rule: RecordedRule,
    pub ceilings: Ceilings,
    pub verdict: Verdict,
    pub report: BandReport,
    pub document: Value,
}

/// Re-measure a pair and check it against the pixels the bundle recorded.
///
/// Fails when the numbers disagree, because then this is not the pair the recorded verdict
/// was made on, and scoring it would ratify an unrelated crop.
pub fn verify_pair(
    record: &manifest::BundleRecord,
    a: &RgbaImage,
    b: &RgbaImage,
) -> Result<RecordedRule, String> {
    if a.width != b.width || a.height != b.height {
        return Err(format!(
            "mask pair is {}x{} against {}x{}; a layout comparison needs both sides at one size",
            a.width, a.height, b.width, b.height
        ));
    }
    let total = u64::from(a.width) * u64::from(a.height);
    if total != record.total_pixels {
        return Err(format!(
            "pair is {total} px, diff.json records total_pixels {}",
            record.total_pixels
        ));
    }
    match record.recorded {
        manifest::RecordedMetrics::Perceptual {
            changed,
            ignored_aa,
        } => {
            let measured = perceptual::compare(a, b, DEFAULT_THRESHOLD, true)?;
            if (measured.changed_pixels, measured.ignored_aa_pixels) != (changed, ignored_aa) {
                return Err(format!(
                    "pair reproduces {} changed px with {} AA excluded, diff.json records \
                     {changed} with {ignored_aa}",
                    measured.changed_pixels, measured.ignored_aa_pixels
                ));
            }
            Ok(RecordedRule::Perceptual)
        }
        manifest::RecordedMetrics::ClassMap { raw } => {
            let region = RegionSpec {
                name: "full".to_string(),
                anchor: Anchor::Top,
                rows: a.height,
            };
            let observed = rawdiff::compare_anchored(a, b, &[region])?
                .first()
                .map(|report| report.diff_pixels)
                .unwrap_or(0);
            if observed != raw {
                return Err(format!(
                    "pair reproduces {observed} raw px, diff.json records {raw}"
                ));
            }
            Ok(RecordedRule::ClassMap)
        }
    }
}

/// The `bands` block: the ceiling applied, the alignment taken, the band that decided the
/// verdict, and every band behind it.
pub fn bands_block(report: &BandReport, verdict: &Verdict, ceilings: Ceilings) -> Value {
    let worst = verdict.worst;
    let worst_changed = report.worst_changed_band();
    json!({
        "ceiling_pct": ceilings.max_band_pct,
        "pass": verdict.band_pass,
        "threshold": report.threshold,
        "band_height": report.band_height,
        "max_shift": report.max_shift,
        "horizontal_dx": report.horizontal.best_dx,
        "horizontal_changed_pixels": report.horizontal.changed_pixels,
        "horizontal_residual_pixels": report.horizontal.residual_at_best_dx,
        "aligned_width": report.aligned_width,
        "worst_band_y0": worst.map(|band| band.y0),
        "worst_band_y1": worst.map(|band| band.y1),
        "worst_band_pct": worst.map(|band| band.residual_pct),
        "worst_band_changed_pixels": worst.map(|band| band.changed_pixels),
        "worst_band_residual_pixels": worst.map(|band| band.residual_pixels),
        "worst_band_total_pixels": worst.map(|band| band.total_pixels),
        "worst_changed_band_y0": worst_changed.map(|band| band.y0),
        "worst_changed_band_y1": worst_changed.map(|band| band.y1),
        "worst_changed_band_pct": worst_changed.map(|band| band.changed_pct),
        "report": report,
        "rule": format!(
            "band identity ceiling: {:.3} % of a band's pixels, on the residue left after the \
             whole-image horizontal alignment within +-{} px and that band's own shift",
            ceilings.max_band_pct, report.max_shift
        ),
    })
}

/// Fold a band result into the recorded document.
///
/// Every legacy key keeps its meaning. `pass` becomes the two-assertion verdict,
/// `thresholds.max_band_pct` records the ceiling this run applied, and `metrics` is left
/// alone, because the gate number stays the gate number. `bands.gate_rule` names the
/// comparator that number came from, since a class-map bundle and its bands are measured
/// by different rules.
pub fn scored_document(
    recorded: &Value,
    report: &BandReport,
    verdict: &Verdict,
    ceilings: Ceilings,
    pair: &MaskPair,
    rule: RecordedRule,
    ceiling_source: &str,
) -> Value {
    let mut block = bands_block(report, verdict, ceilings);
    if let Some(map) = block.as_object_mut() {
        map.insert("ceiling_source".to_string(), json!(ceiling_source));
        map.insert("oracle_mask".to_string(), json!(file_name(&pair.oracle)));
        map.insert("native_mask".to_string(), json!(file_name(&pair.native)));
        map.insert("pair_note".to_string(), json!(pair.note));
        map.insert("crop".to_string(), json!(pair.crop));
        map.insert("gate_rule".to_string(), json!(rule.label()));
    }
    let mut document = recorded.clone();
    if let Some(map) = document.as_object_mut() {
        map.insert("bands".to_string(), block);
        map.insert("pass".to_string(), json!(verdict.pass));
        let thresholds = map
            .entry("thresholds")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .expect("thresholds was written as an object");
        thresholds.insert("max_band_pct".to_string(), json!(ceilings.max_band_pct));
    }
    document
}

/// Score one evidence bundle against both ceilings.
pub fn score_bundle(bundle: &Path, opts: &ScoreOpts) -> Result<Scored, String> {
    let recorded = read_json(&bundle.join("diff.json"))?;
    let record = manifest::bundle_record(&recorded)?;
    let pair = match (&opts.oracle, &opts.native) {
        (Some(oracle), Some(native)) => MaskPair {
            oracle: oracle.clone(),
            native: native.clone(),
            crop: None,
            note: "named on the command line".to_string(),
        },
        (None, None) => select_masks(bundle, &recorded, &record)?,
        _ => {
            return Err(
                "--oracle-mask and --native-mask are both required, or both omitted so the \
                 bundle's own recorded pair is used"
                    .to_string(),
            )
        }
    };
    // A rebuilt pair names the full captures and carries the recorded region; a stored
    // mask is measured whole, so the common path copies nothing.
    let oracle_image = RgbaImage::load(&pair.oracle)?;
    let native_image = RgbaImage::load(&pair.native)?;
    let (a, b) = match pair.crop {
        Some(rect) => (oracle_image.crop(rect)?, native_image.crop(rect)?),
        None => (oracle_image, native_image),
    };
    let rule = verify_pair(&record, &a, &b)?;
    let report = bands::report(
        &a,
        &b,
        DEFAULT_THRESHOLD,
        opts.band_height,
        opts.max_shift,
        opts.tiers,
    )?;
    let ceilings = Ceilings {
        max_changed_pct: record.max_changed_pct,
        max_band_pct: opts
            .max_band_pct
            .or(record.max_band_pct)
            .unwrap_or(bands::RATIFIED_MAX_BAND_PCT),
    };
    // The percentage assertion reads the number the bundle's own gate was made on. A
    // class-map bundle records its mismatch count, which no other comparator reproduces.
    let gate_pct = bands::share(record.gate_pixels, record.total_pixels);
    let verdict = bands::decide(gate_pct, &report, ceilings);
    let document = scored_document(
        &recorded,
        &report,
        &verdict,
        ceilings,
        &pair,
        rule,
        ceiling_source(opts.max_band_pct.is_some(), record.max_band_pct.is_some()),
    );
    Ok(Scored {
        surface: record.surface,
        size: (a.width, a.height),
        pair,
        rule,
        ceilings,
        verdict,
        report,
        document,
    })
}

/// One name per bundle under a root, with either its scored result or the reason it was
/// refused.
pub type BundleResults = Vec<(String, Result<Scored, String>)>;

/// Score every bundle under a root, in name order.
///
/// A bundle that cannot be scored comes back as its refusal reason, never as a pass: an
/// unverifiable record is an evidence gap, and a gap is not parity.
pub fn score_root(root: &Path, opts: &ScoreOpts) -> Result<BundleResults, String> {
    let mut dirs = Vec::new();
    for entry in std::fs::read_dir(root).map_err(|e| format!("read {}: {e}", root.display()))? {
        let entry = entry.map_err(|e| format!("read {}: {e}", root.display()))?;
        if entry.path().join("diff.json").is_file() {
            dirs.push(entry.path());
        }
    }
    dirs.sort();
    Ok(dirs
        .into_iter()
        .map(|dir| {
            let name = dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let result = score_bundle(&dir, opts);
            (name, result)
        })
        .collect())
}

/// The mask pair the recorded verdict was made on.
///
/// Names only order the candidates; a pair is accepted when re-running the ratified
/// comparator returns the pixels the bundle recorded. That is what keeps a red control, or
/// a pane cropped at another size, from being scored as the surface. Where the bundle
/// stores no mask at that size, its own full captures are cropped at the geometry its note
/// records, and that rebuild is accepted only if it then reproduces the record.
fn select_masks(
    bundle: &Path,
    recorded: &Value,
    record: &manifest::BundleRecord,
) -> Result<MaskPair, String> {
    let mut oracle: Vec<(String, RgbaImage)> = Vec::new();
    let mut native: Vec<(String, RgbaImage)> = Vec::new();
    for name in mask_candidates(&list_pngs(bundle)?) {
        let path = bundle.join(&name);
        if png_size(&path).map(|(w, h)| u64::from(w) * u64::from(h)) != Some(record.total_pixels) {
            continue;
        }
        let image = RgbaImage::load(&path)?;
        match mask_side(&name) {
            Some(Side::Oracle) => oracle.push((name, image)),
            Some(Side::Native) => native.push((name, image)),
            None => {}
        }
    }
    for (oracle_name, a) in &oracle {
        for (native_name, b) in &native {
            if verify_pair(record, a, b).is_ok() {
                return Ok(MaskPair {
                    oracle: bundle.join(oracle_name),
                    native: bundle.join(native_name),
                    crop: None,
                    note: "stored masks, verified against the recorded pixels".to_string(),
                });
            }
        }
    }
    let detail = match (oracle.first(), native.first()) {
        (Some((a_name, a)), Some((b_name, b))) => {
            verify_pair(record, a, b).err().map_or(String::new(), |e| {
                format!("; {a_name} against {b_name} gives {e}")
            })
        }
        _ => "; no capture pair at this size on disk".to_string(),
    };
    rebuild_pair(bundle, recorded, record).ok_or_else(|| {
        format!(
            "no pair reproduces the recorded pixels ({} oracle-side and {} native-side \
             candidates at {} px{})",
            oracle.len(),
            native.len(),
            record.total_pixels,
            detail
        )
    })
}

/// The mask rebuilt from the bundle's full captures at the geometry its note records.
/// Only a rebuild that reproduces the recorded pixels is accepted.
fn rebuild_pair(
    bundle: &Path,
    recorded: &Value,
    record: &manifest::BundleRecord,
) -> Option<MaskPair> {
    let (x, y, w, h) = manifest::mask_regions(recorded)
        .into_iter()
        .find(|(_, _, w, h)| u64::from(*w) * u64::from(*h) == record.total_pixels)?;
    let full = |side: Side| -> Option<(String, RgbaImage)> {
        list_pngs(bundle)
            .ok()?
            .into_iter()
            .filter(|name| mask_side(name) == Some(side))
            .filter_map(|name| {
                let image = RgbaImage::load(&bundle.join(&name)).ok()?;
                (u64::from(image.width) * u64::from(image.height) > record.total_pixels)
                    .then_some((name, image))
            })
            .max_by_key(|(_, image)| u64::from(image.width) * u64::from(image.height))
    };
    let (oracle_name, oracle) = full(Side::Oracle)?;
    let (native_name, native) = full(Side::Native)?;
    let crop = Rect {
        x,
        y,
        width: w,
        height: h,
    };
    let a = oracle.crop(crop).ok()?;
    let b = native.crop(crop).ok()?;
    verify_pair(record, &a, &b).ok()?;
    Some(MaskPair {
        oracle: bundle.join(oracle_name),
        native: bundle.join(native_name),
        crop: Some(crop),
        note: format!("rebuilt from the bundle's full captures at x={x} y={y} w={w} h={h}"),
    })
}

/// PNG file names in a directory, sorted, so a re-score walks them the same way twice.
fn list_pngs(dir: &Path) -> Result<Vec<String>, String> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| format!("read {}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| format!("read {}: {e}", dir.display()))?;
        let name = entry.file_name().to_string_lossy().to_string();
        if name.to_ascii_lowercase().ends_with(".png") {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

/// Candidate order: the file a bundle names as its mask first, then the rest by name, so a
/// re-score takes the same pair on every machine. A red control is tried last.
fn mask_candidates(names: &[String]) -> Vec<String> {
    let rank = |name: &&String| -> u8 {
        if name_tokens(name).iter().any(|token| token == "red") {
            2
        } else if name.to_ascii_lowercase().contains("mask") {
            0
        } else {
            1
        }
    };
    let mut sorted = names.to_vec();
    sorted.sort_by_key(|name| (rank(&name), name.clone()));
    sorted
}

/// Which side of a recorded pair a stored capture is: bundles name the sides `oracle` and
/// `native`, the geometry-masked ones use `-o-` and `-n-`.
fn mask_side(name: &str) -> Option<Side> {
    let tokens = name_tokens(name);
    if tokens.iter().any(|token| token == "oracle" || token == "o") {
        return Some(Side::Oracle);
    }
    if tokens.iter().any(|token| token == "native" || token == "n") {
        return Some(Side::Native);
    }
    None
}

/// Lowercased name components, so a token decides a role rather than a substring.
fn name_tokens(name: &str) -> Vec<String> {
    name.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

/// Image size from the PNG header, without decoding the pixels: `IHDR` carries the width
/// at bytes 16..20 and the height at 20..24.
fn png_size(path: &Path) -> Option<(u32, u32)> {
    let mut head = [0u8; 24];
    let mut file = std::fs::File::open(path).ok()?;
    file.read_exact(&mut head).ok()?;
    if head[0..8] != *b"\x89PNG\r\n\x1a\n" || head[12..16] != *b"IHDR" {
        return None;
    }
    let number =
        |at: usize| u32::from_be_bytes([head[at], head[at + 1], head[at + 2], head[at + 3]]);
    Some((number(16), number(20)))
}

fn read_json(path: &Path) -> Result<Value, String> {
    let mut text = String::new();
    let mut file =
        std::fs::File::open(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    file.read_to_string(&mut text)
        .map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))
}

/// The name only, for a line the reader quotes.
fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string())
}

/// Which statement set the band ceiling. Recorded beside the number, so a receipt can show
/// the ceiling was stated rather than invented at the comparison.
fn ceiling_source(stated_by_caller: bool, stated_by_bundle: bool) -> &'static str {
    if stated_by_caller {
        "stated by the caller"
    } else if stated_by_bundle {
        "stated by the bundle"
    } else {
        "the ratified ceiling, which neither the caller nor the bundle overrode"
    }
}

/// One line per scored surface: both readings, the ceilings applied, and what they decided.
fn report_line(scored: &Scored) -> String {
    let worst = scored.verdict.worst;
    format!(
        "{:<8} {:>4}x{:<4}  gate {:>7.3}% of floor {:>7.3}%  worst band {:>4}..{:<4} residue \
         {:>7.3}% of ceiling {:>6.3}%  {}",
        scored.surface,
        scored.size.0,
        scored.size.1,
        scored.verdict.changed_pct,
        scored.ceilings.max_changed_pct,
        worst.map_or(0, |band| band.y0),
        worst.map_or(0, |band| band.y1),
        worst.map_or(0.0, |band| band.residual_pct),
        scored.ceilings.max_band_pct,
        verdict_tag(&scored.verdict),
    )
}

/// What the two assertions concluded, in the words a receipt quotes.
fn verdict_tag(verdict: &Verdict) -> &'static str {
    match (verdict.percent_pass, verdict.band_pass) {
        (true, true) => "PASS",
        (false, true) => "FAIL percentage floor",
        (true, false) => "FAIL band ceiling",
        (false, false) => "FAIL both ceilings",
    }
}

/// The per-band detail behind the ceiling: what differs, the shift found, what is left.
fn report_bands(report: &BandReport) {
    for band in &report.bands {
        println!(
            "    y {:>4}..{:<4} {:>7} px differ ({:>6.3}%), dy {:>3}, residue {:>7} px \
             ({:>6.3}%)",
            band.y0,
            band.y1,
            band.changed_pixels,
            band.changed_pct,
            band.best_dy,
            band.residual_at_best_dy,
            bands::share(band.residual_at_best_dy, band.total_pixels),
        );
    }
}

/// Print a batch run and give its exit code: 1 when any surface trips the band ceiling.
fn report_batch(rows: &BundleResults) -> i32 {
    let mut rejects: Vec<String> = Vec::new();
    let mut refused: Vec<&str> = Vec::new();
    for (name, row) in rows {
        match row {
            Ok(scored) => {
                println!("{}", report_line(scored));
                if !scored.verdict.band_pass {
                    rejects.push(name.clone());
                }
            }
            Err(reason) => {
                refused.push(name.as_str());
                println!("{:<8} refused: {reason}", name);
            }
        }
    }
    println!(
        "measured {}, band-ceiling rejects {}, refused {}",
        rows.len() - refused.len(),
        rejects.len(),
        refused.len()
    );
    if !rejects.is_empty() {
        println!("band-ceiling rejects: {}", rejects.join(", "));
    }
    if !refused.is_empty() {
        println!("refused: {}", refused.join(", "));
    }
    if rejects.is_empty() {
        return 0;
    }
    1
}

/// `xtask parity score --bundle <dir> [--oracle-mask <png> --native-mask <png>]
///                     [--max-band-pct <f>] [--band-height <px>] [--max-shift <px>]
///                     [--tiers <n>] [--out <json>]`
/// `xtask parity score --bundles <root> [--max-band-pct <f>] ...`
///
/// Re-measures the pair a bundle's verdict was made on, adds the band identity block to
/// its `diff.json`, and applies both ceilings. Exits 1 when the surface fails either one,
/// 2 when it cannot be scored at all. Nothing is written unless `--out` names a path.
pub fn cmd_score(args: &[String]) -> i32 {
    let opts = match parse_score_args(args) {
        Ok(opts) => opts,
        Err(code) => return code,
    };
    if let Some(root) = &opts.root {
        return match score_root(root, &opts) {
            Ok(rows) => report_batch(&rows),
            Err(e) => {
                eprintln!("xtask parity score: {e}");
                2
            }
        };
    }
    let bundle = match &opts.bundle {
        Some(bundle) => bundle.clone(),
        None => {
            eprintln!("xtask parity score: --bundle <dir> or --bundles <root> is required.");
            return 2;
        }
    };
    let scored = match score_bundle(&bundle, &opts) {
        Ok(scored) => scored,
        Err(e) => {
            eprintln!("xtask parity score: {e}");
            return 2;
        }
    };
    println!(
        "{} {}x{}: {} against {} ({}, gate rule {})",
        scored.surface,
        scored.size.0,
        scored.size.1,
        file_name(&scored.pair.oracle),
        file_name(&scored.pair.native),
        scored.pair.note,
        scored.rule.label(),
    );
    println!("{}", report_line(&scored));
    println!(
        "    horizontal alignment dx {}, {} px differ, {} remain on the shared {} px width",
        scored.report.horizontal.best_dx,
        scored.report.horizontal.changed_pixels,
        scored.report.horizontal.residual_at_best_dx,
        scored.report.aligned_width,
    );
    report_bands(&scored.report);
    if let Some(out) = &opts.out {
        if let Err(e) = write_json(out.clone(), &scored.document) {
            eprintln!("xtask parity score: {e}");
            return 2;
        }
    }
    if scored.verdict.pass {
        println!("{}: PASS", scored.surface);
        return 0;
    }
    let mut reasons = Vec::new();
    if !scored.verdict.percent_pass {
        reasons.push("percentage floor");
    }
    if !scored.verdict.band_pass {
        reasons.push("band identity ceiling");
    }
    eprintln!("{}: FAIL ({})", scored.surface, reasons.join(" + "));
    1
}

/// Parse the scored run's flags. `Err(code)` is a usage failure, already explained.
fn parse_score_args(args: &[String]) -> Result<ScoreOpts, i32> {
    let bundle = super::flag(args, "--bundle").map(PathBuf::from);
    let root = super::flag(args, "--bundles").map(PathBuf::from);
    if bundle.is_none() && root.is_none() {
        eprintln!(
            "xtask parity score: --bundle <dir> or --bundles <root> is required.\n\
             \x20 One bundle is scored against its recorded pair. A root scores every bundle\n\
             \x20 under it and names the band-ceiling rejects."
        );
        return Err(2);
    }
    if bundle.is_some() && root.is_some() {
        eprintln!("xtask parity score: --bundle and --bundles are alternatives, not both.");
        return Err(2);
    }
    let stated = |name: &str| -> Result<Option<f64>, i32> {
        match super::flag(args, name) {
            None => Ok(None),
            Some(value) => value.parse::<f64>().map(Some).map_err(|_| {
                eprintln!("xtask parity score: {name} must be a number, got `{value}`");
                2
            }),
        }
    };
    let reject = |name: &str, value: f64, need: &str| -> i32 {
        eprintln!("xtask parity score: {name} {need}, got {value}");
        2
    };
    let max_band_pct = match stated("--max-band-pct")? {
        None => None,
        Some(value) if value >= 0.0 => Some(value),
        Some(value) => return Err(reject("--max-band-pct", value, "must not be negative")),
    };
    let band_height = match stated("--band-height")? {
        None => bands::DEFAULT_BAND_HEIGHT,
        Some(value) if value >= 1.0 => value as u32,
        Some(value) => return Err(reject("--band-height", value, "must be at least 1")),
    };
    let max_shift = match stated("--max-shift")? {
        None => bands::DEFAULT_MAX_SHIFT,
        Some(value) if value >= 0.0 => value as i32,
        Some(value) => return Err(reject("--max-shift", value, "must not be negative")),
    };
    let tiers = match stated("--tiers")? {
        None => bands::DEFAULT_TIERS,
        Some(value) if value >= 1.0 => value as usize,
        Some(value) => return Err(reject("--tiers", value, "must be at least 1")),
    };
    Ok(ScoreOpts {
        bundle,
        root,
        oracle: super::flag(args, "--oracle-mask").map(PathBuf::from),
        native: super::flag(args, "--native-mask").map(PathBuf::from),
        out: super::flag(args, "--out").map(PathBuf::from),
        max_band_pct,
        band_height,
        max_shift,
        tiers,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        bands, file_name, mask_candidates, mask_side, perceptual, png_size, read_json,
        report_batch, report_line, resolve_mask, score_bundle, score_root, write_json,
        RecordedRule, ScoreOpts, Side, LEGACY_METRICS_MASK,
    };
    use super::{Path, PathBuf, Rect, RgbaImage, DEFAULT_THRESHOLD};
    use crate::parity::manifest::MetricsMask;
    use serde_json::{json, Value};

    fn declared(w: u32, h: u32) -> MetricsMask {
        MetricsMask {
            x0: 1,
            y0: 2,
            x1: 3,
            y1: 4,
            viewport_width: w,
            viewport_height: h,
        }
    }

    /// The built-in mask still applies at the size it was measured on, so the
    /// existing 1366x820 golden master keeps behaving exactly as before.
    #[test]
    fn legacy_mask_applies_at_its_own_size() {
        let m = resolve_mask(None, 1366, 820).expect("legacy size accepted");
        assert_eq!((m.x0, m.y0, m.x1, m.y1), (620, 801, 994, 816));
    }

    /// The trap this guard exists for: a different viewport must not inherit
    /// coordinates measured elsewhere, because the diff would still read clean
    /// while blanking the wrong pixels.
    #[test]
    fn legacy_mask_is_refused_at_another_size() {
        let err = resolve_mask(None, 1920, 1020).expect_err("must refuse");
        assert!(
            err.contains("1366x820"),
            "names the calibration size: {err}"
        );
        assert!(err.contains("1920x1020"), "names the candidate size: {err}");
        assert!(err.contains("metricsMask"), "says how to fix it: {err}");
    }

    /// A manifest-declared mask is used at its own size and refused at any
    /// other, so declaring a mask is not a way to bypass the check.
    #[test]
    fn declared_mask_must_match_the_candidate() {
        let m = resolve_mask(Some(declared(1920, 1020)), 1920, 1020).expect("matching size");
        assert_eq!((m.x0, m.y0, m.x1, m.y1), (1, 2, 3, 4));

        let err = resolve_mask(Some(declared(1920, 1020)), 1366, 820).expect_err("must refuse");
        assert!(
            err.contains("manifest metricsMask"),
            "blames the manifest: {err}"
        );
    }

    /// The legacy constant really is the documented 1366x820 calibration; a
    /// silent edit to it would otherwise change every masked comparison.
    #[test]
    fn legacy_constant_is_pinned() {
        assert_eq!(LEGACY_METRICS_MASK.viewport_width, 1366);
        assert_eq!(LEGACY_METRICS_MASK.viewport_height, 820);
    }
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
        let measured =
            perceptual::compare(oracle, native, DEFAULT_THRESHOLD, true).expect("measure");
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
}
