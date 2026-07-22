// SPDX-License-Identifier: AGPL-3.0-or-later
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod cursor;
mod menu;
mod response;
mod window_state;

use std::sync::Mutex;
use tauri::{Emitter, Manager};

struct CliState(Mutex<Option<String>>);

fn main() {
    // Accept file path as first CLI argument
    let cli_path: Option<String> = std::env::args().nth(1)
        .filter(|p| !p.starts_with("--"));   // skip tauri dev flags

    tauri::Builder::default()
        .manage(CliState(Mutex::new(cli_path)))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            app.set_menu(menu::build_native_menu(app)?)?;
            app.on_menu_event(|app_handle, event| {
                let id = event.id().0.as_str();
                if menu::is_editor_menu_id(id) {
                    let _ = app_handle.emit(menu::NATIVE_MENU_EVENT, id);
                }
            });

            if let Some(window) = app.get_webview_window("main") {
                let mut saved = window_state::load_window_state(&app.handle());
                // Skip restore on first launch (saved state matches
                // `tauri.conf.json` defaults). Calling set_size/set_position
                // here races the webview's first paint and can cause a brief
                // layout flash on cold start; doing nothing is a true no-op.
                let is_first_launch = saved.x.is_none() && saved.y.is_none() && !saved.maximized;
                if is_first_launch {
                    return Ok(());
                }
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
            commands::read_file_bytes,
            commands::write_file_bytes,
            commands::save_project,
            commands::save_project_binary,
            commands::load_project,
            commands::print_image,
            commands::get_system_printers,
            commands::open_printer_properties,
            commands::delete_file,
            commands::close_app,
        ])
        .run(tauri::generate_context!())
        .expect("Error while running Photrez");
}
