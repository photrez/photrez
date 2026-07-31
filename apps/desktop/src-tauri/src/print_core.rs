// SPDX-License-Identifier: AGPL-3.0-or-later
// --- Print Commands ---
//
// print_image / print_image_raw + shared spooling internals and the
// PrintRateLimiter cooldown state (see also printers.rs / print_settings_cmds.rs).

use serde_json::Value;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::file_io::{check_path_trusted, TrustedPathsState, MAX_FILE_IO_BYTES};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use crate::printers::cups_media_name;
use crate::response::{
    err_response, error_value, ok_response, validate_path_extension, validate_path_safe,
};

const PRINT_MIN_INTERVAL: Duration = Duration::from_secs(2);

/// Global last-print timestamp. Managed as Tauri state so both `print_image`
/// and `print_image_raw` share the same cooldown window.
pub(crate) struct PrintRateLimiter {
    last_print: Mutex<Option<Instant>>,
}

impl Default for PrintRateLimiter {
    fn default() -> Self {
        Self {
            last_print: Mutex::new(None),
        }
    }
}

impl PrintRateLimiter {
    /// Returns Err (E_RATE_LIMIT) when the previous print was more recent
    /// than `PRINT_MIN_INTERVAL` ago; otherwise records `now` and returns Ok.
    pub(crate) fn check(&self) -> Result<(), Value> {
        let mut last = self.last_print.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        if let Some(prev) = *last {
            if now.duration_since(prev) < PRINT_MIN_INTERVAL {
                return Err(error_value(
                    "E_RATE_LIMIT",
                    "Print command called too frequently — wait a moment and try again",
                ));
            }
        }
        *last = Some(now);
        Ok(())
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
    state: tauri::State<'_, TrustedPathsState>,
    rate: tauri::State<'_, PrintRateLimiter>,
) -> Result<Value, Value> {
    rate.check()?;
    validate_path_extension(&path, &["png", "jpg", "jpeg"], "print")?;
    let path = validate_path_safe(&path, "print")?;
    check_path_trusted(&state, &path)?;
    print_image_inner(
        path,
        printer,
        copies,
        paper_width_mm,
        paper_height_mm,
        paper_preset,
        paper_index,
        document_name,
        orientation,
    )
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
                if let Err(e) = print_image_via_gdi(
                    &p,
                    &printer.system_name,
                    print_count,
                    pw_mm,
                    ph_mm,
                    0.0,
                    pidx,
                    doc_name,
                    orientation,
                ) {
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

        // H3: Use spawn + try_wait polling with 60s deadline instead of
        // blocking cmd.status(), matching the print_image_raw pattern.
        let mut cmd = std::process::Command::new("lp");
        if print_count > 1 {
            cmd.arg(format!("-n{}", print_count));
        }
        if let Some(ref name) = printer.as_ref().filter(|s| !s.trim().is_empty()) {
            cmd.arg("-d").arg(name);
        }
        cmd.arg("-t").arg(doc_name); // job title = document name
        cmd.arg("-o").arg("fit-to-page");
        cmd.arg("-o").arg(format!("media={}", media));
        cmd.arg(&p);

        let mut child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let _ = open::that(&p);
                return err_response(
                    "E_PRINTER",
                    &format!("Failed to spawn lp: {e}. File opened in viewer."),
                );
            }
        };

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s,
                Ok(None) => {
                    if std::time::Instant::now() > deadline {
                        let _ = child.kill();
                        let _ = open::that(&p);
                        return err_response(
                            "E_PRINTER",
                            "Print via lp timed out after 60s. File opened in viewer.",
                        );
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(e) => {
                    let _ = open::that(&p);
                    return err_response(
                        "E_IO",
                        &format!("Failed to check lp status: {e}. File opened in viewer."),
                    );
                }
            }
        };

        if !status.success() {
            let _ = open::that(&p);
            return err_response(
                "E_PRINTER",
                "Print via lp failed (non-zero exit). File opened in viewer.",
            );
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
    rate: tauri::State<'_, PrintRateLimiter>,
) -> Result<Value, Value> {
    // Rate-limit before reading the (potentially 256 MB) body — a hot-loop
    // caller is rejected without ever paying for the transfer.
    rate.check()?;

    let tauri::ipc::InvokeBody::Raw(data) = request.body() else {
        return err_response("E_VALIDATION", "Expected raw binary body");
    };

    if data.len() as u64 > MAX_FILE_IO_BYTES {
        return err_response(
            "E_RESOURCE_LIMIT",
            "Print data exceeds maximum allowed size (256 MB)",
        );
    }

    // Parse headers via error_value (returns Value directly, compatible with ?)
    let headers = request.headers();
    let Some(printer_str) = headers.get("printer").and_then(|v| v.to_str().ok()) else {
        return err_response("E_VALIDATION", "Missing printer header");
    };
    let printer = printer_str.to_string();
    let copies: u32 = headers
        .get("copies")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let paper_width_mm: f64 = headers
        .get("paperwidthmm")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(210.0);
    let paper_height_mm: f64 = headers
        .get("paperheightmm")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(297.0);
    let paper_index: i16 = headers
        .get("paperindex")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(9);
    let margin_mm: f64 = headers
        .get("marginmm")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    // Per-side margins (from frontend composite), fallback to uniform
    let margin_left_mm: f64 = headers
        .get("marginleftmm")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(margin_mm);
    let margin_right_mm: f64 = headers
        .get("marginrightmm")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(margin_mm);
    let margin_top_mm: f64 = headers
        .get("margintopmm")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(margin_mm);
    let margin_bottom_mm: f64 = headers
        .get("marginbottommm")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .unwrap_or(margin_mm);
    let document_name = headers
        .get("documentname")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("Untitled")
        .to_string();
    let orientation = headers
        .get("orientation")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("portrait")
        .to_string();

    // Image dimensions (raw RGBA, not encoded format) — required for GDI
    let width: u32 = headers
        .get("width")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| error_value("E_VALIDATION", "Missing width header"))?;
    let height: u32 = headers
        .get("height")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
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
        //
        // M4: This swap assumes frontend always sends RGBA (getImageData
        // returns unpremultiplied RGBA in all browsers). If a future change
        // switches to premultiplied alpha or a different byte order, the
        // swap must be adapted. The pattern is correct: frontend RGBA →
        // Rust swap to BGRA → StretchDIBits(BI_RGB) expects BGRA.
        let mut pixels = data.to_vec();
        for px in pixels.chunks_exact_mut(4) {
            px.swap(0, 2);
        }

