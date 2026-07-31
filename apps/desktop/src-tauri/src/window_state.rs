// SPDX-License-Identifier: AGPL-3.0-or-later
// --- Window State Persistence ---
//
// Saves and restores window size, position, and maximized state across
// app launches. Written to `<app_config_dir>/window-state.json`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{Manager, Runtime};

pub(crate) const DEFAULT_WINDOW_WIDTH: u32 = 1280;
pub(crate) const DEFAULT_WINDOW_HEIGHT: u32 = 832;
const WINDOW_STATE_FILENAME: &str = "window-state.json";

const MIN_DIMENSION: u32 = 320;
const MAX_DIMENSION: u32 = 16384;
const MIN_POSITION: i32 = -32768;
const MAX_POSITION: i32 = 32768;

/// Minimum overlap (px) required to consider a window "on screen".
const ON_SCREEN_THRESHOLD: i32 = 100;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct SavedWindowState {
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub x: Option<i32>,
    #[serde(default)]
    pub y: Option<i32>,
    pub maximized: bool,
}

impl Default for SavedWindowState {
    fn default() -> Self {
        Self {
            width: DEFAULT_WINDOW_WIDTH,
            height: DEFAULT_WINDOW_HEIGHT,
            x: None,
            y: None,
            maximized: false,
        }
    }
}

pub(crate) fn validate_window_state(mut state: SavedWindowState) -> SavedWindowState {
    state.width = state.width.clamp(MIN_DIMENSION, MAX_DIMENSION);
    state.height = state.height.clamp(MIN_DIMENSION, MAX_DIMENSION);
    if let Some(x) = state.x {
        state.x = Some(x.clamp(MIN_POSITION, MAX_POSITION));
    }
    if let Some(y) = state.y {
        state.y = Some(y.clamp(MIN_POSITION, MAX_POSITION));
    }
    state
}

/// Ensures the saved window position is visible on at least one monitor.
/// If not, centers on the primary monitor.
/// Accepts a slice of `(x, y, width, height)` tuples for testability.
pub(crate) fn snap_to_screen(
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    monitors: &[(i32, i32, u32, u32)],
) -> (Option<i32>, Option<i32>) {
    let w = w as i32;
    let h = h as i32;
    let visible = monitors.iter().any(|&(mx, my, mw, mh)| {
        let mw = mw as i32;
        let mh = mh as i32;
        let left = x.max(mx);
        let top = y.max(my);
        let right = (x + w).min(mx + mw);
        let bottom = (y + h).min(my + mh);
        let overlap_w = (right - left).max(0);
        let overlap_h = (bottom - top).max(0);
        overlap_w >= ON_SCREEN_THRESHOLD && overlap_h >= ON_SCREEN_THRESHOLD
    });

    if visible {
        return (Some(x), Some(y));
    }

    // Center on primary (first monitor in list).
    if let Some(&(mx, my, mw, mh)) = monitors.first() {
        (
            Some(mx + (mw as i32) / 2 - w / 2),
            Some(my + (mh as i32) / 2 - h / 2),
        )
    } else {
        (None, None)
    }
}

/// Convenience wrapper: calls `snap_to_screen` with monitors from `AppHandle`.
pub(crate) fn snap_state_to_screen<R: Runtime>(
    state: &mut SavedWindowState,
    app: &tauri::AppHandle<R>,
) {
    let (x, y) = match (state.x, state.y) {
        (Some(x), Some(y)) => (x, y),
        _ => return, // first launch, no position to check
    };
    let monitors: Vec<(i32, i32, u32, u32)> = match app.available_monitors() {
        Ok(ms) => ms
            .iter()
            .map(|m| {
                let pos = m.position();
                let size = m.size();
                (pos.x, pos.y, size.width, size.height)
            })
            .collect(),
        Err(_) => return,
    };
    let (new_x, new_y) = snap_to_screen(x, y, state.width, state.height, &monitors);
    state.x = new_x;
    state.y = new_y;
}

