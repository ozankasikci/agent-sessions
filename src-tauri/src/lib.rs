#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod process;
mod session;
mod terminal;

use tauri::{
    Manager,
    tray::TrayIconBuilder,
    menu::{MenuBuilder, MenuItemBuilder},
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
use std::sync::Mutex;

use session::{get_sessions, SessionsResponse};

// Store tray icon ID for updates
static TRAY_ID: Mutex<Option<String>> = Mutex::new(None);
// Store current shortcut for unregistration
static CURRENT_SHORTCUT: Mutex<Option<Shortcut>> = Mutex::new(None);

#[tauri::command]
fn get_all_sessions() -> SessionsResponse {
    get_sessions()
}

#[tauri::command]
fn focus_session(pid: u32, project_path: String) -> Result<(), String> {
    terminal::focus_terminal_for_pid(pid)
        .or_else(|_| terminal::focus_terminal_by_path(&project_path))
}

#[tauri::command]
fn update_tray_title(app: tauri::AppHandle, total: usize, waiting: usize) -> Result<(), String> {
    let title = if waiting > 0 {
        format!("{} ({} waiting)", total, waiting)
    } else if total > 0 {
        format!("{}", total)
    } else {
        String::new()
    };

    if let Some(tray) = app.tray_by_id("main-tray") {
        tray.set_title(Some(&title))
            .map_err(|e| format!("Failed to set tray title: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
fn register_shortcut(app: tauri::AppHandle, shortcut: String) -> Result<(), String> {
    // Unregister any existing shortcut first
    if let Some(old_shortcut) = CURRENT_SHORTCUT.lock().unwrap().take() {
        let _ = app.global_shortcut().unregister(old_shortcut);
    }

    // Parse the shortcut string
    let parsed_shortcut: Shortcut = shortcut.parse()
        .map_err(|e| format!("Invalid shortcut format: {}", e))?;

    // Register the new shortcut - toggle window visibility
    app.global_shortcut()
        .on_shortcut(parsed_shortcut.clone(), move |app, _shortcut, event| {
            // Only handle key press, not release
            if event.state != tauri_plugin_global_shortcut::ShortcutState::Pressed {
                return;
            }

            if let Some(window) = app.get_webview_window("main") {
                let is_visible = window.is_visible().unwrap_or(false);
                let is_focused = window.is_focused().unwrap_or(false);

                // If window is visible AND focused, hide it
                // Otherwise, show and focus it
                if is_visible && is_focused {
                    let _ = window.hide();
                } else {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .map_err(|e| format!("Failed to register shortcut: {}", e))?;

    // Store the shortcut for later unregistration
    *CURRENT_SHORTCUT.lock().unwrap() = Some(parsed_shortcut);

    Ok(())
}

#[tauri::command]
fn unregister_shortcut(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(shortcut) = CURRENT_SHORTCUT.lock().unwrap().take() {
        app.global_shortcut()
            .unregister(shortcut)
            .map_err(|e| format!("Failed to unregister shortcut: {}", e))?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .invoke_handler(tauri::generate_handler![get_all_sessions, focus_session, update_tray_title, register_shortcut, unregister_shortcut])
        .setup(|app| {
            // Create menu for tray
            let show_item = MenuItemBuilder::with_id("show", "Show Window")
                .build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit")
                .build(app)?;

            let menu = MenuBuilder::new(app)
                .item(&show_item)
                .separator()
                .item(&quit_item)
                .build()?;

            // Create tray icon with menu
            let _tray = TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().unwrap().clone())
                .icon_as_template(true)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| {
                    match event.id().as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let tauri::tray::TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, .. } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // Store tray ID
            *TRAY_ID.lock().unwrap() = Some("main-tray".to_string());

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