        render_rgba_to_printer(
            &pixels,
            width,
            height,
            &target.system_name,
            copies.max(1),
            paper_width_mm,
            paper_height_mm,
            margin_left_mm,
            margin_right_mm,
            margin_top_mm,
            margin_bottom_mm,
            paper_index,
            &document_name,
            &orientation,
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
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let temp_path = temp_dir.join(format!("photrez-print-{ts}.png"));
        img.save(&temp_path)
            .map_err(|e| error_value("E_IO", &format!("Failed to write temp file: {e}")))?;

        let media = cups_media_name("Custom", paper_width_mm, paper_height_mm);
        let mut cmd = std::process::Command::new("lp");
        if copies > 1 {
            cmd.arg(format!("-n{copies}"));
        }
        cmd.arg("-d").arg(&printer);
        cmd.arg("-t").arg(&document_name);
        cmd.arg("-o").arg("fit-to-page");
        cmd.arg("-o").arg(format!("media={media}"));
        cmd.arg(&temp_path);

        // H3: Spawn lp with 60-second timeout.
        // try_wait polling avoids extra crate deps (wait_timeout) while
        // preventing a hung lp from blocking the Tauri command forever.
        let mut child = cmd.spawn().map_err(|e| {
            let _ = std::fs::remove_file(&temp_path);
            error_value("E_PRINTER", &format!("Failed to spawn lp: {e}"))
        })?;

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s,
                Ok(None) => {
                    if std::time::Instant::now() > deadline {
                        let _ = child.kill();
                        let _ = std::fs::remove_file(&temp_path);
                        return err_response("E_PRINTER", "Print via lp timed out after 60s");
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                Err(e) => {
                    let _ = std::fs::remove_file(&temp_path);
                    return err_response("E_IO", &format!("Failed to check lp status: {e}"));
                }
            }
        };

        let _ = std::fs::remove_file(&temp_path);
        if !status.success() {
            return err_response("E_PRINTER", "Print via lp failed (non-zero exit)");
        }
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        return err_response("E_PRINTER", "Printing not supported on this platform");
    }

    ok_response(serde_json::json!({ "status": "printed" }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_print_image_rejects_nonexistent_file() {
        // print_image validates path, then calls print_image_inner which
        // checks p.exists(). With a nonsense path we get E_VALIDATION
        // (validate_path_safe → canonicalize fails) or E_IO if the path
        // passes validation but doesn't exist.
        let result = print_image_inner(
            std::path::PathBuf::from("Z:\\nope\\missing.png"),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert!(result.is_err(), "should fail on nonexistent path");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("E_IO") || err.contains("E_VALIDATION"),
            "error should be E_IO or E_VALIDATION, got: {err}"
        );
    }

    // ── Print rate limiter tests (review #32) ───────────────────────

    #[test]
    fn test_rate_limiter_allows_first_call() {
        let limiter = PrintRateLimiter::default();
        assert!(limiter.check().is_ok(), "first print must be allowed");
    }

    #[test]
    fn test_rate_limiter_rejects_rapid_second_call() {
        let limiter = PrintRateLimiter::default();
        assert!(limiter.check().is_ok());
        // Second call within PRINT_MIN_INTERVAL (2s) must be rejected.
        let result = limiter.check();
        assert!(result.is_err(), "rapid second print must be rate-limited");
        let err = result.unwrap_err().to_string();
        assert!(err.contains("E_RATE_LIMIT"), "got: {err}");
    }

    #[test]
    fn test_rate_limiter_recovers_after_interval() {
        let limiter = PrintRateLimiter::default();
        assert!(limiter.check().is_ok());
        assert!(limiter.check().is_err());
        // Simulate the cooldown elapsing: rewind the stored timestamp.
        {
            let mut last = limiter.last_print.lock().unwrap_or_else(|e| e.into_inner());
            *last = Some(Instant::now() - PRINT_MIN_INTERVAL - Duration::from_secs(1));
        }
        assert!(
            limiter.check().is_ok(),
            "print after cooldown must be allowed"
        );
    }
}
