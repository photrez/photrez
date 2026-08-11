// SPDX-License-Identifier: AGPL-3.0-or-later
// --- System Font Enumeration (native, no permission prompt) ---
//
// The frontend's Local Font Access API (window.queryLocalFonts) works but
// shows a browser-style "let this site see your fonts?" permission prompt,
// which is jarring inside a desktop app. This command enumerates the same
// fonts natively via fontdb (pure Rust, scans the platform font directories:
// %WINDIR%\Fonts + registry on Windows, /System/Library + /Library + ~/Library
// on macOS, fontconfig/dirs on Linux) — no prompt, no network.
//
// The frontend falls back to its WEB_SAFE list when this returns an error
// (e.g. no fonts found), so the text tool never blocks on enumeration.

use serde_json::Value;

use crate::response::{err_response, ok_response};

/// Group (family, style) entries into a sorted `[{family, styles:[...]}]`
/// list, deduplicating styles per family. Pure and deterministic — unit-tested
/// without needing real system fonts.
pub(crate) fn build_font_list(entries: Vec<(String, String)>) -> Vec<Value> {
    let mut by_family: std::collections::BTreeMap<String, std::collections::BTreeSet<String>> =
        std::collections::BTreeMap::new();
    for (family, style) in entries {
        if family.is_empty() {
            continue;
        }
        by_family.entry(family).or_default().insert(style);
    }
    by_family
        .into_iter()
        .map(|(family, styles)| {
            serde_json::json!({
                "family": family,
                "styles": styles.into_iter().collect::<Vec<_>>(),
            })
        })
        .collect()
}

/// Enumerate every system font. Returns `{ fonts: [{family, styles}] }`.
/// Runs on a blocking thread — parsing every system font face's metadata can
/// take hundreds of ms, and a sync Tauri command would stall the UI thread.
#[tauri::command]
pub(crate) async fn list_system_fonts() -> Result<Value, Value> {
    tauri::async_runtime::spawn_blocking(|| {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();

        let entries = db
            .faces()
            .filter_map(|face| {
                // families: Vec<(String, Language)> — first entry is the English US
                // typographic/font family name (e.g. "Arial", never "Arial Bold").
                let family = face.families.first()?.0.clone();
                if family.is_empty() {
                    return None;
                }
                let style = match face.style {
                    fontdb::Style::Normal => "Regular",
                    fontdb::Style::Italic => "Italic",
                    fontdb::Style::Oblique => "Oblique",
                };
                Some((family, style.to_string()))
            })
            .collect();

        let fonts = build_font_list(entries);
        if fonts.is_empty() {
            return err_response("E_FONT_ENUM", "No system fonts found");
        }
        ok_response(serde_json::json!({ "fonts": fonts }))
    })
    .await
    .unwrap_or_else(|e| err_response("E_FONT_ENUM", &format!("Font enumeration task failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_font_list_sorts_families_and_dedupes_styles() {
        let list = build_font_list(vec![
            ("Zapf Dingbats".to_string(), "Regular".to_string()),
            ("Arial".to_string(), "Bold".to_string()),
            ("Arial".to_string(), "Regular".to_string()),
            ("Arial".to_string(), "Regular".to_string()), // duplicate
        ]);

        assert_eq!(list.len(), 2);
        assert_eq!(list[0]["family"], "Arial");
        assert_eq!(list[0]["styles"], serde_json::json!(["Bold", "Regular"]));
        assert_eq!(list[1]["family"], "Zapf Dingbats");
        assert_eq!(list[1]["styles"], serde_json::json!(["Regular"]));
    }

    #[test]
    fn build_font_list_skips_empty_families() {
        let list = build_font_list(vec![
            ("".to_string(), "Regular".to_string()),
            ("Arial".to_string(), "Regular".to_string()),
        ]);
        assert_eq!(list.len(), 1);
    }
}
