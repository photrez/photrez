//! # i18n - locale entity and message catalog
//!
//! Owns: the locale entity and the message catalog that resolves keys to
//! user-visible strings.
//!
//! Must not own: user-visible strings hardcoded anywhere else in the crate -
//! every text the user reads goes through this catalog.
//!
//! Call sites use [`t`] / [`t_args`] / [`t_plural`]; [`set_locale`] swaps the
//! active locale (returning the previous one so the caller that owns the
//! redraw knows whether to re-render); [`initial_locale`] is the startup
//! detection order - stored preference, then the OS locale, then `en`.

#![allow(dead_code)] // consumers land with the view sweep; drop this blanket as they arrive

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

use crate::services::settings::SettingsStore;

/// The settings key the chosen locale persists under - the same key the web
/// build kept in storage.
pub const LOCALE_KEY: &str = "photrez.locale";

/// The embedded catalogs, paired with their locale, in the order the catalog
/// files live under `assets/i18n/`. Adding a locale buys a catalog file and
/// a row here; the parity test and the lookup both read this table.
const CATALOGS: &[(Locale, &str)] = &[
    (Locale::En, include_str!("../../assets/i18n/en.json")),
    (Locale::Id, include_str!("../../assets/i18n/id.json")),
];

// ── locale ───────────────────────────────────────────────────────────────────

/// The locales this build ships. Unknown tags degrade to `En` through
/// [`Locale::normalize`] - the catalog lookup must always have an arm.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Locale {
    En,
    Id,
}

impl Locale {
    /// The language tag used as the catalog subdirectory name and the value
    /// persisted in the settings file.
    pub fn code(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::Id => "id",
        }
    }

    /// The language part of a tag, lowercased: `"id-ID"`, `"id_ID.UTF-8"`
    /// and `"ID"` all name [`Locale::Id`]. Anything else is `En` - the
    /// fallback the web build used when no catalog matched.
    pub fn normalize(tag: &str) -> Self {
        let language = tag.split(['-', '_', '.']).next().unwrap_or_default();
        match language.to_ascii_lowercase().as_str() {
            "id" => Locale::Id,
            _ => Locale::En,
        }
    }
}

/// Index of the active catalog; the atomic keeps lookups lock-free on the
/// UI thread.
static ACTIVE: AtomicUsize = AtomicUsize::new(0);

fn index_of(locale: Locale) -> usize {
    CATALOGS
        .iter()
        .position(|(candidate, _)| *candidate == locale)
        .unwrap_or(0)
}

/// The active locale.
pub fn locale() -> Locale {
    CATALOGS
        .get(ACTIVE.load(Ordering::Relaxed))
        .map(|(locale, _)| *locale)
        .unwrap_or(Locale::En)
}

/// Swaps the active locale and returns the previous one. Persisting the
/// choice is deliberately the caller's job: the write is file I/O, and the
/// caller that flips the preference also decides when the shell re-renders.
pub fn set_locale(next: Locale) -> Locale {
    let previous = locale();
    ACTIVE.store(index_of(next), Ordering::Relaxed);
    previous
}

// ── catalog ──────────────────────────────────────────────────────────────────

/// Why an embedded catalog could not be loaded. All three cases are build
/// defects the tests below catch; the runtime lookup still degrades (keys
/// come back as themselves) instead of panicking.
#[derive(Debug, PartialEq, Eq)]
pub enum LoadError {
    /// The asset is not valid JSON.
    Json(String),
    /// A leaf is not a string (a number, an array, `null`, ...).
    NonStringLeaf(String),
    /// A leaf is an empty string: an empty key is a translation hole, not a
    /// translation.
    EmptyLeaf(String),
}

/// A flattened view of one catalog file: `"status.layers_one"` -> the value.
/// `BTreeMap` so iteration (and any diff a test prints) is deterministic.
struct Catalog {
    entries: BTreeMap<String, String>,
}

impl Catalog {
    fn from_json(text: &str) -> Result<Catalog, LoadError> {
        let root: serde_json::Value =
            serde_json::from_str(text).map_err(|error| LoadError::Json(error.to_string()))?;
        let mut entries = BTreeMap::new();
        flatten("", &root, &mut entries)?;
        Ok(Catalog { entries })
    }

    fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(String::as_str)
    }
}

/// Flattens `{"a": {"b": "x"}}` into `"a.b" -> "x"`; every leaf must be a
/// non-empty string.
fn flatten(
    prefix: &str,
    value: &serde_json::Value,
    out: &mut BTreeMap<String, String>,
) -> Result<(), LoadError> {
    match value {
        serde_json::Value::Object(map) => {
            for (segment, child) in map {
                let key = if prefix.is_empty() {
                    segment.clone()
                } else {
                    format!("{prefix}.{segment}")
                };
                flatten(&key, child, out)?;
            }
            Ok(())
        }
        serde_json::Value::String(text) if text.is_empty() => {
            Err(LoadError::EmptyLeaf(prefix.to_string()))
        }
        serde_json::Value::String(text) => {
            out.insert(prefix.to_string(), text.clone());
            Ok(())
        }
        _ => Err(LoadError::NonStringLeaf(prefix.to_string())),
    }
}

/// Every embedded catalog, parsed once. A failure here can only come from a
/// broken asset (the tests gate those) and degrades to an empty catalog -
/// lookups then return the key, which any capture shows plainly.
fn catalogs() -> &'static [Catalog] {
    static PARSED: OnceLock<Vec<Catalog>> = OnceLock::new();
    PARSED.get_or_init(|| {
        CATALOGS
            .iter()
            .map(|(_, text)| {
                Catalog::from_json(text).unwrap_or_else(|_| Catalog {
                    entries: BTreeMap::new(),
                })
            })
            .collect()
    })
}

