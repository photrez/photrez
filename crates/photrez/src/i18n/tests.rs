use super::*;

/// The pinned reference catalogs, one directory per locale.
// Reference copies live with the test fixtures now that the retired tree is gone.
fn reference_path(code: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/i18n")
        .join(code)
        .join("translation.json")
}

/// Every leaf path in a nested catalog, joined with `.`.
fn leaf_paths(
    value: &serde_json::Value,
    prefix: &str,
    out: &mut std::collections::BTreeMap<String, String>,
) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, item) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                leaf_paths(item, &path, out);
            }
        }
        serde_json::Value::String(s) => {
            out.insert(prefix.to_owned(), s.clone());
        }
        _ => {}
    }
}

fn reference_leaves(code: &str) -> std::collections::BTreeMap<String, String> {
    let text = std::fs::read_to_string(reference_path(code))
        .expect("the reference catalog is in the tree");
    let value: serde_json::Value =
        serde_json::from_str(&text).expect("the reference catalog parses");
    let mut out = std::collections::BTreeMap::new();
    leaf_paths(&value, "", &mut out);
    out
}

/// The catalog is the reference catalog plus the deltas the reference cannot
/// express, and nothing else. A byte copy is impossible: the reference reaches
/// three align strings only through inline defaults, keeps the text tool's
/// commit/newline leaves under a `tools.text` object where the native catalog
/// needs a `tools.text` label, and the reference's own dead menu string is not
/// wanted here. Each of those is a named row below, so a new divergence still
/// fails the test instead of being absorbed by a re-copy.
#[test]
fn catalog_matches_reference_plus_named_deltas() {
    const NATIVE_ONLY: &[&str] = &[
        // The reference hardcodes both of these in the inspector instead of
        // putting them in its catalog. The native Paper Size row reads them,
        // because it answers what the driver reported rather than asserting the
        // chosen paper is missing.
        "dialogs.print.paperListUnavailable",
        "dialogs.print.printerOffers",
        // The reference never prompts at startup for an autosave, so its catalog
        // has no key for the prompt's open-document marker.
        "dialogs.recovery.alreadyOpen",
        // The reference spells these three through `t(key, default)`; here they
        // are real entries so the label resolves without a fallback.
        "properties.alignCenter",
        "properties.alignLeft",
        "properties.alignRight",
        // The reference's `tools.text` is an object, so it has no leaf for the
        // tool's own name; the two action strings move to `tools.textActions`.
        "tools.text",
        "tools.textActions.commit",
        "tools.textActions.newline",
    ];
    const REFERENCE_ONLY: &[&str] = &["tools.text.commit", "tools.text.newline"];
    // A shared key whose native text differs on purpose. The reference renders an
    // em dash in the About tagline; this project's copy rule forbids that glyph in
    // user-facing text, so the tagline diverges by decision. Each row states the
    // accepted text for both locales, so the divergence is checked rather than
    // excused, and a further change to the line still fails here.
    const TEXT_DELTAS: &[(&str, &str, &str)] = &[(
        "dialogs.about.tagline",
        "Native desktop image studio. Lean. Offline. Under 80 MB.",
        "Studio gambar desktop native. Ramping. Luring. Di bawah 80 MB.",
    )];

    for (locale, text) in CATALOGS {
        let native: std::collections::BTreeMap<String, String> = {
            let value: serde_json::Value =
                serde_json::from_str(text).expect("the embedded catalog parses");
            let mut out = std::collections::BTreeMap::new();
            leaf_paths(&value, "", &mut out);
            out
        };
        let reference = reference_leaves(locale.code());
        let native_only: Vec<_> = native
            .keys()
            .filter(|key| !reference.contains_key(*key))
            .cloned()
            .collect();
        let reference_only: Vec<_> = reference
            .keys()
            .filter(|key| !native.contains_key(*key))
            .cloned()
            .collect();
        assert_eq!(
            native_only,
            NATIVE_ONLY,
            "{}: unrecorded keys exist only in the native catalog",
            locale.code()
        );
        assert_eq!(
            reference_only,
            REFERENCE_ONLY,
            "{}: unrecorded keys are missing from the native catalog",
            locale.code()
        );
        let renamed = [
            ("tools.text.commit", "tools.textActions.commit"),
            ("tools.text.newline", "tools.textActions.newline"),
        ];
        for (from, to) in renamed {
            assert_eq!(
                reference[from],
                native[to],
                "{}: {to} must carry the reference text of {from}",
                locale.code()
            );
        }
        for (key, value) in &reference {
            if REFERENCE_ONLY.contains(&key.as_str()) {
                continue;
            }
            if let Some((_, en_text, id_text)) = TEXT_DELTAS
                .iter()
                .find(|(deltas_key, ..)| deltas_key == key)
            {
                let expected = if locale.code() == "en" {
                    *en_text
                } else {
                    *id_text
                };
                assert_eq!(
                    native.get(key).map(String::as_str),
                    Some(expected),
                    "{}: {key} is a named copy divergence and must keep its recorded text",
                    locale.code()
                );
                continue;
            }
            assert_eq!(
                native.get(key).map(String::as_str),
                Some(value.as_str()),
                "{}: shared key {key} diverges from the reference text",
                locale.code()
            );
        }
    }
}

