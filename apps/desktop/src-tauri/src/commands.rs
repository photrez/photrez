// SPDX-License-Identifier: AGPL-3.0-or-later
// â”€â”€â”€ Tauri IPC Commands â”€â”€â”€
//
// All `#[tauri::command]` handlers exposed to the frontend.

use serde_json::Value;
use std::collections::HashMap;

use crate::response::{err_response, error_value, ok_response, validate_path_extension, validate_path_safe, CONTRACT_VERSION};
use crate::CliState;

const MAX_FILE_IO_BYTES: u64 = 256 * 1024 * 1024;
const READ_FILE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff"];
const WRITE_FILE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp"];

#[tauri::command]
pub(crate) fn ping() -> Result<Value, Value> {
    ok_response(serde_json::json!({ "status": "ok", "service": "native" }))
}

#[tauri::command]
pub(crate) fn get_contract_info() -> Result<Value, Value> {
    ok_response(serde_json::json!({
        "name": "photrez-command-contract",
        "version": CONTRACT_VERSION,
        "supported_commands": [
            "ping", "get_contract_info",
            "read_file_bytes", "write_file_bytes",
            "save_project", "load_project",
            "print_image", "get_system_printers", "open_printer_properties"
        ]
    }))
}

/// Returns a file path passed via CLI argument, if any. Used once, then cleared.
#[tauri::command]
pub(crate) fn get_pending_open_path(state: tauri::State<'_, CliState>) -> Result<Value, Value> {
    let mut path = state.0.lock().unwrap_or_else(|e| e.into_inner());
    match path.take() {
        Some(p) => ok_response(serde_json::json!({ "path": p })),
        None => ok_response(serde_json::json!({ "path": null })),
    }
}

/// Read file bytes from disk. Returns base64-encoded bytes.
#[tauri::command]
pub(crate) fn read_file_bytes(path: String) -> Result<Value, Value> {
    validate_path_extension(&path, READ_FILE_EXTENSIONS, "read")?;
    let path = validate_path_safe(&path, "read")?;

    match std::fs::metadata(&path) {
        Ok(metadata) if metadata.len() > MAX_FILE_IO_BYTES => {
            return err_response(
                "E_RESOURCE_LIMIT",
                "File is too large for IPC transfer; max supported size is 256 MB",
            );
        }
        Ok(_) => {}
        Err(e) => return err_response("E_IO", &format!("Failed to inspect file: {}", e)),
    }

    match std::fs::read(&path) {
        Ok(bytes) => {
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            ok_response(serde_json::json!({
                "path": path,
                "size": bytes.len(),
                "data": b64
            }))
        }
        Err(e) => err_response("E_IO", &format!("Failed to read file: {}", e)),
    }
}

/// Write bytes to disk.
#[tauri::command]
pub(crate) fn write_file_bytes(path: String, data: String) -> Result<Value, Value> {
    validate_path_extension(&path, WRITE_FILE_EXTENSIONS, "write")?;
    let path = validate_path_safe(&path, "write")?;

    use base64::Engine;
    let bytes = match base64::engine::general_purpose::STANDARD.decode(&data) {
        Ok(b) => b,
        Err(e) => return err_response("E_VALIDATION", &format!("Invalid base64: {}", e)),
    };
    if bytes.len() as u64 > MAX_FILE_IO_BYTES {
        return err_response(
            "E_RESOURCE_LIMIT",
            "File is too large for IPC transfer; max supported size is 256 MB",
        );
    }

    match std::fs::write(&path, &bytes) {
        Ok(_) => ok_response(serde_json::json!({
            "path": path,
            "size": bytes.len()
        })),
        Err(e) => err_response("E_IO", &format!("Failed to write: {}", e)),
    }
}

#[tauri::command]
pub(crate) fn save_project(
    path: String,
    document_json: String,
    layers: HashMap<String, String>,
) -> Result<Value, Value> {
    validate_path_extension(&path, &["ptz"], "save project")?;
    let path = validate_path_safe(&path, "save project")?;

    let file = std::fs::File::create(&path)
        .map_err(|e| error_value("E_IO", &format!("Failed to create project file: {}", e)))?;

    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("document.json", options)
        .map_err(|e| error_value("E_IO", &format!("Failed to start document.json: {}", e)))?;
    use std::io::Write;
    zip.write_all(document_json.as_bytes())
        .map_err(|e| error_value("E_IO", &format!("Failed to write document.json: {}", e)))?;

    for (layer_id, base64_data) in layers {
        use base64::Engine;
        let bytes = base64::engine::general_purpose::STANDARD.decode(&base64_data)
            .map_err(|e| error_value("E_VALIDATION", &format!("Invalid base64 for layer {}: {}", layer_id, e)))?;

        let zip_layer_path = format!("layers/{}.png", layer_id);
        zip.start_file(&zip_layer_path, options)
            .map_err(|e| error_value("E_IO", &format!("Failed to start layer file {}: {}", zip_layer_path, e)))?;
        zip.write_all(&bytes)
            .map_err(|e| error_value("E_IO", &format!("Failed to write layer {}: {}", layer_id, e)))?;
    }

    zip.finish()
        .map_err(|e| error_value("E_IO", &format!("Failed to finish project archive: {}", e)))?;

    ok_response(serde_json::json!({ "path": path }))
}

