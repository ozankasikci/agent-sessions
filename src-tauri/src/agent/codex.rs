use super::{AgentDetector, AgentProcess};
use crate::process::find_codex_processes;
use crate::session::{get_github_url, AgentType, Session, SessionStatus};
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;
use std::process::Command;

pub struct CodexDetector;

impl AgentDetector for CodexDetector {
    fn name(&self) -> &'static str {
        "Codex CLI"
    }

    fn agent_type(&self) -> AgentType {
        AgentType::Codex
    }

    fn find_processes(&self) -> Vec<AgentProcess> {
        find_codex_processes()
            .into_iter()
            .map(|p| AgentProcess {
                pid: p.pid,
                cpu_usage: p.cpu_usage,
                cwd: p.cwd,
            })
            .collect()
    }

    fn find_sessions(&self, processes: &[AgentProcess]) -> Vec<Session> {
        if processes.is_empty() {
            return Vec::new();
        }
        get_codex_sessions(processes)
    }
}

/// Codex JSONL event structure
#[derive(Deserialize)]
struct CodexEvent {
    timestamp: Option<String>,
    #[serde(rename = "type")]
    event_type: Option<String>,
    payload: Option<serde_json::Value>,
}

/// Metadata extracted from session_meta event
struct SessionMeta {
    id: String,
    cwd: Option<String>,
}

/// Get Codex sessions from JSONL files
fn get_codex_sessions(processes: &[AgentProcess]) -> Vec<Session> {
    let mut sessions = Vec::new();

    // Codex sessions root: ~/.codex/sessions/ or $CODEX_HOME/sessions/
    let sessions_root = match get_codex_sessions_root() {
        Some(root) => root,
        None => return sessions,
    };

    if !sessions_root.exists() {
        log::debug!(
            "Codex sessions directory does not exist: {:?}",
            sessions_root
        );
        return sessions;
    }

    // Track which session files have been matched to avoid duplicates
    let mut matched_files: HashSet<PathBuf> = HashSet::new();
    // Track which processes have been matched
    let mut matched_processes: HashSet<u32> = HashSet::new();

    // Prepare canonicalized cwds for each process
    let process_cwds: Vec<(u32, String)> = processes
        .iter()
        .filter_map(|p| {
            let cwd = p.cwd.as_ref()?;
            let canonical = fs::canonicalize(cwd).unwrap_or_else(|_| cwd.clone());
            Some((p.pid, canonical.to_string_lossy().to_string()))
        })
        .collect();

    // First pass: scan recent files (last 30 days)
    let recent_files = find_all_session_files(&sessions_root);
    log::debug!("Found {} recent Codex session files", recent_files.len());

    let eligible_count = process_cwds.len();
    for (file_path, _mtime) in &recent_files {
        if matched_processes.len() == eligible_count {
            break; // All eligible processes matched, short-circuit
        }
        if matched_files.contains(file_path) {
            continue;
        }

        // Try to match this file to any unmatched process
        for process in processes {
            if matched_processes.contains(&process.pid) {
                continue;
            }

            let cwd_str = match process_cwds.iter().find(|(pid, _)| *pid == process.pid) {
                Some((_, cwd)) => cwd,
                None => continue,
            };

            if let Some(session) = parse_session_file(file_path, cwd_str, process) {
                matched_files.insert(file_path.clone());
                matched_processes.insert(process.pid);
                sessions.push(session);
                break;
            }
        }
    }

    // Second pass: if any eligible processes unmatched, scan older files
    if matched_processes.len() < eligible_count {
        let older_files = find_older_session_files(&sessions_root);
        log::debug!(
            "Scanning {} older session files for {} unmatched processes",
            older_files.len(),
            eligible_count - matched_processes.len()
        );

        for (file_path, _mtime) in &older_files {
            if matched_processes.len() == eligible_count {
                break;
            }
            if matched_files.contains(file_path) {
                continue;
            }

            for process in processes {
                if matched_processes.contains(&process.pid) {
                    continue;
                }

                let cwd_str = match process_cwds.iter().find(|(pid, _)| *pid == process.pid) {
                    Some((_, cwd)) => cwd,
                    None => continue,
                };

                if let Some(session) = parse_session_file(file_path, cwd_str, process) {
                    matched_files.insert(file_path.clone());
                    matched_processes.insert(process.pid);
                    sessions.push(session);
                    break;
                }
            }
        }
    }

    sessions
}