/// A lookup the catalog cannot answer renders its key on screen, and no
/// capture shows that is intentional. Every literal key the views ask for must
/// resolve in both embedded catalogs.
#[test]
fn every_literal_lookup_resolves() {
    let en = load(Locale::En);
    let id = load(Locale::Id);
    let mut unresolved = Vec::new();
    for path in source_files(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .as_path(),
    ) {
        let text = std::fs::read_to_string(&path).expect("a source file reads");
        let production = text.split("#[cfg(test)]").next().unwrap_or_default();
        for key in literal_lookup_keys(production) {
            for (code, catalog) in [("en", &en), ("id", &id)] {
                if !catalog.entries.contains_key(&key) {
                    unresolved.push(format!(
                        "{}:{}: {key} missing from {code}",
                        path.display(),
                        line_of(production, &key)
                    ));
                }
            }
        }
    }
    assert!(
        unresolved.is_empty(),
        "unresolved lookups:\n{}",
        unresolved.join("\n")
    );
}

/// `t("k")`, `t_args("k", ..)` and `t_plural("k", ..)` literals; a plural
/// lookup is expanded to the two variant keys the catalog actually stores.
fn literal_lookup_keys(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut keys = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if (bytes[i] & 0b1100_0000) == 0b1000_0000 {
            i += 1;
            continue; // never cut a multi-byte character in half
        }
        let boundary =
            i == 0 || !matches!(bytes[i - 1], b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_');
        let (name, is_plural) = if boundary && text[i..].starts_with("t_plural(") {
            ("t_plural(", true)
        } else if boundary && text[i..].starts_with("t_args(") {
            ("t_args(", false)
        } else if boundary && text[i..].starts_with("t(") {
            ("t(", false)
        } else {
            i += 1;
            continue;
        };
        let mut j = i + name.len();
        while j < bytes.len() && matches!(bytes[j], b' ' | b'\r' | b'\n' | b'\t') {
            j += 1;
        }
        if bytes.get(j) == Some(&b'"') {
            if let Some(end) = text[j + 1..].find('"') {
                let key = text[j + 1..j + 1 + end].to_owned();
                if is_plural {
                    keys.push(format!("{key}_one"));
                    keys.push(format!("{key}_other"));
                } else {
                    keys.push(key);
                }
            }
        }
        i = j;
    }
    keys.sort();
    keys.dedup();
    keys
}

/// The 1-based line a key is first asked for, for the failure message only.
fn line_of(text: &str, key: &str) -> usize {
    text.match_indices(&format!("\"{key}\""))
        .map(|(at, _)| text[..at].matches('\n').count() + 1)
        .next()
        .unwrap_or(0)
}

