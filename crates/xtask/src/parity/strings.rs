//! The i18n string layer (T3): every localized key maps to a surface.
//!
//! A label cannot render without a key, so the catalogs are the cheapest net
//! for forgotten UI. The validator parses both locale catalogs, requires the
//! `en` and `id` key sets to agree, and requires every key to be claimed by a
//! surface (derived by scanning the oracle source for the files that use the
//! key, then mapping files to surfaces through the inventory) or to be listed
//! as dead text with a reason.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::csv;

pub struct StringsOpts {
    /// Tree whose `src/i18n/locales/{en,id}/translation.json` are the catalogs
    /// and whose components use the keys (the oracle worktree's apps/desktop).
    pub root: PathBuf,
    pub surfaces: PathBuf,
    /// Optional complete key -> stable surface ID map. Columns: key,surface,note.
    /// An empty surface requires an evidence note declaring the key dead text.
    pub map: Option<PathBuf>,
    /// `--write-map <csv>`: emit the full derived key map (for ratification).
    pub write_map: Option<PathBuf>,
}

/// `cargo xtask parity strings [--root <dir>] [--map <csv>]`
pub fn cmd(args: &[String]) -> i32 {
    let root = flag_path(args, "--root").unwrap_or_else(|| PathBuf::from("apps/desktop"));
    let opts = StringsOpts {
        root,
        surfaces: flag_path(args, "--surfaces")
            .unwrap_or_else(|| PathBuf::from(".slim/deepwork/parity-surfaces.csv")),
        map: flag_path(args, "--map"),
        write_map: flag_path(args, "--write-map"),
    };
    run(&opts)
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

/// Flat key set from a translation catalog. Nested objects flatten with a dot
/// separator, matching Solid-i18n lookup keys.
fn read_catalog(path: &Path) -> Result<BTreeMap<String, String>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut out = BTreeMap::new();
    flatten("", &value, &mut out)?;
    Ok(out)
}

fn flatten(
    prefix: &str,
    value: &serde_json::Value,
    out: &mut BTreeMap<String, String>,
) -> Result<(), String> {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let joined = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten(&joined, child, out)?;
            }
            Ok(())
        }
        serde_json::Value::String(s) => {
            out.insert(prefix.to_string(), s.clone());
            Ok(())
        }
        other => Err(format!("key `{prefix}` holds a {other:?}, not a string")),
    }
}

/// Files under `<root>/src` that mention the key (exact literal or a dynamic
/// prefix like `menus.` or `menus.${`).
fn files_using_key(root: &Path, key: &str) -> Result<Vec<String>, String> {
    let src = root.join("src");
    let mut hits = Vec::new();
    collect_files(&src, &src, &mut |path, text| {
        if text.contains(&format!("\"{key}\"")) || text.contains(&format!("'{key}'")) {
            hits.push(format!("apps/desktop/src/{path}"));
            return;
        }
        // Dynamic key construction: `t(`prefix.${variant}`)` uses the key's
        // PARENT prefix, and i18next plurals use `t('base', {count})` which
        // resolves `base_one`/`base_other` at runtime. A key is "used" when
        // its parent prefix feeds a template literal, or when its plural base
        // appears as a literal.
        let parent = key.rsplit_once('.').map(|(p, _)| p);
        let dynamic = parent.map(|p| format!("{p}.${{"));
        let plural_base = key.rsplit_once('_').map(|(b, _)| b);
        let prefix = format!("{key}.");
        if text.contains(&prefix) && (text.contains("t(") || text.contains("i18n")) {
            hits.push(format!("apps/desktop/src/{path}"));
        } else if let Some(dynamic) = dynamic {
            if text.contains(&dynamic) {
                hits.push(format!("apps/desktop/src/{path}"));
            }
        } else if let Some(base) = plural_base {
            if text.contains(&format!("\"{base}\"")) || text.contains(&format!("'{base}'")) {
                hits.push(format!("apps/desktop/src/{path}"));
            }
        }
    })?;
    hits.sort();
    hits.dedup();
    Ok(hits)
}

