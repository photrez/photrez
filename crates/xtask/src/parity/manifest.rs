//! Reader for the golden-master `manifest.json` (the shot index beside the
//! baseline PNGs). Only the fields the parity runs need are typed; everything
//! else in the manifest stays unread.

use std::path::Path;

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Manifest {
    /// The locale the baselines were captured under; the frozen reference's
    /// manifest records `"en"`. Absent in manifests that predate the field.
    #[serde(default)]
    pub locale: Option<String>,
    pub shots: Vec<Shot>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)] // mirrors the published manifest schema; the pipeline reads id, file and box only
pub struct Shot {
    pub id: String,
    pub file: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub note: String,
    /// CSS selector the original capture used; null or missing for shots
    /// without one.
    #[serde(default)]
    pub selector: Option<String>,
    #[serde(default)]
    pub needs_doc: bool,
    #[serde(rename = "box")]
    pub bounds: ShotBox,
    #[serde(default)]
    pub reason: Option<String>,
}

/// The element box the shot was cropped from, in viewport coordinates.
/// Fractional by design (layout widths like 89.859) - consumers anchor at the
/// origin and take size from the PNG.
#[derive(Clone, Copy, Debug, Deserialize)]
pub struct ShotBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// Load and parse the manifest.
pub fn load(path: &Path) -> Result<Manifest, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("parse {}: {e}", path.display()))
}

/// Fail fast when a manifest records a non-English capture locale: every
/// baseline in the golden master was taken with the English catalog, so a
/// diff under another locale is meaningless rather than merely different.
/// A manifest without the field predates it and is accepted.
pub fn require_english_locale(manifest: &Manifest) -> Result<(), String> {
    match manifest.locale.as_deref() {
        None => Ok(()),
        Some(tag) if tag.trim().is_empty() || tag.trim().eq_ignore_ascii_case("en") => Ok(()),
        Some(tag) => Err(format!(
            "manifest records locale `{tag}`; parity diffs are only valid against the English baselines"
        )),
    }
}

/// Shots whose id matches one of `patterns` (exact id, or prefix match -
/// `menu-` selects every menu), in manifest order. Unknown patterns are an
/// error: a typo must not silently compare nothing.
pub fn select<'a>(manifest: &'a Manifest, patterns: &[String]) -> Result<Vec<&'a Shot>, String> {
    let mut picked: Vec<&Shot> = Vec::new();
    for pattern in patterns {
        let mut matched = false;
        for shot in &manifest.shots {
            if shot.id == *pattern || shot.id.starts_with(pattern.as_str()) {
                matched = true;
                if !picked.iter().any(|s| s.id == shot.id) {
                    picked.push(shot);
                }
            }
        }
        if !matched {
            return Err(format!("no shot matches `{pattern}`"));
        }
    }
    if picked.is_empty() {
        return Err("no shots selected".into());
    }
    Ok(picked)
}

#[cfg(test)]
mod tests {
    use super::{require_english_locale, Manifest};

    fn manifest_with(locale: Option<&str>) -> Manifest {
        Manifest {
            locale: locale.map(str::to_owned),
            shots: Vec::new(),
        }
    }

    /// The manifest side of the locale fail-fast: English (any case) and a
    /// pre-field manifest pass; anything else is refused and named.
    #[test]
    fn english_locale_is_required() {
        assert!(require_english_locale(&manifest_with(None)).is_ok());
        assert!(require_english_locale(&manifest_with(Some(""))).is_ok());
        assert!(require_english_locale(&manifest_with(Some("en"))).is_ok());
        assert!(require_english_locale(&manifest_with(Some(" EN "))).is_ok());
        let err =
            require_english_locale(&manifest_with(Some("id"))).expect_err("id must be refused");
        assert!(err.contains("id"), "names the value: {err}");
    }
}
