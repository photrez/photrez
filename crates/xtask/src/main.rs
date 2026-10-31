//! Workspace automation for the Photrez native port.
//!
//! Commands:
//!
//! ```text
//! xtask capture --shot <id> --out <png> [--exe <path>] [--wait-ms <n>]
//!     Launch the built photrez app rendering one baseline state, wait for its
//!     window, capture the client area to a PNG, and exit. One process per
//!     shot; no cross-shot state.
//!
//! xtask parity [--baseline <dir>] [--shot <id|prefix>]... [--candidate <dir|png>]
//!              [--report <dir>] [--mask-metrics] [--accept-ink-residual]
//!     Compare captured shots against the golden-master baselines. Without
//!     --candidate each shot is captured first. Exits 0 only when every
//!     compared shot is green (see parity::run for the verdict rule).
//!
//! xtask parity cross-check --case shape|diff|zoom|measure --a <png> --b <png>
//!     Re-runs the ported tooling on the frozen fixture pair and checks the
//!     numbers against the recorded reports.
//! ```
//!
//! Both `parity` commands are read-only with respect to the repository: reports
//! go to `--report` (default `target/xtask-parity`), never into the baseline
//! directory.

mod capture;
mod dev;
mod parity;

use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        None | Some("-h") | Some("--help") => {
            print_help();
            0
        }
        Some("capture") => cmd_capture(&args[1..]),
        Some("dev") => dev::run(&args[1..]),
        Some("parity") => {
            if args.get(1).map(String::as_str) == Some("cross-check") {
                cmd_cross_check(&args[2..])
            } else if args.get(1).map(String::as_str) == Some("inventory") {
                cmd_parity_inventory(&args[2..])
            } else if args.get(1).map(String::as_str) == Some("surfaces") {
                parity::surfaces::cmd(&args[2..])
            } else if args.get(1).map(String::as_str) == Some("strings") {
                parity::strings::cmd(&args[2..])
            } else if args.get(1).map(String::as_str) == Some("coverage") {
                cmd_parity_coverage(&args[2..])
            } else if args.get(1).map(String::as_str) == Some("diff") {
                cmd_parity_diff(&args[2..])
            } else if args.get(1).map(String::as_str) == Some("perceptual-diff") {
                cmd_parity_perceptual_diff(&args[2..])
            } else {
                cmd_parity(&args[1..])
            }
        }
        Some(other) => {
            eprintln!("xtask: unknown command `{other}`\n");
            print_help();
            2
        }
    };
    std::process::exit(code);
}

fn print_help() {
    println!(
        "xtask - workspace automation for the Photrez native port\n\
         \n\
         USAGE:\n\
         \x20 xtask capture --shot <id> --out <png> [--exe <path>] [--wait-ms <n>]\n\
         \x20 xtask dev [-- <app args>]\n\
         \x20     Build and run the app with the gnu linker directory on the\n\
         \x20     child PATH (the link chain needs it for ld's DLLs and for\n\
         \x20     windres' bare gcc; plain `cargo run` works when the directory\n\
         \x20     is already on PATH).\n\
         \x20 xtask parity [--baseline <dir>] [--shot <id|prefix>]... [--candidate <dir|png>]\n\
         \x20              [--report <dir>] [--mask-metrics] [--accept-ink-residual]\n\
         \x20 xtask parity cross-check --case shape|diff|zoom|measure --a <png> --b <png>\n\
         \x20 xtask parity diff --a <png> --b <png> [--out <json>] [--max-diff <pct>]\n\
         \n\
         Defaults: --baseline crates/photrez/tests/baseline, --report target/xtask-parity."
    );
}

/// First value of `--flag <value>`; `None` when the flag is absent or dangling.
fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == name {
            return it.next().map(String::as_str);
        }
    }
    None
}

/// Every value of a repeatable `--flag <value>` occurrence, in order.
fn flag_all<'a>(args: &'a [String], name: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == name {
            if let Some(v) = it.next() {
                out.push(v.as_str());
            }
        }
    }
    out
}

fn has_flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

/// `xtask parity inventory [--oracle-root <dir>] [--head-root <dir>] [--out <csv>]`
/// — regenerate the parity inventory, validating against the oracle tree.
fn cmd_parity_inventory(args: &[String]) -> i32 {
    let default_out = std::path::PathBuf::from(".slim/deepwork/parity-inventory.csv");
    let opts = parity::inventory::InventoryOpts {
        oracle_root: flag(args, "--oracle-root")
            .map(PathBuf::from)
            .unwrap_or_default(),
        head_root: flag(args, "--head-root")
            .map(PathBuf::from)
            .unwrap_or_default(),
        out: flag(args, "--out")
            .map(PathBuf::from)
            .unwrap_or(default_out),
    };
    parity::inventory::run(&opts)
}