fn collect_files(dir: &Path, base: &Path, f: &mut dyn FnMut(&str, &str)) -> Result<(), String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("cannot read source directory {}: {e}", dir.display()))?;
    for entry in entries {
        let entry =
            entry.map_err(|e| format!("cannot read directory entry in {}: {e}", dir.display()))?;
        let path = entry.path();
        let metadata = std::fs::metadata(&path)
            .map_err(|e| format!("cannot inspect source path {}: {e}", path.display()))?;
        if metadata.is_dir() {
            if path
                .file_name()
                .map(|n| n == "node_modules" || n == "__tests__" || n == "wasm")
                .unwrap_or(false)
            {
                continue;
            }
            collect_files(&path, base, f)?;
        } else {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            let Some(name) = name else { continue };
            if !(name.ends_with(".ts") || name.ends_with(".tsx")) {
                continue;
            }
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("cannot read source file {}: {e}", path.display()))?;
            let rel = path
                .strip_prefix(base)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| name.clone());
            f(&rel, &text);
        }
    }
    Ok(())
}

/// Build file -> stable surface ID and the set of IDs in the surfaces CSV.
fn file_to_surface(
    surfaces: &Path,
) -> Result<(BTreeMap<String, BTreeSet<String>>, BTreeSet<String>), String> {
    let text = std::fs::read_to_string(surfaces)
        .map_err(|e| format!("cannot read {}: {e}", surfaces.display()))?;
    let (header, rows) = csv::parse(&text)?;
    for column in ["id", "frozen_refs"] {
        if !header.iter().any(|name| name == column) {
            return Err(format!(
                "{} is missing `{column}` column",
                surfaces.display()
            ));
        }
    }
    let records = csv::records_as_maps(&header, &rows)?;
    let mut file_to_ids: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut surface_ids = BTreeSet::new();
    for (index, row) in records.iter().enumerate() {
        let refs = row.get("frozen_refs").map(String::as_str).unwrap_or("");
        let id = row.get("id").map(String::as_str).unwrap_or("").trim();
        if id.is_empty() {
            return Err(format!(
                "{} row {} has an empty surface ID",
                surfaces.display(),
                index + 2
            ));
        }
        surface_ids.insert(id.to_string());
        for file in refs.split(';').map(str::trim) {
            if !file.is_empty() {
                file_to_ids
                    .entry(file.to_string())
                    .or_default()
                    .insert(id.to_string());
            }
        }
    }
    Ok((file_to_ids, surface_ids))
}

struct Validation {
    errors: Vec<String>,
    en_count: usize,
    id_count: usize,
    unmapped: usize,
    map_rows: Vec<Vec<String>>,
}

fn has_dead_text_evidence(note: &str) -> bool {
    let Some((label, evidence)) = note.trim().split_once(':') else {
        return false;
    };
    if !label.trim().eq_ignore_ascii_case("dead text") {
        return false;
    }

    let evidence = evidence.to_ascii_lowercase();
    let has_search_provenance = evidence.contains("grep over src")
        || (evidence.contains("production")
            && evidence.contains("search")
            && evidence.contains("matches")
            && evidence.contains("files"));
    let has_locator = evidence.split_whitespace().any(|token| {
        let token = token.trim_matches(|c: char| {
            !c.is_ascii_alphanumeric() && !matches!(c, '/' | '_' | '.' | '-' | ':')
        });
        token
            .rsplit_once(':')
            .is_some_and(|(path, line)| path.contains('/') && line.parse::<u32>().is_ok())
    });
    has_search_provenance || has_locator
}

