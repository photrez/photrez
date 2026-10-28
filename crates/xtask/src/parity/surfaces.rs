//! Surface-matrix generator and validator (T2 + T5).
//!
//! The frozen Python `parity_surfaces.py` was a hand-written 51-row literal
//! with no generator; this module derives the surface proposals from the
//! re-based inventory (one proposal per inventory family) so no directory can
//! be silently omitted, and validates the matrix in both directions: every
//! mapable inventory file appears in a surface, and every surface names real
//! files. A `VERIFIED` row additionally requires an evidence bundle on disk
//! with matching provenance (T5).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::csv;

/// Inventory statuses that carry no surface obligation.
const NO_SURFACE_NEEDED: &[&str] = &["NA", "OUT-OF-TARGET", "DEAD", "REFERENCE"];

/// One proposed surface's accumulation per inventory family.
type FamilyGroups = BTreeMap<String, (Vec<String>, BTreeMap<String, ()>, BTreeMap<String, ()>)>;

pub struct SurfaceOpts {
    pub inventory: PathBuf,
    pub surfaces: PathBuf,
    /// Root directory holding per-surface evidence bundles. Required for
    /// `VERIFIED` rows.
    pub bundles: PathBuf,
    /// Expected provenance for bundle checks: `oracle=<commit>` and
    /// `build=<id>`. Empty disables the provenance match.
    pub oracle_commit: String,
    pub build_id: String,
}

/// `cargo xtask parity surfaces --generate --inventory <csv> --out <csv>`
/// proposes one surface per inventory family. `cargo xtask parity surfaces`
/// validates the matrix.
pub fn cmd(args: &[String]) -> i32 {
    let inventory = flag_path(args, "--inventory")
        .unwrap_or_else(|| PathBuf::from(".slim/deepwork/parity-inventory.csv"));
    if has_flag(args, "--generate") {
        let out = flag_path(args, "--out")
            .unwrap_or_else(|| PathBuf::from(".slim/deepwork/parity-surfaces.csv"));
        return generate(&inventory, &out);
    }
    let surfaces = flag_path(args, "--surfaces")
        .unwrap_or_else(|| PathBuf::from(".slim/deepwork/parity-surfaces.csv"));
    let opts = SurfaceOpts {
        inventory,
        surfaces,
        bundles: flag_path(args, "--bundles")
            .unwrap_or_else(|| PathBuf::from(".slim/deepwork/parity-evidence")),
        oracle_commit: flag(args, "--oracle-commit")
            .unwrap_or_default()
            .to_string(),
        build_id: flag(args, "--build-id").unwrap_or_default().to_string(),
    };
    validate(&opts)
}

fn has_flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == name {
            return it.next().map(String::as_str);
        }
    }
    None
}

fn flag_path(args: &[String], name: &str) -> Option<PathBuf> {
    flag(args, name).map(PathBuf::from)
}

/// One proposed surface row per inventory family that maps files.
fn generate(inventory: &Path, out: &Path) -> i32 {
    let text = match std::fs::read_to_string(inventory) {
        Ok(t) => t,
        Err(e) => {
            eprintln!(
                "surfaces generate: cannot read {}: {e}",
                inventory.display()
            );
            return 1;
        }
    };
    let (header, rows) = match csv::parse(&text) {
        Ok((h, r)) => (h, r),
        Err(e) => {
            eprintln!("surfaces generate: {e}");
            return 1;
        }
    };
    let records = match csv::records_as_maps(&header, &rows) {
        Ok(maps) => maps,
        Err(e) => {
            eprintln!("surfaces generate: {e}");
            return 1;
        }
    };

    // family -> (files, native targets, statuses), sorted for stable output.
    let mut families: FamilyGroups = BTreeMap::new();
    for row in &records {
        let status = row.get("status").cloned().unwrap_or_default();
        if NO_SURFACE_NEEDED.contains(&status.as_str()) || status == "UNMAPPED" {
            continue;
        }
        let family = row.get("family").cloned().unwrap_or_default();
        let path = row.get("path").cloned().unwrap_or_default();
        let target = row.get("native_target").cloned().unwrap_or_default();
        let entry = families.entry(family).or_default();
        entry.0.push(path);
        if target != "-" {
            entry.1.insert(target, ());
        }
        entry.2.insert(status, ());
    }

    let mut rows: Vec<Vec<String>> = Vec::new();
    for (id, (family, (files, targets, statuses))) in families.iter().enumerate() {
        let mut files = files.clone();
        files.sort();
        let overall = if statuses.contains_key("MISSING") && statuses.len() == 1 {
            "MISSING"
        } else {
            "WIRED"
        };
        rows.push(vec![
            format!("S{:03}", id + 1),
            family.clone(),
            files.join("; "),
            targets.keys().cloned().collect::<Vec<_>>().join("; "),
            "TODO".into(),
            "TODO".into(),
            "TODO".into(),
            "TODO".into(),
            "TODO".into(),
            overall.to_string(),
            String::new(),
            "proposed by the surface generator; ratify dimensions before VERIFIED".into(),
        ]);
    }

    let header = [
        "id",
        "surface",
        "frozen_refs",
        "native_refs",
        "layout",
        "data",
        "states",
        "input",
        "perf",
        "overall",
        "evidence",
        "notes",
    ];
    let document = csv::write(&header, &rows);
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(out, &document) {
        eprintln!("surfaces generate: cannot write {}: {e}", out.display());
        return 1;
    }
    println!("wrote {} ({} proposed surfaces)", out.display(), rows.len());
    0
}

