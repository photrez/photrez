// SPDX-License-Identifier: AGPL-3.0-or-later
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cursor;
mod file_io;
mod menu;
mod print_core;
mod print_geometry;
mod print_settings;
mod print_settings_cmds;
#[cfg(target_os = "windows")]
mod print_windows;
mod printers;
mod response;
mod save_stream;
mod window_state;

use print_settings::PrintSettings;
use std::sync::Mutex;
use tauri::{Emitter, Manager};
use tauri_plugin_log::{Target, TargetKind};

struct CliState(Mutex<Option<String>>);

/// Accept only existing files with a readable extension (same whitelist as
/// `file_io::READ_FILE_EXTENSIONS` plus `.ptz`). Returns None for anything
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
        log::warn!(
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
        .manage(save_stream::StreamingSaveState::default())
        .manage(print_core::PrintRateLimiter::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            // Structured logging: stdout in dev, file in app log dir in prod.
            // Level Info hides debug!() noise from release builds; set
            // RUST_LOG=debug (or trace) when diagnosing issues.
            tauri_plugin_log::Builder::default()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: None }),
                ])
                .level(log::LevelFilter::Info)
                .build(),
        )
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
            let trusted = file_io::TrustedPathsState::new(
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
                    log::info!(
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
            file_io::ping,
            file_io::get_contract_info,
            file_io::get_pending_open_path,
            file_io::set_trusted_paths,
            file_io::read_file_bytes,
            file_io::write_file_bytes,
            file_io::save_project,
            file_io::save_project_binary,
            save_stream::save_project_streaming_begin,
            save_stream::save_project_streaming_write_layer,
            save_stream::save_project_streaming_end,
            save_stream::save_project_streaming_cancel,
            file_io::load_project,
            print_core::print_image,
            print_core::print_image_raw,
            printers::get_system_printers,
            printers::get_printer_paper_sizes,
            printers::open_printer_properties,
            print_settings_cmds::get_print_settings,
            print_settings_cmds::set_paper,
            print_settings_cmds::toggle_orientation,
            print_settings_cmds::set_orientation,
            print_settings_cmds::set_margin,
            print_settings_cmds::set_per_side_margins,
            print_settings_cmds::set_scale_to_fit,
            print_settings_cmds::set_scale_percent,
            print_settings_cmds::set_center_image,
            print_settings_cmds::set_top_offset_mm,
            print_settings_cmds::set_left_offset_mm,
            print_settings_cmds::set_copies,
            print_settings_cmds::set_unit,
            print_settings_cmds::set_show_paper_white,
            print_settings_cmds::set_color_handling,
            print_settings_cmds::set_rendering_intent,
            print_settings_cmds::set_black_point_compensation,
            print_settings_cmds::set_printer,
            print_settings_cmds::open_printer_properties_and_apply,
            print_settings_cmds::convert_mm_to_current_unit,
            print_settings_cmds::convert_current_unit_to_mm,
            cursor::set_native_cursor,
            file_io::delete_file,
            file_io::delete_autosave_file,
            file_io::close_app,
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
