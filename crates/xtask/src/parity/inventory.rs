//! Parity inventory generator and validator.
//!
//! Replaces the frozen Python generator (`parity_inventory.py`) with the same
//! classification tables (see `inventory_rules.rs`) plus the two things the
//! Python version could not do: read a different tree (`--oracle-root`, the
//! read-only 2026-08-16 worktree) and measure the aggregate reference rows
//! (Tauri `.rs` files, frozen tests) from that tree instead of carrying
//! HEAD-era numbers.
//!
//! Row kinds: `F` = oracle production file (the parity target), `O` =
//! out-of-target file (exists only in the current tree; kept so the inventory
//! stays a complete map of both trees), `R` = reference artifact.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::csv;
use super::inventory_rules::{OVERRIDES, PREFIX_RULES, REFERENCE_FILES};

const STATUSES: &[&str] = &[
    "REFERENCE",
    "NA",
    "OUT-OF-TARGET",
    "DEAD",
    "MISSING",
    "STATIC",
    "WIRED",
    "INTERACTIVE",
    "VERIFIED",
];

/// Production-file walker: `.ts`/`.tsx` under `<root>/src`, excluding test
/// files, `__tests__`/`node_modules`/`test` directories, ambient `.d.ts`
/// declarations and dotfiles — the same exclusions the frozen census uses to
/// reach its 208 production files. Returns slash-separated paths relative to
/// `<root>/src`, sorted.
pub(super) fn production_files(root: &Path) -> Vec<String> {
    let src = root.join("src");
    let mut out = Vec::new();
    walk(&src, &src, &mut out);
    out.sort();
    out
}

fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy().into_owned();
        let path = entry.path();
        if path.is_dir() {
            if name == "__tests__" || name == "node_modules" || name == "test" {
                continue;
            }
            walk(&path, base, out);
        } else if name.ends_with(".ts") || name.ends_with(".tsx") {
            if name.starts_with('.')
                || name.ends_with(".d.ts")
                || name.contains(".test.")
                || name.contains(".spec.")
            {
                continue;
            }
            if let Ok(rel) = path.strip_prefix(base) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
}

/// Files matching the test vocabulary (`*.test.*` / `*.spec.*`), counted for
/// the aggregate frozen-test reference row. Name-pattern only, matching the
/// frozen census command (`grep -c '\.(test|spec)\.(ts|tsx|js)$'` over the
/// tracked tree). Walks the whole `apps/desktop` tree (the oracle's frozen
/// corpus includes `e2e/`), skipping dependency and build-output directories.
fn count_test_files(root: &Path) -> usize {
    let mut count = 0;
    walk_all_skipping(root, &["node_modules", "pkg"], &mut |path| {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
        let Some(name) = name else { return };
        let is_ts = name.ends_with(".ts") || name.ends_with(".tsx") || name.ends_with(".js");
        if is_ts && (name.contains(".test.") || name.contains(".spec.")) {
            count += 1;
        }
    });
    count
}

fn count_rs_files(dir: &Path) -> usize {
    let mut count = 0;
    walk_all_skipping(dir, &[], &mut |path| {
        if path.extension().map(|e| e == "rs").unwrap_or(false) {
            count += 1;
        }
    });
    count
}

fn walk_all_skipping(dir: &Path, skip: &[&str], f: &mut dyn FnMut(&Path)) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            if name.as_deref().map(|n| skip.contains(&n)).unwrap_or(false) {
                continue;
            }
            walk_all_skipping(&path, skip, f);
        } else {
            f(&path);
        }
    }
}

/// Short oracle commit recorded into the CSV header comment, or `unknown`
/// when the tree is not a git checkout.
fn oracle_commit(root: &Path) -> String {
    let output = std::process::Command::new("git")
        .args(["-C", root.to_string_lossy().as_ref()])
        .args(["rev-parse", "--short=8", "HEAD"])
        .output();
    match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => "unknown".to_string(),
    }
}

