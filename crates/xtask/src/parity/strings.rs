//! The i18n string layer (T3): every localized key maps to a surface.
//!
//! A label cannot render without a key, so the catalogs are the cheapest net
//! for forgotten UI. The validator parses both locale catalogs, requires the
//! `en` and `id` key sets to agree, and requires every key to be claimed by a
//! surface (derived by scanning the oracle source for the files that use the
//! key, then mapping files to surfaces through the inventory) or to be listed
//! as dead text with a reason.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::csv;

pub struct StringsOpts {
    /// Tree whose `src/i18n/locales/{en,id}/translation.json` are the catalogs
    /// and whose components use the keys (the oracle worktree's apps/desktop).
    pub root: PathBuf,
    pub surfaces: PathBuf,
    /// Optional key -> surface map overriding the derived mapping. Columns:
    /// key,surface,note. Rows with a note and an empty surface are dead-text
    /// declarations.
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
fn files_using_key(root: &Path, key: &str) -> Vec<String> {
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
    });
    hits.sort();
    hits.dedup();
    hits
}

fn collect_files(dir: &Path, base: &Path, f: &mut dyn FnMut(&str, &str)) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path
                .file_name()
                .map(|n| n == "node_modules" || n == "__tests__" || n == "wasm")
                .unwrap_or(false)
            {
                continue;
            }
            collect_files(&path, base, f);
        } else {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            let Some(name) = name else { continue };
            if !(name.ends_with(".ts") || name.ends_with(".tsx")) {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&path) {
                let rel = path
                    .strip_prefix(base)
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_else(|_| name.clone());
                f(&rel, &text);
            }
        }
    }
}

/// Build file -> surface from the surfaces CSV's file references.
fn file_to_surface(surfaces: &Path) -> Result<BTreeMap<String, String>, String> {
    let text = std::fs::read_to_string(surfaces)
        .map_err(|e| format!("cannot read {}: {e}", surfaces.display()))?;
    let (header, rows) = csv::parse(&text)?;
    let records = csv::records_as_maps(&header, &rows)?;
    let mut map = BTreeMap::new();
    for row in &records {
        let refs = row.get("frozen_refs").map(String::as_str).unwrap_or("");
        let surface = row.get("surface").cloned().unwrap_or_default();
        for file in refs.split(';').map(str::trim) {
            if !file.is_empty() {
                map.insert(file.to_string(), surface.clone());
            }
        }
    }
    Ok(map)
}

fn run(opts: &StringsOpts) -> i32 {
    let en = match read_catalog(&opts.root.join("src/i18n/locales/en/translation.json")) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("strings: {e}");
            return 1;
        }
    };
    let id = match read_catalog(&opts.root.join("src/i18n/locales/id/translation.json")) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("strings: {e}");
            return 1;
        }
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

    let surface_by_file = match file_to_surface(&opts.surfaces) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("strings: {e}");
            return 1;
        }
    };

    // Optional explicit map: key -> surface (or dead-text note).
    let mut explicit: BTreeMap<String, (String, String)> = BTreeMap::new();
    if let Some(map_path) = &opts.map {
        let parsed = std::fs::read_to_string(map_path)
            .map_err(|e| format!("cannot read {}: {e}", map_path.display()))
            .and_then(|t| csv::parse(&t))
            .and_then(|(h, r)| csv::records_as_maps(&h, &r));
        match parsed {
            Ok(rows) => {
                for row in rows {
                    let key = row.get("key").cloned().unwrap_or_default();
                    explicit.insert(
                        key,
                        (
                            row.get("surface").cloned().unwrap_or_default(),
                            row.get("note").cloned().unwrap_or_default(),
                        ),
                    );
                }
            }
            Err(e) => {
                eprintln!("strings: {e}");
                return 1;
            }
        }
    }

    let mut unmapped = 0;
    let mut map_rows: Vec<Vec<String>> = Vec::new();
    for key in en.keys() {
        if let Some((surface, note)) = explicit.get(key) {
            if surface.is_empty() && note.is_empty() {
                errors.push(format!("key `{key}` has an empty map row"));
            }
            map_rows.push(vec![key.clone(), surface.clone(), note.clone()]);
            continue;
        }
        let files = files_using_key(&opts.root, key);
        let surfaces_for_key: Vec<String> = files
            .iter()
            .filter_map(|f| surface_by_file.get(f).cloned())
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
                surfaces_for_key.join("; "),
                String::new(),
            ]);
        }
    }

    // `--write-map <csv>` emits the full derived key -> surface map so the
    // unmapped rows can be ratified (a surface, or a dead-text reason).
    if let Some(map_out) = &opts.write_map {
        let header = ["key", "surface", "note"];
        let document = csv::write(&header, &map_rows);
        if let Some(parent) = map_out.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(map_out, &document);
        println!("wrote key map -> {}", map_out.display());
    }

    println!(
        "strings: en={} keys, id={} keys, {} unmapped",
        en.len(),
        id.len(),
        unmapped
    );
    if errors.is_empty() {
        println!("strings validate: PASS");
        0
    } else {
        for e in errors.iter().take(10) {
            eprintln!("strings validate: FAIL {e}");
        }
        eprintln!("strings validate: {} problem(s)", errors.len());
        1
    }
}
