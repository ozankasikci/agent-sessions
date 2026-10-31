mod applescript;
mod iterm;
mod terminal_app;
mod tmux;

use applescript::execute_applescript;

/// Focus the terminal containing the Claude process with the given PID
pub fn focus_terminal_for_pid(pid: u32) -> Result<(), String> {
    // First, get the TTY for this process
    let tty = get_tty_for_pid(pid)?;

    // Try tmux first (if the process is running inside tmux)
    if tmux::focus_tmux_pane_by_tty(&tty).is_ok() {
        return Ok(());
    }

    // Try iTerm2 next
    if iterm::focus_iterm_by_tty(&tty).is_ok() {
        return Ok(());
    }

    // Fall back to Terminal.app
    terminal_app::focus_terminal_app_by_tty(&tty)
}

/// Bring a macOS app (e.g. "Claude", "Codex", "Antigravity") to the front
pub fn activate_app(app_name: &str) -> Result<(), String> {
    // Escape for an AppleScript string literal
    let escaped = app_name.replace('\\', "\\\\").replace('"', "\\\"");
    execute_applescript(&format!("tell application \"{}\" to activate", escaped))
}

/// Fallback: focus terminal by matching path in session name
pub fn focus_terminal_by_path(path: &str) -> Result<(), String> {
    // Fallback: focus by matching session name (which often contains the path) in iTerm2
    let script = format!(r#"
        tell application "System Events"
            if exists process "iTerm2" then
                tell application "iTerm2"
                    activate
                    repeat with w in windows
                        repeat with t in tabs of w
                            repeat with s in sessions of t
                                if name of s contains "{}" then
                                    select s
                                    select t
                                    set index of w to 1
                                    return "found"
                                end if
                            end repeat
                        end repeat
                    end repeat
                end tell
            end if
        end tell
        return "not found"
    "#, path.split('/').last().unwrap_or(path));

    execute_applescript(&script)
}

/// Get the TTY device for a given PID using ps command
fn get_tty_for_pid(pid: u32) -> Result<String, String> {
    use std::process::Command;

    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "tty="])
        .output()
        .map_err(|e| format!("Failed to get TTY: {}", e))?;

    if output.status.success() {
        let tty = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if tty.is_empty() || tty == "??" {
            Err("Process has no TTY".to_string())
        } else {
            Ok(tty)
        }
    } else {
        Err("Failed to get TTY for process".to_string())
    }
}