/// Exact-path override, then directory prefix, then UNMAPPED. The optional
/// `extra` table carries classifications decided during the oracle re-baseline
/// that the frozen Python tables could not know about.
fn classify(
    rel: &str,
    extra: &[(&str, &str, &str, &str, &str, &str)],
) -> (String, String, String, String, String) {
    for (path, family, target, status, evidence, notes) in OVERRIDES {
        if *path == rel {
            return (
                family.to_string(),
                target.to_string(),
                status.to_string(),
                evidence.to_string(),
                notes.to_string(),
            );
        }
    }
    for (path, family, target, status, evidence, notes) in extra {
        if *path == rel {
            return (
                family.to_string(),
                target.to_string(),
                status.to_string(),
                evidence.to_string(),
                notes.to_string(),
            );
        }
    }
    for (prefix, family, target, status, note) in PREFIX_RULES {
        if rel == *prefix || rel.starts_with(prefix) {
            return (
                family.to_string(),
                target.to_string(),
                status.to_string(),
                String::new(),
                note.to_string(),
            );
        }
    }
    (
        "UNMAPPED".to_string(),
        "-".to_string(),
        "UNMAPPED".to_string(),
        String::new(),
        "no rule; map before claiming completeness".to_string(),
    )
}

pub struct InventoryOpts {
    /// Tree whose `src/` defines the parity target (the oracle worktree's
    /// `apps/desktop`). Defaults to the repository's own `apps/desktop`.
    pub oracle_root: PathBuf,
    /// Tree scanned for out-of-target files. Defaults to the repository's
    /// own `apps/desktop`.
    pub head_root: PathBuf,
    /// Output CSV path.
    pub out: PathBuf,
}

/// Generate the inventory CSV and run the validator over it. Returns the
/// process exit code.
pub fn run(opts: &InventoryOpts) -> i32 {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .expect("xtask sits two levels below the repository root");

    let oracle_root = if opts.oracle_root.as_os_str().is_empty() {
        repo_root.join("apps/desktop")
    } else {
        opts.oracle_root.clone()
    };
    let head_root = if opts.head_root.as_os_str().is_empty() {
        repo_root.join("apps/desktop")
    } else {
        opts.head_root.clone()
    };

    let commit = oracle_commit(&oracle_root);
    let oracle_files = production_files(&oracle_root);
    let head_files = production_files(&head_root);
    let oracle_set: BTreeSet<&String> = oracle_files.iter().collect();

    let extra: &[(&str, &str, &str, &str, &str, &str)] = &[
        // Re-baseline classifications for the three oracle files that no
        // October-era rule could see (they are absent at HEAD). Decisions
        // recorded in the 2026-10-28 re-baseline notes.
        (
            "components/editor/paintStrokeRenderer.ts",
            "brush",
            "app/input.rs, crates/core paint",
            "WIRED",
            "",
            "dab-to-patch rendering; natively covered by the core paint pipeline",
        ),
        (
            "components/editor/transformUndoRedo.ts",
            "transform",
            "app/transform_move.rs, crates/core history",
            "WIRED",
            "transform_drag_previews_before_commit_and_undo_restores",
            "transform coalescing handled by core history entries",
        ),
        (
            "engine/paintHistoryBudget.ts",
            "brush",
            "crates/core paint history",
            "WIRED",
            "",
            "stroke history budget enforced by the core pixel history",
        ),
    ];

    let mut rows: Vec<Vec<String>> = Vec::new();
    for (i, rel) in oracle_files.iter().enumerate() {
        let (family, target, status, evidence, notes) = classify(rel, extra);
        let kind = if rel.ends_with(".tsx") {
            "component"
        } else {
            "behavior"
        };
        rows.push(vec![
            format!("F{:03}", i + 1),
            kind.to_string(),
            format!("apps/desktop/src/{rel}"),
            family,
            target,
            status,
            evidence,
            notes,
        ]);
    }

    let mut out_of_target = 0;
    for rel in &head_files {
        if oracle_set.contains(rel) {
            continue;
        }
        out_of_target += 1;
        // The October-era rule status is intentionally replaced: the row is
        // out of target regardless of how far the port got.
        let (family, target, _status, evidence, notes) = classify(rel, extra);
        let kind = if rel.ends_with(".tsx") {
            "component"
        } else {
            "behavior"
        };
        rows.push(vec![
            format!("O{:03}", out_of_target),
            kind.to_string(),
            format!("apps/desktop/src/{rel}"),
            family,
            target,
            "OUT-OF-TARGET".to_string(),
            evidence,
            if notes.is_empty() {
                "exists only in the current tree, not in the 2026-08-16 target".to_string()
            } else {
                format!("{notes}; exists only in the current tree, not in the 2026-08-16 target")
            },
        ]);
    }

    // Reference rows; the two aggregate globs are measured from the oracle
    // tree instead of carrying HEAD-era numbers.
    let rs_count = count_rs_files(&oracle_root.join("src-tauri"));
    let test_count = count_test_files(&oracle_root);
    for (j, (path, family, target, status, notes)) in REFERENCE_FILES.iter().enumerate() {
        let (path, notes) = if path.contains("src-tauri/**/*.rs") {
            (
                format!("apps/desktop/src-tauri/**/*.rs ({rs_count} files)"),
                format!("{notes}; count measured from the oracle tree"),
            )
        } else if path.contains("*.test.(ts|tsx)") {
            (
                format!("apps/desktop/src/**/*.test.(ts|tsx) ({test_count} files)"),
                format!("{notes}; count measured from the oracle tree"),
            )
        } else {
            (path.to_string(), notes.to_string())
        };
        rows.push(vec![
            format!("R{:03}", j + 1),
            "reference".to_string(),
            path,
            family.to_string(),
            target.to_string(),
            status.to_string(),
            String::new(),
            notes,
        ]);
    }

    let header = [
        "id",
        "kind",
        "path",
        "family",
        "native_target",
        "status",
        "evidence",
        "notes",
    ];
    let mut document = format!(
        "# Parity inventory generated by `cargo xtask parity inventory`.\n\
         # oracle: {commit}\n\
         # oracle production files: {} | out-of-target rows: {out_of_target} | reference rows: {}\n",
        oracle_files.len(),
        REFERENCE_FILES.len()
    );
    document.push_str(&super::csv::write(&header, &rows));

    if let Some(parent) = opts.out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&opts.out, &document) {
        eprintln!("inventory: cannot write {}: {e}", opts.out.display());
        return 1;
    }

    println!(
        "wrote {} (oracle {} production rows, {out_of_target} out-of-target, {} references)",
        opts.out.display(),
        oracle_files.len(),
        REFERENCE_FILES.len()
    );
    validate(&opts.out, &oracle_root)
}