/// Get the Codex sessions root directory
fn get_codex_sessions_root() -> Option<PathBuf> {
    // Check CODEX_HOME environment variable first
    if let Ok(codex_home) = std::env::var("CODEX_HOME") {
        if !codex_home.is_empty() {
            return Some(PathBuf::from(codex_home).join("sessions"));
        }
    }

    // Default: ~/.codex/sessions/
    dirs::home_dir().map(|home| home.join(".codex").join("sessions"))
}

/// Collect session files from date-sharded directories
/// Returns files sorted by modification time (newest first)
/// Scans recent days first (last 30 days), then older directories only if needed
fn find_all_session_files(sessions_root: &PathBuf) -> Vec<(PathBuf, std::time::SystemTime)> {
    let mut files = Vec::new();

    // First pass: scan last 30 days (covers most active sessions)
    let now = chrono::Local::now();
    for day_offset in 0..30 {
        let date = now - chrono::Duration::days(day_offset as i64);
        let day_dir = sessions_root
            .join(date.format("%Y").to_string())
            .join(date.format("%m").to_string())
            .join(date.format("%d").to_string());

        collect_session_files_from_dir(&day_dir, &mut files);
    }

    // Sort by modification time descending (newest first)
    files.sort_by(|a, b| b.1.cmp(&a.1));
    files
}

/// Collect session files from older directories (called only if needed)
fn find_older_session_files(sessions_root: &PathBuf) -> Vec<(PathBuf, std::time::SystemTime)> {
    let mut files = Vec::new();
    let now = chrono::Local::now();

    // Build set of recent day paths to skip
    let mut recent_days: HashSet<PathBuf> = HashSet::new();
    for day_offset in 0..30 {
        let date = now - chrono::Duration::days(day_offset as i64);
        let day_dir = sessions_root
            .join(date.format("%Y").to_string())
            .join(date.format("%m").to_string())
            .join(date.format("%d").to_string());
        recent_days.insert(day_dir);
    }

    // Scan all year/month/day directories, skipping recent ones
    if let Ok(year_dirs) = fs::read_dir(sessions_root) {
        for year_entry in year_dirs.flatten() {
            let year_path = year_entry.path();
            if !year_path.is_dir() {
                continue;
            }

            if let Ok(month_dirs) = fs::read_dir(&year_path) {
                for month_entry in month_dirs.flatten() {
                    let month_path = month_entry.path();
                    if !month_path.is_dir() {
                        continue;
                    }

                    if let Ok(day_dirs) = fs::read_dir(&month_path) {
                        for day_entry in day_dirs.flatten() {
                            let day_path = day_entry.path();
                            if !day_path.is_dir() || recent_days.contains(&day_path) {
                                continue;
                            }

                            collect_session_files_from_dir(&day_path, &mut files);
                        }
                    }
                }
            }
        }
    }

    // Sort by modification time descending
    files.sort_by(|a, b| b.1.cmp(&a.1));
    files
}

/// Helper to collect rollout-*.jsonl files from a day directory
fn collect_session_files_from_dir(
    day_dir: &PathBuf,
    files: &mut Vec<(PathBuf, std::time::SystemTime)>,
) {
    if let Ok(entries) = fs::read_dir(day_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("rollout-") && name.ends_with(".jsonl") {
                    let mtime = entry
                        .metadata()
                        .and_then(|m| m.modified())
                        .unwrap_or(std::time::UNIX_EPOCH);
                    files.push((path, mtime));
                }
            }
        }
    }
}

