// SPDX-License-Identifier: AGPL-3.0-or-later
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod cursor;
mod menu;
mod print_geometry;
mod print_settings;
#[cfg(target_os = "windows")]
mod print_windows;
mod response;
mod window_state;

use print_settings::PrintSettings;
use std::sync::Mutex;
use tauri::{Emitter, Manager};

struct CliState(Mutex<Option<String>>);

/// Accept only existing files with a readable extension (same whitelist as
/// `commands::READ_FILE_EXTENSIONS` plus `.ptz`). Returns None for anything
/// else so garbage CLI args never enter `TrustedPathsState`.
fn validate_cli_open_path(p: &str) -> Option<String> {
    let path = std::path::PathBuf::from(p);
    let ext_ok = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            [
                "ptz", "png", "jpg", "jpeg", "webp", "gif", "bmp", "tif", "tiff", "json",
            ]
            .contains(&e.to_ascii_lowercase().as_str())
        })
        .unwrap_or(false);
    if path.is_file() && ext_ok {
        Some(p.to_string())
    } else {
        eprintln!(
            "[RUST:startup] Ignoring CLI open path (not a readable file): {}",
            p
        );
        None
    }
}

fn main() {
    // Accept file path as first CLI argument, but only after trust-boundary
    // validation: it must be an existing file with a readable extension.
    // `trust_path` (setup below) canonicalizes and rejects traversal; this
    // pre-check keeps garbage CLI args (typos, folders, foreign types) out of
    // the trusted-path store entirely. The frontend re-validates on open.
    let cli_path: Option<String> = std::env::args()
        .nth(1)
        .filter(|p| !p.starts_with("--")) // skip tauri dev flags
        .and_then(|p| validate_cli_open_path(&p));

    tauri::Builder::default()
        .manage(CliState(Mutex::new(cli_path)))
        .manage(Mutex::new(PrintSettings::default()))
        .manage(commands::StreamingSaveState::default())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // ── Trusted-path state (dialog/CLI-approved file I/O) ─────────
            // Autosave writes under the app cache dir are auto-approved;
            // user-approved paths persist in the app config dir.
            let cache_dir = app
                .path()
                .app_cache_dir()
                .unwrap_or_else(|_| std::env::temp_dir());
            std::fs::create_dir_all(&cache_dir).ok();
            let cache_dir = std::fs::canonicalize(&cache_dir).unwrap_or(cache_dir);
            let config_dir = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| std::env::temp_dir());
            let trusted = commands::TrustedPathsState::new(
                cache_dir,
                config_dir.join("photrez").join("trusted-paths.json"),
            );
            // The CLI-arg path is implicitly user-approved (passed at launch).
            if let Some(cli) = app
                .state::<CliState>()
                .0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone()
            {
                trusted.trust_path(&cli);
            }
            app.manage(trusted);

            // ── Initialize default printer in print settings state ──────
            // Prevents a race condition on first dialog open: the Effect 1
            // frontend code would otherwise call setPrinter via IPC, emitting
            // an event that can be lost before the event listener is registered.
            if let Ok(mut settings) = app.state::<Mutex<PrintSettings>>().lock() {
                let had = settings.selected_printer.is_some();
                settings.initialize_default_printer();
                if !had && settings.selected_printer.is_some() {
                    eprintln!(
                        "[RUST:setup] Initialized default printer: {:?}",
                        settings.selected_printer
                    );
                }
            }

            app.set_menu(menu::build_native_menu(app)?)?;
            app.on_menu_event(|app_handle, event| {
                let id = event.id().0.as_str();
                if menu::is_editor_menu_id(id) {
                    let _ = app_handle.emit(menu::NATIVE_MENU_EVENT, id);
                }
            });

            if let Some(window) = app.get_webview_window("main") {
                let mut saved = window_state::load_window_state(&app.handle());
                // First launch: saved state matches tauri.conf.json defaults,
                // nothing to restore — just show the window.
                let is_first_launch = saved.x.is_none() && saved.y.is_none() && !saved.maximized;
                if !is_first_launch {
                    // Guard: if saved position is off-screen (e.g., external monitor
                    // disconnected), snap back to primary monitor center.
                    window_state::snap_state_to_screen(&mut saved, app.handle());
                    let _ = window.set_size(tauri::PhysicalSize::new(saved.width, saved.height));
                    if let (Some(x), Some(y)) = (saved.x, saved.y) {
                        let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
                    }
                    if saved.maximized {
                        let _ = window.maximize();
                    }
                }
                // Window starts hidden ("visible": false in tauri.conf.json).
                // Show after state is fully applied — prevents resize flash.
                let _ = window.show();
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    window_state::save_window_state(window);
                    // Prevent the window from closing immediately. The frontend
                    // will show sequential save-confirm dialogs for each dirty
                    // document, then invoke("close_app") to exit the app via Rust's
                    // app.exit() which bypasses this CloseRequested handler.
                    api.prevent_close();
                    let _ = window.emit("close-requested", ());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::ping,
            commands::get_contract_info,
            commands::get_pending_open_path,
            commands::set_trusted_paths,
            commands::read_file_bytes,
            commands::write_file_bytes,
            commands::save_project,
            commands::save_project_binary,
            commands::save_project_streaming_begin,
            commands::save_project_streaming_write_layer,
            commands::save_project_streaming_end,
            commands::save_project_streaming_cancel,
            commands::load_project,
            commands::print_image,
            commands::print_image_raw,
            commands::get_system_printers,
            commands::get_printer_paper_sizes,
            commands::open_printer_properties,
            commands::get_print_settings,
            commands::set_paper,
            commands::toggle_orientation,
            commands::set_orientation,
            commands::set_margin,
            commands::set_per_side_margins,
            commands::set_scale_to_fit,
            commands::set_scale_percent,
            commands::set_center_image,
            commands::set_top_offset_mm,
            commands::set_left_offset_mm,
            commands::set_copies,
            commands::set_unit,
            commands::set_show_paper_white,
            commands::set_color_handling,
            commands::set_rendering_intent,
            commands::set_black_point_compensation,
            commands::set_printer,
            commands::open_printer_properties_and_apply,
            commands::convert_mm_to_current_unit,
            commands::convert_current_unit_to_mm,
            cursor::set_native_cursor,
            commands::delete_file,
            commands::delete_autosave_file,
            commands::close_app,
        ])
        .run(tauri::generate_context!())
        .expect("Error while running Photrez");
}

#[cfg(test)]
mod tests {
    use super::validate_cli_open_path;
    use std::io::Write;

    fn touch(name: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(name);
        let _ = std::fs::File::create(&path).unwrap().write_all(b"x");
        path
    }

    #[test]
    fn accepts_existing_readable_file() {
        let p = touch("photrez-cli-test.png");
        assert!(validate_cli_open_path(&p.to_string_lossy()).is_some());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn extension_match_is_case_insensitive() {
        let p = touch("photrez-cli-test.PTZ");
        assert!(validate_cli_open_path(&p.to_string_lossy()).is_some());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn rejects_unsupported_extension() {
        let p = touch("photrez-cli-test.txt");
        assert!(validate_cli_open_path(&p.to_string_lossy()).is_none());
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn rejects_missing_file() {
        let p = std::env::temp_dir().join("photrez-cli-missing.png");
        let _ = std::fs::remove_file(&p);
        assert!(validate_cli_open_path(&p.to_string_lossy()).is_none());
    }

    #[test]
    fn rejects_directory() {
        let dir = std::env::temp_dir().join("photrez-cli-dir-test");
        let _ = std::fs::create_dir_all(&dir);
        assert!(validate_cli_open_path(&dir.to_string_lossy()).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