/// Validate an inventory CSV against the oracle tree. Returns the exit code.
pub fn validate(csv_path: &Path, oracle_root: &Path) -> i32 {
    let text = match std::fs::read_to_string(csv_path) {
        Ok(text) => text,
        Err(e) => {
            eprintln!(
                "inventory validate: cannot read {}: {e}",
                csv_path.display()
            );
            return 1;
        }
    };
    let records = match csv::parse(&text) {
        Ok((_h, r)) => match csv::records_as_maps(&_h, &r) {
            Ok(maps) => maps,
            Err(e) => {
                eprintln!("inventory validate: {e}");
                return 1;
            }
        },
        Err(e) => {
            eprintln!("inventory validate: {e}");
            return 1;
        }
    };

    let mut errors: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for row in &records {
        let path = row.get("path").cloned().unwrap_or_default();
        let status = row.get("status").cloned().unwrap_or_default();
        if !STATUSES.contains(&status.as_str()) {
            errors.push(format!(
                "{path}: status `{status}` is not in the vocabulary"
            ));
        }
        if status == "NA" && row.get("notes").map(|n| n.is_empty()).unwrap_or(true) {
            errors.push(format!("{path}: NA without a reason"));
        }
        if status == "VERIFIED" && row.get("evidence").map(|n| n.is_empty()).unwrap_or(true) {
            errors.push(format!("{path}: VERIFIED without evidence"));
        }
        if !seen.insert(path.clone()) {
            errors.push(format!("{path}: duplicate row"));
        }
    }

    let oracle_files = production_files(oracle_root);
    for rel in &oracle_files {
        let path = format!("apps/desktop/src/{rel}");
        if !seen.contains(&path) {
            errors.push(format!("{path}: oracle production file is unmapped"));
        }
    }

    if errors.is_empty() {
        println!(
            "inventory validate: PASS ({} rows, {} oracle production files)",
            records.len(),
            oracle_files.len()
        );
        0
    } else {
        for e in errors.iter().take(20) {
            eprintln!("inventory validate: FAIL {e}");
        }
        eprintln!(
            "inventory validate: {} problem(s) across {} rows",
            errors.len(),
            records.len()
        );
        1
    }
}

#[cfg(test)]
mod tests;