fn window_state_path<R: Runtime>(app: &tauri::AppHandle<R>) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join(WINDOW_STATE_FILENAME))
}

pub(crate) fn load_window_state<R: Runtime>(app: &tauri::AppHandle<R>) -> SavedWindowState {
    let Some(path) = window_state_path(app) else {
        return SavedWindowState::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(json) => serde_json::from_str::<SavedWindowState>(&json)
            .map(validate_window_state)
            .unwrap_or_default(),
        Err(_) => SavedWindowState::default(),
    }
}

pub(crate) fn save_window_state<R: Runtime>(window: &tauri::Window<R>) {
    let Some(path) = window_state_path(window.app_handle()) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let (size, pos, maximized) = match (
        window.inner_size(),
        window.outer_position(),
        window.is_maximized(),
    ) {
        (Ok(s), Ok(p), Ok(m)) => (s, p, m),
        _ => return,
    };
    let state = SavedWindowState {
        width: size.width,
        height: size.height,
        x: Some(pos.x),
        y: Some(pos.y),
        maximized,
    };
    if let Ok(json) = serde_json::to_string_pretty(&state) {
        // Atomic write: write to .tmp first, then rename.
        let tmp_path = path.with_extension("json.tmp");
        if std::fs::write(&tmp_path, &json).is_ok() {
            let _ = std::fs::rename(&tmp_path, &path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_state_roundtrip() {
        let original = SavedWindowState {
            width: 1600,
            height: 900,
            x: Some(120),
            y: Some(80),
            maximized: false,
        };
        let json = serde_json::to_string_pretty(&original).expect("serialize");
        let parsed: SavedWindowState = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.width, original.width);
        assert_eq!(parsed.height, original.height);
        assert_eq!(parsed.x, original.x);
        assert_eq!(parsed.y, original.y);
        assert_eq!(parsed.maximized, original.maximized);
    }

    #[test]
    fn test_window_state_default_matches_tauri_config() {
        let defaults = SavedWindowState::default();
        assert_eq!(defaults.width, DEFAULT_WINDOW_WIDTH);
        assert_eq!(defaults.height, DEFAULT_WINDOW_HEIGHT);
        assert_eq!(defaults.x, None, "first launch should not force position");
        assert_eq!(defaults.y, None, "first launch should not force position");
        assert!(!defaults.maximized);
    }

    #[test]
    fn test_window_state_legacy_format_without_optional_position() {
        let legacy_json = r#"{ "width": 1024, "height": 768, "maximized": false }"#;
        let parsed: SavedWindowState = serde_json::from_str(legacy_json).expect("deserialize");
        assert_eq!(parsed.width, 1024);
        assert_eq!(parsed.height, 768);
        assert_eq!(parsed.x, None);
        assert_eq!(parsed.y, None);
        assert!(!parsed.maximized);
    }

    #[test]
    fn test_window_state_clamps_out_of_range_values() {
        let json = r#"{
            "width": 99999999,
            "height": 1,
            "x": -99999,
            "y": 99999,
            "maximized": false
        }"#;
        let parsed: SavedWindowState = serde_json::from_str(json).expect("deserialize");
        let clamped = validate_window_state(parsed);
        assert_eq!(clamped.width, MAX_DIMENSION, "width clamped to max");
        assert_eq!(clamped.height, MIN_DIMENSION, "height clamped to min");
        assert_eq!(clamped.x, Some(MIN_POSITION), "x clamped to min");
        assert_eq!(clamped.y, Some(MAX_POSITION), "y clamped to max");
    }

    #[test]
    fn test_window_state_in_range_values_pass_through() {
        let json = r#"{
            "width": 1600,
            "height": 900,
            "x": 120,
            "y": 80,
            "maximized": false
        }"#;
        let parsed: SavedWindowState = serde_json::from_str(json).expect("deserialize");
        let clamped = validate_window_state(parsed);
        assert_eq!(clamped.width, 1600);
        assert_eq!(clamped.height, 900);
        assert_eq!(clamped.x, Some(120));
        assert_eq!(clamped.y, Some(80));
    }

    #[test]
    fn test_snap_to_screen_window_on_monitor_unchanged() {
        // Single monitor at (0,0, 1920x1080), window at (100, 100, 800x600)
        let monitors = [(0, 0, 1920u32, 1080u32)];
        let (new_x, new_y) = snap_to_screen(100, 100, 800, 600, &monitors);
        assert_eq!(new_x, Some(100));
        assert_eq!(new_y, Some(100));
    }

    #[test]
    fn test_snap_to_screen_window_off_screen_centers() {
        // Single monitor at (0,0, 1920x1080), window at (3000, 400, 800x600)
        let monitors = [(0, 0, 1920u32, 1080u32)];
        let (new_x, new_y) = snap_to_screen(3000, 400, 800, 600, &monitors);
        // Centered on monitor: (1920-800)/2 = 560, (1080-600)/2 = 240
        assert_eq!(new_x, Some(560));
        assert_eq!(new_y, Some(240));
    }

    #[test]
    fn test_snap_to_screen_primary_monitor_offset() {
        // Dual monitor: primary at (0,0, 1920x1080), secondary at (1920,0, 1920x1080)
        // Window on secondary at (2000, 100, 800x600) -- visible, should stay
        let monitors = [(0, 0, 1920u32, 1080u32), (1920, 0, 1920u32, 1080u32)];
        let (new_x, new_y) = snap_to_screen(2000, 100, 800, 600, &monitors);
        assert_eq!(new_x, Some(2000));
        assert_eq!(new_y, Some(100));
    }

    #[test]
    fn test_snap_to_screen_disconnected_secondary_monitor() {
        // Secondary monitor disconnected; window stuck at old position
        let monitors = [(0, 0, 1920u32, 1080u32)];
        let (new_x, new_y) = snap_to_screen(2500, 200, 800, 600, &monitors);
        assert_eq!(new_x, Some(560)); // (1920-800)/2
        assert_eq!(new_y, Some(240)); // (1080-600)/2
    }

    #[test]
    fn test_snap_to_screen_partially_visible_stays() {
        // Window is partially (500px overlap) on monitor -- should stay
        let monitors = [(0, 0, 1920u32, 1080u32)];
        let (new_x, new_y) = snap_to_screen(1800, 100, 800, 600, &monitors);
        // Overlap: 120px (1920-1800) x 980px (1080-100) -- well over 100px threshold
        assert_eq!(new_x, Some(1800));
        assert_eq!(new_y, Some(100));
    }

    #[test]
    fn test_snap_to_screen_barely_visible_stays() {
        // Only 150px visible -- still above threshold
        let monitors = [(0, 0, 1920u32, 1080u32)];
        let (x, _y) = snap_to_screen(1770, 500, 800, 600, &monitors);
        // Overlap: 150px -- above 100px threshold
        assert_eq!(x, Some(1770));
    }

    #[test]
    fn test_snap_to_screen_below_threshold_centers() {
        // Only 50px visible -- below 100px threshold
        let monitors = [(0, 0, 1920u32, 1080u32)];
        let (x, _y) = snap_to_screen(1870, 500, 800, 600, &monitors);
        // Overlap: 50px -- below 100px threshold, should center
        assert_eq!(x, Some(560));
    }

    #[test]
    fn test_snap_to_screen_empty_monitors() {
        // No monitors -- should return None
        let monitors: [(i32, i32, u32, u32); 0] = [];
        let (new_x, new_y) = snap_to_screen(100, 100, 800, 600, &monitors);
        assert_eq!(new_x, None);
        assert_eq!(new_y, None);
    }
}