/// `xtask parity perceptual-diff --a <png> --b <png> [--threshold <t>] [--ignore-aa] [--out <json>]`
///
/// pixelmatch-style YIQ distance with optional anti-aliasing exclusion. The
/// threshold is explicit (default 0.1, pixelmatch's default); nothing here
/// ratifies a tolerance.
fn cmd_parity_perceptual_diff(args: &[String]) -> i32 {
    let (Some(a), Some(b)) = (flag(args, "--a"), flag(args, "--b")) else {
        eprintln!("xtask parity perceptual-diff: --a <png> and --b <png> are required");
        return 2;
    };
    let threshold: f64 = match flag(args, "--threshold") {
        None => parity::perceptual::DEFAULT_THRESHOLD,
        Some(v) => match v.parse() {
            Ok(n) => n,
            Err(_) => {
                eprintln!("xtask parity perceptual-diff: --threshold must be a number, got `{v}`");
                return 2;
            }
        },
    };
    let ignore_aa = args.iter().any(|arg| arg == "--ignore-aa");
    let max_diff: Option<f64> = match flag(args, "--max") {
        None => None,
        Some(v) => match v.parse() {
            Ok(n) => Some(n),
            Err(_) => {
                eprintln!("xtask parity perceptual-diff: --max must be a number, got `{v}`");
                return 2;
            }
        },
    };
    let img_a = match parity::pngio::RgbaImage::load(std::path::Path::new(a)) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("xtask parity perceptual-diff: {e}");
            return 2;
        }
    };
    let img_b = match parity::pngio::RgbaImage::load(std::path::Path::new(b)) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("xtask parity perceptual-diff: {e}");
            return 2;
        }
    };
    let report = match parity::perceptual::compare(&img_a, &img_b, threshold, ignore_aa) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("xtask parity perceptual-diff: {e}");
            return 1;
        }
    };
    println!(
        "perceptual diff: {:.4}% ({} px of {}, {} AA px ignored, threshold {})",
        report.changed_pct,
        report.changed_pixels,
        report.total_pixels,
        report.ignored_aa_pixels,
        report.threshold
    );
    if let Some(max) = max_diff {
        if report.changed_pct > max {
            eprintln!(
                "xtask parity perceptual-diff: FAIL {:.4}% > {max:.4}%",
                report.changed_pct
            );
            return 1;
        }
    }
    if let Some(out) = flag(args, "--out") {
        let payload = serde_json::json!({
            "a": a,
            "b": b,
            "rule": "pixelmatch YIQ distance with anti-aliasing exclusion (industry-standard screenshot comparator)",
            "report": report,
            "max": max_diff,
        });
        if let Err(e) = std::fs::write(out, serde_json::to_string_pretty(&payload).unwrap()) {
            eprintln!("xtask parity perceptual-diff: writing {out}: {e}");
            return 2;
        }
    }
    0
}

/// `xtask parity diff --a <png> --b <png> [--out <json>] [--max-diff <pct>]`
///
/// Whole-image pixel diff over the same class-map the shot comparison uses.
/// The threshold is explicit and printed: nothing here ratifies a tolerance,
/// and the default (0.000%) is the strictest possible verdict.
fn cmd_parity_diff(args: &[String]) -> i32 {
    let (Some(a), Some(b)) = (flag(args, "--a"), flag(args, "--b")) else {
        eprintln!("xtask parity diff: --a <png> and --b <png> are required");
        return 2;
    };
    let max_diff: f64 = match flag(args, "--max-diff") {
        None => 0.0,
        Some(v) => match v.parse() {
            Ok(n) => n,
            Err(_) => {
                eprintln!("xtask parity diff: --max-diff must be a number, got `{v}`");
                return 2;
            }
        },
    };
    let img_a = match parity::pngio::RgbaImage::load(std::path::Path::new(a)) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("xtask parity diff: {e}");
            return 2;
        }
    };
    let img_b = match parity::pngio::RgbaImage::load(std::path::Path::new(b)) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("xtask parity diff: {e}");
            return 2;
        }
    };
    if img_a.width != img_b.width || img_a.height != img_b.height {
        eprintln!(
            "parity diff: size mismatch: {}x{} vs {}x{} - a layout comparison needs both captures at the same size",
            img_a.width, img_a.height, img_b.width, img_b.height
        );
        return 1;
    }
    let region = parity::RegionSpec {
        name: "full".to_string(),
        anchor: parity::Anchor::Top,
        rows: img_a.height,
    };
    let report = match parity::rawdiff::compare_anchored(&img_a, &img_b, &[region]) {
        Ok(mut v) => v.remove(0),
        Err(e) => {
            eprintln!("parity diff: {e}");
            return 2;
        }
    };
    println!(
        "diff: {:.4}% ({} px of {}), max channel delta {} at {},{}",
        report.diff_pct,
        report.diff_pixels,
        u64::from(img_a.width) * u64::from(img_a.height),
        report.max_channel_delta,
        report.max_at[0],
        report.max_at[1]
    );
    if let Some(out) = flag(args, "--out") {
        let payload = serde_json::json!({
            "a": a,
            "b": b,
            "max_diff": max_diff,
            "region": report,
        });
        if let Err(e) = std::fs::write(
            out,
            serde_json::to_string_pretty(&payload).unwrap_or_default(),
        ) {
            eprintln!("xtask parity diff: write {out}: {e}");
            return 2;
        }
    }
    if report.diff_pct > max_diff {
        println!(
            "parity diff: FAIL ({:.4}% > max-diff {:.4}%)",
            report.diff_pct, max_diff
        );
        1
    } else {
        println!(
            "parity diff: PASS ({:.4}% <= max-diff {:.4}%)",
            report.diff_pct, max_diff
        );
        0
    }
}

