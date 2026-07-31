// SPDX-License-Identifier: AGPL-3.0-or-later
// --- Print Settings Commands (Rust-owns-state pattern) ---
//
// Get/set commands for the print settings state owned by Rust. Emits
// print-settings-changed on every successful mutation.

use serde_json::Value;
use std::sync::Mutex;

use crate::print_geometry;
use crate::print_settings::PrintSettings;
use crate::response::{err_response, ok_response};
use tauri::{AppHandle, Emitter, State};

/// Event name for print settings changes (dash-separated lowercase).
const EVENT_PRINT_SETTINGS_CHANGED: &str = "print-settings-changed";

/// Helper: emit print settings to all listeners.
fn emit_print_settings(app: &AppHandle, settings: &PrintSettings) {
    if let Ok(json) = serde_json::to_value(settings) {
        log::debug!("[RUST:commands] emit_print_settings — paper_name={}, paper_index={}, paper=({}, {}), orientation={}, printer={:?}, copies={}",
            settings.paper_name, settings.paper_index, settings.paper_width_mm, settings.paper_height_mm,
            settings.orientation, settings.selected_printer, settings.copies);
        app.emit(EVENT_PRINT_SETTINGS_CHANGED, json).ok();
    }
}

#[tauri::command]
pub(crate) fn get_print_settings(state: State<'_, Mutex<PrintSettings>>) -> Result<Value, Value> {
    let settings = state.lock().unwrap_or_else(|e| e.into_inner());
    log::debug!("[RUST:commands] get_print_settings — paper_name={}, paper_index={}, paper=({}, {}), orientation={}, printer={:?}, copies={}",
        settings.paper_name, settings.paper_index, settings.paper_width_mm, settings.paper_height_mm,
        settings.orientation, settings.selected_printer, settings.copies);
    ok_response(settings.clone())
}

/// Set paper size by name, DMPAPER index, and dimensions.
/// Skips emit if nothing changed (prevents cascading re-renders when Effect 2
/// calls setPaper with values already matching current state).
#[tauri::command]
pub(crate) fn set_paper(
    name: String,
    paper_index: i16,
    width_mm: f64,
    height_mm: f64,
    state: State<'_, Mutex<PrintSettings>>,
    app: AppHandle,
) -> Result<Value, Value> {
    let mut settings = state.lock().unwrap_or_else(|e| e.into_inner());
    // Guard: skip if nothing changed
    let changed = settings.paper_name != name
        || settings.paper_index != paper_index
        || (settings.paper_width_mm - width_mm).abs() > 0.001
        || (settings.paper_height_mm - height_mm).abs() > 0.001;
    if changed {
        log::info!(
            "[RUST:commands] set_paper — name={}, paper_index={}, width_mm={}, height_mm={}",
            name,
            paper_index,
            width_mm,
            height_mm
        );
        settings.set_paper(&name, paper_index, width_mm, height_mm);
        emit_print_settings(&app, &settings);
    } else {
        log::debug!("[RUST:commands] set_paper — unchanged, skipping emit");
    }
    ok_response(settings.clone())
}

/// Toggle orientation (portrait <-> landscape).
#[tauri::command]
pub(crate) fn toggle_orientation(
    state: State<'_, Mutex<PrintSettings>>,
    app: AppHandle,
) -> Result<Value, Value> {
    let mut settings = state.lock().unwrap_or_else(|e| e.into_inner());
    let new_orientation = if settings.orientation == "portrait" {
        "landscape"
    } else {
        "portrait"
    };
    log::info!(
        "[RUST:commands] toggle_orientation — from={} to={}, dims_before=({}, {})",
        settings.orientation,
        new_orientation,
        settings.paper_width_mm,
        settings.paper_height_mm
    );
    settings.set_orientation(new_orientation);
    let value = ok_response(settings.clone());
    log::info!(
        "[RUST:commands] toggle_orientation — after: dims=({}, {}), orientation={}",
        settings.paper_width_mm,
        settings.paper_height_mm,
        settings.orientation
    );
    emit_print_settings(&app, &settings);
    value
}