/// Validate the surface matrix against the inventory, both directions.
fn validate(opts: &SurfaceOpts) -> i32 {
    let inv_text = match std::fs::read_to_string(&opts.inventory) {
        Ok(t) => t,
        Err(e) => {
            eprintln!(
                "surfaces validate: cannot read {}: {e}",
                opts.inventory.display()
            );
            return 1;
        }
    };
    let sur_text = match std::fs::read_to_string(&opts.surfaces) {
        Ok(t) => t,
        Err(e) => {
            eprintln!(
                "surfaces validate: cannot read {}: {e}",
                opts.surfaces.display()
            );
            return 1;
        }
    };
    let (inv_header, inv_rows) = match csv::parse(&inv_text) {
        Ok((h, r)) => (h, r),
        Err(e) => {
            eprintln!("surfaces validate: inventory: {e}");
            return 1;
        }
    };
    let inv_records = match csv::records_as_maps(&inv_header, &inv_rows) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("surfaces validate: inventory: {e}");
            return 1;
        }
    };
    let (sur_header, sur_rows) = match csv::parse(&sur_text) {
        Ok((h, r)) => (h, r),
        Err(e) => {
            eprintln!("surfaces validate: surfaces: {e}");
            return 1;
        }
    };
    let sur_records = match csv::records_as_maps(&sur_header, &sur_rows) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("surfaces validate: surfaces: {e}");
            return 1;
        }
    };

    // Files named across all surfaces.
    let mut surface_files: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (index, row) in sur_records.iter().enumerate() {
        let refs: Vec<String> = row
            .get("frozen_refs")
            .map(|r| r.split(';').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default();
        for file in refs {
            if file.is_empty() {
                continue;
            }
            surface_files
                .entry(file)
                .or_default()
                .push(format!("S{:03}", index + 1));
        }
    }

    let mut errors: Vec<String> = Vec::new();
    // Direction 1: every mapable inventory file is named by a surface.
    for row in &inv_records {
        let path = row.get("path").cloned().unwrap_or_default();
        let status = row.get("status").cloned().unwrap_or_default();
        if NO_SURFACE_NEEDED.contains(&status.as_str()) || status == "UNMAPPED" {
            continue;
        }
        if !surface_files.contains_key(&path) {
            errors.push(format!("orphan file (no surface names it): {path}"));
        }
    }
    // Direction 2: every surface names real inventory files, has dimensions,
    // and VERIFIED rows carry a complete evidence bundle.
    for (index, row) in sur_records.iter().enumerate() {
        let id = format!("S{:03}", index + 1);
        let refs: Vec<String> = row
            .get("frozen_refs")
            .map(|r| r.split(';').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default();
        if refs.is_empty() {
            errors.push(format!("{id}: surface names no files"));
        }
        for file in &refs {
            if inv_records
                .iter()
                .any(|r| r.get("path").map(|p| p == file).unwrap_or(false))
            {
                continue;
            }
            errors.push(format!("{id}: frozen ref is not an inventory row: {file}"));
        }
        let overall = row.get("overall").map(String::as_str).unwrap_or("");
        let dimensions = ["layout", "data", "states", "input", "perf"];
        for dim in dimensions {
            let value = row.get(dim).map(String::as_str).unwrap_or("");
            if value.is_empty() {
                errors.push(format!("{id}: dimension `{dim}` is empty"));
            }
        }
        if overall == "VERIFIED" {
            for dim in dimensions {
                let value = row.get(dim).map(String::as_str).unwrap_or("");
                if value == "TODO" {
                    errors.push(format!("{id}: VERIFIED with `{dim}` still TODO"));
                }
            }
            let bundle = opts.bundles.join(&id);
            if !bundle.is_dir() {
                errors.push(format!(
                    "{id}: VERIFIED without an evidence bundle at {}",
                    bundle.display()
                ));
            } else {
                for artifact in ["oracle-capture", "native-capture", "diff", "state", "perf"] {
                    let has = std::fs::read_dir(&bundle)
                        .map(|entries| {
                            entries
                                .flatten()
                                .any(|e| e.file_name().to_string_lossy().starts_with(artifact))
                        })
                        .unwrap_or(false);
                    if !has {
                        errors.push(format!(
                            "{id}: evidence bundle lacks a `{artifact}` artifact"
                        ));
                    }
                }
                let provenance = bundle.join("provenance.txt");
                let prov = std::fs::read_to_string(&provenance).unwrap_or_default();
                if !opts.oracle_commit.is_empty()
                    && !prov.contains(&format!("oracle: {}", opts.oracle_commit))
                {
                    errors.push(format!(
                        "{id}: bundle provenance does not record oracle commit {}",
                        opts.oracle_commit
                    ));
                }
                if !opts.build_id.is_empty() && !prov.contains(&format!("build: {}", opts.build_id))
                {
                    errors.push(format!(
                        "{id}: bundle provenance does not record build id {}",
                        opts.build_id
                    ));
                }
            }
        }
    }

    if errors.is_empty() {
        println!(
            "surfaces validate: PASS ({} surfaces, {} inventory files)",
            sur_records.len(),
            inv_records.len()
        );
        0
    } else {
        for e in errors.iter().take(20) {
            eprintln!("surfaces validate: FAIL {e}");
        }
        eprintln!(
            "surfaces validate: {} problem(s) across {} surfaces",
            errors.len(),
            sur_records.len()
        );
        1
    }
}
