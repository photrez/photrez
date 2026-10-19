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
}

/// The nondeterministic footer metrics strip, in viewport coordinates.
/// x 620..=994, y 801..=816 (measured run-to-run variance, per baseline README).
const METRICS_FOOTER_X: (u32, u32) = (620, 994);
const METRICS_FOOTER_Y: (u32, u32) = (801, 816);

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
        let res = process_shot(shot, opts, &cand_dir, &sbs_dir, candidate_is_dir);
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
        mask_footer(&mut candidate_crop, 0, 0);
        let bx = bounds.x;
        let by = bounds.y;
        mask_footer_baseline(&mut baseline_crop, bx, by);
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
fn mask_footer(img: &mut RgbaImage, ox: u32, oy: u32) {
    let (x0, x1) = METRICS_FOOTER_X;
    let (y0, y1) = METRICS_FOOTER_Y;
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
fn mask_footer_baseline(img: &mut RgbaImage, box_x: u32, box_y: u32) {
    let (x0, x1) = METRICS_FOOTER_X;
    let (y0, y1) = METRICS_FOOTER_Y;
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