fn validate(opts: &StringsOpts) -> Result<Validation, String> {
    let en = match read_catalog(&opts.root.join("src/i18n/locales/en/translation.json")) {
        Ok(c) => c,
        Err(e) => return Err(e),
    };
    let id = match read_catalog(&opts.root.join("src/i18n/locales/id/translation.json")) {
        Ok(c) => c,
        Err(e) => return Err(e),
    };

    let mut errors: Vec<String> = Vec::new();
    for key in en.keys() {
        if !id.contains_key(key) {
            errors.push(format!("key `{key}` is in en but missing in id"));
        }
    }
    for key in id.keys() {
        if !en.contains_key(key) {
            errors.push(format!("key `{key}` is in id but missing in en"));
        }
    }

    let (surface_by_file, surface_ids) = file_to_surface(&opts.surfaces)?;

    // An explicit map is the reviewed, complete key -> stable surface ID map.
    let mut explicit: BTreeMap<String, (String, String)> = BTreeMap::new();
    if let Some(map_path) = &opts.map {
        let text = std::fs::read_to_string(map_path)
            .map_err(|e| format!("cannot read {}: {e}", map_path.display()))?;
        let (header, rows) = csv::parse(&text)?;
        for column in ["key", "surface", "note"] {
            if !header.iter().any(|name| name == column) {
                return Err(format!(
                    "{} is missing `{column}` column",
                    map_path.display()
                ));
            }
        }
        let rows = csv::records_as_maps(&header, &rows)?;
        for (index, row) in rows.iter().enumerate() {
            let row_number = index + 2;
            let key = row.get("key").map(String::as_str).unwrap_or("");
            let surface = row.get("surface").map(String::as_str).unwrap_or("");
            let note = row.get("note").map(String::as_str).unwrap_or("");
            if key.trim().is_empty() {
                errors.push(format!("map row {row_number} has an empty key"));
                continue;
            }
            if explicit.contains_key(key) {
                errors.push(format!("map row {row_number} duplicates key `{key}`"));
            } else {
                explicit.insert(key.to_string(), (surface.to_string(), note.to_string()));
            }
            if !en.contains_key(key) {
                errors.push(format!("map row {row_number} has unknown key `{key}`"));
            }
            if note
                .split(|c: char| !c.is_ascii_alphanumeric())
                .any(|word| {
                    word.eq_ignore_ascii_case("unresolved") || word.eq_ignore_ascii_case("todo")
                })
            {
                errors.push(format!("key `{key}` has an unresolved/TODO note"));
            }
            if surface.trim().is_empty() {
                if !has_dead_text_evidence(note) {
                    errors.push(format!(
                        "key `{key}` has no structured dead-text evidence for its empty mapping"
                    ));
                }
            } else {
                for destination in surface.split(';').map(str::trim) {
                    if destination.is_empty() {
                        errors.push(format!("key `{key}` has an empty destination ID"));
                    } else if !surface_ids.contains(destination) {
                        errors.push(format!(
                            "key `{key}` maps to unknown surface ID `{destination}`"
                        ));
                    }
                }
            }
        }
    }

    let mut unmapped = 0;
    let mut map_rows: Vec<Vec<String>> = Vec::new();
    for key in en.keys() {
        if opts.map.is_some() {
            if let Some((surface, note)) = explicit.get(key) {
                map_rows.push(vec![key.clone(), surface.clone(), note.clone()]);
            } else {
                unmapped += 1;
                errors.push(format!("key `{key}` is missing from the explicit map"));
                map_rows.push(vec![key.clone(), String::new(), String::new()]);
            }
            continue;
        }
        let files = files_using_key(&opts.root, key)
            .map_err(|e| format!("key `{key}` source scan failed: {e}"))?;
        let surfaces_for_key: BTreeSet<String> = files
            .iter()
            .filter_map(|f| surface_by_file.get(f))
            .flat_map(|ids| ids.iter().cloned())
            .collect();
        if surfaces_for_key.is_empty() {
            unmapped += 1;
            map_rows.push(vec![
                key.clone(),
                String::new(),
                if files.is_empty() {
                    "no usage found in the oracle source; dead-text candidate".to_string()
                } else {
                    format!("used by {}", files.join(", "))
                },
            ]);
            if unmapped <= 10 {
                errors.push(format!(
                    "key `{key}` is not claimed by any surface (used by: {})",
                    if files.is_empty() {
                        "nothing found; dead text?".to_string()
                    } else {
                        files.join(", ")
                    }
                ));
            }
        } else {
            map_rows.push(vec![
                key.clone(),
                surfaces_for_key.into_iter().collect::<Vec<_>>().join("; "),
                String::new(),
            ]);
        }
    }

    Ok(Validation {
        errors,
        en_count: en.len(),
        id_count: id.len(),
        unmapped,
        map_rows,
    })
}

