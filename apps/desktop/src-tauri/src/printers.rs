// SPDX-License-Identifier: AGPL-3.0-or-later
// --- Printer Discovery (Windows GDI + macOS/Linux CUPS PPD) ---
//
// Printer enumeration, paper-size queries, and the native properties dialog
// (used by print_core.rs). CUPS helpers are macOS/Linux-only.

use serde_json::Value;

use crate::response::{err_response, ok_response};

// ── CUPS PPD helpers (macOS/Linux) ──────────────────────────────────────

#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::path::{Path, PathBuf};

#[cfg(any(target_os = "macos", target_os = "linux"))]
/// Locate the PPD file path for `printer_name` via `lpoptions`.
/// Returns None if lpoptions fails or path doesn't exist on disk.
fn find_cups_ppd(printer_name: &str) -> Option<PathBuf> {
    let output = std::process::Command::new("lpoptions")
        .args(["-p", printer_name, "-o", "ppd"])
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.trim();
    // lpoptions output format: "ppd=/etc/cups/ppd/PrinterName.ppd"
    let path_str = line.strip_prefix("ppd=").unwrap_or(line);
    let p = Path::new(path_str);
    if p.is_file() {
        Some(p.to_path_buf())
    } else {
        None
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
/// Parse a CUPS PPD file for PaperDimension and ImageableArea entries.
///
/// PaperDimension: `*PaperDimension Name/Desc: "width_pt height_pt"`
/// ImageableArea:  `*ImageableArea Name/Desc: "x1 y1 x2 y2"`
/// PostScript points: 1 pt = 1/72 inch = 25.4/72 mm
fn parse_ppd_file(
    ppd_path: &Path,
) -> (
    Vec<(String, f64, f64)>,
    HashMap<String, (f64, f64, f64, f64)>,
) {
    let pt_to_mm = |pt: f64| pt * 25.4 / 72.0;
    let content = match std::fs::read_to_string(ppd_path) {
        Ok(c) => c,
        Err(_) => return (vec![], HashMap::new()),
    };
    let mut paper_dims: Vec<(String, f64, f64)> = Vec::new();
    let mut imageable_areas: HashMap<String, (f64, f64, f64, f64)> = HashMap::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("*PaperDimension ") {
            // e.g. `*PaperDimension A4/A4 paper: "595.28 841.89"`
            if let Some(dq_start) = trimmed.rfind('"') {
                let dims_str = &trimmed[dq_start + 1..].trim_end_matches('"');
                let parts: Vec<&str> = dims_str.split_whitespace().collect();
                if parts.len() >= 2 {
                    let name = trimmed["*PaperDimension ".len()..]
                        .split('/')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if let (Ok(w), Ok(h)) = (parts[0].parse::<f64>(), parts[1].parse::<f64>()) {
                        paper_dims.push((name, pt_to_mm(w), pt_to_mm(h)));
                    }
                }
            }
        } else if trimmed.starts_with("*ImageableArea ") {
            if let Some(dq_start) = trimmed.rfind('"') {
                let area_str = &trimmed[dq_start + 1..].trim_end_matches('"');
                let parts: Vec<&str> = area_str.split_whitespace().collect();
                if parts.len() >= 4 {
                    let name = trimmed["*ImageableArea ".len()..]
                        .split('/')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if let Some(coords) = parts
                        .iter()
                        .map(|s| s.parse::<f64>().ok())
                        .collect::<Option<Vec<_>>>()
                    {
                        imageable_areas.insert(
                            name,
                            (
                                pt_to_mm(coords[0]),
                                pt_to_mm(coords[1]),
                                pt_to_mm(coords[2]),
                                pt_to_mm(coords[3]),
                            ),
                        );
                    }
                }
            }
        }
    }
    paper_dims.sort_by(|a, b| a.0.cmp(&b.0));
    (paper_dims, imageable_areas)
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
/// Map a paper preset name to a CUPS standard media name for macOS/Linux.
/// Using standard names ("A4", "Letter") is more reliable than raw
/// "Custom.WxHmm" syntax because many CUPS printers only recognise
/// named media sizes from their PPD.
pub(crate) fn cups_media_name(preset: &str, width_mm: f64, height_mm: f64) -> String {
    match preset {
        "A4" | "Letter" | "A3" | "Legal" | "A5" | "A6" => preset.to_string(),
        "Tabloid" => "Tabloid".to_string(),
        "Executive" => "Executive".to_string(),
        "Photo4x6" => "4x6".to_string(),
        "Photo5x7" => "5x7".to_string(),
        "Photo8x10" => "8x10".to_string(),
        // Custom or unknown — fall back to dimension syntax
        _ => format!("Custom.{:.1}x{:.1}mm", width_mm, height_mm),
    }
}

/// Open the printer driver's native settings dialog (paper size, orientation, etc.)
/// using DocumentPropertiesW with DM_IN_PROMPT (Windows) or a fallback.
/// Returns the user's paper size choice if they clicked OK, or cancelled: true.
///
/// Accepts `paper_index`, `paper_width_mm`, `paper_height_mm`, `orientation` from
/// the frontend to initialise the dialog with the user's current paper selection.
/// Without this, the native dialog would show the printer driver's stale default
/// (last-used paper from the driver), which would overwrite the user's choice.
#[tauri::command]
pub(crate) fn open_printer_properties(
    window: tauri::Window,
    printer: String,
    paper_index: i16,
    paper_width_mm: f64,
    paper_height_mm: f64,
    orientation: String,
) -> Result<Value, Value> {
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
            Ok(Some((preset, w_mm, h_mm, orientation, _paper_index))) => {
                ok_response(serde_json::json!({
                    "applied": true,
                    "paperPreset": preset,
                    "paperWidthMm": w_mm,
                    "paperHeightMm": h_mm,
                    "orientation": orientation,
                }))
            }
            Ok(None) => ok_response(serde_json::json!({ "applied": false, "cancelled": true })),
            Err(e) => err_response("E_PRINTER", &e),
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        // On macOS/Linux, suggest using system print dialog
        let _ = window; // suppress unused warning
        ok_response(serde_json::json!({
            "applied": false,
            "cancelled": false,
            "message": "Use your system's Print dialog (Cmd+P) to change printer settings."
        }))
    }
}

/// Enumerate system printers using the `printers` crate.
/// Returns list of printer names, the default printer name, and (on Windows)
/// the default paper size of the default printer.
#[tauri::command]
pub(crate) fn get_system_printers() -> Result<Value, Value> {
    let printer_list = printers::get_printers();
    let printers_vec: Vec<String> = printer_list.iter().map(|p| p.name.clone()).collect();

    let default_name = printers::get_default_printer().map(|p| p.name.clone());

    // Try to get the default printer's current paper size (best-effort)
    let mut default_paper: Option<serde_json::Value> = None;

    // Try to get the default printer's hardware margins (best-effort)
    let mut default_margins: Option<serde_json::Value> = None;

    #[cfg(target_os = "windows")]
    {
        if let Some(ref def_name) = default_name {
            if let Ok((preset, w_mm, h_mm)) =
                crate::print_windows::get_default_paper_size_win(def_name)
            {
                default_paper = Some(serde_json::json!({
                    "preset": preset,
                    "widthMm": w_mm,
                    "heightMm": h_mm,
                }));
            }
            if let Ok((l, t, r, b)) = crate::print_windows::get_printer_margins_win(def_name) {
                default_margins = Some(serde_json::json!({
                    "leftMm": l,
                    "topMm": t,
                    "rightMm": r,
                    "bottomMm": b,
                }));
            }
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        if let Some(ref def_name) = default_name {
            // Resolve PPD file for real paper dimensions + margins
            if let Some(ppd_path) = find_cups_ppd(def_name) {
                let (paper_dims, imageable_areas) = parse_ppd_file(&ppd_path);

                // Also get the current default paper size name via lpoptions
                if let Ok(output) = std::process::Command::new("lpoptions")
                    .arg("-p")
                    .arg(def_name)
                    .arg("-l")
                    .output()
                {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    for line in stdout.lines() {
                        if line.starts_with("PageSize/") || line.starts_with("PageSize:") {
                            if let Some(val_part) = line.split(':').nth(1) {
                                for token in val_part.split_whitespace() {
                                    let t = token.trim_start_matches('*');
                                    if t != token || token.starts_with('*') {
                                        // Look up dimensions from parsed PPD
                                        if let Some((_, w, h)) =
                                            paper_dims.iter().find(|(n, _, _)| n == t)
                                        {
                                            default_paper = Some(serde_json::json!({
                                                "preset": t,
                                                "widthMm": w,
                                                "heightMm": h,
                                            }));
                                        } else {
                                            // Standard sizes not in PPD — use known values
                                            let (w, h) = match t {
                                                "A4" => (210.0, 297.0),
                                                "Letter" => (215.9, 279.4),
                                                "A3" => (297.0, 420.0),
                                                "Legal" => (215.9, 355.6),
                                                "A5" => (148.0, 210.0),
                                                "A6" => (105.0, 148.0),
                                                "Tabloid" => (279.4, 431.8),
                                                "Executive" => (184.15, 266.7),
                                                "4x6" => (101.6, 152.4),
                                                "5x7" => (127.0, 177.8),
                                                "8x10" => (203.2, 254.0),
                                                _ => (0.0, 0.0),
                                            };
                                            default_paper = Some(serde_json::json!({
                                                "preset": t,
                                                "widthMm": w,
                                                "heightMm": h,
                                            }));
                                        }
                                        break;
                                    }
                                }
                            }
                            break;
                        }
                    }
                }

                // Derive margins from ImageableArea of the selected paper
                if let Some(ref dp) = default_paper {
                    if let Some(preset_name) = dp.get("preset").and_then(|v| v.as_str()) {
                        if let Some((x1, y1, x2, y2)) = imageable_areas.get(preset_name) {
                            let paper_w =
                                dp.get("widthMm").and_then(|v| v.as_f64()).unwrap_or(210.0);
                            let paper_h =
                                dp.get("heightMm").and_then(|v| v.as_f64()).unwrap_or(297.0);
                            default_margins = Some(serde_json::json!({
                                "leftMm":   x1,
                                "topMm":    y1,
                                "rightMm":  paper_w - x2,
                                "bottomMm": paper_h - y2,
                            }));
                        }
                    }
                }
            }

            // Final fallback: if no PPD or no match, use 3mm standard
            if default_margins.is_none() {
                default_margins = Some(serde_json::json!({
                    "leftMm": 3.0,
                    "topMm": 3.0,
                    "rightMm": 3.0,
                    "bottomMm": 3.0,
                }));
            }
        }
    }

    log::debug!("[RUST:commands] get_system_printers — printers={:?}, default={:?}, defaultPaperSize={:?}, defaultMargins={:?}",
        printers_vec, default_name, default_paper, default_margins);
    ok_response(serde_json::json!({
        "printers": printers_vec,
        "default": default_name,
        "defaultPaperSize": default_paper,
        "defaultMargins": default_margins,
    }))
}

/// Query a specific printer's supported paper sizes, default paper size,
/// and hardware margins — all for the requested printer (not just the default).
/// Returns an object with `sizes`, `defaultPaperSize`, and `defaultMargins`.
///
/// This is the per-printer counterpart of `get_system_printers` — Effect 2
/// reads defaults from this resource (which is reactive to printer changes)
/// instead of from `printersRes` (which is fetched once at mount and stale
/// for non-default printers).
#[tauri::command]
pub(crate) fn get_printer_paper_sizes(printer: String) -> Result<Value, Value> {
    #[cfg(target_os = "windows")]
    {
        match crate::print_windows::get_printer_paper_sizes_win(&printer) {
            Ok(sizes) => {
                log::debug!(
                    "[RUST:commands] get_printer_paper_sizes — printer={}, count={}",
                    printer,
                    sizes.len()
                );
                let entries: Vec<serde_json::Value> = sizes
                    .into_iter()
                    .map(|(name, w, h, idx)| {
                        log::debug!("[RUST:commands] get_printer_paper_sizes — entry: name={}, w={}, h={}, idx={}", name, w, h, idx);
                        serde_json::json!({
                            "name": name,
                            "widthMm": (w * 10.0).round() / 10.0,
                            "heightMm": (h * 10.0).round() / 10.0,
                            "dmPaperIndex": idx,
                        })
                    })
                    .collect();
                // Per-printer defaults (not from the default printer)
                let default_paper = crate::print_windows::get_default_paper_size_win(&printer)
                    .ok().map(|(preset, w_mm, h_mm)| {
                        serde_json::json!({ "preset": preset, "widthMm": w_mm, "heightMm": h_mm })
                    });
                let default_margins = crate::print_windows::get_printer_margins_win(&printer)
                    .ok().map(|(l, t, r, b)| {
                        serde_json::json!({ "leftMm": l, "topMm": t, "rightMm": r, "bottomMm": b })
                    });
                ok_response(serde_json::json!({
                    "sizes": entries,
                    "defaultPaperSize": default_paper,
                    "defaultMargins": default_margins,
                }))
            }
            Err(e) => {
                log::error!("[RUST:commands] get_printer_paper_sizes — error: {}", e);
                err_response("E_PRINTER", &e)
            }
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        // Resolve PPD for real paper dimensions; fall back to lpoptions list if no PPD
        let ppd_result = find_cups_ppd(&printer).map(|p| parse_ppd_file(&p));
        // ppd_result.0 = paper_dims, ppd_result.1 = imageable_areas
        let ppd_dims = ppd_result.as_ref().map(|(dims, _)| dims);
        let ppd_areas = ppd_result.as_ref().map(|(_, areas)| areas);

        match std::process::Command::new("lpoptions")
            .arg("-p")
            .arg(&printer)
            .arg("-l")
            .output()
        {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let mut sizes: Vec<serde_json::Value> = Vec::new();
                let mut default_page_size: Option<String> = None;
                for line in stdout.lines() {
                    if line.starts_with("PageSize/") || line.starts_with("PageSize:") {
                        if let Some(val_part) = line.split(':').nth(1) {
                            for token in val_part.split_whitespace() {
                                let name = token.trim_start_matches('*').to_string();
                                // Detect default (has * prefix — the active/selected option)
                                if default_page_size.is_none()
                                    && (token.starts_with('*') || name != token)
                                {
                                    default_page_size = Some(name.clone());
                                }
                                if !name.is_empty() {
                                    // Try PPD first; fall back to known table; then 0.0
                                    let (w, h) = ppd_dims
                                        .and_then(|dims| {
                                            dims.iter()
                                                .find(|(n, _, _)| n == &name)
                                                .map(|(_, w, h)| (*w, *h))
                                        })
                                        .or_else(|| match name.as_str() {
                                            "A4" => Some((210.0, 297.0)),
                                            "Letter" => Some((215.9, 279.4)),
                                            "A3" => Some((297.0, 420.0)),
                                            "Legal" => Some((215.9, 355.6)),
                                            "A5" => Some((148.0, 210.0)),
                                            "A6" => Some((105.0, 148.0)),
                                            "Tabloid" => Some((279.4, 431.8)),
                                            "Executive" => Some((184.15, 266.7)),
                                            "4x6" => Some((101.6, 152.4)),
                                            "5x7" => Some((127.0, 177.8)),
                                            "8x10" => Some((203.2, 254.0)),
                                            _ => None,
                                        })
                                        .unwrap_or((0.0, 0.0));
                                    sizes.push(serde_json::json!({
                                        "name": name,
                                        "widthMm": w,
                                        "heightMm": h,
                                    }));
                                }
                            }
                        }
                        break;
                    }
                }

                // Build defaultPaperSize from detected default + dimensions
                let default_paper = default_page_size.as_ref().and_then(|name| {
                    let (w, h) = ppd_dims
                        .and_then(|dims| {
                            dims.iter()
                                .find(|(n, _, _)| n == name)
                                .map(|(_, w, h)| (*w, *h))
                        })
                        .or_else(|| match name.as_str() {
                            "A4" => Some((210.0, 297.0)),
                            "Letter" => Some((215.9, 279.4)),
                            _ => None,
                        })
                        .unwrap_or((0.0, 0.0));
                    if w > 0.0 && h > 0.0 {
                        Some(serde_json::json!({ "preset": name, "widthMm": w, "heightMm": h }))
                    } else {
                        None
                    }
                });

                // Build defaultMargins from PPD ImageableArea
                let default_margins = default_page_size.as_ref().and_then(|name| {
                    ppd_areas
                        .and_then(|areas| areas.get(name))
                        .map(|(x1, y1, x2, y2)| {
                            let paper_w = default_paper
                                .as_ref()
                                .and_then(|v| v.get("widthMm").and_then(|v| v.as_f64()))
                                .unwrap_or(210.0);
                            let paper_h = default_paper
                                .as_ref()
                                .and_then(|v| v.get("heightMm").and_then(|v| v.as_f64()))
                                .unwrap_or(297.0);
                            serde_json::json!({
                                "leftMm": x1,
                                "topMm": y1,
                                "rightMm": paper_w - x2,
                                "bottomMm": paper_h - y2,
                            })
                        })
                        .or_else(|| {
                            // Fallback: 3mm standard margin
                            Some(serde_json::json!({
                                "leftMm": 3.0, "topMm": 3.0, "rightMm": 3.0, "bottomMm": 3.0,
                            }))
                        })
                });

                ok_response(serde_json::json!({
                    "sizes": sizes,
                    "defaultPaperSize": default_paper,
                    "defaultMargins": default_margins,
                }))
            }
            Err(e) => err_response("E_PRINTER", &format!("Failed to query printer: {}", e)),
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        ok_response(
            serde_json::json!({ "sizes": [], "defaultPaperSize": null, "defaultMargins": null }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_system_printers_returns_envelope() {
        let result = get_system_printers();
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["ok"], true);
        assert!(value["data"]["printers"].is_array());
    }
}
