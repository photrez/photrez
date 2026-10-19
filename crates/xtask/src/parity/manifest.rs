//! Reader for the golden-master `manifest.json` (the shot index beside the
//! baseline PNGs). Only the fields the parity runs need are typed; everything
//! else in the manifest stays unread.

use std::path::Path;

use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct Manifest {
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
