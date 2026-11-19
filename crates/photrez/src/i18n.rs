//! # i18n - locale entity and message catalog
//!
//! Owns: the locale entity and lookup helpers for dynamic keys.
//!
//! Call sites use [`t`] / [`t_args`] / [`t_plural`] for dynamic keys; [`set_locale`]
//! swaps the active locale; [`initial_locale`] is the startup detection order.

use crate::services::settings::SettingsStore;
use std::sync::LazyLock;

/// The settings key the chosen locale persists under. It is the same key the
/// 0.1.0 release wrote.
pub const LOCALE_KEY: &str = "photrez.locale";

const ENV_LOCALE: &str = "PHOTREZ_LOCALE";

// ── locale ───────────────────────────────────────────────────────────────────

/// A locale shipped by this build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Locale {
    En,
    Id,
    Bjn,
    Catalog(&'static str),
}

impl Locale {
    /// The language tag used as the catalog file name and the value
    /// persisted in the settings file.
    pub fn code(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::Id => "id",
            Locale::Bjn => "bjn",
            Locale::Catalog(code) => code,
        }
    }

    /// Returns this locale's name from its catalog.
    pub fn name(self) -> String {
        t_in(self, "locale.name")
    }

    fn from_code(code: &'static str) -> Self {
        match code {
            "en" => Self::En,
            "id" => Self::Id,
            "bjn" => Self::Bjn,
            code => Self::Catalog(code),
        }
    }

    /// Matches an exact catalog tag or its most specific parent. Unknown tags
    /// use English. POSIX separators and codesets are accepted.
    pub fn normalize(tag: &str) -> Self {
        let tag = tag
            .trim()
            .split(['.', '@'])
            .next()
            .unwrap_or_default()
            .replace('_', "-");
        locales()
            .iter()
            .filter(|locale| {
                let code = locale.code().replace('_', "-");
                tag.eq_ignore_ascii_case(&code)
                    || tag.get(..code.len()).is_some_and(|prefix| {
                        prefix.eq_ignore_ascii_case(&code)
                            && tag.as_bytes().get(code.len()) == Some(&b'-')
                    })
            })
            .max_by_key(|locale| locale.code().len())
            .copied()
            .unwrap_or(Locale::En)
    }
}

/// Serialises every test that mutates the process-wide locale, including
/// mounted shell tests that switch it through the Settings dialog.
#[cfg(test)]
pub(crate) static TEST_LOCALE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

static LOCALES: LazyLock<Vec<Locale>> = LazyLock::new(|| {
    let app_catalogs = crate::_rust_i18n_backend().available_locales();
    let mut locales: Vec<_> = rust_i18n::available_locales!()
        .into_iter()
        .filter(|code| app_catalogs.contains(code))
        // Owned codes are supplied by extension backends, not embedded catalogs.
        .filter_map(|code| match code {
            std::borrow::Cow::Borrowed(code) => Some(Locale::from_code(code)),
            std::borrow::Cow::Owned(_) => None,
        })
        .collect();
    locales.sort_by_key(|locale| (locale.code() != "en", locale.code().to_owned()));
    locales
});

/// Returns this build's catalogs, with English first and the rest sorted by code.
pub fn locales() -> &'static [Locale] {
    &LOCALES
}

/// The active locale.
pub fn locale() -> Locale {
    Locale::normalize(&rust_i18n::locale())
}

/// Swaps the active locale and returns the previous one. Persisting the
/// choice is deliberately the caller's job: the write is file I/O, and the
/// caller that flips the preference also decides when the shell re-renders.
pub fn set_locale(next: Locale) -> Locale {
    let previous = locale();
    rust_i18n::set_locale(next.code());
    previous
}

/// The `rust-i18n` extension slot holds one backend per namespace, and a
/// second `extend!` for the same namespace panics. Every mounted app wires the
/// catalog, and the test binary mounts many apps in one process, so the call
/// runs once per process.
static COMPONENT_CATALOG_WIRED: std::sync::Once = std::sync::Once::new();

/// Merges gpui-component's catalog into this crate's rust-i18n backend.
/// Call once per process before the first UI lookup or `gpui_kit::init`.
pub fn wire_component_catalog() {
    COMPONENT_CATALOG_WIRED.call_once(|| rust_i18n::extend!(gpui_component));
}

fn translate_key(locale: &str, key: &str) -> Option<String> {
    crate::_rust_i18n_try_translate(locale, key).map(|cow| cow.into_owned())
}

