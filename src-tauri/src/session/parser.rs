use log::{debug, info, trace, warn};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::Command;

use crate::process::ClaudeProcess;
use super::model::{Session, SessionStatus, SessionsResponse, JsonlMessage};
use super::status::{determine_status, has_tool_use, has_tool_result, is_local_slash_command, is_interrupted_request, status_sort_priority};

/// Get GitHub URL from a project's git remote origin
fn get_github_url(project_path: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(project_path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let remote_url = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Convert SSH format to HTTPS
    // git@github.com:user/repo.git -> https://github.com/user/repo
    if remote_url.starts_with("git@github.com:") {
        let path = remote_url
            .strip_prefix("git@github.com:")?
            .strip_suffix(".git")
            .unwrap_or(&remote_url[15..]);
        return Some(format!("https://github.com/{}", path));
    }

    // Already HTTPS format
    // https://github.com/user/repo.git -> https://github.com/user/repo
    if remote_url.starts_with("https://github.com/") {
        let url = remote_url
            .strip_suffix(".git")
            .unwrap_or(&remote_url);
        return Some(url.to_string());
    }

    None
}

/// Convert a directory name like "-Users-ozan-Projects-ai-image-dashboard" back to a path
/// The challenge is that both path separators AND project names can contain dashes
/// We handle this by recognizing that the path structure is predictable:
/// /Users/<username>/Projects/<project-name> or /Users/<username>/.../<project-name>
///
/// Special case: Double dashes (--) indicate a hidden folder (starting with .)
/// followed by subfolders separated by single dashes
/// e.g., "ai-image-dashboard--rsworktree-analytics" becomes "ai-image-dashboard/.rsworktree/analytics"
pub fn convert_dir_name_to_path(dir_name: &str) -> String {
    // Remove leading dash if present
    let name = dir_name.strip_prefix('-').unwrap_or(dir_name);

    // Split by dash
    let parts: Vec<&str> = name.split('-').collect();

    if parts.is_empty() {
        return String::new();
    }

    // Find "Projects" or "UnityProjects" index - everything after that is the project name
    let projects_idx = parts.iter().position(|&p| p == "Projects" || p == "UnityProjects");

    if let Some(idx) = projects_idx {
        // Path components are before and including "Projects"
        let path_parts = &parts[..=idx];
        // Project name is everything after "Projects"
        let project_parts = &parts[idx + 1..];

        let mut path = String::from("/");
        path.push_str(&path_parts.join("/"));

        if !project_parts.is_empty() {
            path.push('/');
            // Handle the project path with potential hidden folders
            // Double dash (empty string between dashes when split) indicates hidden folder
            // After a hidden folder marker, subsequent parts are subfolders
            let mut in_hidden_folder = false;
            let mut segments: Vec<String> = Vec::new();
            let mut current_segment = String::new();

            for part in project_parts {
                if part.is_empty() {
                    // Empty part means we hit a double dash - start hidden folder
                    if !current_segment.is_empty() {
                        segments.push(current_segment);
                        current_segment = String::new();
                    }
                    in_hidden_folder = true;
                } else if in_hidden_folder {
                    // After double dash, each part is a subfolder
                    // First part after -- gets the dot prefix
                    if current_segment.is_empty() {
                        current_segment = format!(".{}", part);
                    } else {
                        segments.push(current_segment);
                        current_segment = part.to_string();
                    }
                } else {
                    // Normal project name part - join with dashes
                    if current_segment.is_empty() {
                        current_segment = part.to_string();
                    } else {
                        current_segment.push('-');
                        current_segment.push_str(part);
                    }
                }
            }
            if !current_segment.is_empty() {
                segments.push(current_segment);
            }

            path.push_str(&segments.join("/"));
        }

        path
    } else {
        // Fallback: just replace dashes with slashes (old behavior)
        format!("/{}", name.replace('-', "/"))
    }
}

/// Get all active Claude Code sessions
pub fn get_sessions() -> SessionsResponse {
    use crate::process::find_claude_processes;

    info!("=== Getting all sessions ===");

    let claude_processes = find_claude_processes();
    debug!("Found {} Claude processes total", claude_processes.len());

    let mut sessions = Vec::new();

    // Build a map of cwd -> list of processes (multiple sessions can run in same folder)
    let mut cwd_to_processes: HashMap<String, Vec<&ClaudeProcess>> = HashMap::new();
    for process in &claude_processes {
        if let Some(cwd) = &process.cwd {
            let cwd_str = cwd.to_string_lossy().to_string();
            debug!("Mapping process pid={} to cwd={}", process.pid, cwd_str);
            cwd_to_processes.entry(cwd_str).or_default().push(process);
        } else {
            warn!("Process pid={} has no cwd, skipping", process.pid);
        }
    }

    // Scan ~/.claude/projects for session files
    let claude_dir = dirs::home_dir()
        .map(|h| h.join(".claude").join("projects"))
        .unwrap_or_default();

    debug!("Claude projects directory: {:?}", claude_dir);

    if !claude_dir.exists() {
        warn!("Claude projects directory does not exist: {:?}", claude_dir);
        return SessionsResponse {
            sessions: vec![],
            total_count: 0,
            waiting_count: 0,
        };
    }

    // For each project directory
    if let Ok(entries) = fs::read_dir(&claude_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            // Convert directory name back to path
            let dir_name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            let project_path = convert_dir_name_to_path(dir_name);
            trace!("Checking project: {} -> {}", dir_name, project_path);

            // Check if this project has active Claude processes
            let processes = match cwd_to_processes.get(&project_path) {
                Some(p) => {
                    debug!("Project {} has {} active processes", project_path, p.len());
                    p
                },
                None => {
                    trace!("Project {} has no active processes, skipping", project_path);
                    continue;
                }
            };

            // Find all JSONL files that were recently modified (within last 30 seconds)
            // These are likely the active sessions
            let jsonl_files = get_recently_active_jsonl_files(&path, processes.len());
            debug!("Found {} JSONL files for project {}", jsonl_files.len(), project_path);

            // Match processes to JSONL files
            for (index, process) in processes.iter().enumerate() {
                debug!("Matching process pid={} to JSONL file index {}", process.pid, index);
                if let Some(session) = find_session_for_process(&jsonl_files, &path, &project_path, process, index) {
                    info!(
                        "Session created: id={}, project={}, status={:?}, pid={}",
                        session.id, session.project_name, session.status, session.pid
                    );
                    sessions.push(session);
                } else {
                    warn!("Failed to create session for process pid={} in project {}", process.pid, project_path);
                }
            }
        }
    }

    // Sort by status priority first, then by most recent activity within same priority
    // Priority: Waiting (needs attention) > Thinking/Processing (active) > Idle
    // Within same priority, sort by most recent activity
    sessions.sort_by(|a, b| {
        let priority_a = status_sort_priority(&a.status);
        let priority_b = status_sort_priority(&b.status);

        if priority_a != priority_b {
            priority_a.cmp(&priority_b)
        } else {
            b.last_activity_at.cmp(&a.last_activity_at)
        }
    });

    let waiting_count = sessions.iter()
        .filter(|s| matches!(s.status, SessionStatus::Waiting))
        .count();
    let total_count = sessions.len();

    info!(
        "=== Session scan complete: {} total, {} waiting ===",
        total_count, waiting_count
    );

    SessionsResponse {
        sessions,
        total_count,
        waiting_count,
    }
}