/// Parse a session file and return a Session if the session's cwd matches expected_cwd
fn parse_session_file(
    file_path: &PathBuf,
    expected_cwd: &str,
    process: &AgentProcess,
) -> Option<Session> {
    let file = fs::File::open(file_path).ok()?;
    let metadata = file.metadata().ok()?;
    let file_size = metadata.len();
    let modified_time = metadata.modified().ok()?;

    let mut reader = BufReader::new(file);

    // Parse first lines to extract session_meta (increased from 10 to 50 for files with preamble)
    let session_meta = extract_session_meta(&mut reader)?;

    // Only return if session's cwd matches the expected cwd (process's working directory)
    let session_cwd = session_meta.cwd.as_ref()?;

    // Canonicalize session cwd for comparison (resolves symlinks)
    let canonical_session_cwd = fs::canonicalize(session_cwd)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| session_cwd.clone());

    if canonical_session_cwd != expected_cwd {
        return None;
    }

    // Check if file was recently modified (for status determination)
    let file_recently_modified = modified_time
        .elapsed()
        .map(|d| d.as_secs() < 3)
        .unwrap_or(false);

    // Parse last few lines to get last message info and status signals
    let message_info = extract_last_message(file_path, file_size);

    // Determine status using Claude-compatible logic
    let status = determine_status(
        process.cpu_usage,
        message_info.last_role.as_deref(),
        message_info.has_tool_use,
        message_info.has_tool_result,
        message_info.is_local_command,
        message_info.is_interrupted,
        file_recently_modified,
    );

    let last_role = message_info.last_role;
    let last_message = message_info.last_text;
    let last_timestamp = message_info.last_timestamp;

    // Extract project name from cwd
    let project_name = session_cwd
        .split('/')
        .filter(|s| !s.is_empty())
        .last()
        .unwrap_or("Unknown")
        .to_string();

    // Get git branch from the project directory
    let git_branch = get_git_branch(session_cwd);

    // Get GitHub URL from git remote
    let github_url = get_github_url(session_cwd);

    // Convert modified time to ISO string
    let last_activity_at = chrono::DateTime::<chrono::Utc>::from(modified_time)
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    // Use timestamp from last message if available, otherwise file mtime
    let activity_time = last_timestamp.unwrap_or(last_activity_at);

    log::info!(
        "Codex session: id={}, project={}, status={:?}, last_role={:?}, cpu={:.1}%",
        session_meta.id,
        project_name,
        status,
        last_role,
        process.cpu_usage
    );

    Some(Session {
        id: session_meta.id,
        agent_type: AgentType::Codex,
        project_name,
        project_path: session_cwd.clone(),
        git_branch,
        github_url,
        status,
        last_message,
        last_message_role: last_role,
        last_activity_at: activity_time,
        pid: process.pid,
        cpu_usage: process.cpu_usage,
        active_subagent_count: 0,
    })
}

/// Get git branch from a project directory
fn get_git_branch(project_path: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(project_path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if branch.is_empty() || branch == "HEAD" {
        None
    } else {
        Some(branch)
    }
}

/// Extract session metadata from the first lines
fn extract_session_meta<R: BufRead>(reader: &mut R) -> Option<SessionMeta> {
    let mut lines_read = 0;
    let max_lines = 50; // Increased from 10 to handle files with preamble events

    while lines_read < max_lines {
        let mut line = String::new();
        if reader.read_line(&mut line).ok()? == 0 {
            break;
        }
        lines_read += 1;

        if let Ok(event) = serde_json::from_str::<CodexEvent>(&line) {
            if event.event_type.as_deref() == Some("session_meta") {
                if let Some(payload) = event.payload {
                    let id = payload
                        .get("id")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())?;

                    let cwd = payload
                        .get("cwd")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    return Some(SessionMeta { id, cwd });
                }
            }
        }
    }

    None
}

/// Information extracted from the last message for status determination
struct MessageInfo {
    last_role: Option<String>,
    last_text: Option<String>,
    last_timestamp: Option<String>,
    has_tool_use: bool,
    has_tool_result: bool,
    is_local_command: bool,
    is_interrupted: bool,
}

impl Default for MessageInfo {
    fn default() -> Self {
        Self {
            last_role: None,
            last_text: None,
            last_timestamp: None,
            has_tool_use: false,
            has_tool_result: false,
            is_local_command: false,
            is_interrupted: false,
        }
    }
}