fn lookup_template(key: &str) -> Option<String> {
    let active = rust_i18n::locale();
    translate_key(&active, key).or_else(|| translate_key("en", key))
}

/// Resolves `key` in the active locale, then `en`, then returns `key`.
pub fn t(key: &str) -> String {
    lookup_template(key).unwrap_or_else(|| key.to_owned())
}

/// Resolves `key` in a fixed locale (tests and fixtures).
pub fn t_in(locale: Locale, key: &str) -> String {
    translate_key(locale.code(), key)
        .or_else(|| translate_key("en", key))
        .unwrap_or_else(|| key.to_owned())
}

/// [`t`] for values that name `%{placeholders}`. A render mismatch degrades
/// to the key.
pub fn t_args(key: &str, args: &[(&str, &str)]) -> String {
    let Some(template) = lookup_template(key) else {
        return key.to_owned();
    };
    if template.contains("%{") {
        let mut need = Vec::new();
        let mut rest = template.as_str();
        while let Some(open) = rest.find("%{") {
            let after = &rest[open + 2..];
            let Some(close) = after.find('}') else {
                return key.to_owned();
            };
            need.push(after[..close].trim());
            rest = &after[close + 1..];
        }
        for name in need {
            if !args.iter().any(|(k, _)| *k == name) {
                return key.to_owned();
            }
        }
    }
    let patterns: Vec<&str> = args.iter().map(|(k, _)| *k).collect();
    let values: Vec<String> = args.iter().map(|(_, v)| (*v).to_string()).collect();
    rust_i18n::replace_patterns(&template, &patterns, &values)
}

/// The active locale's `{base}_one` / `{base}_other` pair for `count`, with
/// `%{count}` interpolated when the value names it.
pub fn t_plural(base: &str, count: u32) -> String {
    let key = format!("{base}_{}", if count == 1 { "one" } else { "other" });
    let Some(template) = lookup_template(&key) else {
        return key;
    };
    let count_text = count.to_string();
    if template.contains("%{count}") {
        rust_i18n::replace_patterns(&template, &["count"], &[count_text])
    } else {
        template
    }
}

/// Picks the locale before the first window reads the catalog.
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
    if let Some(tag) = env
        && !tag.trim().is_empty()
    {
        return Locale::normalize(tag);
    }
    if let Some(tag) = stored
        && !tag.trim().is_empty()
    {
        return Locale::normalize(&tag);
    }
    Locale::normalize(os_tag)
}

/// Snapshot helpers for tests: the embedded catalog, read per locale without
/// the fallback chain, so a missing translation reads as missing instead of
/// as the English text.
#[cfg(test)]
pub(crate) mod catalog_snapshot {
    use std::collections::BTreeMap;

    use super::Locale;

    pub struct Catalog {
        pub entries: BTreeMap<String, String>,
    }

    pub fn load(locale: Locale) -> Catalog {
        let messages = crate::_rust_i18n_backend()
            .messages_for_locale(locale.code())
            .expect("the embedded catalog ships this locale");
        let entries = messages
            .into_iter()
            .map(|(key, value)| (key.into_owned(), value.into_owned()))
            .collect();
        Catalog { entries }
    }
}

/// The OS language as a POSIX-style tag (`"id_ID.UTF-8"`), read from the
/// environment. Windows sets none of these variables, so the Windows path
/// asks the user default locale name instead.
#[cfg(not(windows))]
fn os_language_tag() -> String {
    std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default()
}

/// The Windows user default locale name, for example `"id-ID"`. The buffer
/// length is the `LOCALE_NAME_MAX_LENGTH` of the Windows API.
#[cfg(windows)]
#[allow(unsafe_code)] // SAFETY: one GetUserDefaultLocaleName call that writes at most the buffer length.
fn os_language_tag() -> String {
    const LOCALE_NAME_MAX_LENGTH: usize = 85;
    let mut buffer = [0u16; LOCALE_NAME_MAX_LENGTH];
    let written = unsafe {
        windows_sys::Win32::Globalization::GetUserDefaultLocaleName(
            buffer.as_mut_ptr(),
            buffer.len() as i32,
        )
    };
    if written <= 0 {
        return String::new();
    }
    decode_windows_locale_tag(&buffer[..written as usize])
}

#[cfg(any(windows, test))]
fn decode_windows_locale_tag(buffer: &[u16]) -> String {
    String::from_utf16_lossy(buffer)
        .trim_end_matches('\0')
        .to_owned()
}

#[cfg(test)]
mod tests;