/// Check if a JSONL file is a subagent file (named agent-*.jsonl)
fn is_subagent_file(path: &PathBuf) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|name| name.starts_with("agent-") && name.ends_with(".jsonl"))
        .unwrap_or(false)
}

/// Extract sessionId from a subagent JSONL file by reading the first few lines
fn get_subagent_session_id(path: &PathBuf) -> Option<String> {
    let file = File::open(path).ok()?;
    let reader = BufReader::new(file);

    // Check first 5 lines for sessionId
    for line in reader.lines().take(5).flatten() {
        if let Ok(msg) = serde_json::from_str::<JsonlMessage>(&line) {
            if let Some(session_id) = msg.session_id {
                return Some(session_id);
            }
        }
    }
    None
}

/// Count active subagents for a given parent session
fn count_active_subagents(project_dir: &PathBuf, parent_session_id: &str) -> usize {
    use std::time::{Duration, SystemTime};

    let active_threshold = Duration::from_secs(30);
    let now = SystemTime::now();

    let count = fs::read_dir(project_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| is_subagent_file(&e.path()))
        .filter(|e| {
            // Check if file was recently modified
            e.metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|modified| now.duration_since(modified).ok())
                .map(|d| d < active_threshold)
                .unwrap_or(false)
        })
        .filter(|e| {
            // Check if sessionId matches parent
            get_subagent_session_id(&e.path())
                .map(|id| id == parent_session_id)
                .unwrap_or(false)
        })
        .count();

    trace!("Found {} active subagents for session {}", count, parent_session_id);
    count
}