/// Extract the last message info from the end of the file
fn extract_last_message(file_path: &PathBuf, file_size: u64) -> MessageInfo {
    let mut info = MessageInfo::default();

    // Read last ~64KB of file to find recent messages
    let tail_size: u64 = 65536;
    let start_pos = if file_size > tail_size {
        file_size - tail_size
    } else {
        0
    };

    let file = match fs::File::open(file_path) {
        Ok(f) => f,
        Err(_) => return info,
    };

    let mut reader = BufReader::new(file);
    if reader.seek(SeekFrom::Start(start_pos)).is_err() {
        return info;
    }

    // If we seeked into middle of file, skip partial first line
    if start_pos > 0 {
        let mut partial = String::new();
        let _ = reader.read_line(&mut partial);
    }

    for line in reader.lines().flatten() {
        if let Ok(event) = serde_json::from_str::<CodexEvent>(&line) {
            // Check event type for tool-related events
            if let Some(event_type) = &event.event_type {
                let et = event_type.as_str();
                // Codex emits function_call events for tool use
                if et == "function_call" || et == "tool_call" || et == "tool_use" {
                    info.last_role = Some("assistant".to_string());
                    info.last_timestamp = event.timestamp.clone();
                    info.has_tool_use = true;
                    info.has_tool_result = false;
                    info.is_local_command = false;
                    info.is_interrupted = false;
                    // Don't continue - still process payload if present
                }
                // Tool results come back as function_result or tool_result
                // These are effectively "user" messages (tool output going to model)
                if et == "function_result" || et == "tool_result" {
                    info.last_role = Some("user".to_string());
                    info.last_timestamp = event.timestamp.clone();
                    info.has_tool_result = true;
                    info.has_tool_use = false;
                    info.is_local_command = false;
                    info.is_interrupted = false;
                    // Don't continue - still process payload if present
                }
            }

            // Look for events with message content
            if let Some(payload) = &event.payload {
                // Check for role in payload
                if let Some(role) = payload.get("role").and_then(|v| v.as_str()) {
                    if role == "user" || role == "assistant" {
                        info.last_role = Some(role.to_string());
                        info.last_timestamp = event.timestamp.clone();

                        // Reset all flags when we see a new message with role
                        if role == "assistant" {
                            info.has_tool_use = check_for_tool_use(payload);
                            info.has_tool_result = false;
                            info.is_local_command = false;
                            info.is_interrupted = false;
                        } else {
                            // user role - reset flags first, then check content
                            info.has_tool_result = check_for_tool_result(payload);
                            info.has_tool_use = false;
                            info.is_local_command = false;
                            info.is_interrupted = false;
                        }

                        // Extract text content
                        let text = extract_message_text(payload);
                        if let Some(t) = &text {
                            info.last_text = Some(truncate_message(t, 200));

                            // Check for interruption and local commands in user messages
                            if role == "user" {
                                info.is_interrupted = t.contains("[Request interrupted")
                                    || t.contains("interrupted by user");
                                info.is_local_command = is_local_slash_command(t);
                            }
                        }
                    }
                }
            }
        }
    }

    info
}

/// Check if payload contains tool_use blocks (assistant message)
fn check_for_tool_use(payload: &serde_json::Value) -> bool {
    // Check content array for tool_use type
    if let Some(content) = payload.get("content").and_then(|c| c.as_array()) {
        return content.iter().any(|item| {
            item.get("type")
                .and_then(|t| t.as_str())
                .map(|t| t == "tool_use" || t == "function_call")
                .unwrap_or(false)
        });
    }
    // Check for function_call field
    payload.get("function_call").is_some() || payload.get("tool_calls").is_some()
}

/// Check if payload contains tool_result blocks (user message with tool output)
fn check_for_tool_result(payload: &serde_json::Value) -> bool {
    // Check content array for tool_result type
    if let Some(content) = payload.get("content").and_then(|c| c.as_array()) {
        return content.iter().any(|item| {
            item.get("type")
                .and_then(|t| t.as_str())
                .map(|t| t == "tool_result" || t == "function_result")
                .unwrap_or(false)
        });
    }
    false
}