/// Set orientation directly.
#[tauri::command]
pub(crate) fn set_orientation(
    orientation: String,
    state: State<'_, Mutex<PrintSettings>>,
    app: AppHandle,
) -> Result<Value, Value> {
    let mut settings = state.lock().unwrap_or_else(|e| e.into_inner());
    if settings.orientation != orientation {
        log::info!(
            "[RUST:commands] set_orientation — orientation={}",
            orientation
        );
        settings.set_orientation(&orientation);
        emit_print_settings(&app, &settings);
    } else {
        log::debug!("[RUST:commands] set_orientation — unchanged, skipping emit");
    }
    ok_response(settings.clone())
}

/// Set margin in mm.
/// Skips emit if value hasn't changed (prevents cascading events).
/// Optional `hardware_min_mm` stores the printer's unprintable-area floor;
/// subsequent `set_margin` calls (even without the param) clamp to it.
#[tauri::command]
pub(crate) fn set_margin(
    margin_mm: f64,
    hardware_min_mm: Option<f64>,
    state: State<'_, Mutex<PrintSettings>>,
    app: AppHandle,
) -> Result<Value, Value> {
    let mut settings = state.lock().unwrap_or_else(|e| e.into_inner());

    // Update hardware floor when the frontend provides it (e.g. on printer switch)
    if let Some(hw_min) = hardware_min_mm {
        settings.hardware_margin_min_mm = hw_min;
    }

    let changed = (settings.margin_mm - margin_mm).abs() > 0.001 || hardware_min_mm.is_some();
    if changed {
        settings.set_margin_mm(margin_mm);
        emit_print_settings(&app, &settings);
    } else {
        log::debug!(
            "[RUST:commands] set_margin — unchanged ({:.1}), skipping emit",
            margin_mm
        );
    }
    ok_response(settings.clone())
}

/// Set per-side margins (from printer driver hardware margins).
/// Also updates uniform margin_mm to max of all sides.
#[tauri::command]
pub(crate) fn set_per_side_margins(
    left_mm: f64,
    right_mm: f64,
    top_mm: f64,
    bottom_mm: f64,
    state: State<'_, Mutex<PrintSettings>>,
    app: AppHandle,
) -> Result<Value, Value> {
    let mut settings = state.lock().unwrap_or_else(|e| e.into_inner());
    settings.set_per_side_margins(left_mm, right_mm, top_mm, bottom_mm);
    emit_print_settings(&app, &settings);
    ok_response(settings.clone())
}

/// Set scale-to-fit enabled/disabled.
#[tauri::command]
pub(crate) fn set_scale_to_fit(
    enabled: bool,
    state: State<'_, Mutex<PrintSettings>>,
    app: AppHandle,
) -> Result<Value, Value> {
    let mut settings = state.lock().unwrap_or_else(|e| e.into_inner());
    if settings.scale_to_fit != enabled {
        settings.set_scale_to_fit(enabled);
        emit_print_settings(&app, &settings);
    }
    ok_response(settings.clone())
}

/// Set scale percent directly (when scale-to-fit is off).
/// Skips emit if value hasn't changed (prevents Effect 3 oscillation).
#[tauri::command]
pub(crate) fn set_scale_percent(
    percent: f64,
    state: State<'_, Mutex<PrintSettings>>,
    app: AppHandle,
) -> Result<Value, Value> {
    let mut settings = state.lock().unwrap_or_else(|e| e.into_inner());
    let changed = (settings.scale_percent - percent).abs() > 0.01;
    if changed {
        settings.set_scale_percent(percent);
        emit_print_settings(&app, &settings);
    } else {
        log::debug!(
            "[RUST:commands] set_scale_percent — unchanged ({:.2}), skipping emit",
            percent
        );
    }
    ok_response(settings.clone())
}

/// Set center image.
#[tauri::command]
pub(crate) fn set_center_image(
    center: bool,
    state: State<'_, Mutex<PrintSettings>>,
    app: AppHandle,
) -> Result<Value, Value> {
    let mut settings = state.lock().unwrap_or_else(|e| e.into_inner());
    if settings.center_image != center {
        settings.set_center_image(center);
        emit_print_settings(&app, &settings);
    }
    ok_response(settings.clone())
}

/// Set top offset in mm.
#[tauri::command]
pub(crate) fn set_top_offset_mm(
    offset: f64,
    state: State<'_, Mutex<PrintSettings>>,
    app: AppHandle,
) -> Result<Value, Value> {
    let mut settings = state.lock().unwrap_or_else(|e| e.into_inner());
    let changed = (settings.top_offset_mm - offset).abs() > 0.001;
    if changed {
        settings.set_top_offset_mm(offset);
        emit_print_settings(&app, &settings);
    }
    ok_response(settings.clone())
}

