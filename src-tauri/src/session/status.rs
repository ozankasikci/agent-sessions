use super::model::SessionStatus;

/// Check if message content contains a tool_use block
pub fn has_tool_use(content: &serde_json::Value) -> bool {
    if let serde_json::Value::Array(arr) = content {
        arr.iter().any(|item| {
            item.get("type")
                .and_then(|t| t.as_str())
                .map(|t| t == "tool_use")
                .unwrap_or(false)
        })
    } else {
        false
    }
}

/// Check if message content contains a tool_result block
pub fn has_tool_result(content: &serde_json::Value) -> bool {
    if let serde_json::Value::Array(arr) = content {
        arr.iter().any(|item| {
            item.get("type")
                .and_then(|t| t.as_str())
                .map(|t| t == "tool_result")
                .unwrap_or(false)
        })
    } else {
        false
    }
}

/// Extract text content from a message content value
fn extract_text_content(content: &serde_json::Value) -> &str {
    match content {
        serde_json::Value::String(s) => s.as_str(),
        serde_json::Value::Array(arr) => {
            // Find first text block
            arr.iter().find_map(|v| {
                v.get("text").and_then(|t| t.as_str())
            }).unwrap_or("")
        }
        _ => "",
    }
}

/// Check if message content indicates an interrupted request (user pressed Escape)
pub fn is_interrupted_request(content: &serde_json::Value) -> bool {
    let text = extract_text_content(content);
    text.contains("[Request interrupted by user]")
}

/// Check if message content is a local slash command that doesn't trigger Claude response
/// These commands are handled locally by Claude Code and don't require thinking
pub fn is_local_slash_command(content: &serde_json::Value) -> bool {
    let text = extract_text_content(content);
    let trimmed = text.trim();

    // Local commands that don't trigger Claude to think
    // These are handled by the CLI itself
    let local_commands = [
        "/clear",
        "/compact",
        "/help",
        "/config",
        "/cost",
        "/doctor",
        "/init",
        "/login",
        "/logout",
        "/memory",
        "/model",
        "/permissions",
        "/pr-comments",
        "/review",
        "/status",
        "/terminal-setup",
        "/vim",
    ];

    local_commands.iter().any(|cmd| {
        trimmed == *cmd || trimmed.starts_with(&format!("{} ", cmd))
    })
}

/// Returns sort priority for status (lower = higher priority in list)
/// Active sessions (thinking/processing) appear first, then waiting, then idle
pub fn status_sort_priority(status: &SessionStatus) -> u8 {
    match status {
        SessionStatus::Thinking => 0,   // Active - Claude is working - show first
        SessionStatus::Processing => 0, // Active - tool is running - show first
        SessionStatus::Waiting => 1,    // Needs attention - show second
        SessionStatus::Idle => 2,       // Inactive - show last
    }
}

/// Determine session status based on the last message in the conversation
///
/// Status determination logic:
/// - If file is being actively modified (within last 3s) -> active state (Thinking or Processing)
/// - If last message is user with tool_result -> Processing (tool just ran, Claude processing result)
/// - If last message is from assistant with tool_use AND file recently modified -> Processing
/// - If last message is from assistant with tool_use AND file stale -> Waiting (stuck/needs attention)
/// - If last message is from assistant with only text -> Waiting (Claude finished, waiting for user)
/// - If last message is from user -> Thinking (Claude is generating a response)
/// - If last message is a local slash command (/clear, /help, etc.) -> Waiting (these don't trigger Claude)
/// - If last message indicates interrupted request -> Waiting (user pressed Escape)
pub fn determine_status(
    last_msg_type: Option<&str>,
    has_tool_use: bool,
    has_tool_result: bool,
    is_local_command: bool,
    is_interrupted: bool,
    file_recently_modified: bool,
) -> SessionStatus {
    match last_msg_type {
        Some("assistant") => {
            if has_tool_use {
                if file_recently_modified {
                    // Tool is actively executing (file still being updated)
                    SessionStatus::Processing
                } else {
                    // Tool_use sent but no activity - session is stuck/waiting
                    // This happens when tool execution hangs or user hasn't responded
                    SessionStatus::Waiting
                }
            } else if file_recently_modified {
                // Assistant sent text but file was just modified - Claude might still be
                // streaming or about to send another message. Treat as active.
                SessionStatus::Processing
            } else {
                // Assistant sent a text response and file hasn't been modified recently
                // Claude is done and waiting for user input
                SessionStatus::Waiting
            }
        }
        Some("user") => {
            if is_local_command || is_interrupted {
                // Local slash commands like /clear, /help, /compact don't trigger Claude
                // Interrupted requests (user pressed Escape) also mean session is waiting
                SessionStatus::Waiting
            } else if has_tool_result {
                // User message contains tool_result - tool execution complete
                if file_recently_modified {
                    // Claude is actively processing the result
                    SessionStatus::Thinking
                } else {
                    // Tool result was sent but no activity since - session is stuck
                    SessionStatus::Waiting
                }
            } else if file_recently_modified {
                // Regular user input, Claude is actively thinking/generating
                SessionStatus::Thinking
            } else {
                // User input but no activity - Claude might be stuck or waiting
                SessionStatus::Waiting
            }
        }
        _ => {
            // No recognized message type - check if file is active
            if file_recently_modified {
                SessionStatus::Thinking
            } else {
                SessionStatus::Idle
            }
        }
    }
}
