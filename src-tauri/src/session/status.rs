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

/// Check if all tool_use blocks in content are user-input tools (e.g., AskUserQuestion).
/// These tools block on user input and should be treated as Waiting, not Processing.
/// Returns false if any tool_use has no name or an unrecognized name.
pub fn is_waiting_for_user_input(content: &serde_json::Value) -> bool {
    let user_input_tools = ["AskUserQuestion"];

    if let serde_json::Value::Array(arr) = content {
        let tool_use_blocks: Vec<_> = arr.iter()
            .filter(|item| {
                item.get("type")
                    .and_then(|t| t.as_str())
                    .map(|t| t == "tool_use")
                    .unwrap_or(false)
            })
            .collect();

        !tool_use_blocks.is_empty() && tool_use_blocks.iter().all(|item| {
            item.get("name")
                .and_then(|n| n.as_str())
                .map(|name| user_input_tools.contains(&name))
                .unwrap_or(false) // unnamed tool_use -> not a user-input tool
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
        SessionStatus::Thinking => 0,    // Active - Claude is working - show first
        SessionStatus::Processing => 0,  // Active - tool is running - show first
        SessionStatus::Compacting => 0,  // Active - compressing context - show first
        SessionStatus::Waiting => 1,     // Needs attention - show second
        SessionStatus::Idle => 2,        // Inactive - show last
    }
}

/// Determine session status based on the last message in the conversation
///
/// Status is determined purely from the message content — no file age or CPU heuristics.
/// The last message reliably indicates what the session is doing:
/// - assistant with tool_use -> Processing (tool is executing, could take minutes)
/// - assistant text-only -> Waiting (Claude finished, waiting for user input)
/// - user message -> Thinking (Claude is generating a response)
/// - user with tool_result -> Thinking (Claude is processing tool output)
/// - local slash command or interrupted -> Waiting (no Claude response expected)
pub fn determine_status(
    last_msg_type: Option<&str>,
    has_tool_use: bool,
    _has_tool_result: bool,
    is_local_command: bool,
    is_interrupted: bool,
    is_user_input_tool: bool,
    file_recently_modified: bool,
) -> SessionStatus {
    match last_msg_type {
        Some("assistant") => {
            if has_tool_use && is_user_input_tool {
                // Tool like AskUserQuestion - waiting for user input
                SessionStatus::Waiting
            } else if has_tool_use {
                // Tool is executing - could take seconds or minutes
                SessionStatus::Processing
            } else if file_recently_modified {
                // Text response but file is still being written to
                // (streaming, compacting, or about to send tool_use)
                SessionStatus::Processing
            } else {
                // Assistant sent a text response and file is quiet - waiting for user
                SessionStatus::Waiting
            }
        }
        Some("user") => {
            if is_local_command || is_interrupted {
                // Local slash commands and interrupted requests don't trigger Claude
                SessionStatus::Waiting
            } else {
                // User sent a message or tool result - Claude is working
                SessionStatus::Thinking
            }
        }
        _ => {
            // Couldn't determine message type (e.g., only progress entries in lookback)
            if file_recently_modified {
                SessionStatus::Processing
            } else {
                SessionStatus::Waiting
            }
        }
    }
}
