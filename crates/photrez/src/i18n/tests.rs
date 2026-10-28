use super::*;

/// The frozen reference the web build shipped, one directory per locale.
fn reference_path(code: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../apps/desktop/src/i18n/locales")
        .join(code)
        .join("translation.json")
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

/// The embedded copies are the frozen reference, byte for byte. A diff
/// here is never a translation decision: the reference changes first and
/// the copy step re-copies it.
#[test]
fn catalog_matches_reference() {
    for (locale, text) in CATALOGS {
        let reference = std::fs::read_to_string(reference_path(locale.code()))
            .expect("the frozen reference is in the tree");
        assert_eq!(
            *text,
            reference,
            "embedded {} catalog drifted from the reference",
            locale.code()
        );
    }
}

/// Every key in one locale exists in the other: a missing translation
/// renders as its key in the UI, which no capture can paper over.
#[test]
fn locale_parity() {
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

/// The leaf census the catalogs were frozen at; a wholesale loss (a
/// botched copy, a truncated file) fails here even though parity alone
/// would still hold.
#[test]
fn leaf_counts_are_pinned() {
    for locale in [Locale::En, Locale::Id] {
        assert_eq!(
            load(locale).entries.len(),
            457,
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
            continue; // parity owns missing keys
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
/// resolved them in the browser. Each entry is a reviewed decision: a new
/// code point either gains a glyph in the face or is added here with its
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