/// Get JSONL files for a project, sorted by modification time (newest first)
/// Excludes subagent files (agent-*.jsonl) as they are counted separately
fn get_recently_active_jsonl_files(project_dir: &PathBuf, _expected_count: usize) -> Vec<PathBuf> {
    let mut jsonl_files: Vec<_> = fs::read_dir(project_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            let path = e.path();
            path.extension()
                .map(|ext| ext == "jsonl")
                .unwrap_or(false)
                && !is_subagent_file(&path)
        })
        .filter_map(|e| {
            let path = e.path();
            let modified = e.metadata().and_then(|m| m.modified()).ok()?;
            Some((path, modified))
        })
        .collect();

    // Sort by modification time (newest first)
    jsonl_files.sort_by(|a, b| b.1.cmp(&a.1));

    jsonl_files
        .into_iter()
        .map(|(path, _)| path)
        .collect()
}

/// Find a session for a specific process from available JSONL files
/// Checks all recent files and uses the most "active" status found
fn find_session_for_process(
    jsonl_files: &[PathBuf],
    project_dir: &PathBuf,
    project_path: &str,
    process: &ClaudeProcess,
    index: usize,
) -> Option<Session> {
    use std::time::{Duration, SystemTime};

    // Get the primary JSONL file at the given index
    let primary_jsonl = jsonl_files.get(index)?;

    // Parse the primary file first
    let mut session = parse_session_file(primary_jsonl, project_path, process)?;

    // Count active subagents for this session
    session.active_subagent_count = count_active_subagents(project_dir, &session.id);

    // Check if any other recent files show more active status
    // This handles subagent scenarios where main session file stops updating
    let now = SystemTime::now();
    let active_threshold = Duration::from_secs(10); // Check files modified in last 10 seconds

    for jsonl_path in jsonl_files {
        if jsonl_path == primary_jsonl {
            continue;
        }

        // Only check recently modified files
        let is_recent = jsonl_path
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .map(|d| d < active_threshold)
            .unwrap_or(false);

        if !is_recent {
            continue;
        }

        // Parse this file and check its status
        if let Some(other_session) = parse_session_file(jsonl_path, project_path, process) {
            // If this file shows a more active status, use it
            let current_priority = status_sort_priority(&session.status);
            let other_priority = status_sort_priority(&other_session.status);

            if other_priority < current_priority {
                debug!(
                    "Found more active status in {:?}: {:?} -> {:?}",
                    jsonl_path, session.status, other_session.status
                );
                session.status = other_session.status;
                // Keep the original session's other fields (id, last_message, etc.)
            }
        }
    }

    // Additional check: if CPU usage is high, the process is likely working
    // Override Waiting status if CPU > 5%
    if matches!(session.status, SessionStatus::Waiting) && process.cpu_usage > 5.0 {
        debug!(
            "Process has high CPU ({:.1}%), overriding Waiting -> Processing",
            process.cpu_usage
        );
        session.status = SessionStatus::Processing;
    }

    Some(session)
}