fn cmd_capture(args: &[String]) -> i32 {
    if let Some(target) = flag(args, "--window") {
        let Some(out) = flag(args, "--out") else {
            eprintln!("xtask capture --window: --out <png> is required");
            return 2;
        };
        let spec = capture::window::WindowSpec {
            target: target.to_string(),
            maximize: has_flag(args, "--maximize"),
            wait_ms: 10_000,
            out: PathBuf::from(out),
        };
        return match capture::window::capture_window(&spec) {
            Ok(p) => {
                println!("captured -> {}", p.display());
                0
            }
            Err(e) => {
                eprintln!("window capture failed: {e}");
                1
            }
        };
    }
    let Some(out) = flag(args, "--out") else {
        eprintln!("xtask capture: --out <png> is required");
        return 2;
    };
    let wait_ms = match flag(args, "--wait-ms") {
        Some(v) => match v.parse::<u64>() {
            Ok(n) => n,
            Err(_) => {
                eprintln!("xtask capture: --wait-ms must be an integer, got `{v}`");
                return 2;
            }
        },
        None => 30_000,
    };
    let spec = capture::CaptureSpec {
        shot: flag(args, "--shot").map(str::to_owned),
        exe: flag(args, "--exe").map(PathBuf::from),
        out: PathBuf::from(out),
        wait_ms,
    };
    match capture::capture(&spec) {
        Ok(p) => {
            println!("captured -> {}", p.display());
            0
        }
        Err(e) => {
            eprintln!("capture failed: {e}");
            1
        }
    }
}

fn cmd_parity(args: &[String]) -> i32 {
    let opts = parity::run::ParityOpts {
        baseline: PathBuf::from(
            flag(args, "--baseline").unwrap_or("crates/photrez/tests/baseline"),
        ),
        shots: flag_all(args, "--shot")
            .into_iter()
            .map(str::to_owned)
            .collect(),
        candidate: flag(args, "--candidate").map(PathBuf::from),
        report_dir: flag(args, "--report")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("target/xtask-parity")),
        mask_metrics: has_flag(args, "--mask-metrics"),
        accept_ink_residual: has_flag(args, "--accept-ink-residual"),
    };
    if opts.shots.is_empty() {
        eprintln!("xtask parity: at least one --shot <id|prefix> is required");
        return 2;
    }
    match parity::run::run_parity(&opts) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("parity failed: {e}");
            1
        }
    }
}

fn cmd_parity_coverage(args: &[String]) -> i32 {
    let inventory =
        Path::new(flag(args, "--inventory").unwrap_or(".slim/deepwork/parity-inventory.csv"));
    let surfaces =
        Path::new(flag(args, "--surfaces").unwrap_or(".slim/deepwork/parity-surfaces.csv"));
    match parity::coverage::run(inventory, surfaces) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("parity coverage: {error}");
            1
        }
    }
}

fn cmd_cross_check(args: &[String]) -> i32 {
    let (Some(case), Some(a), Some(b)) =
        (flag(args, "--case"), flag(args, "--a"), flag(args, "--b"))
    else {
        eprintln!("xtask parity cross-check: --case <shape|diff|zoom|measure> --a <png> --b <png> are required");
        return 2;
    };
    match parity::crosscheck::run(case, Path::new(a), Path::new(b)) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("cross-check failed: {e}");
            1
        }
    }
}
