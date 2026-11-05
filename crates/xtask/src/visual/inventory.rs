//! Parity inventory generator and validator.
//!
//! Replaces the frozen Python generator (`visual_inventory.py`) with the same
//! classification tables (see `inventory_rules.rs`) plus the two things the
//! Python version could not do: read a different tree (`--reference-root`, the
//! read-only 2026-08-16 worktree) and measure the aggregate reference rows
//! (Tauri `.rs` files, frozen tests) from that tree instead of carrying
//! HEAD-era numbers.
//!
//! Row kinds: `F` = oracle production file (the parity target), `O` =
//! out-of-target file (exists only in the current tree; kept so the inventory
//! stays a complete map of both trees), `R` = reference artifact.

use std::collections::{BTreeSet, HashMap};
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
const UNKNOWN_BACKEND_NOTE: &str = "No per-file native destination is evidenced.";

/// Production-file walker: `.ts`/`.tsx` under `<root>/src`, excluding test
/// files, `__tests__`/`node_modules`/`test` directories, ambient `.d.ts`
/// declarations and dotfiles — the same exclusions the frozen census uses to
/// reach its 208 production files. Returns slash-separated paths relative to
/// `<root>/src`, sorted.
pub(super) fn production_files(root: &Path) -> Result<Vec<String>, String> {
    let src = root.join("src");
    let mut out = Vec::new();
    walk(&src, &src, &mut out)?;
    out.sort();
    duplicate_paths(&out, "filesystem scan")?;
    Ok(out)
}

fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) -> Result<(), String> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("cannot scan {}: {e}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("cannot read entry in {}: {e}", dir.display()))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| format!("non-UTF-8 path in {}", dir.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|e| format!("cannot inspect {}: {e}", path.display()))?;
        if file_type.is_dir() {
            if name == "__tests__" || name == "node_modules" || name == "test" {
                continue;
            }
            walk(&path, base, out)?;
        } else if let Some(rel) = path.strip_prefix(base).ok().and_then(Path::to_str) {
            let rel = rel.replace('\\', "/");
            if is_production_path(&rel) {
                out.push(rel);
            }
        } else {
            return Err(format!(
                "cannot make source path relative: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

fn is_production_path(rel: &str) -> bool {
    let Some(name) = rel.rsplit('/').next() else {
        return false;
    };
    (name.ends_with(".ts") || name.ends_with(".tsx"))
        && !name.starts_with('.')
        && !name.ends_with(".d.ts")
        && !name.contains(".test.")
        && !name.contains(".spec.")
        && !rel
            .split('/')
            .take(rel.split('/').count().saturating_sub(1))
            .any(|part| matches!(part, "__tests__" | "node_modules" | "test"))
}

fn duplicate_paths(paths: &[String], source: &str) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for path in paths {
        if !seen.insert(path) {
            return Err(format!("{path}: duplicate path in {source}"));
        }
    }
    Ok(())
}

fn is_test_reference_path(path: &str) -> bool {
    let Some(name) = path.rsplit('/').next() else {
        return false;
    };
    let is_source = name.ends_with(".ts") || name.ends_with(".tsx") || name.ends_with(".js");
    let dirs = path
        .rsplit_once('/')
        .map(|(dirs, _)| dirs)
        .unwrap_or_default();
    is_source
        && (name.contains(".test.") || name.contains(".spec."))
        && !dirs
            .split('/')
            .any(|part| matches!(part, "node_modules" | "pkg"))
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

fn reference_rows(snapshot: &OracleSnapshot, manifest_name: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut next_id = 1;
    for (path, family, target, status, notes) in REFERENCE_FILES {
        if path.contains("*.test.") {
            rows.push(vec![
                format!("R{next_id:03}"),
                "reference".to_string(),
                manifest_name.to_string(),
                family.to_string(),
                target.to_string(),
                status.to_string(),
                String::new(),
                format!(
                    "{notes}; exact tracked manifest ({} paths); oracle {}",
                    snapshot.test_files.len(),
                    snapshot.commit
                ),
            ]);
            next_id += 1;
        } else if path.contains("src-tauri/**/*.rs") {
            for backend_path in &snapshot.backend_files {
                rows.push(vec![
                    format!("R{next_id:03}"),
                    "reference".to_string(),
                    format!("apps/desktop/{backend_path}"),
                    family.to_string(),
                    "UNKNOWN".to_string(),
                    status.to_string(),
                    String::new(),
                    UNKNOWN_BACKEND_NOTE.to_string(),
                ]);
                next_id += 1;
            }
        } else {
            rows.push(vec![
                format!("R{next_id:03}"),
                "reference".to_string(),
                path.to_string(),
                family.to_string(),
                target.to_string(),
                status.to_string(),
                String::new(),
                notes.to_string(),
            ]);
            next_id += 1;
        }
    }
    rows
}

fn test_manifest(test_files: &[String]) -> String {
    test_files
        .iter()
        .map(|path| format!("apps/desktop/{path}\n"))
        .collect()
}

pub struct InventoryOpts {
    /// Tree whose `src/` defines the parity target - a checkout of the
    /// archived reference application, whose sources no longer live in this
    /// repository. An empty value falls back to a legacy in-repo location
    /// that a fresh clone does not contain, so pass `--reference-root` at an
    /// archived checkout.
    pub reference_root: PathBuf,
    /// Tree scanned for out-of-target files; same fallback rule as
    /// `reference_root`.
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

    let reference_root = if opts.reference_root.as_os_str().is_empty() {
        repo_root.join("apps/desktop")
    } else {
        opts.reference_root.clone()
    };
    let head_root = if opts.head_root.as_os_str().is_empty() {
        repo_root.join("apps/desktop")
    } else {
        opts.head_root.clone()
    };

    let oracle_snapshot = match git_snapshot(&reference_root) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            eprintln!("inventory: cannot resolve oracle Git set: {error}");
            return 1;
        }
    };
    let head_snapshot = match git_snapshot(&head_root) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            eprintln!("inventory: cannot resolve HEAD Git set: {error}");
            return 1;
        }
    };
    run_with_snapshot(
        opts,
        &reference_root,
        &head_root,
        &oracle_snapshot,
        &head_snapshot,
    )
}