/// Parse a JSONL session file and create a Session struct
pub fn parse_session_file(jsonl_path: &PathBuf, project_path: &str, process: &ClaudeProcess) -> Option<Session> {
    use std::time::SystemTime;

    debug!("Parsing JSONL file: {:?}", jsonl_path);

    // Check if the file was modified very recently (indicates active processing)
    let file_age_secs = jsonl_path
        .metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .map(|d| d.as_secs_f32());

    let file_recently_modified = file_age_secs.map(|age| age < 3.0).unwrap_or(false);

    debug!(
        "File age: {:.1}s, recently_modified: {}",
        file_age_secs.unwrap_or(-1.0),
        file_recently_modified
    );

    // Parse the JSONL file to get session info
    let file = File::open(jsonl_path).ok()?;
    let reader = BufReader::new(file);

    let mut session_id = None;
    let mut git_branch = None;
    let mut last_timestamp = None;
    let mut last_message = None;
    let mut last_role = None;
    let mut last_msg_type = None;
    let mut last_has_tool_use = false;
    let mut last_has_tool_result = false;
    let mut last_is_local_command = false;
    let mut last_is_interrupted = false;
    let mut found_status_info = false;

    // Read last N lines for efficiency
    let lines: Vec<_> = reader.lines().flatten().collect();
    let recent_lines: Vec<_> = lines.iter().rev().take(100).collect();

    trace!("File has {} total lines, checking last {}", lines.len(), recent_lines.len());

    for line in &recent_lines {
        if let Ok(msg) = serde_json::from_str::<JsonlMessage>(line) {
            if session_id.is_none() {
                session_id = msg.session_id;
            }
            if git_branch.is_none() {
                git_branch = msg.git_branch;
            }
            if last_timestamp.is_none() {
                last_timestamp = msg.timestamp;
            }

            // For status detection, we need to find the most recent message that has CONTENT
            if !found_status_info {
                if let Some(content) = &msg.message {
                    if let Some(c) = &content.content {
                        let has_content = match c {
                            serde_json::Value::String(s) => !s.is_empty(),
                            serde_json::Value::Array(arr) => !arr.is_empty(),
                            _ => false,
                        };

                        if has_content {
                            last_msg_type = msg.msg_type.clone();
                            last_role = content.role.clone();
                            last_has_tool_use = has_tool_use(c);
                            last_has_tool_result = has_tool_result(c);
                            last_is_local_command = is_local_slash_command(c);
                            last_is_interrupted = is_interrupted_request(c);
                            found_status_info = true;

                            debug!(
                                "Found status info: type={:?}, role={:?}, has_tool_use={}, has_tool_result={}, is_local_cmd={}, is_interrupted={}",
                                last_msg_type, last_role, last_has_tool_use, last_has_tool_result, last_is_local_command, last_is_interrupted
                            );
                        }
                    }
                }
            }

            if session_id.is_some() && found_status_info {
                break;
            }
        }
    }

    // Now find the last meaningful text message (keep looking even after finding status)
    for line in &recent_lines {
        if let Ok(msg) = serde_json::from_str::<JsonlMessage>(line) {
            if let Some(content) = &msg.message {
                if let Some(c) = &content.content {
                    let text = match c {
                        serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
                        serde_json::Value::Array(arr) => {
                            arr.iter().find_map(|v| {
                                v.get("text").and_then(|t| t.as_str())
                                    .filter(|s| !s.is_empty())
                                    .map(String::from)
                            })
                        }
                        _ => None,
                    };

                    if text.is_some() {
                        last_message = text;
                        break;
                    }
                }
            }
        }
    }

    let session_id = session_id?;

    // Determine status based on message type, content, and file activity
    let status = determine_status(
        last_msg_type.as_deref(),
        last_has_tool_use,
        last_has_tool_result,
        last_is_local_command,
        last_is_interrupted,
        file_recently_modified,
    );

    debug!(
        "Status determination: type={:?}, tool_use={}, tool_result={}, local_cmd={}, interrupted={}, recent={} -> {:?}",
        last_msg_type, last_has_tool_use, last_has_tool_result, last_is_local_command, last_is_interrupted, file_recently_modified, status
    );

    // Extract project name from path
    let project_name = project_path
        .split('/')
        .filter(|s| !s.is_empty())
        .last()
        .unwrap_or("Unknown")
        .to_string();

    // Truncate message for preview (respecting UTF-8 char boundaries)
    let last_message = last_message.map(|m| {
        if m.chars().count() > 100 {
            format!("{}...", m.chars().take(100).collect::<String>())
        } else {
            m
        }
    });

    // Get GitHub URL from git remote
    let github_url = get_github_url(project_path);

    Some(Session {
        id: session_id,
        project_name,
        project_path: project_path.to_string(),
        git_branch,
        github_url,
        status,
        last_message,
        last_message_role: last_role,
        last_activity_at: last_timestamp.unwrap_or_else(|| "Unknown".to_string()),
        pid: process.pid,
        cpu_usage: process.cpu_usage,
        active_subagent_count: 0, // Set by find_session_for_process
    })
}