fn source_files(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(source_files(&path));
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default();
            let parent = path
                .parent()
                .and_then(|p| p.file_name().and_then(|n| n.to_str()).map(str::to_owned));
            // The fixture catalogs and the reference-copies are data, not lookups.
            if name != "tests.rs" && parent.as_deref() != Some("tests") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

fn load(locale: Locale) -> Catalog {
    let (_, text) = CATALOGS
        .iter()
        .find(|(candidate, _)| *candidate == locale)
        .expect("every locale ships a catalog");
    Catalog::from_json(text).expect("the embedded catalog parses")
}

/// `{{name}}` tokens as they appear in a value, in order.
fn tokens(value: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut rest = value;
    while let Some(open) = rest.find("{{") {
        let after_open = &rest[open + 2..];
        let Some(close) = after_open.find("}}") else {
            break;
        };
        found.push(after_open[..close].trim().to_owned());
        rest = &after_open[close + 2..];
    }
    found
}

/// `{{name}}` tokens as a sorted, deduplicated set.
fn token_set(value: &str) -> Vec<String> {
    let mut set = tokens(value);
    set.sort();
    set.dedup();
    set
}

/// Every key in one locale exists in the other: a missing translation
/// renders as its key in the UI, which no capture can paper over.
#[test]
fn locale_key_sets_match() {
    let en = load(Locale::En);
    let id = load(Locale::Id);
    let missing_in_id: Vec<_> = en
        .entries
        .keys()
        .filter(|key| !id.entries.contains_key(*key))
        .collect();
    let extra_in_id: Vec<_> = id
        .entries
        .keys()
        .filter(|key| !en.entries.contains_key(*key))
        .collect();
    assert!(
        missing_in_id.is_empty() && extra_in_id.is_empty(),
        "id is missing {missing_in_id:?}; id has extra {extra_in_id:?}"
    );
}

/// Loading IS the leaf check: [`flatten`] rejects empty and non-string
/// leaves, so this passing means neither catalog has any.
#[test]
fn no_empty_or_nonstring_leaves() {
    for locale in [Locale::En, Locale::Id] {
        load(locale);
    }
}

/// The strictness is real, one case per rejection kind.
#[test]
fn leaf_rejections_are_enforced() {
    assert!(
        matches!(Catalog::from_json("{\"a\": \"\"}"), Err(LoadError::EmptyLeaf(key)) if key == "a")
    );
    assert!(
        matches!(Catalog::from_json("{\"a\": 1}"), Err(LoadError::NonStringLeaf(key)) if key == "a")
    );
    assert!(matches!(Catalog::from_json("{"), Err(LoadError::Json(_))));
    assert!(matches!(
        Catalog::from_json("[\"a\"]"),
        Err(LoadError::NonStringLeaf(_))
    ));
}

/// The leaf census the catalogs were pinned at; a wholesale loss (a
/// botched copy, a truncated file) fails here even though the key-set
/// comparison alone would still hold.
#[test]
fn leaf_counts_are_pinned() {
    for locale in [Locale::En, Locale::Id] {
        assert_eq!(
            load(locale).entries.len(),
            462,
            "{} leaf count",
            locale.code()
        );
    }
}

/// The placeholder inventory is the engine's whole workload, and every
/// key must name the same tokens in both locales: a translation that
/// drops `{{count}}` renders a plural silently wrong.
#[test]
fn placeholder_sets_match_across_locales() {
    let en = load(Locale::En);
    let id = load(Locale::Id);
    let all: std::collections::BTreeSet<String> = en
        .entries
        .values()
        .flat_map(|value| tokens(value))
        .collect();
    let expected: std::collections::BTreeSet<String> = ["angle", "count", "distance", "name"]
        .map(str::to_owned)
        .into_iter()
        .collect();
    assert_eq!(
        all, expected,
        "the placeholder inventory changed; the renderer must learn the new token"
    );
    for (key, en_value) in &en.entries {
        let Some(id_value) = id.entries.get(key) else {
            continue; // the key-set comparison owns missing keys
        };
        assert_eq!(
            token_set(en_value),
            token_set(id_value),
            "placeholder mismatch on {key}"
        );
    }
}

#[test]
fn interpolation_renders_placeholders() {
    assert_eq!(
        render("{{count}} layers", &[("count", "3")]).as_deref(),
        Ok("3 layers")
    );
    assert_eq!(render("no tokens", &[]).as_deref(), Ok("no tokens"));
    assert_eq!(
        render("{{a}} then {{b}}", &[("a", "1"), ("b", "2")]).as_deref(),
        Ok("1 then 2")
    );
    assert_eq!(
        render("{{ spaced }}", &[("spaced", "x")]).as_deref(),
        Ok("x")
    );
}

#[test]
fn interpolation_rejects_mismatches() {
    assert_eq!(
        render("Hello {{name}}", &[]),
        Err(RenderError::UnknownPlaceholder("name".to_owned()))
    );
    assert_eq!(
        render("Hello", &[("name", "Ada")]),
        Err(RenderError::UnusedArg("name".to_owned()))
    );
    assert_eq!(
        render("Hi {{name", &[("name", "Ada")]),
        Err(RenderError::Malformed("Hi {{name".to_owned()))
    );
}

/// The one locale-switching test: locale-dependent lookup assertions
/// live here or in [`plural_selection_is_real`], both serialised on
/// `super::TEST_LOCALE_LOCK`, because the harness runs tests on several threads and
/// `set_locale` is process-wide.
#[test]
fn active_locale_switches_text() {
    let _guard = super::TEST_LOCALE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let previous = locale();
    let en = load(Locale::En);
    let id = load(Locale::Id);
    let differing = en
        .entries
        .iter()
        .find(|(key, value)| {
            id.entries
                .get(*key)
                .is_some_and(|id_value| id_value != *value)
        })
        .map(|(key, _)| key.clone())
        .expect("the catalogs differ somewhere");

    set_locale(Locale::En);
    assert_eq!(t(&differing), en.entries[&differing]);

    set_locale(Locale::Id);
    assert_eq!(t(&differing), id.entries[&differing]);

    set_locale(previous);
}

/// `count` selects the variant through real keys: `en` distinguishes
/// 1-vs-many, `id` ships identical text for both variants on purpose
/// (correct for Indonesian) and keeps both keys rather than collapsing
/// to one.
#[test]
fn plural_selection_is_real() {
    let _guard = super::TEST_LOCALE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let previous = locale();
    let en = load(Locale::En);
    let id = load(Locale::Id);
    assert_ne!(
        en.entries["status.layers_one"], en.entries["status.layers_other"],
        "en must keep a real 1-vs-many distinction"
    );
    assert_eq!(
        id.entries["status.layers_one"], id.entries["status.layers_other"],
        "id ships the same text for both variants; the keys stay explicit"
    );

    set_locale(Locale::En);
    let one = t_plural("status.layers", 1);
    let many = t_plural("status.layers", 2);
    assert_eq!(
        one,
        en.entries["status.layers_one"].replace("{{count}}", "1")
    );
    assert_eq!(
        many,
        en.entries["status.layers_other"].replace("{{count}}", "2")
    );
    assert_eq!(
        t_plural("status.layers", 0),
        en.entries["status.layers_other"].replace("{{count}}", "0")
    );
    assert_ne!(
        one, many,
        "count=1 and count=2 must render differently in en"
    );

    set_locale(Locale::Id);
    let one = t_plural("status.layers", 1);
    let many = t_plural("status.layers", 2);
    assert_eq!(
        one,
        id.entries["status.layers_one"].replace("{{count}}", "1")
    );
    assert_eq!(
        many,
        id.entries["status.layers_other"].replace("{{count}}", "2")
    );
    assert_ne!(
        one, many,
        "the interpolated count differs even though the templates match"
    );

    set_locale(previous);
}

/// Missing keys and wrong arguments come back as the key, never as a
/// panic. The outcome is locale-independent, so this test is safe beside
/// the one that switches locales.
#[test]
fn lookup_failures_degrade_to_the_key() {
    assert_eq!(t("no.such.key"), "no.such.key");
    // `status.layers_other` names {{count}} in both locales
    assert_eq!(t_args("status.layers_other", &[]), "status.layers_other");
    assert_eq!(t_args("no.such.key", &[("count", "1")]), "no.such.key");
}

#[test]
fn locale_normalisation_degrades_to_english() {
    assert_eq!(Locale::normalize("id"), Locale::Id);
    assert_eq!(Locale::normalize("id-ID"), Locale::Id);
    assert_eq!(Locale::normalize("ID"), Locale::Id);
    assert_eq!(Locale::normalize("id_ID.UTF-8"), Locale::Id);
    assert_eq!(Locale::normalize("en"), Locale::En);
    assert_eq!(Locale::normalize("en-US"), Locale::En);
    assert_eq!(Locale::normalize("fr-FR"), Locale::En);
    assert_eq!(Locale::normalize(""), Locale::En);
}

#[test]
fn detection_order_is_env_then_stored_then_os_then_english() {
    assert_eq!(
        detect_locale(None, Some("id".to_owned()), "en-US"),
        Locale::Id
    );
    assert_eq!(
        detect_locale(None, Some("id-ID".to_owned()), "en"),
        Locale::Id
    );
    assert_eq!(detect_locale(None, None, "id_ID.UTF-8"), Locale::Id);
    assert_eq!(
        detect_locale(None, Some("  ".to_owned()), "id_ID.UTF-8"),
        Locale::Id
    );
    assert_eq!(detect_locale(None, None, ""), Locale::En);
    assert_eq!(
        detect_locale(None, Some("fr".to_owned()), "id_ID"),
        Locale::En
    );
}

/// The capture driver's pin outranks everything else: a machine with a
/// stored `id` preference and an `id` OS tag still resolves `en` under
/// `PHOTREZ_LOCALE=en`; a blank value falls through untouched.
#[test]
fn env_override_outranks_stored_and_os() {
    assert_eq!(
        detect_locale(Some("en"), Some("id".to_owned()), "id_ID.UTF-8"),
        Locale::En
    );
    assert_eq!(
        detect_locale(Some("id"), Some("en".to_owned()), "en-US"),
        Locale::Id
    );
    assert_eq!(
        detect_locale(Some("  "), Some("id".to_owned()), "en"),
        Locale::Id
    );
    assert_eq!(
        detect_locale(Some("fr"), Some("en".to_owned()), "en"),
        Locale::En
    );
}

/// A stored preference survives a reload, through the same store the
/// startup detection reads.
#[test]
fn locale_round_trip_through_settings() {
    let dir = std::env::temp_dir().join(format!("photrez-i18n-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let path = dir.join("settings.json");

    let store = crate::services::settings::FileSettings::at(&path);
    store.write(LOCALE_KEY, Locale::Id.code());
    let reloaded = crate::services::settings::FileSettings::at(&path);
    assert_eq!(
        initial_locale(Some(&reloaded as &dyn SettingsStore)),
        Locale::Id
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Collect every character a parsed catalog carries: object keys, string
/// values, array items.
fn collect_catalog_chars(value: &serde_json::Value, out: &mut std::collections::BTreeSet<char>) {
    match value {
        serde_json::Value::String(s) => {
            out.extend(s.chars());
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_catalog_chars(item, out);
            }
        }
        serde_json::Value::Object(map) => {
            for (key, item) in map {
                out.extend(key.chars());
                collect_catalog_chars(item, out);
            }
        }
        _ => {}
    }
}

/// Code points the embedded catalogs use that `inter-latin.ttf` does not
/// carry. They render through the toolkit's per-glyph fallback stack -
/// `gpui_pre::TextSystem` defaults to `.ZedMono, .ZedSans, Helvetica,
/// Segoe UI, ...` and this app never overrides it; `Segoe UI` was measured
/// to carry all five, which is also how the reference's Latin subsets
/// resolved them in the previous renderer. Each entry is a reviewed decision:
/// a new code point either gains a glyph in the face or is added here with its
/// covering fallback named.
const FALLBACK_RENDERED: &[char] = &[
    '\u{00B0}', // degree sign - rotation labels and tooltips
    '\u{2014}', // em dash
    '\u{2026}', // horizontal ellipsis - menu labels
    '\u{2192}', // rightwards arrow - the foreground/background swatch label
    '\u{2194}', // left-right arrow
];

/// Gate: every non-ASCII code point the embedded `id` catalog uses must
/// render - from the registered face (`crate::INTER`, the same bytes
/// `app.rs` hands to `add_fonts`) or, for the reviewed list above, from
/// the toolkit's fallback stack. Ignored by default because it parses the
/// font binary; run explicitly:
/// `cargo test -p photrez i18n::tests::indonesian_glyph_coverage -- --ignored`
#[test]
#[ignore = "font gate; run explicitly"]
fn indonesian_glyph_coverage() {
    let id_json = CATALOGS
        .iter()
        .find(|(locale, _)| *locale == Locale::Id)
        .map(|(_, json)| *json)
        .expect("the id catalog is embedded");

    let value: serde_json::Value =
        serde_json::from_str(id_json).expect("the embedded id catalog parses");
    let mut corpus = std::collections::BTreeSet::new();
    collect_catalog_chars(&value, &mut corpus);
    corpus.retain(|c| !c.is_ascii());
    assert!(
        !corpus.is_empty(),
        "the id catalog carries non-ASCII characters"
    );

    let face = ttf_parser::Face::parse(crate::INTER, 0).expect("the embedded face parses");
    let unreviewed: Vec<char> = corpus
        .iter()
        .copied()
        .filter(|c| face.glyph_index(*c).is_none() && !FALLBACK_RENDERED.contains(c))
        .collect();
    assert!(
        unreviewed.is_empty(),
        "neither in the registered face nor reviewed fallbacks: {}",
        unreviewed
            .iter()
            .map(|c| format!("U+{:04X} {c:?}", *c as u32))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let in_face = corpus
        .iter()
        .filter(|c| face.glyph_index(**c).is_some())
        .count();
    eprintln!(
        "glyph coverage: {} code point(s) - {in_face} in the face, {} fallback-reviewed",
        corpus.len(),
        corpus.len() - in_face
    );
}