fn run_with_snapshot(
    opts: &InventoryOpts,
    reference_root: &Path,
    head_root: &Path,
    oracle_snapshot: &OracleSnapshot,
    head_snapshot: &OracleSnapshot,
) -> i32 {
    let oracle_scan = match production_files(reference_root) {
        Ok(files) => files,
        Err(error) => {
            eprintln!("inventory: oracle source scan failed: {error}");
            return 1;
        }
    };
    let oracle_errors =
        compare_production_sets(&oracle_snapshot.production_files, &oracle_scan, "Oracle");
    if !oracle_errors.is_empty() {
        for error in oracle_errors.iter().take(20) {
            eprintln!("inventory: oracle source mismatch: {error}");
        }
        eprintln!(
            "inventory: {} oracle source problem(s)",
            oracle_errors.len()
        );
        return 1;
    }
    let head_files = match production_files(head_root) {
        Ok(files) => files,
        Err(error) => {
            eprintln!("inventory: HEAD source scan failed: {error}");
            return 1;
        }
    };
    let (missing_head_files, extra_head_files) =
        head_scan_differences(&head_snapshot.production_files, &head_files);
    if !extra_head_files.is_empty() {
        let paths = extra_head_files
            .iter()
            .map(|path| format!("apps/desktop/src/{path}"))
            .collect::<Vec<_>>();
        eprintln!(
            "inventory: HEAD working-tree-only production paths excluded from target ({}): {}",
            paths.len(),
            paths.join(", ")
        );
    }
    if !missing_head_files.is_empty() {
        for path in missing_head_files.iter().take(20) {
            eprintln!(
                "inventory: HEAD tracked production file missing from filesystem scan: apps/desktop/src/{path}"
            );
        }
        eprintln!(
            "inventory: {} HEAD tracked production file(s) missing",
            missing_head_files.len()
        );
        return 1;
    }
    let manifest_path = opts.out.with_extension("tests.txt");
    let Some(manifest_name) = manifest_path.file_name().and_then(|name| name.to_str()) else {
        eprintln!("inventory: test manifest filename is not UTF-8");
        return 1;
    };
    let (document, manifest, out_of_target, reference_count) =
        build_inventory(oracle_snapshot, head_snapshot, manifest_name);

    if let Some(parent) = opts.out.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            eprintln!("inventory: cannot create {}: {error}", parent.display());
            return 1;
        }
    }
    if let Err(error) = std::fs::write(&manifest_path, &manifest) {
        eprintln!(
            "inventory: cannot write test manifest {}: {error}",
            manifest_path.display()
        );
        return 1;
    }
    if let Err(e) = std::fs::write(&opts.out, &document) {
        eprintln!("inventory: cannot write {}: {e}", opts.out.display());
        return 1;
    }

    println!(
        "wrote {} (oracle {} production rows, {out_of_target} out-of-target, {} references, {} tests, {} backend files; manifest {})",
        opts.out.display(),
        oracle_snapshot.production_files.len(),
        reference_count,
        oracle_snapshot.test_files.len(),
        oracle_snapshot.backend_files.len(),
        manifest_path.display()
    );
    validate_with_snapshot(
        &opts.out,
        &manifest_path,
        oracle_snapshot,
        head_snapshot,
        &oracle_scan,
        &head_files,
    )
}

fn build_inventory(
    oracle_snapshot: &OracleSnapshot,
    head_snapshot: &OracleSnapshot,
    manifest_name: &str,
) -> (String, String, usize, usize) {
    let oracle_set: BTreeSet<&String> = oracle_snapshot.production_files.iter().collect();
    let extra: &[(&str, &str, &str, &str, &str, &str)] = &[
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
    let mut rows = Vec::new();
    for (i, rel) in oracle_snapshot.production_files.iter().enumerate() {
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
    for rel in &head_snapshot.production_files {
        if oracle_set.contains(rel) {
            continue;
        }
        out_of_target += 1;
        let (family, target, _status, evidence, notes) = classify(rel, extra);
        let kind = if rel.ends_with(".tsx") {
            "component"
        } else {
            "behavior"
        };
        rows.push(vec![
            format!("O{out_of_target:03}"),
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

    let references = reference_rows(oracle_snapshot, manifest_name);
    let reference_count = references.len();
    rows.extend(references);
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
        "# Parity inventory generated by `cargo xtask visual inventory`.\n\
         # oracle: {}\n\
         # head: {}\n\
         # oracle production files: {} | HEAD production files: {} | out-of-target rows: {out_of_target} | reference rows: {reference_count}\n",
        oracle_snapshot.commit,
        head_snapshot.commit,
        oracle_snapshot.production_files.len(),
        head_snapshot.production_files.len(),
    );
    document.push_str(&super::csv::write(&header, &rows));
    (
        document,
        test_manifest(&oracle_snapshot.test_files),
        out_of_target,
        reference_count,
    )
}

use snapshot::*;
use validate::*;

mod snapshot;
mod validate;

#[cfg(test)]
mod tests;
