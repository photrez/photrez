//! The scored run: re-measure one evidence bundle's own mask pair and apply the
//! two ratified assertions - the whole-surface percentage floor and the band
//! identity ceiling from [`super::bands`]. A percentage inside the floor is not
//! colour evidence, so a surface whose worst aligned band exceeds the ceiling is
//! a FAIL there even when its percentage passes.
//!
//! The pair must reproduce the numbers the bundle recorded before it is scored,
//! so a wrong crop cannot be read as a clean surface. Ceilings come from the
//! caller or the bundle, never from a literal in the comparison.

use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use super::bands::{self, BandReport, Ceilings, Verdict};
use super::manifest;
use super::perceptual::{self, DEFAULT_THRESHOLD};
use super::pngio::RgbaImage;
use super::rawdiff;
use super::run::write_json;
use super::{Anchor, Rect, RegionSpec};

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
/// by different rules. Fails when the bundle records `thresholds` as anything but an
/// object, because the ceiling this run applied would have nowhere to go.
pub fn scored_document(
    recorded: &Value,
    report: &BandReport,
    verdict: &Verdict,
    ceilings: Ceilings,
    pair: &MaskPair,
    rule: RecordedRule,
    ceiling_source: &str,
) -> Result<Value, String> {
    let mut block = bands_block(report, verdict, ceilings);
    if let Some(map) = block.as_object_mut() {
        map.insert("ceiling_source".to_string(), json!(ceiling_source));
        map.insert("reference_mask".to_string(), json!(file_name(&pair.oracle)));
        map.insert("native_mask".to_string(), json!(file_name(&pair.native)));
        map.insert("pair_note".to_string(), json!(pair.note));
        map.insert("crop".to_string(), json!(pair.crop));
        map.insert("gate_rule".to_string(), json!(rule.label()));
    }
    let mut document = recorded.clone();
    if let Some(map) = document.as_object_mut() {
        map.insert("bands".to_string(), block);
        map.insert("pass".to_string(), json!(verdict.pass));
        let entry = map.entry("thresholds").or_insert_with(|| json!({}));
        match entry {
            Value::Object(thresholds) => {
                thresholds.insert("max_band_pct".to_string(), json!(ceilings.max_band_pct));
            }
            // A bundle is hand-edited evidence, so its thresholds block can hold any JSON
            // value. Naming the recorded value keeps this a readable gate failure instead
            // of a panic inside the run.
            other => {
                return Err(format!(
                    "thresholds is {other}, not an object, so the applied band ceiling has \
                     nowhere to go"
                ));
            }
        }
    }
    Ok(document)
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
                "--reference-mask and --native-mask are both required, or both omitted so the \
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
    )?;
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
/// unverifiable record is an evidence gap, and a gap is not a verdict.
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

/// `xtask visual score --bundle <dir> [--reference-mask <png> --native-mask <png>]
///                     [--max-band-pct <f>] [--band-height <px>] [--max-shift <px>]
///                     [--tiers <n>] [--out <json>]`
/// `xtask visual score --bundles <root> [--max-band-pct <f>] ...`
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
                eprintln!("xtask visual score: {e}");
                2
            }
        };
    }
    let bundle = match &opts.bundle {
        Some(bundle) => bundle.clone(),
        None => {
            eprintln!("xtask visual score: --bundle <dir> or --bundles <root> is required.");
            return 2;
        }
    };
    let scored = match score_bundle(&bundle, &opts) {
        Ok(scored) => scored,
        Err(e) => {
            eprintln!("xtask visual score: {e}");
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
            eprintln!("xtask visual score: {e}");
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
            "xtask visual score: --bundle <dir> or --bundles <root> is required.\n\
             \x20 One bundle is scored against its recorded pair. A root scores every bundle\n\
             \x20 under it and names the band-ceiling rejects."
        );
        return Err(2);
    }
    if bundle.is_some() && root.is_some() {
        eprintln!("xtask visual score: --bundle and --bundles are alternatives, not both.");
        return Err(2);
    }
    let stated = |name: &str| -> Result<Option<f64>, i32> {
        match super::flag(args, name) {
            None => Ok(None),
            Some(value) => value.parse::<f64>().map(Some).map_err(|_| {
                eprintln!("xtask visual score: {name} must be a number, got `{value}`");
                2
            }),
        }
    };
    let reject = |name: &str, value: f64, need: &str| -> i32 {
        eprintln!("xtask visual score: {name} {need}, got {value}");
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
        oracle: super::flag(args, "--reference-mask").map(PathBuf::from),
        native: super::flag(args, "--native-mask").map(PathBuf::from),
        out: super::flag(args, "--out").map(PathBuf::from),
        max_band_pct,
        band_height,
        max_shift,
        tiers,
    })
}

#[cfg(test)]
mod tests;