fn run(opts: &StringsOpts) -> i32 {
    let validation = match validate(opts) {
        Ok(validation) => validation,
        Err(e) => {
            eprintln!("strings: {e}");
            return 1;
        }
    };

    if let Some(map_out) = &opts.write_map {
        let header = ["key", "surface", "note"];
        let document = csv::write(&header, &validation.map_rows);
        if let Some(parent) = map_out.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(map_out, &document);
        println!("wrote key map -> {}", map_out.display());
    }

    println!(
        "strings: en={} keys, id={} keys, {} unmapped",
        validation.en_count, validation.id_count, validation.unmapped
    );
    if validation.errors.is_empty() {
        println!("strings validate: PASS");
        0
    } else {
        for e in validation.errors.iter().take(10) {
            eprintln!("strings validate: FAIL {e}");
        }
        eprintln!("strings validate: {} problem(s)", validation.errors.len());
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    struct Fixture {
        path: PathBuf,
        opts: StringsOpts,
        map: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let path = loop {
                let path = std::env::temp_dir().join(format!(
                    "photrez-strings-{}-{}",
                    std::process::id(),
                    NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
                ));
                match std::fs::create_dir(&path) {
                    Ok(()) => break path,
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("create fixture {}: {error}", path.display()),
                }
            };
            let root = path.join("oracle");
            std::fs::create_dir_all(root.join("src/i18n/locales/en")).unwrap();
            std::fs::create_dir_all(root.join("src/i18n/locales/id")).unwrap();
            std::fs::write(
                root.join("src/i18n/locales/en/translation.json"),
                r#"{"tools":{"open":"Open"}}"#,
            )
            .unwrap();
            std::fs::write(
                root.join("src/i18n/locales/id/translation.json"),
                r#"{"tools":{"open":"Buka"}}"#,
            )
            .unwrap();
            std::fs::write(root.join("src/ui.ts"), r#"const label = t("tools.open");"#).unwrap();
            let surfaces = path.join("surfaces.csv");
            std::fs::write(
                &surfaces,
                "id,surface,frozen_refs\nS001,Workspace,apps/desktop/src/ui.ts\n",
            )
            .unwrap();
            let map = path.join("map.csv");
            std::fs::write(&map, "key,surface,note\ntools.open,S001,\n").unwrap();
            Self {
                path,
                opts: StringsOpts {
                    root,
                    surfaces,
                    map: Some(map.clone()),
                    write_map: None,
                },
                map,
            }
        }

        fn set_map(&self, csv: &str) {
            std::fs::write(&self.map, csv).unwrap();
        }
    }

    fn assert_error(fixture: &Fixture, expected: &str) {
        let errors = validate(&fixture.opts).unwrap().errors;
        assert!(
            errors.iter().any(|error| error.contains(expected)),
            "expected {expected:?} in {errors:?}"
        );
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn valid_id_mapping_passes() {
        assert!(validate(&Fixture::new().opts).unwrap().errors.is_empty());
    }

    #[test]
    fn key_can_map_to_multiple_surface_ids() {
        let fixture = Fixture::new();
        let surfaces = std::fs::read_to_string(&fixture.opts.surfaces).unwrap();
        std::fs::write(&fixture.opts.surfaces, format!("{surfaces}S002,Dialog,\n")).unwrap();
        fixture.set_map("key,surface,note\ntools.open,S001;S002,\n");
        assert!(validate(&fixture.opts).unwrap().errors.is_empty());
    }

    #[test]
    fn missing_used_key_fails() {
        let fixture = Fixture::new();
        fixture.set_map("key,surface,note\n");
        assert_error(
            &fixture,
            "key `tools.open` is missing from the explicit map",
        );
    }

    #[test]
    fn duplicate_key_fails() {
        let fixture = Fixture::new();
        fixture.set_map("key,surface,note\ntools.open,S001,\ntools.open,S001,\n");
        assert_error(&fixture, "duplicates key `tools.open`");
    }

    #[test]
    fn unknown_key_fails() {
        let fixture = Fixture::new();
        fixture.set_map("key,surface,note\ntools.open,S001,\nunknown.key,S001,\n");
        assert_error(&fixture, "unknown key `unknown.key`");
    }

    #[test]
    fn invalid_destination_fails() {
        let fixture = Fixture::new();
        fixture.set_map("key,surface,note\ntools.open,UNKNOWN,\n");
        assert_error(
            &fixture,
            "key `tools.open` maps to unknown surface ID `UNKNOWN`",
        );
    }

    #[test]
    fn restoring_valid_mapping_passes() {
        let fixture = Fixture::new();
        fixture.set_map("key,surface,note\n");
        assert_error(
            &fixture,
            "key `tools.open` is missing from the explicit map",
        );
        fixture.set_map("key,surface,note\ntools.open,S001,\n");
        assert!(validate(&fixture.opts).unwrap().errors.is_empty());
    }

    #[test]
    fn derived_mapping_uses_stable_surface_id() {
        let mut fixture = Fixture::new();
        fixture.opts.map = None;
        let validation = validate(&fixture.opts).unwrap();
        let row = validation
            .map_rows
            .iter()
            .find(|row| row[0] == "tools.open")
            .unwrap();
        assert_eq!(row[1], "S001");
    }

    #[test]
    fn derived_mapping_preserves_shared_file_surfaces() {
        let mut fixture = Fixture::new();
        fixture.opts.map = None;
        let surfaces = std::fs::read_to_string(&fixture.opts.surfaces).unwrap();
        std::fs::write(
            &fixture.opts.surfaces,
            format!("{surfaces}S002,Dialog,apps/desktop/src/ui.ts\n"),
        )
        .unwrap();

        let validation = validate(&fixture.opts).unwrap();
        let row = validation
            .map_rows
            .iter()
            .find(|row| row[0] == "tools.open")
            .unwrap();
        assert_eq!(row[1], "S001; S002");
    }

    #[test]
    fn locale_key_sets_must_match() {
        let fixture = Fixture::new();
        std::fs::write(
            fixture
                .opts
                .root
                .join("src/i18n/locales/id/translation.json"),
            "{}",
        )
        .unwrap();
        assert_error(&fixture, "key `tools.open` is in en but missing in id");
        std::fs::write(
            fixture
                .opts
                .root
                .join("src/i18n/locales/id/translation.json"),
            r#"{"tools":{"open":"Buka","save":"Simpan"}}"#,
        )
        .unwrap();
        assert_error(&fixture, "key `tools.save` is in id but missing in en");
    }

    #[test]
    fn dead_text_requires_an_evidence_note() {
        let fixture = Fixture::new();
        fixture.set_map("key,surface,note\ntools.open,,\n");
        assert_error(
            &fixture,
            "key `tools.open` has no structured dead-text evidence",
        );
        fixture.set_map("key,surface,note\ntools.open,,not rendered in any shipped view\n");
        assert_error(
            &fixture,
            "key `tools.open` has no structured dead-text evidence",
        );
        fixture.set_map(
            "key,surface,note\ntools.open,,dead text: production source search found 0 matches across 209 files; frozen test locator src/i18n/__tests__/i18n.test.ts:30\n",
        );
        assert!(validate(&fixture.opts).unwrap().errors.is_empty());
    }

    #[test]
    fn unresolved_todo_notes_fail_by_key() {
        let fixture = Fixture::new();
        fixture.set_map("key,surface,note\ntools.open,S001,UNRESOLVED pending review\n");
        assert_error(&fixture, "key `tools.open` has an unresolved/TODO note");
        fixture.set_map("key,surface,note\ntools.open,S001,TODO: owner review needed\n");
        assert_error(&fixture, "key `tools.open` has an unresolved/TODO note");
    }

    #[test]
    fn unreadable_source_entry_does_not_become_dead_text() {
        let mut fixture = Fixture::new();
        fixture.opts.map = None;
        std::fs::write(fixture.opts.root.join("src/ui.ts"), "const unused = true;").unwrap();
        let unreadable = fixture.opts.root.join("src/unreadable.ts");
        std::fs::write(&unreadable, [0xff]).unwrap();

        match validate(&fixture.opts) {
            Err(error) => {
                assert!(error.contains("key `tools.open`"), "{error}");
                assert!(
                    error.contains(unreadable.file_name().unwrap().to_string_lossy().as_ref()),
                    "{error}"
                );
            }
            Ok(validation) => {
                let candidate = validation
                    .map_rows
                    .iter()
                    .find(|row| row[0] == "tools.open")
                    .unwrap();
                assert!(
                    !(candidate[1].is_empty() && candidate[2].contains("dead-text candidate")),
                    "incomplete source scan emitted dead-text candidate {candidate:?}"
                );
                panic!("expected named source scan failure, got {candidate:?}");
            }
        }
    }

    #[test]
    fn missing_source_directory_reports_its_path() {
        let fixture = Fixture::new();
        let root = fixture.path.join("missing-source-root");
        let source = root.join("src");
        let error = files_using_key(&root, "tools.open").unwrap_err();
        assert!(error.contains(&source.display().to_string()), "{error}");
    }

    #[test]
    fn empty_map_rows_are_named() {
        let fixture = Fixture::new();
        fixture.set_map("key,surface,note\n,S001,missing key\n");
        assert_error(&fixture, "map row 2 has an empty key");
    }
}
