use tauri::Manager;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
use std::sync::Mutex;

use crate::session::{get_sessions, SessionsResponse};
use crate::terminal;

// Store current shortcut for unregistration
static CURRENT_SHORTCUT: Mutex<Option<Shortcut>> = Mutex::new(None);

/// Get all active Claude Code sessions
#[tauri::command]
pub fn get_all_sessions() -> SessionsResponse {
    get_sessions()
}

/// Focus the terminal (or app) containing a specific session
///
/// Terminal sessions are focused by TTY. Sessions hosted by a desktop app or IDE
/// (`app_name` set) have no TTY of their own, so the app is brought to the front.
#[tauri::command]
pub fn focus_session(pid: u32, project_path: String, app_name: Option<String>) -> Result<(), String> {
    if let Ok(()) = terminal::focus_terminal_for_pid(pid) {
        return Ok(());
    }
    if let Some(app) = app_name.filter(|a| !a.is_empty()) {
        return terminal::activate_app(&app);
    }
    terminal::focus_terminal_by_path(&project_path)
}

/// Update the tray icon title with session counts
#[tauri::command]
pub fn update_tray_title(app: tauri::AppHandle, total: usize, waiting: usize) -> Result<(), String> {
    let title = if waiting > 0 {
        format!("{} ({} idle)", total, waiting)
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

/// Register a global keyboard shortcut to toggle the window
#[tauri::command]
pub fn register_shortcut(app: tauri::AppHandle, shortcut: String) -> Result<(), String> {
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

/// Unregister the current global keyboard shortcut
#[tauri::command]
pub fn unregister_shortcut(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(shortcut) = CURRENT_SHORTCUT.lock().unwrap().take() {
        app.global_shortcut()
            .unregister(shortcut)
            .map_err(|e| format!("Failed to unregister shortcut: {}", e))?;
    }
    Ok(())
}

/// Kill an agent process by PID
#[tauri::command]
pub fn kill_session(pid: u32) -> Result<(), String> {
    use std::process::Command;

    // Use SIGKILL (-9) to forcefully terminate the process
    // SIGTERM often doesn't work for agent processes with child processes
    let output = Command::new("kill")
        .arg("-9")
        .arg(pid.to_string())
        .output()
        .map_err(|e| format!("Failed to execute kill command: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("Failed to kill process {}: {}", pid, stderr))
    }
}
