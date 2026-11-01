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

struct OracleSnapshot {
    commit: String,
    production_files: Vec<String>,
    test_files: Vec<String>,
    backend_files: Vec<String>,
}

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

fn git_snapshot(root: &Path) -> Result<OracleSnapshot, String> {
    let head = git_output(root, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    let head = String::from_utf8(head)
        .map_err(|e| format!("git returned a non-UTF-8 HEAD: {e}"))?
        .trim()
        .to_string();
    if !matches!(head.len(), 40 | 64) || !head.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("git returned an invalid full HEAD: {head}"));
    }

    let listing = git_output(
        root,
        &[
            "ls-tree",
            "-r",
            "-z",
            "--name-only",
            head.as_str(),
            "--",
            ".",
        ],
    )?;
    snapshot_from_paths(head, parse_ls_tree_paths(&listing)?)
}

fn parse_ls_tree_paths(listing: &[u8]) -> Result<Vec<String>, String> {
    if !listing.is_empty() && !listing.ends_with(&[0]) {
        return Err("git ls-tree output is missing its NUL terminator".to_string());
    }
    let mut paths = Vec::new();
    for bytes in listing
        .split(|byte| *byte == 0)
        .filter(|bytes| !bytes.is_empty())
    {
        let path = std::str::from_utf8(bytes)
            .map_err(|e| format!("git returned a non-UTF-8 source path: {e}"))?;
        paths.push(path.to_string());
    }
    paths.sort();
    duplicate_paths(&paths, "git ls-tree HEAD")?;
    Ok(paths)
}