/// Set left offset in mm.
#[tauri::command]
pub(crate) fn set_left_offset_mm(
    offset: f64,
    state: State<'_, Mutex<PrintSettings>>,
    app: AppHandle,
) -> Result<Value, Value> {
    let mut settings = state.lock().unwrap_or_else(|e| e.into_inner());
    let changed = (settings.left_offset_mm - offset).abs() > 0.001;
    if changed {
        settings.set_left_offset_mm(offset);
        emit_print_settings(&app, &settings);
    }
    ok_response(settings.clone())
}

/// Set copies count.
#[tauri::command]
pub(crate) fn set_copies(
    copies: u32,
    state: State<'_, Mutex<PrintSettings>>,
    app: AppHandle,
) -> Result<Value, Value> {
    let mut settings = state.lock().unwrap_or_else(|e| e.into_inner());
    if settings.copies != copies {
        settings.set_copies(copies);
        emit_print_settings(&app, &settings);
    }
    ok_response(settings.clone())
}

/// Set display unit.
#[tauri::command]
pub(crate) fn set_unit(
    unit: String,
    state: State<'_, Mutex<PrintSettings>>,
    app: AppHandle,
) -> Result<Value, Value> {
    let mut settings = state.lock().unwrap_or_else(|e| e.into_inner());
    if settings.unit != unit {
        settings.set_unit(&unit);
        emit_print_settings(&app, &settings);
    }
    ok_response(settings.clone())
}

/// Set show paper white.
#[tauri::command]
pub(crate) fn set_show_paper_white(
    show: bool,
    state: State<'_, Mutex<PrintSettings>>,
    app: AppHandle,
) -> Result<Value, Value> {
    let mut settings = state.lock().unwrap_or_else(|e| e.into_inner());
    if settings.show_paper_white != show {
        settings.set_show_paper_white(show);
        emit_print_settings(&app, &settings);
    }
    ok_response(settings.clone())
}

/// Set color handling mode.
#[tauri::command]
pub(crate) fn set_color_handling(
    handling: String,
    state: State<'_, Mutex<PrintSettings>>,
    app: AppHandle,
) -> Result<Value, Value> {
    let mut settings = state.lock().unwrap_or_else(|e| e.into_inner());
    if settings.color_handling != handling {
        settings.set_color_handling(&handling);
        emit_print_settings(&app, &settings);
    }
    ok_response(settings.clone())
}

/// Set rendering intent.
#[tauri::command]
pub(crate) fn set_rendering_intent(
    intent: String,
    state: State<'_, Mutex<PrintSettings>>,
    app: AppHandle,
) -> Result<Value, Value> {
    let mut settings = state.lock().unwrap_or_else(|e| e.into_inner());
    if settings.rendering_intent != intent {
        settings.set_rendering_intent(&intent);
        emit_print_settings(&app, &settings);
    }
    ok_response(settings.clone())
}

/// Set black point compensation.
#[tauri::command]
pub(crate) fn set_black_point_compensation(
    enabled: bool,
    state: State<'_, Mutex<PrintSettings>>,
    app: AppHandle,
) -> Result<Value, Value> {
    let mut settings = state.lock().unwrap_or_else(|e| e.into_inner());
    if settings.black_point_compensation != enabled {
        settings.set_black_point_compensation(enabled);
        emit_print_settings(&app, &settings);
    }
    ok_response(settings.clone())
}

/// Set selected printer.
#[tauri::command]
pub(crate) fn set_printer(
    printer: String,
    state: State<'_, Mutex<PrintSettings>>,
    app: AppHandle,
) -> Result<Value, Value> {
    let mut settings = state.lock().unwrap_or_else(|e| e.into_inner());
    let changed = settings.selected_printer.as_deref() != Some(&printer);
    if changed {
        log::info!(
            "[RUST:commands] set_printer — printer={}, previous={:?}",
            printer,
            settings.selected_printer
        );
        settings.set_selected_printer(Some(printer));
        emit_print_settings(&app, &settings);
    } else {
        log::debug!("[RUST:commands] set_printer — unchanged, skipping emit");
    }
    ok_response(settings.clone())
}

