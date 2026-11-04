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
        bundles: bundle_root(args),
        oracle_commit: flag(args, "--oracle-commit")
            .unwrap_or_default()
            .to_string(),
        build_id: flag(args, "--build-id").unwrap_or_default().to_string(),
    };
    validate(&opts)
}

/// The root that holds the per-surface evidence bundles. `--bundles` overrides
/// it; the default is the tree the inventory and matrix CSVs live beside.
fn bundle_root(args: &[String]) -> PathBuf {
    flag_path(args, "--bundles").unwrap_or_else(|| PathBuf::from(".slim/deepwork/78-evidence"))
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

    // Files named across all surfaces, keyed by each row's stable id.
    let mut surface_files: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for row in sur_records.iter() {
        let id = row.get("id").cloned().unwrap_or_default();
        let refs: Vec<String> = row
            .get("frozen_refs")
            .map(|r| r.split(';').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default();
        for file in refs {
            if file.is_empty() {
                continue;
            }
            surface_files.entry(file).or_default().push(id.clone());
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
    // and VERIFIED rows carry a complete evidence bundle. IDs come from the
    // CSV, never from row position.
    let mut seen_ids: BTreeMap<String, ()> = BTreeMap::new();
    for row in sur_records.iter() {
        let id = row.get("id").cloned().unwrap_or_default();
        if !is_stable_id(&id) {
            errors.push(format!(
                "surface id `{id}` is empty or not S<digits> (stable ids are required)"
            ));
            continue;
        }
        if seen_ids.insert(id.clone(), ()).is_some() {
            errors.push(format!("{id}: duplicate surface id"));
        }
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
        if !matches!(overall, "WIRED" | "MISSING" | "VERIFIED" | "NA") {
            errors.push(format!(
                "{id}: overall `{overall}` is not one of WIRED, MISSING, VERIFIED, NA"
            ));
        }
        let dimensions = ["layout", "data", "states", "input", "perf"];
        for dim in dimensions {
            let value = row.get(dim).map(String::as_str).unwrap_or("");
            if !matches!(value, "TODO" | "PASS" | "FAIL") {
                errors.push(format!(
                    "{id}: dimension `{dim}` is `{value}`; expected TODO, PASS or FAIL"
                ));
            }
        }
        if overall == "NA" {
            if row
                .get("notes")
                .map(|notes| notes.trim().is_empty())
                .unwrap_or(true)
            {
                errors.push(format!("{id}: NA without a written reason"));
            }
            if row
                .get("evidence")
                .map(|evidence| evidence.trim().is_empty())
                .unwrap_or(true)
            {
                errors.push(format!("{id}: NA without an owner-decision reference"));
            }
        }
        if overall == "VERIFIED" {
            for dim in dimensions {
                let value = row.get(dim).map(String::as_str).unwrap_or("");
                if value != "PASS" {
                    errors.push(format!("{id}: VERIFIED with `{dim}` = `{value}`"));
                }
            }
            errors.extend(bundle::bundle_errors(
                &id,
                &opts.bundles.join(&id),
                &opts.oracle_commit,
                &opts.build_id,
            ));
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

fn is_stable_id(id: &str) -> bool {
    let Some(number) = id.strip_prefix('S') else {
        return false;
    };
    number.len() >= 3 && number.chars().all(|c| c.is_ascii_digit())
}

/// Strict bundle contract (T5, G20). This validator is the specification:
///
/// `manifest.json`: `surface`, `oracle_commit`, `build_id`, `fixture`,
/// `locale`, `dpi` (> 0), `physical_size` ([w, h]), `oracle_capture`,
/// `native_capture`.
/// `diff.json`: `surface`, `pass` (true), `thresholds` {`max_changed_pixels`,
/// `mask`}, `inputs` {`oracle`, `native`} content labels, `metrics`
/// {`changed_pixels`}, `states` [{`id`, `pass`}] (nonempty, all true).
/// `states.json`: `surface`, `states` [{`id`, `result` ("PASS"), `capture`,
/// `test`}] (nonempty; TODO/FAIL/MISSING rejected).
/// `interaction.txt`: receipts naming every `states[].test` plus `RED` and
/// `GREEN` lines.
/// `perf.json`: `surface`, `build_id`, `profile`, `cold_warm`, `samples`
/// (>= 3), `n` == samples.len(), `p50` <= `p95`, `budget` > 0,
/// `p95` <= `budget`, `pass` true.
/// `provenance.txt`: `key: value` lines for `oracle`, `build`, `fixture`,
/// `catalog`, `input`, `native-rev` and `tool`.
///
/// Cross-checks link the files: every JSON names this surface; provenance
/// identity equals the manifest and the expected identities; the diff's
/// state id set equals `states.json`; capture files exist and are PNGs of
/// `physical_size`; diff input labels equal FNV-1a-64 content labels of the
/// captures; every state capture/test reference resolves.
/// True when both spellings identify the same git commit.
///
/// Evidence bundles record the canonical abbreviation (`390f7358`) while the
/// pin is the full 40-hex SHA, so comparing the two strings byte-for-byte
/// rejected correct provenance. A shared prefix of at least git's seven-character
/// minimum still identifies one commit; a divergent or over-shortened value does
/// not, so a genuinely different commit still fails.
fn same_commit(recorded: Option<&str>, expected: &str) -> bool {
    let Some(recorded) = recorded else {
        return false;
    };
    if recorded == expected {
        return true;
    }
    if !is_commit_token(expected) || !is_commit_token(recorded) {
        return false;
    }
    let (short, long) = if recorded.len() <= expected.len() {
        (recorded, expected)
    } else {
        (expected, recorded)
    };
    long.starts_with(short)
}

/// A hex string git could resolve: at least its seven-character minimum
/// abbreviation and no longer than a full SHA.
fn is_commit_token(value: &str) -> bool {
    (MIN_COMMIT_ABBREV..=FULL_COMMIT_LEN).contains(&value.len())
        && value.chars().all(|c| c.is_ascii_hexdigit())
}

const MIN_COMMIT_ABBREV: usize = 7;
const FULL_COMMIT_LEN: usize = 40;

mod bundle;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    type FailureCase = (&'static str, &'static str, Box<dyn FnOnce(&Path)>);

    #[test]
    fn bundle_root_names_the_evidence_tree_and_obeys_the_override() {
        assert_eq!(
            bundle_root(&[]),
            PathBuf::from(".slim/deepwork/78-evidence"),
            "a run with no flags looks in the tree that holds the bundles"
        );
        assert_eq!(
            bundle_root(&["--bundles".to_string(), "somewhere-else".to_string()]),
            PathBuf::from("somewhere-else"),
            "an explicit root is respected"
        );
    }

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "xtask-t5-{}-{}-{}",
            std::process::id(),
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn write(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let mut file = std::fs::File::create(path).unwrap();
        file.write_all(bytes).unwrap();
    }

    fn write_json(path: &Path, value: serde_json::Value) {
        write(
            path,
            serde_json::to_string_pretty(&value).unwrap().as_bytes(),
        );
    }

    fn read_json(path: &Path) -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
        bytes
    }

    fn valid_bundle(dir: &Path, id: &str) {
        let oracle = png_bytes(2, 2);
        let native = png_bytes(2, 2);
        write(&dir.join("oracle-capture.png"), &oracle);
        write(&dir.join("native-capture.png"), &native);
        write_json(
            &dir.join("manifest.json"),
            serde_json::json!({
                "surface": id,
                "oracle_commit": "abc123",
                "build_id": "build-1",
                "fixture": "fixture.psd",
                "locale": "en",
                "dpi": 120,
                "physical_size": [2, 2],
                "oracle_capture": "oracle-capture.png",
                "native_capture": "native-capture.png",
            }),
        );
        write_json(
            &dir.join("diff.json"),
            serde_json::json!({
                "surface": id,
                "pass": true,
                "thresholds": { "max_changed_pixels": 4, "mask": "class-map" },
                "inputs": {
                    "oracle": bundle::content_label_bytes(&oracle),
                    "native": bundle::content_label_bytes(&native),
                },
                "metrics": { "changed_pixels": 0 },
                "states": [{ "id": "resting", "pass": true }],
            }),
        );
        write_json(
            &dir.join("states.json"),
            serde_json::json!({
                "surface": id,
                "states": [{
                    "id": "resting",
                    "result": "PASS",
                    "capture": "oracle-capture.png",
                    "test": "menu_test",
                }],
            }),
        );
        write(
            &dir.join("interaction.txt"),
            b"menu_test: cargo test --lib menu_test -> ok\nRED: mutated (exit 101)\nGREEN: restored (exit 0)\n",
        );
        write_json(
            &dir.join("perf.json"),
            serde_json::json!({
                "surface": id,
                "build_id": "build-1",
                "profile": "dev",
                "cold_warm": "warm",
                "samples": [12.1, 12.4, 12.2],
                "n": 3,
                "p50": 12.2,
                "p95": 12.4,
                "budget": 33.3,
                "pass": true,
            }),
        );
        write(
            &dir.join("provenance.txt"),
            b"oracle: abc123\nbuild: build-1\nfixture: fixture.psd\ncatalog: en.json\ninput: oracle.png\nnative-rev: 8032417e\ntool: xtask 0.1.0\n",
        );
    }

    fn run(root: &Path, rows: &[&str], bundle: Option<&str>) -> i32 {
        let inventory = root.join("inventory.csv");
        let surfaces = root.join("surfaces.csv");
        write(
            &inventory,
            b"path,status,native_target,family\na.tsx,WIRED,-,fam\nb.tsx,WIRED,-,fam\n",
        );
        let mut csv = String::from(
            "id,surface,frozen_refs,native_refs,layout,data,states,input,perf,overall,evidence,notes\n",
        );
        for row in rows {
            csv.push_str(row);
            csv.push('\n');
        }
        write(&surfaces, csv.as_bytes());
        let bundles = root.join("bundles");
        if let Some(bundle) = bundle {
            let dir = bundles.join(bundle);
            std::fs::create_dir_all(&dir).unwrap();
            valid_bundle(&dir, bundle);
        }
        cmd(&[
            "--inventory".into(),
            inventory.to_string_lossy().into_owned(),
            "--surfaces".into(),
            surfaces.to_string_lossy().into_owned(),
            "--bundles".into(),
            bundles.to_string_lossy().into_owned(),
            "--oracle-commit".into(),
            "abc123".into(),
            "--build-id".into(),
            "build-1".into(),
        ])
    }

    /// Isolated fixture per RED case: a fresh root whose `bundles/S001` holds
    /// one valid bundle, mutated in exactly one way. Asserts the validator
    /// names the reason and that the CLI exits nonzero for the same tree.
    fn case(name: &str, expect: &str, mutate: impl FnOnce(&Path)) {
        let root = temp_root(&format!("case-{name}"));
        let dir = root.join("bundles").join("S001");
        std::fs::create_dir_all(&dir).unwrap();
        valid_bundle(&dir, "S001");
        mutate(&dir);
        let errors = bundle::bundle_errors("S001", &dir, "abc123", "build-1");
        assert!(
            errors.iter().any(|error| error.contains(expect)),
            "{name}: expected `{expect}` in {errors:?}"
        );
        assert_eq!(run(&root, &[ROW], None), 1, "{name}: CLI must exit 1");
        std::fs::remove_dir_all(root).unwrap();
    }

    const ROW: &str = "S001,test,a.tsx; b.tsx,,PASS,PASS,PASS,PASS,PASS,VERIFIED,,";

    #[test]
    fn verified_bundle_passes_and_stable_ids_are_position_independent() {
        let root = temp_root("green");
        assert_eq!(run(&root, &[ROW], Some("S001")), 0);
        let rows = [
            "S002,second,a.tsx,,PASS,PASS,PASS,PASS,PASS,WIRED,,",
            "S001,test,b.tsx,,PASS,PASS,PASS,PASS,PASS,VERIFIED,,",
        ];
        let root2 = temp_root("order");
        assert_eq!(run(&root2, &rows, Some("S001")), 0);
        std::fs::remove_dir_all(root).unwrap();
        std::fs::remove_dir_all(root2).unwrap();
    }

    #[test]
    fn na_needs_a_reason_and_an_owner_reference() {
        let root = temp_root("na-ok");
        let good = "S001,test,a.tsx; b.tsx,,TODO,TODO,TODO,TODO,TODO,NA,owner-decision-2026-10-31,a-module-level-surface";
        assert_eq!(run(&root, &[good], None), 0);
        std::fs::remove_dir_all(root).unwrap();

        let root = temp_root("na-no-reason");
        let bare = "S001,test,a.tsx; b.tsx,,TODO,TODO,TODO,TODO,TODO,NA,,";
        assert_eq!(run(&root, &[bare], None), 1, "NA without reason must fail");
        std::fs::remove_dir_all(root).unwrap();

        let root = temp_root("na-no-owner");
        let no_owner = "S001,test,a.tsx; b.tsx,,TODO,TODO,TODO,TODO,TODO,NA,,a-reason";
        assert_eq!(
            run(&root, &[no_owner], None),
            1,
            "NA without an owner reference must fail"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_commit_abbreviation_identifies_the_same_commit() {
        let full = "390f73581037bcfe92c70af6cb03d070754a792d";
        assert!(same_commit(Some(full), full), "the exact spelling");
        assert!(
            same_commit(Some("390f7358"), full),
            "the canonical abbreviation"
        );
        assert!(
            same_commit(Some(full), "390f7358"),
            "the pin abbreviated against a full record"
        );
        assert!(
            !same_commit(Some("390f73581037bcfe92c70af6cb03d070754a792e"), full),
            "a differing commit"
        );
        assert!(
            !same_commit(Some("390f73"), full),
            "shorter than git's seven-character minimum"
        );
        assert!(!same_commit(Some("notacommit"), full), "a non-hex value");
        assert!(
            !same_commit(Some("0"), full),
            "a one-character prefix match"
        );
        assert!(!same_commit(None, full), "a missing record");
        assert!(
            !same_commit(Some("390f7358"), ""),
            "an empty pin is not an identity"
        );
    }

    #[test]
    fn missing_bundle_fails() {
        let root = temp_root("missing");
        assert_eq!(run(&root, &[ROW], None), 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn empty_corrupt_and_wrong_provenance_fail() {
        let cases: Vec<FailureCase> = vec![
            (
                "empty-perf",
                "`perf.json` is empty",
                Box::new(|dir: &Path| write(&dir.join("perf.json"), b"")),
            ),
            (
                "corrupt-png",
                "not a valid PNG",
                Box::new(|dir: &Path| write(&dir.join("oracle-capture.png"), b"not a png")),
            ),
            (
                "wrong-size",
                "manifest says 2x2",
                Box::new(|dir: &Path| write(&dir.join("native-capture.png"), &png_bytes(3, 2))),
            ),
            (
                "wrong-build",
                "does not record exactly `build: build-1`",
                Box::new(|dir: &Path| {
                    write(
                        &dir.join("provenance.txt"),
                        b"oracle: abc123\nbuild: other\nfixture: fixture.psd\ncatalog: en.json\ninput: oracle.png\nnative-rev: 8032417e\ntool: xtask\n",
                    );
                }),
            ),
            (
                "wrong-oracle",
                "does not identify oracle commit",
                Box::new(|dir: &Path| {
                    write(
                        &dir.join("provenance.txt"),
                        b"oracle: 1e4f6a7b8c9d0e1f2a3b4c5d6e7f8091a2b3c4d5\nbuild: build-1\nfixture: fixture.psd\ncatalog: en.json\ninput: oracle.png\nnative-rev: 8032417e\ntool: xtask\n",
                    );
                }),
            ),
            (
                "missing-key",
                "provenance lacks `catalog`",
                Box::new(|dir: &Path| {
                    write(
                        &dir.join("provenance.txt"),
                        b"oracle: abc123\nbuild: build-1\nfixture: fixture.psd\ninput: oracle.png\nnative-rev: 8032417e\ntool: xtask\n",
                    );
                }),
            ),
            (
                "duplicate-key",
                "recorded twice with different values",
                Box::new(|dir: &Path| {
                    let mut text = std::fs::read_to_string(dir.join("provenance.txt")).unwrap();
                    text.push_str("tool: another\n");
                    write(&dir.join("provenance.txt"), text.as_bytes());
                }),
            ),
            (
                "extra-schema",
                "unexpected evidence schema `notes.json`",
                Box::new(|dir: &Path| {
                    write_json(&dir.join("notes.json"), serde_json::json!({ "pass": true }))
                }),
            ),
        ];
        for (name, expect, mutate) in cases {
            case(name, expect, mutate);
        }
    }

    #[test]
    fn schema_and_cross_reference_violations_fail() {
        let cases: Vec<FailureCase> = vec![
            (
                "corrupt-json",
                "is not valid JSON",
                Box::new(|dir: &Path| write(&dir.join("diff.json"), b"{not json")),
            ),
            (
                "diff-false",
                "does not record `pass: true`",
                Box::new(|dir: &Path| {
                    let mut diff = read_json(&dir.join("diff.json"));
                    diff["pass"] = serde_json::json!(false);
                    write_json(&dir.join("diff.json"), diff);
                }),
            ),
            (
                "diff-input",
                "input `oracle` does not match the capture content",
                Box::new(|dir: &Path| {
                    let mut diff = read_json(&dir.join("diff.json"));
                    diff["inputs"]["oracle"] = serde_json::json!("fnv64:0000000000000000");
                    write_json(&dir.join("diff.json"), diff);
                }),
            ),
            (
                "diff-surface",
                "names surface `S999`",
                Box::new(|dir: &Path| {
                    let mut diff = read_json(&dir.join("diff.json"));
                    diff["surface"] = serde_json::json!("S999");
                    write_json(&dir.join("diff.json"), diff);
                }),
            ),
            (
                "state-fail",
                "expected PASS",
                Box::new(|dir: &Path| {
                    let mut states = read_json(&dir.join("states.json"));
                    states["states"][0]["result"] = serde_json::json!("FAIL");
                    write_json(&dir.join("states.json"), states);
                }),
            ),
            (
                "dangling-capture",
                "capture `missing.png` does not exist",
                Box::new(|dir: &Path| {
                    let mut states = read_json(&dir.join("states.json"));
                    states["states"][0]["capture"] = serde_json::json!("missing.png");
                    write_json(&dir.join("states.json"), states);
                }),
            ),
            (
                "dangling-test",
                "has no interaction receipt",
                Box::new(|dir: &Path| {
                    let mut states = read_json(&dir.join("states.json"));
                    states["states"][0]["test"] = serde_json::json!("unknown_test");
                    write_json(&dir.join("states.json"), states);
                }),
            ),
            (
                "id-mismatch",
                "diff state ids do not match states.json",
                Box::new(|dir: &Path| {
                    let mut diff = read_json(&dir.join("diff.json"));
                    diff["states"] = serde_json::json!([{ "id": "hover", "pass": true }]);
                    write_json(&dir.join("diff.json"), diff);
                }),
            ),
            (
                "perf-n",
                "at least 3 numeric samples",
                Box::new(|dir: &Path| {
                    let mut perf = read_json(&dir.join("perf.json"));
                    perf["samples"] = serde_json::json!([1.0, 2.0]);
                    perf["n"] = serde_json::json!(2);
                    write_json(&dir.join("perf.json"), perf);
                }),
            ),
            (
                "perf-budget",
                "p95 exceeds the adopted budget",
                Box::new(|dir: &Path| {
                    let mut perf = read_json(&dir.join("perf.json"));
                    perf["p95"] = serde_json::json!(99.0);
                    write_json(&dir.join("perf.json"), perf);
                }),
            ),
            (
                "manifest-surface",
                "names surface `S002`",
                Box::new(|dir: &Path| {
                    let mut manifest = read_json(&dir.join("manifest.json"));
                    manifest["surface"] = serde_json::json!("S002");
                    write_json(&dir.join("manifest.json"), manifest);
                }),
            ),
        ];
        for (name, expect, mutate) in cases {
            case(name, expect, mutate);
        }
    }

    #[test]
    fn invalid_reference_incomplete_dimension_and_duplicate_id_fail() {
        let root = temp_root("rows");
        let invalid_ref = "S001,test,z.tsx,,PASS,PASS,PASS,PASS,PASS,VERIFIED,,";
        assert_eq!(run(&root, &[invalid_ref], Some("S001")), 1);
        let todo = "S001,test,a.tsx,,TODO,PASS,PASS,PASS,PASS,VERIFIED,,";
        assert_eq!(run(&root, &[todo], Some("S001")), 1);
        let duplicate = [ROW, ROW];
        assert_eq!(run(&root, &duplicate, Some("S001")), 1);
        let bad_format = "X001,test,a.tsx,,PASS,PASS,PASS,PASS,PASS,VERIFIED,,";
        assert_eq!(run(&root, &[bad_format], Some("S001")), 1);
        std::fs::remove_dir_all(root).unwrap();
    }
}
