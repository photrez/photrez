// SPDX-License-Identifier: AGPL-3.0-or-later
// â”€â”€â”€ Tauri IPC Commands â”€â”€â”€
//
// All `#[tauri::command]` handlers exposed to the frontend.

use serde_json::Value;
use std::collections::HashMap;

use crate::response::{err_response, error_value, ok_response, validate_no_traversal, validate_path_extension, validate_path_safe, CONTRACT_VERSION};
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
        let mut file = archive.by_index(i)
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

/// Spools the image to native print system.
/// Accepts optional target `printer` name and `copies` count.
#[tauri::command]
pub(crate) fn print_image(
    path: String,
    printer: Option<String>,
    copies: Option<u32>,
) -> Result<Value, Value> {
    validate_path_extension(&path, &["png", "jpg", "jpeg"], "print")?;
    let path = validate_path_safe(&path, "print")?;

    let p = path;
    if !p.exists() {
        return err_response("E_IO", &format!("File not found: {}", p.display()));
    }

    let print_count = copies.unwrap_or(1).max(1);

    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use std::ptr::null_mut;

        extern "system" {
            fn ShellExecuteW(
                hwnd: *mut std::ffi::c_void,
                operation: *const u16,
                file: *const u16,
                parameters: *const u16,
                directory: *const u16,
                show_cmd: i32,
            ) -> isize;
        }

        let verb = if printer.as_ref().map_or(false, |s| !s.trim().is_empty()) {
            "printto"
        } else {
            "print"
        };

        let verb_wide: Vec<u16> = OsStr::new(verb).encode_wide().chain(std::iter::once(0)).collect();
        let file_wide: Vec<u16> = p.as_os_str().encode_wide().chain(std::iter::once(0)).collect();

        let params_wide: Option<Vec<u16>> = printer.as_ref().filter(|s| !s.trim().is_empty()).map(|name| {
            let formatted = format!("\"{}\"", name);
            OsStr::new(&formatted).encode_wide().chain(std::iter::once(0)).collect()
        });

        let params_ptr = match &params_wide {
            Some(vec) => vec.as_ptr(),
            None => null_mut(),
        };

        for _ in 0..print_count {
            let result = unsafe {
                ShellExecuteW(
                    null_mut(),
                    verb_wide.as_ptr(),
                    file_wide.as_ptr(),
                    params_ptr,
                    null_mut(),
                    1, // SW_SHOWNORMAL
                )
            };

            if result as i32 <= 32 {
                return err_response("E_IO", &format!("ShellExecuteW returned {}", result));
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        for _ in 0..print_count {
            match open::that(&p) {
                Ok(_) => {}
                Err(e) => return err_response("E_IO", &format!("Failed to open file: {}", e)),
            }
        }
    }

    ok_response(serde_json::json!({ "printed": p.to_string_lossy(), "copies": print_count }))
}

/// Open native OS printer driver properties dialog for the specified printer.
#[tauri::command]
pub(crate) fn open_printer_properties(printer: String) -> Result<Value, Value> {
    #[cfg(target_os = "windows")]
    {
        let arg = format!("/e /n \"{}\"", printer);
        let status = std::process::Command::new("rundll32")
            .arg("printui.dll,PrintUIEntry")
            .arg(&arg)
            .status();

        match status {
            Ok(_) => ok_response(serde_json::json!({ "opened": printer })),
            Err(e) => err_response("E_IO", &format!("Failed to open printer properties: {}", e)),
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        ok_response(serde_json::json!({ "opened": printer }))
    }
}

/// Enumerate system printers using the `printers` crate.
/// Returns list of printer names and the default printer name if set.
#[tauri::command]
pub(crate) fn get_system_printers() -> Result<Value, Value> {
    let printer_list = printers::get_printers();
    let printers_vec: Vec<String> = printer_list
        .iter()
        .map(|p| p.name.clone())
        .collect();

    let default_name = printers::get_default_printer()
        .map(|p| p.name.clone());

    ok_response(serde_json::json!({
        "printers": printers_vec,
        "default": default_name,
    }))
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
}