/// Check if text is a local slash command that doesn't trigger model response
/// Based on Codex CLI source: codex-rs/tui/src/chatwidget.rs dispatch_command
fn is_local_slash_command(text: &str) -> bool {
    let trimmed = text.trim();
    // Local commands that don't send to the AI model
    let local_commands = [
        "/feedback",
        "/new",
        "/resume",
        "/fork",
        "/rename",
        "/model",
        "/personality",
        "/collab",
        "/agent",
        "/approvals",
        "/permissions",
        "/setup-elevated-sandbox",
        "/experimental",
        "/quit",
        "/exit",
        "/logout",
        "/diff",
        "/mention",
        "/skills",
        "/status",
        "/debug-config",
        "/statusline",
        "/ps",
        "/mcp",
        "/apps",
        "/rollout",
        "/test-approval",
    ];
    local_commands
        .iter()
        .any(|cmd| trimmed == *cmd || trimmed.starts_with(&format!("{} ", cmd)))
}

/// Extract displayable text from a Codex message payload
fn extract_message_text(payload: &serde_json::Value) -> Option<String> {
    // Try content field directly as string
    if let Some(content) = payload.get("content").and_then(|v| v.as_str()) {
        if !content.is_empty() {
            return Some(content.to_string());
        }
    }

    // Try content as array of parts
    if let Some(content_arr) = payload.get("content").and_then(|v| v.as_array()) {
        let mut pieces: Vec<String> = Vec::new();
        for item in content_arr {
            if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
                pieces.push(text.to_string());
            }
        }
        if !pieces.is_empty() {
            return Some(pieces.join(""));
        }
    }

    // Try text field
    if let Some(text) = payload.get("text").and_then(|v| v.as_str()) {
        if !text.is_empty() {
            return Some(text.to_string());
        }
    }

    None
}

/// Truncate message text safely (respects UTF-8 char boundaries)
fn truncate_message(text: &str, max_chars: usize) -> String {
    if text.chars().count() > max_chars {
        format!("{}...", text.chars().take(max_chars - 3).collect::<String>())
    } else {
        text.to_string()
    }
}

/// Determine session status based on message analysis and file activity
/// Matches Claude's status determination logic for UI consistency
///
/// Status determination logic:
/// - If file is being actively modified (within last 3s) -> active state (Thinking or Processing)
/// - If last message is user with tool_result -> Processing (tool just ran, model processing result)
/// - If last message is from assistant with tool_use AND file recently modified -> Processing
/// - If last message is from assistant with tool_use AND file stale -> Waiting (stuck/needs attention)
/// - If last message is from assistant with only text -> Waiting (model finished, waiting for user)
/// - If last message is from user -> Thinking (model is generating a response)
/// - If last message is a local slash command (/clear, /help, etc.) -> Waiting (these don't trigger model)
/// - If last message indicates interrupted request -> Waiting (user pressed Escape)
fn determine_status(
    cpu_usage: f32,
    last_role: Option<&str>,
    has_tool_use: bool,
    has_tool_result: bool,
    is_local_command: bool,
    is_interrupted: bool,
    file_recently_modified: bool,
) -> SessionStatus {
    let base_status = match last_role {
        Some("assistant") => {
            if has_tool_use {
                if file_recently_modified {
                    // Tool is actively executing (file still being updated)
                    SessionStatus::Processing
                } else {
                    // Tool_use sent but no activity - session is stuck/waiting
                    SessionStatus::Waiting
                }
            } else if file_recently_modified {
                // Assistant sent text but file was just modified - might still be streaming
                SessionStatus::Processing
            } else {
                // Assistant sent a text response and file hasn't been modified recently
                SessionStatus::Waiting
            }
        }
        Some("user") => {
            if is_local_command || is_interrupted {
                // Local slash commands don't trigger the model
                // Interrupted requests also mean session is waiting
                SessionStatus::Waiting
            } else if has_tool_result {
                // User message contains tool_result - tool execution complete
                if file_recently_modified {
                    // Model is actively processing the result
                    SessionStatus::Thinking
                } else {
                    // Tool result was sent but no activity since - session is stuck
                    SessionStatus::Waiting
                }
            } else if file_recently_modified {
                // Regular user input, model is actively thinking/generating
                SessionStatus::Thinking
            } else {
                // User input but no activity - model might be stuck or waiting
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
    };

    // CPU only upgrades Waiting -> Processing (matches Claude behavior)
    if cpu_usage > 5.0 && base_status == SessionStatus::Waiting {
        return SessionStatus::Processing;
    }

    base_status
}
