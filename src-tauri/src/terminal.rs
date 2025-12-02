use std::process::Command;

pub fn focus_terminal_for_pid(pid: u32) -> Result<(), String> {
    // First, get the TTY for this process
    let tty = get_tty_for_pid(pid)?;

    // Try iTerm2 first, then Terminal.app
    if focus_iterm_by_tty(&tty).is_ok() {
        return Ok(());
    }

    focus_terminal_app_by_tty(&tty)
}

/// Get the TTY device for a given PID using ps command
fn get_tty_for_pid(pid: u32) -> Result<String, String> {
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

fn focus_iterm_by_tty(tty: &str) -> Result<(), String> {
    let script = format!(r#"
        tell application "System Events"
            if not (exists process "iTerm2") then
                error "iTerm2 not running"
            end if
        end tell

        tell application "iTerm2"
            activate
            repeat with w in windows
                repeat with t in tabs of w
                    repeat with s in sessions of t
                        if tty of s contains "{}" then
                            select s
                            select t
                            set index of w to 1
                            return "found"
                        end if
                    end repeat
                end repeat
            end repeat
        end tell
        return "not found"
    "#, tty);

    execute_applescript(&script)
}

fn focus_terminal_app_by_tty(tty: &str) -> Result<(), String> {
    // Check if Terminal is running first
    let check_script = r#"
        tell application "System Events"
            return exists process "Terminal"
        end tell
    "#;

    let check_output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(check_script)
        .output()
        .map_err(|e| format!("Failed to check Terminal: {}", e))?;

    let is_running = String::from_utf8_lossy(&check_output.stdout).trim() == "true";
    if !is_running {
        return Err("Terminal is not running".to_string());
    }

    let script = format!(r#"
        tell application "Terminal"
            activate
            repeat with w in windows
                repeat with t in tabs of w
                    try
                        if tty of t contains "{}" then
                            set selected of t to true
                            set index of w to 1
                            return "found"
                        end if
                    end try
                end repeat
            end repeat
        end tell
        return "not found"
    "#, tty);

    execute_applescript(&script)
}

fn execute_applescript(script: &str) -> Result<(), String> {
    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| format!("Failed to execute AppleScript: {}", e))?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // Check if script returned "found" - otherwise consider it a failure
        if stdout == "not found" {
            Err("Tab not found".to_string())
        } else {
            Ok(())
        }
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("AppleScript error: {}", stderr))
    }
}

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
