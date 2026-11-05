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

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

use crate::services::settings::SettingsStore;

/// The settings key the chosen locale persists under - the same key the
/// previous build kept in storage.
pub const LOCALE_KEY: &str = "photrez.locale";

/// The embedded catalogs, paired with their locale, in the order the catalog
/// files live under `assets/i18n/`. Adding a locale buys a catalog file and
/// a row here; the catalog tests and the lookup both read this table.
const CATALOGS: &[(Locale, &str)] = &[
    (Locale::En, include_str!("../assets/i18n/en.json")),
    (Locale::Id, include_str!("../assets/i18n/id.json")),
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
    /// fallback the previous build used when no catalog matched.
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

/// Serialises every test that mutates the process-wide locale, including
/// mounted shell tests that switch it through the Settings dialog.
#[cfg(test)]
pub(crate) static TEST_LOCALE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

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

fn lookup_in(locale: Locale, key: &str) -> Option<&'static str> {
    catalogs()
        .get(index_of(locale))
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
/// itself: the catalog tests gate the catalogs, and a key rendered in a
/// capture says exactly which lookup failed.
pub fn t(key: &str) -> String {
    lookup(key)
        .map(str::to_owned)
        .unwrap_or_else(|| key.to_owned())
}

/// [`t`] in an explicit locale. Callers that need a locale they can assert
/// without reading - or racing - the process-wide one use this.
pub fn t_in(locale: Locale, key: &str) -> String {
    lookup_in(locale, key)
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
    render(value, args).unwrap_or(key)
}

// ── startup detection ────────────────────────────────────────────────────────

/// The environment override the capture driver pins (`PHOTREZ_LOCALE=en`; see
/// the `xtask` capture runner). It outranks the stored preference so a reference
/// capture resolves `en` on any machine.
const ENV_LOCALE: &str = "PHOTREZ_LOCALE";

/// The startup detection order: the `PHOTREZ_LOCALE` override, then the stored
/// preference, then the OS locale, then `en`. Every failure at every step
/// degrades to `en`.
pub fn initial_locale(store: Option<&dyn SettingsStore>) -> Locale {
    detect_locale(
        std::env::var(ENV_LOCALE).ok().as_deref(),
        store.and_then(|store| store.read(LOCALE_KEY)),
        &os_language_tag(),
    )
}

/// The detection rule, split from its three inputs so tests stay deterministic
/// (neither the file system nor the environment is test input).
fn detect_locale(env: Option<&str>, stored: Option<String>, os_tag: &str) -> Locale {
    if let Some(tag) = env {
        if !tag.trim().is_empty() {
            return Locale::normalize(tag);
        }
    }
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
mod tests;
