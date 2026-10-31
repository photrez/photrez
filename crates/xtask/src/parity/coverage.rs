//! Inventory ↔ surface coverage: the component-reference layer.
//!
//! A file listed in the inventory is not a covered surface. This layer fails
//! when the two tables drift apart:
//!
//! 1. a surface id is duplicated;
//! 2. a surface names a frozen file the inventory does not carry;
//! 3. a component family has no surface naming any of its files (an `NA`
//!    component must use the framework family instead);
//! 4. a component that is meant to be covered is named by no surface;
//! 5. a `MISSING` file has no owning surface row;
//! 6. a `MISSING` surface names a native target, or an implemented surface
//!    names none.
//!
//! The Python layer kept two hardcoded tables (family → surface id, missing
//! file → surface id) written against the earlier `S01..S51` numbering. Those
//! tables cannot be ported verbatim into a `S001..S064` repository without
//! silently checking the wrong ids, so the same contract is derived from the
//! tables themselves: a family is covered when a surface names one of its
//! files, and a missing file is owned when a surface names it. That keeps the
//! check honest across renumbering and still fails on every drift the layer
//! was written for. Infrastructure families (`shell`, `optionbar`, `i18n`)
//! are exempt from rule 4 exactly as before.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use super::csv;

/// Families that cross-cut many surfaces instead of owning one; rule 4 does
/// not require them to be named individually.
const INFRASTRUCTURE: &[&str] = &["shell", "optionbar", "i18n"];

/// Statuses that exempt a component row from rule 4: it is not part of the
/// ported surface set at all.
const EXEMPT_STATUSES: &[&str] = &["NA", "OUT-OF-TARGET", "DEAD", "REFERENCE"];

/// Statuses that mean "implemented surface, so it must name a native target".
const IMPLEMENTED: &[&str] = &["WIRED", "INTERACTIVE", "VERIFIED"];

struct Table {
    rows: Vec<HashMap<String, String>>,
}

impl Table {
    fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        let (headers, records) =
            csv::parse(&text).map_err(|error| format!("parse {}: {error}", path.display()))?;
        let mut rows = Vec::with_capacity(records.len());
        for record in records {
            let mut row = HashMap::new();
            for (index, header) in headers.iter().enumerate() {
                row.insert(
                    header.clone(),
                    record.get(index).cloned().unwrap_or_default(),
                );
            }
            rows.push(row);
        }
        Ok(Self { rows })
    }

    fn get<'a>(&'a self, row: &'a HashMap<String, String>, key: &str) -> &'a str {
        let _ = self;
        row.get(key).map(String::as_str).unwrap_or("")
    }
}

/// File basenames a surface cell names. Frozen refs are `;`-separated paths
/// whose individual segments may hold two files joined by `+`.
fn ref_basenames(cell: &str) -> Vec<String> {
    let mut out = Vec::new();
    for part in cell.replace(';', "+").split('+') {
        let token = part.trim();
        if token.ends_with(".tsx") || token.ends_with(".ts") {
            let base = token
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(token)
                .to_string();
            out.push(base);
        }
    }
    out
}

fn basename(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}

/// Run the coverage layer. Returns the process exit code: 0 on PASS, 1 when
/// any rule fails, with one line per failure.
pub fn run(inventory: &Path, surfaces: &Path) -> Result<i32, String> {
    let inventory = Table::load(inventory)?;
    let surfaces = Table::load(surfaces)?;

    let mut errors: Vec<String> = Vec::new();

    // Rule 1: unique surface ids.
    let mut ids: BTreeSet<&str> = BTreeSet::new();
    for row in &surfaces.rows {
        let id = surfaces.get(row, "id");
        if !ids.insert(id) {
            errors.push(format!("duplicate surface id: {id}"));
        }
    }

    // Inventory files of the ported set (what a frozen ref may resolve to).
    let mut known: BTreeSet<String> = BTreeSet::new();
    for row in &inventory.rows {
        let kind = inventory.get(row, "kind");
        if kind == "component" || kind == "behavior" {
            known.insert(basename(inventory.get(row, "path")));
        }
    }

    // Basenames named by at least one surface.
    let mut named: BTreeSet<String> = BTreeSet::new();
    for row in &surfaces.rows {
        named.extend(ref_basenames(surfaces.get(row, "frozen_refs")));
    }

    // Rule 2: every frozen ref resolves.
    for row in &surfaces.rows {
        let id = surfaces.get(row, "id").to_string();
        for base in ref_basenames(surfaces.get(row, "frozen_refs")) {
            if !known.contains(&base) {
                errors.push(format!("{id}: frozen ref not in inventory: {base}"));
            }
        }
    }

    // Rule 3 + 4: family coverage and per-component coverage.
    let mut family_files: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut family_covered: BTreeMap<String, bool> = BTreeMap::new();
    for row in &inventory.rows {
        if inventory.get(row, "kind") != "component" {
            continue;
        }
        let id = inventory.get(row, "id").to_string();
        let family = inventory.get(row, "family").to_string();
        let status = inventory.get(row, "status").to_string();
        let base = basename(inventory.get(row, "path"));
        if status == "NA" && family != "na-framework" {
            errors.push(format!(
                "{id}: NA component with non-framework family {family}"
            ));
            continue;
        }
        if status == "NA" {
            continue;
        }
        let covered = named.contains(&base);
        family_covered
            .entry(family.clone())
            .and_modify(|value| *value |= covered)
            .or_insert(covered);
        family_files
            .entry(family.clone())
            .or_default()
            .push(base.clone());
        if !covered
            && !INFRASTRUCTURE.contains(&family.as_str())
            && !EXEMPT_STATUSES.contains(&status.as_str())
        {
            errors.push(format!(
                "{id}: component not referenced by any surface: {base}"
            ));
        }
    }
    for (family, covered) in &family_covered {
        if !covered {
            let files = family_files.get(family).cloned().unwrap_or_default();
            errors.push(format!(
                "component family has no surface mapping: {family} ({})",
                files.join(", ")
            ));
        }
    }

    // Rule 5: every MISSING file has an owning surface row.
    for row in &inventory.rows {
        if inventory.get(row, "status") != "MISSING" {
            continue;
        }
        let id = inventory.get(row, "id").to_string();
        let base = basename(inventory.get(row, "path"));
        if !named.contains(&base) {
            errors.push(format!("{id}: MISSING file has no surface owner: {base}"));
        }
    }

    // Rule 6: MISSING surfaces name no native target; implemented ones do.
    for row in &surfaces.rows {
        let id = surfaces.get(row, "id");
        let overall = surfaces.get(row, "overall");
        let native = surfaces.get(row, "native_refs");
        if overall == "MISSING" && native != "-" {
            errors.push(format!("{id}: MISSING surface names a native target"));
        }
        if IMPLEMENTED.contains(&overall) && (native == "-" || native.is_empty()) {
            errors.push(format!("{id}: implemented surface has no native target"));
        }
    }

    println!(
        "coverage: {} surfaces, {} named files, {} component families",
        surfaces.rows.len(),
        named.len(),
        family_covered.len()
    );
    if errors.is_empty() {
        println!("coverage: PASS");
        return Ok(0);
    }
    println!("coverage: FAIL ({} problem(s))", errors.len());
    for error in errors.iter().take(60) {
        println!("  {error}");
    }
    Ok(1)
}

#[cfg(test)]
mod tests;