/// Like `save_project` but accepts raw layer bytes instead of base64 strings,
/// avoiding a large base64 round-trip over the IPC channel for big documents.
#[tauri::command]
pub(crate) fn save_project_binary(
    path: String,
    document_json: String,
    layers: HashMap<String, Vec<u8>>,
) -> Result<Value, Value> {
    validate_path_extension(&path, &["ptz"], "save project")?;
    let path = validate_path_safe(&path, "save project")?;

    let file = std::fs::File::create(&path)
        .map_err(|e| error_value("E_IO", &format!("Failed to create project file: {}", e)))?;

    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    zip.start_file("document.json", options)
        .map_err(|e| error_value("E_IO", &format!("Failed to start document.json: {}", e)))?;
    use std::io::Write;
    zip.write_all(document_json.as_bytes())
        .map_err(|e| error_value("E_IO", &format!("Failed to write document.json: {}", e)))?;

    for (layer_id, bytes) in layers {
        let zip_layer_path = format!("layers/{}.png", layer_id);
        zip.start_file(&zip_layer_path, options)
            .map_err(|e| error_value("E_IO", &format!("Failed to start layer file {}: {}", zip_layer_path, e)))?;
        zip.write_all(&bytes)
            .map_err(|e| error_value("E_IO", &format!("Failed to write layer {}: {}", layer_id, e)))?;
    }

    zip.finish()
        .map_err(|e| error_value("E_IO", &format!("Failed to finish project archive: {}", e)))?;

    ok_response(serde_json::json!({ "path": path }))
}

#[tauri::command]
pub(crate) fn load_project(path: String) -> Result<Value, Value> {
    validate_path_extension(&path, &["ptz"], "load project")?;
    let path = validate_path_safe(&path, "load project")?;

    let file = std::fs::File::open(&path)
        .map_err(|e| error_value("E_IO", &format!("Failed to open project file: {}", e)))?;

    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| error_value("E_IO", &format!("Failed to read project archive: {}", e)))?;

    let mut document_json = String::new();
    let mut layers = HashMap::new();

    for i in 0..archive.len() {
        let file = archive.by_index(i)
            .map_err(|e| error_value("E_IO", &format!("Failed to read index {} inside project archive: {}", i, e)))?;

        let name = file.name().to_string();
        if name == "document.json" {
            use std::io::Read;
            let mut json_limit = file.take(1024 * 1024); // 1MB cukup untuk JSON
            json_limit.read_to_string(&mut document_json)
                .map_err(|e| error_value("E_IO", &format!("Failed to read document.json: {}", e)))?;
        } else if name.starts_with("layers/") && name.ends_with(".png") {
            let layer_id = name.strip_prefix("layers/")
                .and_then(|s| s.strip_suffix(".png"))
                .unwrap_or(&name)
                .to_string();

            use std::io::Read;
            let mut bytes = Vec::new();
            // Zip bomb protection: limit decompressed size per entry
            let mut limit_reader = file.take(MAX_FILE_IO_BYTES);
            limit_reader.read_to_end(&mut bytes)
                .map_err(|e| error_value("E_IO", &format!("Failed to read layer file {}: {}", name, e)))?;

            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
            layers.insert(layer_id, b64);
        }
    }

    if document_json.is_empty() {
        return err_response("E_IO", "document.json not found in the project archive");
    }

    ok_response(serde_json::json!({
        "document_json": document_json,
        "layers": layers,
    }))
}

/// Delete a file from disk. Restricted to the temp directory for safety â€”
/// used for cleaning up temp files (e.g. exported print spool) after print.
#[tauri::command]
pub(crate) fn delete_file(path: String) -> Result<Value, Value> {
    validate_path_extension(&path, &["png", "ptz"], "delete")?;
    let path = validate_path_safe(&path, "delete")?;

    // Alpha mitigation: only allow delete inside the OS temp directory.
    let temp_dir = std::env::temp_dir();
    let canonical_temp = std::fs::canonicalize(&temp_dir).unwrap_or(temp_dir);
    if !path.starts_with(&canonical_temp) {
        return err_response(
            "E_VALIDATION",
            "Delete is only allowed inside the temporary directory",
        );
    }

    match std::fs::remove_file(&path) {
        Ok(_) => ok_response(serde_json::json!({ "deleted": path.to_string_lossy() })),
        Err(e) => err_response("E_IO", &format!("Failed to delete file: {}", e)),
    }
}

/// Close the application. Called by the frontend after all dirty documents
/// have been handled. Bypasses CloseRequested handler entirely by exiting
/// the Tauri app process directly.
#[tauri::command]
pub(crate) fn close_app(app: tauri::AppHandle) -> Result<Value, Value> {
    app.exit(0);
    ok_response(serde_json::json!({ "closed": true }))
}

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
    if p.is_file() { Some(p.to_path_buf()) } else { None }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
