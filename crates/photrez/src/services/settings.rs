//! # settings - persisted user preferences behind a store trait
//!
//! Owns: reading and writing the flat settings file the shell persists user
//! preferences in (`photrez.locale` today; later keys arrive with the
//! settings surface they belong to).
//!
//! Must not own: locale policy (`i18n/` owns the detection order and the
//! fallback), any UI, or the decision of when a preference is written - the
//! caller that changes a preference performs the write.

#![allow(dead_code)] // the preference writer lands with the locale switcher; drop this blanket as it arrives

use std::path::{Path, PathBuf};

/// The file name inside the per-user config directory.
const FILE_NAME: &str = "settings.json";

/// Flat key-value persistence for user preferences.
///
/// Implementations never panic and never block on anything but local file
/// I/O: a read that fails is a missing preference and a write that fails is
/// a lost one; both degrade instead of surfacing an error the user cannot
/// act on.
pub trait SettingsStore: Send + Sync {
    fn read(&self, key: &str) -> Option<String>;
    fn write(&self, key: &str, value: &str);
}

/// A `settings.json`-backed store: one flat JSON object, keys as written.
///
/// The file is read-modify-written on every change so keys written by other
/// parts of the shell survive, and replaced atomically (temp file + rename)
/// so a torn write cannot lose every preference at once.
#[derive(Clone, Debug)]
pub struct FileSettings {
    path: PathBuf,
}

impl FileSettings {
    /// The per-user location: `%APPDATA%\photrez\settings.json` on Windows,
    /// `$XDG_CONFIG_HOME/photrez/settings.json` (or `~/.config/...`)
    /// elsewhere. `None` when the environment names no config directory -
    /// the shell then runs on defaults only.
    pub fn user_default() -> Option<Self> {
        if cfg!(windows) {
            let base = std::env::var_os("APPDATA")?;
            Some(Self::at(Path::new(&base).join("photrez").join(FILE_NAME)))
        } else {
            let base = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config"))
                })?;
            Some(Self::at(base.join("photrez").join(FILE_NAME)))
        }
    }

    /// A store at an explicit path: tests, and any harness that wants its
    /// own profile.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl SettingsStore for FileSettings {
    fn read(&self, key: &str) -> Option<String> {
        let text = std::fs::read_to_string(&self.path).ok()?;
        let root: serde_json::Value = serde_json::from_str(&text).ok()?;
        root.get(key)?.as_str().map(str::to_owned)
    }

    fn write(&self, key: &str, value: &str) {
        let mut root = self.existing_object();
        root.insert(key.to_owned(), serde_json::Value::String(value.to_owned()));
        let Ok(payload) = serde_json::to_string_pretty(&root) else {
            // a flat map of strings cannot fail to serialize; the write is
            // simply lost, deliberately not an error surfaced to the user
            return;
        };
        let Some(parent) = self.path.parent() else {
            return;
        };
        if std::fs::create_dir_all(parent).is_err() {
            return;
        }
        let mut temp = self.path.clone();
        temp.as_mut_os_string().push(".tmp");
        if std::fs::write(&temp, payload).is_err() {
            return;
        }
        // `std::fs::rename` replaces the destination on both platforms
        // (MOVEFILE_REPLACE_EXISTING on Windows).
        let _ = std::fs::rename(&temp, &self.path);
    }
}

impl FileSettings {
    /// The file's current object, or an empty one when it is missing,
    /// unreadable, or not a JSON object.
    fn existing_object(&self) -> serde_json::Map<String, serde_json::Value> {
        std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
            .and_then(|value| match value {
                serde_json::Value::Object(map) => Some(map),
                _ => None,
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch path unique per test and per process; the file does not
    /// exist yet.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "photrez-settings-test-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join(FILE_NAME)
    }

    #[test]
    fn write_then_read_round_trips() {
        let path = scratch("round-trip");
        let store = FileSettings::at(&path);
        store.write("photrez.locale", "id");
        assert_eq!(store.read("photrez.locale").as_deref(), Some("id"));
        // a fresh store at the same path is what a reload sees
        let reloaded = FileSettings::at(&path);
        assert_eq!(reloaded.read("photrez.locale").as_deref(), Some("id"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn missing_and_junk_files_read_as_none() {
        let path = scratch("junk");
        let store = FileSettings::at(&path);
        assert_eq!(store.read("photrez.locale"), None); // no file at all
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(store.read("photrez.locale"), None);
        std::fs::write(&path, "[1,2,3]").unwrap(); // valid JSON, wrong shape
        assert_eq!(store.read("photrez.locale"), None);
        std::fs::write(&path, "{\"photrez.locale\": 7}").unwrap(); // wrong value type
        assert_eq!(store.read("photrez.locale"), None);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn write_preserves_other_keys() {
        let path = scratch("preserve");
        let store = FileSettings::at(&path);
        store.write("a", "1");
        store.write("photrez.locale", "id");
        assert_eq!(store.read("a").as_deref(), Some("1"));
        assert_eq!(store.read("photrez.locale").as_deref(), Some("id"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn write_replaces_a_junk_file_with_a_fresh_object() {
        let path = scratch("junk-replaced");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "garbage").unwrap();
        let store = FileSettings::at(&path);
        store.write("photrez.locale", "id");
        assert_eq!(store.read("photrez.locale").as_deref(), Some("id"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