fn snapshot_from_paths(commit: String, mut paths: Vec<String>) -> Result<OracleSnapshot, String> {
    paths.sort();
    duplicate_paths(&paths, "Git tree snapshot")?;

    let production_files = paths
        .iter()
        .filter_map(|path| path.strip_prefix("src/"))
        .filter(|path| is_production_path(path))
        .map(str::to_string)
        .collect();
    let test_files: Vec<String> = paths
        .iter()
        .filter(|path| is_test_reference_path(path))
        .cloned()
        .collect();
    if let Some(path) = test_files
        .iter()
        .find(|path| path.contains('\r') || path.contains('\n'))
    {
        return Err(format!(
            "test path cannot be represented in a line manifest: {path:?}"
        ));
    }
    let backend_files = paths
        .iter()
        .filter(|path| {
            path.starts_with("src-tauri/") && Path::new(path).extension().is_some_and(|e| e == "rs")
        })
        .cloned()
        .collect();

    Ok(OracleSnapshot {
        commit,
        production_files,
        test_files,
        backend_files,
    })
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

fn git_output(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("cannot run git in {}: {e}", root.display()))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed in {}: {}",
            args.join(" "),
            root.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

fn compare_production_sets(tracked: &[String], scanned: &[String], tree: &str) -> Vec<String> {
    let mut errors = Vec::new();
    if let Err(error) = duplicate_paths(tracked, "Git tree snapshot") {
        errors.push(error);
    }
    if let Err(error) = duplicate_paths(scanned, "filesystem scan") {
        errors.push(error);
    }
    let tracked_set: BTreeSet<&str> = tracked.iter().map(String::as_str).collect();
    let scanned_set: BTreeSet<&str> = scanned.iter().map(String::as_str).collect();
    for rel in tracked_set.difference(&scanned_set) {
        errors.push(format!(
            "src/{rel}: tracked {tree} production file missing from filesystem scan"
        ));
    }
    for rel in scanned_set.difference(&tracked_set) {
        errors.push(format!(
            "src/{rel}: untracked/generated production file is not tracked in {tree}"
        ));
    }
    errors
}

fn head_scan_differences(tracked: &[String], scanned: &[String]) -> (Vec<String>, Vec<String>) {
    let tracked_set: BTreeSet<&str> = tracked.iter().map(String::as_str).collect();
    let scanned_set: BTreeSet<&str> = scanned.iter().map(String::as_str).collect();
    let missing = tracked_set
        .difference(&scanned_set)
        .map(|path| path.to_string())
        .collect();
    let extra = scanned_set
        .difference(&tracked_set)
        .map(|path| path.to_string())
        .collect();
    (missing, extra)
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
    /// that a fresh clone does not contain, so pass `--oracle-root` at an
    /// archived checkout.
    pub oracle_root: PathBuf,
    /// Tree scanned for out-of-target files; same fallback rule as
    /// `oracle_root`.
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

    let oracle_snapshot = match git_snapshot(&oracle_root) {
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
        &oracle_root,
        &head_root,
        &oracle_snapshot,
        &head_snapshot,
    )
}

fn run_with_snapshot(
    opts: &InventoryOpts,
    oracle_root: &Path,
    head_root: &Path,
    oracle_snapshot: &OracleSnapshot,
    head_snapshot: &OracleSnapshot,
) -> i32 {
    let oracle_scan = match production_files(oracle_root) {
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
        "# Parity inventory generated by `cargo xtask parity inventory`.\n\
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

fn validate_rows(
    records: &[HashMap<String, String>],
    oracle_snapshot: &OracleSnapshot,
    head_snapshot: &OracleSnapshot,
    manifest_name: &str,
) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut seen_ids = BTreeSet::new();
    let oracle_paths: BTreeSet<String> = oracle_snapshot
        .production_files
        .iter()
        .map(|rel| format!("apps/desktop/src/{rel}"))
        .collect();
    let head_paths: BTreeSet<String> = head_snapshot
        .production_files
        .iter()
        .map(|rel| format!("apps/desktop/src/{rel}"))
        .collect();
    let mut production_rows = BTreeSet::new();
    let mut out_of_target_rows = BTreeSet::new();
    for row in records {
        let path = row.get("path").cloned().unwrap_or_default();
        let status = row.get("status").cloned().unwrap_or_default();
        let id = row.get("id").map(String::as_str).unwrap_or_default();
        if !seen_ids.insert(id.to_string()) {
            errors.push(format!("{id}: duplicate row ID"));
        }
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
        if id.starts_with('F') {
            production_rows.insert(path.clone());
            if !oracle_paths.contains(&path) {
                errors.push(format!(
                    "{path}: generated/untracked Oracle production row is not in pinned HEAD"
                ));
            }
        } else if id.starts_with('O') {
            out_of_target_rows.insert(path);
        }
    }

    for rel in &oracle_snapshot.production_files {
        let path = format!("apps/desktop/src/{rel}");
        if !production_rows.contains(&path) {
            errors.push(format!("{path}: oracle production file is unmapped"));
        }
    }
    let expected_out_of_target: BTreeSet<String> =
        head_paths.difference(&oracle_paths).cloned().collect();
    for path in expected_out_of_target.difference(&out_of_target_rows) {
        errors.push(format!(
            "{path}: tracked HEAD-only production file is unmapped"
        ));
    }
    for path in out_of_target_rows.difference(&expected_out_of_target) {
        errors.push(format!(
            "{path}: extra out-of-target row is not in the tracked Oracle/HEAD difference"
        ));
    }

    let test_index = REFERENCE_FILES
        .iter()
        .position(|(path, _, _, _, _)| path.contains("*.test."));
    if let Some(test_index) = test_index {
        let (_, family, target, status, notes) = REFERENCE_FILES[test_index];
        let test_id = test_index + 1;
        let id = format!("R{test_id:03}");
        let matches: Vec<_> = records
            .iter()
            .filter(|row| row.get("id").map(String::as_str) == Some(id.as_str()))
            .collect();
        if matches.len() != 1 {
            errors.push(format!("{id}: expected one test-manifest summary row"));
        }
        let expected_notes = format!(
            "{notes}; exact tracked manifest ({} paths); oracle {}",
            oracle_snapshot.test_files.len(),
            oracle_snapshot.commit
        );
        if let Some(row) = matches.first() {
            for (field, expected) in [
                ("path", manifest_name),
                ("family", family),
                ("native_target", target),
                ("status", status),
                ("notes", &expected_notes),
            ] {
                if row.get(field).map(String::as_str) != Some(expected) {
                    errors.push(format!("{id}: {field} does not match the test manifest"));
                }
            }
        }
    }

    let backend_index = REFERENCE_FILES
        .iter()
        .position(|(path, _, _, _, _)| path.contains("src-tauri/**/*.rs"));
    if let Some(backend_index) = backend_index {
        let (_, family, _, status, _) = REFERENCE_FILES[backend_index];
        let backend_start = backend_index + 1;
        let expected_paths: BTreeSet<String> = oracle_snapshot
            .backend_files
            .iter()
            .map(|path| format!("apps/desktop/{path}"))
            .collect();
        let actual_rows: Vec<_> = records
            .iter()
            .filter(|row| {
                row.get("path")
                    .is_some_and(|path| path.starts_with("apps/desktop/src-tauri/"))
            })
            .collect();
        let actual_paths: BTreeSet<String> = actual_rows
            .iter()
            .filter_map(|row| row.get("path").cloned())
            .collect();
        for path in expected_paths.difference(&actual_paths) {
            errors.push(format!("{path}: backend reference row is missing"));
        }
        for path in actual_paths.difference(&expected_paths) {
            errors.push(format!("{path}: extra backend reference row"));
        }
        for (index, expected_path) in oracle_snapshot.backend_files.iter().enumerate() {
            let id = format!("R{:03}", backend_start + index);
            let expected_path = format!("apps/desktop/{expected_path}");
            let Some(row) = records
                .iter()
                .find(|row| row.get("id").map(String::as_str) == Some(id.as_str()))
            else {
                errors.push(format!("{expected_path}: backend row {id} is missing"));
                continue;
            };
            if row.get("path").map(String::as_str) != Some(expected_path.as_str()) {
                errors.push(format!("{id}: expected backend path {expected_path}"));
            }
            for (field, expected) in [
                ("family", family),
                ("native_target", "UNKNOWN"),
                ("status", status),
                ("notes", UNKNOWN_BACKEND_NOTE),
            ] {
                if row.get(field).map(String::as_str) != Some(expected) {
                    errors.push(format!(
                        "{expected_path}: backend disposition field {field} differs"
                    ));
                }
            }
        }
    }
    errors
}

fn validate_with_snapshot(
    csv_path: &Path,
    manifest_path: &Path,
    oracle_snapshot: &OracleSnapshot,
    head_snapshot: &OracleSnapshot,
    oracle_scan: &[String],
    head_scan: &[String],
) -> i32 {
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
    let manifest = match std::fs::read_to_string(manifest_path) {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!(
                "inventory validate: cannot read test manifest {}: {error}",
                manifest_path.display()
            );
            return 1;
        }
    };
    let Some(manifest_name) = manifest_path.file_name().and_then(|name| name.to_str()) else {
        eprintln!("inventory validate: test manifest filename is not UTF-8");
        return 1;
    };
    match validate_text(
        &text,
        &manifest,
        oracle_snapshot,
        head_snapshot,
        oracle_scan,
        head_scan,
        manifest_name,
    ) {
        Ok(rows) => {
            println!(
                "inventory validate: PASS ({rows} rows, {} Oracle and {} HEAD production files)",
                oracle_snapshot.production_files.len(),
                head_snapshot.production_files.len()
            );
            0
        }
        Err((errors, rows)) => {
            for error in errors.iter().take(20) {
                eprintln!("inventory validate: FAIL {error}");
            }
            eprintln!(
                "inventory validate: {} problem(s) across {} rows",
                errors.len(),
                rows
            );
            1
        }
    }
}

fn validate_text(
    text: &str,
    manifest: &str,
    oracle_snapshot: &OracleSnapshot,
    head_snapshot: &OracleSnapshot,
    oracle_scan: &[String],
    head_scan: &[String],
    manifest_name: &str,
) -> Result<usize, (Vec<String>, usize)> {
    let (header, rows) = csv::parse(text).map_err(|error| (vec![error], 0))?;
    let records = csv::records_as_maps(&header, &rows).map_err(|error| (vec![error], 0))?;
    let mut errors = Vec::new();
    if !text
        .lines()
        .any(|line| line == format!("# oracle: {}", oracle_snapshot.commit))
    {
        errors.push(format!(
            "oracle header does not identify pinned HEAD {}",
            oracle_snapshot.commit
        ));
    }
    if !text
        .lines()
        .any(|line| line == format!("# head: {}", head_snapshot.commit))
    {
        errors.push(format!(
            "HEAD header does not identify tracked HEAD {}",
            head_snapshot.commit
        ));
    }
    errors.extend(compare_production_sets(
        &oracle_snapshot.production_files,
        oracle_scan,
        "Oracle",
    ));
    let (missing_head_files, _) = head_scan_differences(&head_snapshot.production_files, head_scan);
    errors.extend(missing_head_files.into_iter().map(|path| {
        format!("src/{path}: tracked HEAD production file missing from filesystem scan")
    }));
    errors.extend(validate_manifest(manifest, oracle_snapshot));
    errors.extend(validate_rows(
        &records,
        oracle_snapshot,
        head_snapshot,
        manifest_name,
    ));
    let out_of_target = records
        .iter()
        .filter(|row| row.get("id").is_some_and(|id| id.starts_with('O')))
        .count();
    let reference_count = records
        .iter()
        .filter(|row| row.get("id").is_some_and(|id| id.starts_with('R')))
        .count();
    let expected_summary = format!(
        "# oracle production files: {} | HEAD production files: {} | out-of-target rows: {out_of_target} | reference rows: {reference_count}",
        oracle_snapshot.production_files.len(),
        head_snapshot.production_files.len()
    );
    if !text.lines().any(|line| line == expected_summary) {
        errors.push("inventory header counts do not match emitted rows".to_string());
    }
    if errors.is_empty() {
        Ok(records.len())
    } else {
        Err((errors, records.len()))
    }
}

fn validate_manifest(manifest: &str, snapshot: &OracleSnapshot) -> Vec<String> {
    let actual: Vec<String> = manifest.lines().map(str::to_string).collect();
    let expected: Vec<String> = snapshot
        .test_files
        .iter()
        .map(|path| format!("apps/desktop/{path}"))
        .collect();
    let mut errors = Vec::new();
    let mut seen = BTreeSet::new();
    for path in &actual {
        if !seen.insert(path) {
            errors.push(format!("{path}: duplicate test manifest path"));
        }
    }
    let expected_set: BTreeSet<&str> = expected.iter().map(String::as_str).collect();
    let actual_set: BTreeSet<&str> = actual.iter().map(String::as_str).collect();
    for path in expected_set.difference(&actual_set) {
        errors.push(format!("{path}: test manifest path is missing"));
    }
    for path in actual_set.difference(&expected_set) {
        errors.push(format!("{path}: extra test manifest path"));
    }
    if actual != expected {
        errors.push("test manifest paths are not in expected sorted order".to_string());
    }
    errors
}

#[cfg(test)]
mod tests;