/// Open native printer properties dialog and apply result to settings.
#[tauri::command]
pub(crate) fn open_printer_properties_and_apply(
    window: tauri::Window,
    state: State<'_, Mutex<PrintSettings>>,
    app: AppHandle,
) -> Result<Value, Value> {
    let (printer, paper_index, paper_width_mm, paper_height_mm, orientation) = {
        let settings = state.lock().unwrap_or_else(|e| e.into_inner());
        log::debug!("[RUST:commands] open_printer_properties_and_apply — reading state: printer={:?}, paper_index={}, paper=({}, {}), orientation={}",
            settings.selected_printer, settings.paper_index, settings.paper_width_mm, settings.paper_height_mm, settings.orientation);
        (
            settings.selected_printer.clone().unwrap_or_default(),
            settings.paper_index,
            settings.paper_width_mm,
            settings.paper_height_mm,
            settings.orientation.clone(),
        )
    };

    if printer.is_empty() {
        return err_response("E_PRINTER", "No printer selected");
    }

    #[cfg(target_os = "windows")]
    {
        use crate::print_windows::show_printer_settings_dialog;
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};

        let hwnd = match window.window_handle() {
            Ok(handle) => match handle.as_raw() {
                RawWindowHandle::Win32(win32) => win32.hwnd.get() as isize,
                _ => 0,
            },
            Err(_) => 0,
        };

        match show_printer_settings_dialog(
            &printer,
            hwnd,
            paper_index,
            paper_width_mm,
            paper_height_mm,
            &orientation,
        ) {
            Ok(Some((name, w_mm, h_mm, new_orientation, new_paper_index))) => {
                log::info!("[RUST:commands] open_printer_properties_and_apply — dialog OK: name={}, w_mm={}, h_mm={}, new_orientation={}, new_paper_index={}",
                    name, w_mm, h_mm, new_orientation, new_paper_index);
                let mut settings = state.lock().unwrap_or_else(|e| e.into_inner());
                // BUG-02 defense-in-depth: skip update if nothing changed
                if settings.paper_name == name
                    && settings.paper_index == new_paper_index
                    && (settings.paper_width_mm - w_mm).abs() < 0.01
                    && (settings.paper_height_mm - h_mm).abs() < 0.01
                    && settings.orientation == new_orientation
                {
                    log::debug!(
                        "[RUST:commands] open_printer_properties_and_apply — unchanged, skipping"
                    );
                    return ok_response(serde_json::json!({
                        "applied": false,
                        "cancelled": false,
                        "unchanged": true,
                    }));
                }
                log::info!("[RUST:commands] open_printer_properties_and_apply — applying: paper_name={}, paper_index={}, paper=({}, {}), orientation={}",
                    name, new_paper_index, w_mm, h_mm, new_orientation);
                settings.set_paper(&name, new_paper_index, w_mm, h_mm);
                settings.set_orientation(&new_orientation);
                let value = ok_response(serde_json::json!({
                    "applied": true,
                    "settings": &*settings,
                }));
                emit_print_settings(&app, &settings);
                value
            }
            Ok(None) => {
                log::info!("[RUST:commands] open_printer_properties_and_apply — cancelled");
                ok_response(serde_json::json!({ "applied": false, "cancelled": true }))
            }
            Err(e) => {
                log::error!(
                    "[RUST:commands] open_printer_properties_and_apply — error: {}",
                    e
                );
                err_response("E_PRINTER", &e)
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        ok_response(serde_json::json!({
            "applied": false,
            "cancelled": false,
            "message": "Use your system's Print dialog."
        }))
    }
}

/// Convert mm value to current unit.
#[tauri::command]
pub(crate) fn convert_mm_to_current_unit(
    val_mm: f64,
    state: State<'_, Mutex<PrintSettings>>,
) -> Result<Value, Value> {
    let unit = state.lock().unwrap_or_else(|e| e.into_inner()).unit.clone();
    let converted = print_geometry::mm_to_unit(val_mm, &unit);
    ok_response(converted)
}

/// Convert value in current unit to mm.
#[tauri::command]
pub(crate) fn convert_current_unit_to_mm(
    val: f64,
    state: State<'_, Mutex<PrintSettings>>,
) -> Result<Value, Value> {
    let unit = state.lock().unwrap_or_else(|e| e.into_inner()).unit.clone();
    let mm = print_geometry::unit_to_mm(val, &unit);
    ok_response(mm)
}