/// Parse a CUPS PPD file for PaperDimension and ImageableArea entries.
///
/// PaperDimension: `*PaperDimension Name/Desc: "width_pt height_pt"`
/// ImageableArea:  `*ImageableArea Name/Desc: "x1 y1 x2 y2"`
/// PostScript points: 1 pt = 1/72 inch = 25.4/72 mm
fn parse_ppd_file(ppd_path: &Path) -> (Vec<(String, f64, f64)>, HashMap<String, (f64, f64, f64, f64)>) {
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
                        .split('/').next().unwrap_or("").trim().to_string();
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
                        .split('/').next().unwrap_or("").trim().to_string();
                    if let Some(coords) = parts.iter().map(|s| s.parse::<f64>().ok()).collect::<Option<Vec<_>>>() {
                        imageable_areas.insert(name, (pt_to_mm(coords[0]), pt_to_mm(coords[1]), pt_to_mm(coords[2]), pt_to_mm(coords[3])));
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
fn cups_media_name(preset: &str, width_mm: f64, height_mm: f64) -> String {
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

#[tauri::command]
pub(crate) fn print_image(
    path: String,
    printer: Option<String>,
    copies: Option<u32>,
    paper_width_mm: Option<f64>,
    paper_height_mm: Option<f64>,
    paper_preset: Option<String>,
    paper_index: Option<i16>,
    document_name: Option<String>,
    orientation: Option<String>,
) -> Result<Value, Value> {
    validate_path_extension(&path, &["png", "jpg", "jpeg"], "print")?;
    let path = validate_path_safe(&path, "print")?;
    print_image_inner(path, printer, copies, paper_width_mm, paper_height_mm,
        paper_preset, paper_index, document_name, orientation)
}

/// Internal print dispatch — called by `print_image`.
/// `path` must already be validated via `validate_path_safe`.
fn print_image_inner(
    path: std::path::PathBuf,
    printer: Option<String>,
    copies: Option<u32>,
    paper_width_mm: Option<f64>,
    paper_height_mm: Option<f64>,
    paper_preset: Option<String>,
    paper_index: Option<i16>,
    document_name: Option<String>,
    orientation: Option<String>,
) -> Result<Value, Value> {
    let p = path;
    if !p.exists() {
        return err_response("E_IO", &format!("File not found: {}", p.display()));
    }

    let print_count = copies.unwrap_or(1).max(1);
    // Hoist paper dimensions for cross-platform use (avoids repeating unwrap in each cfg block)
    let pw_mm = paper_width_mm.unwrap_or(210.0);
    let ph_mm = paper_height_mm.unwrap_or(297.0);
    // Orientation defaults to portrait for backwards compat / safety
    let orientation = orientation.as_deref().unwrap_or("portrait");

    #[cfg(target_os = "windows")]
    {
        // Use GDI printing (printer driver renders the image) instead of
        // the `printers` crate's print_file() which sends RAW PNG bytes
        // that most printer drivers cannot interpret as an image.
        use crate::print_windows::print_image_via_gdi;
        // paper_preset is unused on Windows (paper_index replaces it);
        // macOS/Linux path still uses it for CUPS media names.
        let _ = &paper_preset;

        let target = match printer.as_ref().filter(|s| !s.trim().is_empty()) {
            Some(name) => printers::get_printer_by_name(name),
            None => printers::get_default_printer(),
        };

        let pidx = paper_index.unwrap_or(256); // DMPAPER_USER default

        let doc_name = document_name.as_deref().unwrap_or("Untitled");

        match target {
            Some(printer) => {
                if let Err(e) = print_image_via_gdi(&p, &printer.system_name, print_count, pw_mm, ph_mm, pidx, doc_name, orientation) {
                    return err_response("E_PRINTER", &e);
                }
            }
            None => {
                // No printer found — open file in default viewer
                let _ = open::that(&p);
                return err_response("E_PRINTER", "No printer found. File opened in viewer.");
            }
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        // macOS/Linux: use lp (CUPS) for headless command-line printing.
        // lp accepts -n N for copies and prints PNG via CUPS filter.
        // Best practice from CUPS.org: use -o fit-to-page to ensure the
        // image scales to fit the printable area; also pass media size
        // so CUPS uses the correct paper dimensions.
        // Use standard CUPS media names (e.g. "A4", "Letter") for presets
        // instead of raw "WxHmm" format, which is more reliable across
        // different printer PPDs.
        let preset = paper_preset.as_deref().unwrap_or("Custom");
        let media = cups_media_name(preset, pw_mm, ph_mm);
        let doc_name = document_name.as_deref().unwrap_or("Untitled");

        let mut cmd = std::process::Command::new("lp");
        if print_count > 1 {
            cmd.arg(format!("-n{}", print_count));
        }
        if let Some(ref name) = printer.as_ref().filter(|s| !s.trim().is_empty()) {
            cmd.arg("-d").arg(name);
        }
        cmd.arg("-t").arg(doc_name);  // job title = document name
        cmd.arg("-o").arg("fit-to-page");
        cmd.arg("-o").arg(format!("media={}", media));
        cmd.arg(&p);
        match cmd.status() {
            Ok(status) if status.success() => {}
            Ok(_) | Err(_) => {
                // Fallback: open file in default viewer
                let _ = open::that(&p);
                return err_response("E_PRINTER", "Print via lp failed. File opened in viewer.");
            }
        }
    }

    ok_response(serde_json::json!({ "printed": p.to_string_lossy(), "copies": print_count }))
}

/// Print a composited image from raw RGBA pixels via Tauri v2 raw IPC.
/// The frontend sends raw RGBA pixels (via ctx.getImageData) as the binary
/// body and metadata (printer, dimensions, paper, orientation) in headers.
/// Raw pixels → GDI → printer driver. Zero format encoding.
///
/// Windows: sends raw RGBA directly to GDI (render_rgba_to_printer).
/// macOS/Linux: re-encodes as PNG temp file and dispatches via CUPS lp.
#[tauri::command]
pub(crate) fn print_image_raw(
    request: tauri::ipc::Request<'_>,
) -> Result<Value, Value> {
    let tauri::ipc::InvokeBody::Raw(data) = request.body() else {
        return err_response("E_VALIDATION", "Expected raw binary body");
    };

    if data.len() as u64 > MAX_FILE_IO_BYTES {
        return err_response("E_RESOURCE_LIMIT",
            "Print data exceeds maximum allowed size (256 MB)");
    }

    // Parse headers via error_value (returns Value directly, compatible with ?)
    let headers = request.headers();
    let Some(printer_str) = headers.get("printer").and_then(|v| v.to_str().ok()) else {
        return err_response("E_VALIDATION", "Missing printer header");
    };
    let printer = printer_str.to_string();
    let copies: u32 = headers.get("copies")
        .and_then(|v| v.to_str().ok()).and_then(|s| s.parse().ok()).unwrap_or(1);
    let paper_width_mm: f64 = headers.get("paperwidthmm")
        .and_then(|v| v.to_str().ok()).and_then(|s| s.parse().ok()).unwrap_or(210.0);
    let paper_height_mm: f64 = headers.get("paperheightmm")
        .and_then(|v| v.to_str().ok()).and_then(|s| s.parse().ok()).unwrap_or(297.0);
    let paper_index: i16 = headers.get("paperindex")
        .and_then(|v| v.to_str().ok()).and_then(|s| s.parse().ok()).unwrap_or(9);
    let document_name = headers.get("documentname")
        .and_then(|v| v.to_str().ok()).unwrap_or("Untitled").to_string();
    let orientation = headers.get("orientation")
        .and_then(|v| v.to_str().ok()).unwrap_or("portrait").to_string();

    // Image dimensions (raw RGBA, not encoded format) — required for GDI
    let width: u32 = headers.get("width")
        .and_then(|v| v.to_str().ok()).and_then(|s| s.parse().ok())
        .ok_or_else(|| error_value("E_VALIDATION", "Missing width header"))?;
    let height: u32 = headers.get("height")
        .and_then(|v| v.to_str().ok()).and_then(|s| s.parse().ok())
        .ok_or_else(|| error_value("E_VALIDATION", "Missing height header"))?;

    // Resolve printer — use error_value for ? compatibility
    let target = printers::get_printer_by_name(&printer)
        .or_else(|| printers::get_default_printer())
        .ok_or_else(|| error_value("E_PRINTER", "No printer found"))?;

    #[cfg(target_os = "windows")]
    {
        use crate::print_windows::render_rgba_to_printer;

        // Raw RGBA from frontend (ctx.getImageData) is RGBA byte order.
        // Windows GDI StretchDIBits with BI_RGB | 32bpp expects BGRA.
        // Swap R↔B to match Windows DIB byte order.
        let mut pixels = data.to_vec();
        for px in pixels.chunks_exact_mut(4) {
            px.swap(0, 2);
        }

        render_rgba_to_printer(
            &pixels, width, height,
            &target.system_name, copies.max(1),
            paper_width_mm, paper_height_mm, paper_index,
            &document_name, &orientation,
        )
        .map_err(|e| error_value("E_PRINTER", &e))?;
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        // Encode raw RGBA to PNG in Rust (image::load_from_memory no longer
        // applies since the bytes are raw RGBA, not PNG). This adds encode
        // time on macOS/Linux, but the primary optimization target is Windows.
        use image::{ImageBuffer, Rgba};
        let img = ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, data.to_vec())
            .ok_or_else(|| error_value("E_INTERNAL", "Failed to create image buffer"))?;

        let temp_dir = std::env::temp_dir();
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos()).unwrap_or(0);
        let temp_path = temp_dir.join(format!("photrez-print-{ts}.png"));
        img.save(&temp_path)
            .map_err(|e| error_value("E_IO", &format!("Failed to write temp file: {e}")))?;

        let media = cups_media_name("Custom", paper_width_mm, paper_height_mm);
        let mut cmd = std::process::Command::new("lp");
        if copies > 1 { cmd.arg(format!("-n{copies}")); }
        cmd.arg("-d").arg(&printer);
        cmd.arg("-t").arg(&document_name);
        cmd.arg("-o").arg("fit-to-page");
        cmd.arg("-o").arg(format!("media={media}"));
        cmd.arg(&temp_path);

        if !cmd.status().map(|s| s.success()).unwrap_or(false) {
            let _ = std::fs::remove_file(&temp_path);
            return err_response("E_PRINTER", "Print via lp failed");
        }
        let _ = std::fs::remove_file(&temp_path);
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        return err_response("E_PRINTER", "Printing not supported on this platform");
    }

    ok_response(serde_json::json!({ "status": "printed" }))
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
            Ok(None) => {
                ok_response(serde_json::json!({ "applied": false, "cancelled": true }))
            }
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
    let printers_vec: Vec<String> = printer_list
        .iter()
        .map(|p| p.name.clone())
        .collect();

    let default_name = printers::get_default_printer()
        .map(|p| p.name.clone());

    // Try to get the default printer's current paper size (best-effort)
    let mut default_paper: Option<serde_json::Value> = None;

    // Try to get the default printer's hardware margins (best-effort)
    let mut default_margins: Option<serde_json::Value> = None;

    #[cfg(target_os = "windows")]
    {
        if let Some(ref def_name) = default_name {
            if let Ok((preset, w_mm, h_mm)) = crate::print_windows::get_default_paper_size_win(def_name) {
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
                    .arg("-p").arg(def_name).arg("-l").output()
                {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    for line in stdout.lines() {
                        if line.starts_with("PageSize/") || line.starts_with("PageSize:") {
                            if let Some(val_part) = line.split(':').nth(1) {
                                for token in val_part.split_whitespace() {
                                    let t = token.trim_start_matches('*');
                                    if t != token || token.starts_with('*') {
                                        // Look up dimensions from parsed PPD
                                        if let Some((_, w, h)) = paper_dims.iter().find(|(n, _, _)| n == t) {
                                            default_paper = Some(serde_json::json!({
                                                "preset": t,
                                                "widthMm": w,
                                                "heightMm": h,
                                            }));
                                        } else {
                                            // Standard sizes not in PPD — use known values
                                            let (w, h) = match t {
                                                "A4"     => (210.0, 297.0),
                                                "Letter" => (215.9, 279.4),
                                                "A3"     => (297.0, 420.0),
                                                "Legal"  => (215.9, 355.6),
                                                "A5"     => (148.0, 210.0),
                                                "A6"     => (105.0, 148.0),
                                                "Tabloid"    => (279.4, 431.8),
                                                "Executive"  => (184.15, 266.7),
                                                "4x6"    => (101.6, 152.4),
                                                "5x7"    => (127.0, 177.8),
                                                "8x10"   => (203.2, 254.0),
                                                _        => (0.0, 0.0),
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
                            let paper_w = dp.get("widthMm").and_then(|v| v.as_f64()).unwrap_or(210.0);
                            let paper_h = dp.get("heightMm").and_then(|v| v.as_f64()).unwrap_or(297.0);
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

    eprintln!("[RUST:commands] get_system_printers — printers={:?}, default={:?}, defaultPaperSize={:?}, defaultMargins={:?}",
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
                eprintln!("[RUST:commands] get_printer_paper_sizes — printer={}, count={}", printer, sizes.len());
                let entries: Vec<serde_json::Value> = sizes
                    .into_iter()
                    .map(|(name, w, h, idx)| {
                        eprintln!("[RUST:commands] get_printer_paper_sizes — entry: name={}, w={}, h={}, idx={}", name, w, h, idx);
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
                eprintln!("[RUST:commands] get_printer_paper_sizes — error: {}", e);
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
            .arg("-p").arg(&printer).arg("-l").output()
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
                                if default_page_size.is_none() && (token.starts_with('*') || name != token) {
                                    default_page_size = Some(name.clone());
                                }
                                if !name.is_empty() {
                                    // Try PPD first; fall back to known table; then 0.0
                                    let (w, h) = ppd_dims.and_then(|dims| {
                                        dims.iter().find(|(n, _, _)| n == &name).map(|(_, w, h)| (*w, *h))
                                    })
                                    .or_else(|| match name.as_str() {
                                        "A4"     => Some((210.0, 297.0)),
                                        "Letter" => Some((215.9, 279.4)),
                                        "A3"     => Some((297.0, 420.0)),
                                        "Legal"  => Some((215.9, 355.6)),
                                        "A5"     => Some((148.0, 210.0)),
                                        "A6"     => Some((105.0, 148.0)),
                                        "Tabloid"    => Some((279.4, 431.8)),
                                        "Executive"  => Some((184.15, 266.7)),
                                        "4x6"    => Some((101.6, 152.4)),
                                        "5x7"    => Some((127.0, 177.8)),
                                        "8x10"   => Some((203.2, 254.0)),
                                        _        => None,
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
                    let (w, h) = ppd_dims.and_then(|dims| {
                        dims.iter().find(|(n, _, _)| n == name).map(|(_, w, h)| (*w, *h))
                    })
                    .or_else(|| match name.as_str() {
                        "A4"     => Some((210.0, 297.0)),
                        "Letter" => Some((215.9, 279.4)),
                        _        => None,
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
                    ppd_areas.and_then(|areas| areas.get(name)).map(|(x1, y1, x2, y2)| {
                        let paper_w = default_paper.as_ref()
                            .and_then(|v| v.get("widthMm").and_then(|v| v.as_f64()))
                            .unwrap_or(210.0);
                        let paper_h = default_paper.as_ref()
                            .and_then(|v| v.get("heightMm").and_then(|v| v.as_f64()))
                            .unwrap_or(297.0);
                        serde_json::json!({
                            "leftMm": x1,
                            "topMm": y1,
                            "rightMm": paper_w - x2,
                            "bottomMm": paper_h - y2,
                        })
                    }).or_else(|| {
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
        ok_response(serde_json::json!({ "sizes": [], "defaultPaperSize": null, "defaultMargins": null }))
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Print State Commands (Rust-owns-state pattern)
// ═══════════════════════════════════════════════════════════════════════

use crate::print_geometry;
use crate::print_settings::PrintSettings;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, State};

/// Event name for print settings changes (dash-separated lowercase).
const EVENT_PRINT_SETTINGS_CHANGED: &str = "print-settings-changed";

/// Helper: emit print settings to all listeners.
fn emit_print_settings(app: &AppHandle, settings: &PrintSettings) {
    if let Ok(json) = serde_json::to_value(settings) {
        eprintln!("[RUST:commands] emit_print_settings — paper_name={}, paper_index={}, paper=({}, {}), orientation={}, printer={:?}, copies={}",
            settings.paper_name, settings.paper_index, settings.paper_width_mm, settings.paper_height_mm,
            settings.orientation, settings.selected_printer, settings.copies);
        app.emit(EVENT_PRINT_SETTINGS_CHANGED, json).ok();
    }
}

/// Get current print settings.
#[tauri::command]
pub(crate) fn get_print_settings(
    state: State<'_, Mutex<PrintSettings>>,
) -> Result<Value, Value> {
    let settings = state.lock().unwrap();
    eprintln!("[RUST:commands] get_print_settings — paper_name={}, paper_index={}, paper=({}, {}), orientation={}, printer={:?}, copies={}",
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
    let mut settings = state.lock().unwrap();
    // Guard: skip if nothing changed
    let changed = settings.paper_name != name
        || settings.paper_index != paper_index
        || (settings.paper_width_mm - width_mm).abs() > 0.001
        || (settings.paper_height_mm - height_mm).abs() > 0.001;
    if changed {
        eprintln!("[RUST:commands] set_paper — name={}, paper_index={}, width_mm={}, height_mm={}",
            name, paper_index, width_mm, height_mm);
        settings.set_paper(&name, paper_index, width_mm, height_mm);
        emit_print_settings(&app, &settings);
    } else {
        eprintln!("[RUST:commands] set_paper — unchanged, skipping emit");
    }
    ok_response(settings.clone())
}

/// Toggle orientation (portrait <-> landscape).
#[tauri::command]
pub(crate) fn toggle_orientation(
    state: State<'_, Mutex<PrintSettings>>,
    app: AppHandle,
) -> Result<Value, Value> {
    let mut settings = state.lock().unwrap();
    let new_orientation = if settings.orientation == "portrait" {
        "landscape"
    } else {
        "portrait"
    };
    eprintln!("[RUST:commands] toggle_orientation — from={} to={}, dims_before=({}, {})",
        settings.orientation, new_orientation, settings.paper_width_mm, settings.paper_height_mm);
    settings.set_orientation(new_orientation);
    let value = ok_response(settings.clone());
    eprintln!("[RUST:commands] toggle_orientation — after: dims=({}, {}), orientation={}",
        settings.paper_width_mm, settings.paper_height_mm, settings.orientation);
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
    let mut settings = state.lock().unwrap();
    if settings.orientation != orientation {
        eprintln!("[RUST:commands] set_orientation — orientation={}", orientation);
        settings.set_orientation(&orientation);
        emit_print_settings(&app, &settings);
    } else {
        eprintln!("[RUST:commands] set_orientation — unchanged, skipping emit");
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
    let mut settings = state.lock().unwrap();

    // Update hardware floor when the frontend provides it (e.g. on printer switch)
    if let Some(hw_min) = hardware_min_mm {
        settings.hardware_margin_min_mm = hw_min;
    }

    let changed = (settings.margin_mm - margin_mm).abs() > 0.001
        || hardware_min_mm.is_some();
    if changed {
        settings.set_margin_mm(margin_mm);
        emit_print_settings(&app, &settings);
    } else {
        eprintln!("[RUST:commands] set_margin — unchanged ({:.1}), skipping emit", margin_mm);
    }
    ok_response(settings.clone())
}

/// Set scale-to-fit enabled/disabled.
#[tauri::command]
pub(crate) fn set_scale_to_fit(
    enabled: bool,
    state: State<'_, Mutex<PrintSettings>>,
    app: AppHandle,
) -> Result<Value, Value> {
    let mut settings = state.lock().unwrap();
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
    let mut settings = state.lock().unwrap();
    let changed = (settings.scale_percent - percent).abs() > 0.01;
    if changed {
        settings.set_scale_percent(percent);
        emit_print_settings(&app, &settings);
    } else {
        eprintln!("[RUST:commands] set_scale_percent — unchanged ({:.2}), skipping emit", percent);
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
    let mut settings = state.lock().unwrap();
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
    let mut settings = state.lock().unwrap();
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
    let mut settings = state.lock().unwrap();
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
    let mut settings = state.lock().unwrap();
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
    let mut settings = state.lock().unwrap();
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
    let mut settings = state.lock().unwrap();
    if settings.show_paper_white != show {
        settings.set_show_paper_white(show);
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
    let mut settings = state.lock().unwrap();
    let changed = settings.selected_printer.as_deref() != Some(&printer);
    if changed {
        eprintln!("[RUST:commands] set_printer — printer={}, previous={:?}", printer, settings.selected_printer);
        settings.set_selected_printer(Some(printer));
        emit_print_settings(&app, &settings);
    } else {
        eprintln!("[RUST:commands] set_printer — unchanged, skipping emit");
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
        let settings = state.lock().unwrap();
        eprintln!("[RUST:commands] open_printer_properties_and_apply — reading state: printer={:?}, paper_index={}, paper=({}, {}), orientation={}",
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
                eprintln!("[RUST:commands] open_printer_properties_and_apply — dialog OK: name={}, w_mm={}, h_mm={}, new_orientation={}, new_paper_index={}",
                    name, w_mm, h_mm, new_orientation, new_paper_index);
                let mut settings = state.lock().unwrap();
                // BUG-02 defense-in-depth: skip update if nothing changed
                if settings.paper_name == name
                    && settings.paper_index == new_paper_index
                    && (settings.paper_width_mm - w_mm).abs() < 0.01
                    && (settings.paper_height_mm - h_mm).abs() < 0.01
                    && settings.orientation == new_orientation
                {
                    eprintln!("[RUST:commands] open_printer_properties_and_apply — unchanged, skipping");
                    return ok_response(serde_json::json!({
                        "applied": false,
                        "cancelled": false,
                        "unchanged": true,
                    }));
                }
                eprintln!("[RUST:commands] open_printer_properties_and_apply — applying: paper_name={}, paper_index={}, paper=({}, {}), orientation={}",
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
                eprintln!("[RUST:commands] open_printer_properties_and_apply — cancelled");
                ok_response(serde_json::json!({ "applied": false, "cancelled": true }))
            }
            Err(e) => {
                eprintln!("[RUST:commands] open_printer_properties_and_apply — error: {}", e);
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
    let unit = state.lock().unwrap().unit.clone();
    let converted = print_geometry::mm_to_unit(val_mm, &unit);
    ok_response(converted)
}

/// Convert value in current unit to mm.
#[tauri::command]
pub(crate) fn convert_current_unit_to_mm(
    val: f64,
    state: State<'_, Mutex<PrintSettings>>,
) -> Result<Value, Value> {
    let unit = state.lock().unwrap().unit.clone();
    let mm = print_geometry::unit_to_mm(val, &unit);
    ok_response(mm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;

    fn temp_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("photrez-test");
        let _ = std::fs::create_dir_all(&dir);
        dir.join(name)
    }

    #[test]
    fn test_write_file_bytes_creates_file() {
        let path = temp_path("test_write_creates.png");
        let _ = std::fs::remove_file(&path);

        let data = b"hello photrez export";
        let b64 = base64::engine::general_purpose::STANDARD.encode(data);
        let result = write_file_bytes(path.to_str().unwrap().to_string(), b64.clone());

        assert!(
            result.is_ok(),
            "write_file_bytes should succeed: {:?}",
            result
        );
        assert!(path.exists(), "file should exist on disk");

        let written = std::fs::read(&path).unwrap();
        assert_eq!(written, data, "written content should match input");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_write_file_bytes_roundtrip() {
        let path = temp_path("test_roundtrip.png");
        let _ = std::fs::remove_file(&path);

        // PNG magic bytes: 89 50 4E 47 0D 0A 1A 0A + minimal valid pixel
        let original: Vec<u8> = vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // PNG header
            0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, // IHDR chunk
            0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
            0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63,
            0x60, 0x60, 0x60, 0x00, 0x00, 0x00, 0x04, 0x00, 0x01, 0x27, 0x34, 0x27, 0x00, 0x00,
            0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82, // IEND chunk
        ];
        let b64 = base64::engine::general_purpose::STANDARD.encode(&original);

        // Write
        let write_result = write_file_bytes(path.to_str().unwrap().to_string(), b64.clone());
        assert!(write_result.is_ok());
        assert!(path.exists());

        // Read back via read_file_bytes
        let read_result = read_file_bytes(path.to_str().unwrap().to_string());
        assert!(read_result.is_ok());

        let value = read_result.unwrap();
        let obj = value.as_object().unwrap();
        let data_str = obj["data"]["data"].as_str().unwrap();
        let roundtrip = base64::engine::general_purpose::STANDARD
            .decode(data_str)
            .unwrap();
        assert_eq!(roundtrip, original, "roundtrip content should match");

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_write_file_bytes_invalid_base64() {
        let path = temp_path("test_invalid_b64.png");
        let result = write_file_bytes(
            path.to_str().unwrap().to_string(),
            "not-valid-base64!!!".to_string(),
        );
        assert!(result.is_err(), "invalid base64 should produce error");
        let err_value = result.unwrap_err();
        assert!(err_value.to_string().contains("E_VALIDATION"));
    }

    #[test]
    fn test_write_file_bytes_rejects_unsupported_extension() {
        let path = temp_path("test_unsupported_export.txt");
        let _ = std::fs::remove_file(&path);
        let b64 = base64::engine::general_purpose::STANDARD.encode(b"test");
        let result = write_file_bytes(path.to_str().unwrap().to_string(), b64);
        assert!(result.is_err(), "unsupported export extension should error");
        let err_value = result.unwrap_err();
        assert!(err_value.to_string().contains("E_VALIDATION"));
        assert!(
            !path.exists(),
            "unsupported export should not create a file"
        );
    }

    #[test]
    fn test_write_file_bytes_to_invalid_path() {
        let bad_path = format!("Z:\\nope\\{}", std::process::id());
        let b64 = base64::engine::general_purpose::STANDARD.encode(b"test");
        let result = write_file_bytes(bad_path, b64);
        assert!(result.is_err(), "write to invalid path should error");
    }

    #[test]
    fn test_read_file_bytes_nonexistent_file() {
        let result = read_file_bytes("Z:\\nonexistent_file_12345.png".to_string());
        assert!(result.is_err(), "reading nonexistent file should error");
        let err_value = result.unwrap_err();
        // The path cannot be canonicalized (drive Z: does not exist), so the
        // error may come from path validation (E_VALIDATION) rather than the
        // later metadata stat (E_IO). Either is an acceptable rejection.
        assert!(
            err_value.to_string().contains("E_IO")
                || err_value.to_string().contains("E_VALIDATION")
        );
    }

    #[test]
    fn test_read_file_bytes_rejects_unsupported_extension() {
        let path = temp_path("test_unsupported_import.txt");
        std::fs::write(&path, b"not an image").unwrap();
        let result = read_file_bytes(path.to_str().unwrap().to_string());
        assert!(result.is_err(), "unsupported import extension should error");
        let err_value = result.unwrap_err();
        assert!(err_value.to_string().contains("E_VALIDATION"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn test_ping_response() {
        let result = ping();
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["data"]["status"], "ok");
        assert_eq!(value["data"]["service"], "native");
    }

    #[test]
    fn test_get_contract_info_includes_write_command() {
        let result = get_contract_info();
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["contract_version"], CONTRACT_VERSION);
        assert_eq!(value["data"]["version"], CONTRACT_VERSION);
        let commands = value["data"]["supported_commands"].as_array().unwrap();
        let names: Vec<&str> = commands.iter().map(|c| c.as_str().unwrap()).collect();
        assert_eq!(
            names,
            vec![
                "ping",
                "get_contract_info",
                "read_file_bytes",
                "write_file_bytes",
                "save_project",
                "load_project",
                "print_image",
                "get_system_printers",
                "open_printer_properties"
            ]
        );
    }

    #[test]
    fn test_get_system_printers_returns_envelope() {
        let result = get_system_printers();
        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["ok"], true);
        assert!(value["data"]["printers"].is_array());
    }

    // ── Print dispatch tests ─────────────────────────────────────────
    // `print_image_inner` is private; we test it through `print_image`.

    #[test]
    fn test_print_image_rejects_nonexistent_file() {
        // print_image validates path, then calls print_image_inner which
        // checks p.exists(). With a nonsense path we get E_VALIDATION
        // (validate_path_safe → canonicalize fails) or E_IO if the path
        // passes validation but doesn't exist.
        let result = print_image(
            "Z:\\nope\\missing.png".to_string(),
            None, None, None, None, None, None, None, None,
        );
        assert!(result.is_err(), "should fail on nonexistent path");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("E_IO") || err.contains("E_VALIDATION"),
            "error should be E_IO or E_VALIDATION, got: {err}"
        );
    }
}