fn lookup(key: &str) -> Option<&'static str> {
    catalogs()
        .get(ACTIVE.load(Ordering::Relaxed))
        .and_then(|catalog| catalog.get(key))
}

// ── interpolation ────────────────────────────────────────────────────────────

/// Why a template could not be rendered. Both mismatch kinds are rejected
/// rather than passed through: a call site that passes the wrong arguments
/// is a programming defect, and tests catch it the moment a value grows a
/// placeholder the caller does not feed.
#[derive(Debug, PartialEq, Eq)]
pub enum RenderError {
    /// The value names a `{{placeholder}}` no argument fills.
    UnknownPlaceholder(String),
    /// An argument was supplied that the value never names.
    UnusedArg(String),
    /// A `{{` that never closes - not a template this engine can render.
    Malformed(String),
}

/// Renders `{{name}}` placeholders from `args`. The grammar is exactly the
/// shipped catalogs': no nesting, no escaping, names trimmed of surrounding
/// spaces.
pub fn render(value: &str, args: &[(&str, &str)]) -> Result<String, RenderError> {
    let mut out = String::with_capacity(value.len());
    let mut used: Vec<&str> = Vec::new();
    let mut rest = value;
    while let Some(open) = rest.find("{{") {
        out.push_str(&rest[..open]);
        let after_open = &rest[open + 2..];
        let Some(close) = after_open.find("}}") else {
            return Err(RenderError::Malformed(value.to_string()));
        };
        let name = after_open[..close].trim();
        let Some((_, replacement)) = args.iter().find(|(key, _)| *key == name) else {
            return Err(RenderError::UnknownPlaceholder(name.to_string()));
        };
        out.push_str(replacement);
        used.push(name);
        rest = &after_open[close + 2..];
    }
    out.push_str(rest);
    if let Some((key, _)) = args.iter().find(|(key, _)| !used.contains(key)) {
        return Err(RenderError::UnusedArg((*key).to_string()));
    }
    Ok(out)
}

// ── text lookup ──────────────────────────────────────────────────────────────

/// The active locale's text for `key`. A missing key comes back as the key
/// itself: the parity test gates the catalogs, and a key rendered in a
/// capture says exactly which lookup failed.
pub fn t(key: &str) -> String {
    lookup(key)
        .map(str::to_owned)
        .unwrap_or_else(|| key.to_owned())
}

/// [`t`] for values that name `{{placeholders}}`. A render mismatch degrades
/// to the key - again the loudest thing a capture can show without
/// panicking.
pub fn t_args(key: &str, args: &[(&str, &str)]) -> String {
    let Some(value) = lookup(key) else {
        return key.to_owned();
    };
    render(value, args).unwrap_or_else(|_| key.to_owned())
}

/// The active locale's `{base}_one` / `{base}_other` pair for `count`, with
/// `{{count}}` interpolated when the value names it. Two plural forms are
/// the whole rule both shipped locales use.
pub fn t_plural(base: &str, count: u32) -> String {
    let key = format!("{base}_{}", if count == 1 { "one" } else { "other" });
    let Some(value) = lookup(&key) else {
        return key;
    };
    let count_text = count.to_string();
    let args: &[(&str, &str)] = if value.contains("{{count}}") {
        &[("count", count_text.as_str())]
    } else {
        &[]
    };
    render(value, args).unwrap_or_else(|_| key)
}

// ── startup detection ────────────────────────────────────────────────────────

/// The startup detection order: the stored preference, then the OS locale,
/// then `en`. Every failure at every step degrades to `en`.
pub fn initial_locale(store: Option<&dyn SettingsStore>) -> Locale {
    detect_locale(
        store.and_then(|store| store.read(LOCALE_KEY)),
        &os_language_tag(),
    )
}

/// The detection rule, split from its two inputs so tests stay deterministic
/// (neither the file system nor the environment is test input).
fn detect_locale(stored: Option<String>, os_tag: &str) -> Locale {
    if let Some(tag) = stored {
        if !tag.trim().is_empty() {
            return Locale::normalize(&tag);
        }
    }
    Locale::normalize(os_tag)
}

/// The OS language as a POSIX-style tag (`"id_ID.UTF-8"`), read from the
/// environment. Windows sets none of these variables, so the common Windows
/// path lands on the `en` fallback in [`Locale::normalize`]; the Win32
/// user-default-locale call is a later step, deliberately not a dependency
/// taken today.
fn os_language_tag() -> String {
    std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialises the two tests that mutate the process-wide locale; the
    /// harness runs tests on several threads, and `set_locale` is global.
    static LOCALE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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
    /// `LOCALE_LOCK`, because the harness runs tests on several threads and
    /// `set_locale` is process-wide.
    #[test]
    fn active_locale_switches_text() {
        let _guard = LOCALE_LOCK
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
        let _guard = LOCALE_LOCK
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
    fn detection_order_is_stored_then_os_then_english() {
        assert_eq!(detect_locale(Some("id".to_owned()), "en-US"), Locale::Id);
        assert_eq!(detect_locale(Some("id-ID".to_owned()), "en"), Locale::Id);
        assert_eq!(detect_locale(None, "id_ID.UTF-8"), Locale::Id);
        assert_eq!(
            detect_locale(Some("  ".to_owned()), "id_ID.UTF-8"),
            Locale::Id
        );
        assert_eq!(detect_locale(None, ""), Locale::En);
        assert_eq!(detect_locale(Some("fr".to_owned()), "id_ID"), Locale::En);
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
}
