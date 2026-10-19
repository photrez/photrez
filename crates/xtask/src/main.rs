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
        Some("parity") => {
            if args.get(1).map(String::as_str) == Some("cross-check") {
                cmd_cross_check(&args[2..])
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
         \x20 xtask parity [--baseline <dir>] [--shot <id|prefix>]... [--candidate <dir|png>]\n\
         \x20              [--report <dir>] [--mask-metrics] [--accept-ink-residual]\n\
         \x20 xtask parity cross-check --case shape|diff|zoom|measure --a <png> --b <png>\n\
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

fn cmd_capture(args: &[String]) -> i32 {
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
