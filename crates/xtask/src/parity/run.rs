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
//! The scored run is a different gate and lives in [`super::bundle`].
//!
//! Outputs into `--report` (default `target/xtask-parity/`):
//! `shape-report.json` (array over compared shots, the shape verdict surface),
//! `raw-report.json`, `ink-report.json`, and a stdout table. The baseline
//! directory is never written to.
//!
//! Candidate resolution per shot: `--candidate <dir>` -> `<dir>/<id>.png`;
//! `--candidate <file.png>` -> that file (only valid with exactly one
//! selected shot); otherwise a fresh capture via `capture::capture`.

use std::path::{Path, PathBuf};

use super::manifest::{self, Shot};
use super::pngio::RgbaImage;
use super::rawdiff;
use super::shapediff;
use super::zoom;
use super::Rect;
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
    /// A percent forwarded to every fresh capture, so a run can render at the
    /// scale a reference frame was taken at instead of the live fit.
    pub zoom: Option<String>,
    /// A page origin forwarded to every fresh capture, for reference frames
    /// whose camera was placed before the chrome changed.
    pub pan: Option<String>,
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

    // A FAIL is only interpretable against a known baseline revision, so the run
    // names it before any verdict. These baselines are the predecessor
    // revision's pixels, not the pinned reference's, and a row that fails is a
    // question about that difference rather than a defect verdict on this build.
    match (&manifest.source_commit, &manifest.captured_at) {
        (Some(commit), Some(captured)) => println!(
            "baselines: source commit {} captured {}",
            commit.get(..8).unwrap_or(commit),
            captured
        ),
        _ => println!(
            "baselines: the manifest records no source commit or capture time, so \
             a FAIL here cannot be read as a defect against the current revision"
        ),
    }

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
        expect_client: None,
        open: None,
        zoom: opts.zoom.clone(),
        pan: opts.pan.clone(),
        hover: None,
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

pub(crate) fn write_json<T: serde::Serialize>(path: PathBuf, value: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| format!("serialize {}: {e}", path.display()))?;
    std::fs::write(&path, json).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::{resolve_mask, LEGACY_METRICS_MASK};
    use crate::parity::manifest::MetricsMask;

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
}
