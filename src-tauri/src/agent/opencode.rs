use super::{AgentDetector, AgentProcess};
use crate::session::{AgentType, Session, SessionStatus};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

pub struct OpenCodeDetector;

impl AgentDetector for OpenCodeDetector {
    fn name(&self) -> &'static str {
        "OpenCode"
    }

    fn agent_type(&self) -> AgentType {
        AgentType::OpenCode
    }

    fn find_processes(&self) -> Vec<AgentProcess> {
        find_opencode_processes()
    }

    fn find_sessions(&self, processes: &[AgentProcess]) -> Vec<Session> {
        if processes.is_empty() {
            return Vec::new();
        }
        get_opencode_sessions(processes)
    }
}

// JSON structures for OpenCode data files

#[derive(Deserialize)]
struct OpenCodeProject {
    id: String,
    worktree: String,
    #[serde(default)]
    sandboxes: Vec<String>,
    #[serde(default)]
    time: OpenCodeTime,
}

#[derive(Deserialize, Default)]
struct OpenCodeTime {
    #[serde(default)]
    created: u64,
    #[serde(default)]
    updated: u64,
}

#[derive(Deserialize)]
struct OpenCodeSession {
    id: String,
    #[serde(rename = "projectID")]
    project_id: String,
    #[serde(default)]
    directory: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    time: OpenCodeTime,
}

#[derive(Deserialize)]
struct OpenCodeMessage {
    id: String,
    #[serde(rename = "sessionID")]
    session_id: String,
    role: String,
    #[serde(default)]
    time: OpenCodeTime,
}

#[derive(Deserialize)]
struct OpenCodePart {
    #[serde(rename = "type")]
    part_type: String,
    #[serde(default)]
    text: Option<String>,
}

// Reuse System instance to get accurate CPU readings (requires previous measurement)
static OPENCODE_SYSTEM: std::sync::Mutex<Option<sysinfo::System>> = std::sync::Mutex::new(None);

/// Find running opencode processes
fn find_opencode_processes() -> Vec<AgentProcess> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System, UpdateKind};

    let mut system_guard = OPENCODE_SYSTEM.lock().unwrap();

    // Initialize system if not already done
    let system = system_guard.get_or_insert_with(|| {
        log::debug!("Initializing new System instance for OpenCode");
        System::new_with_specifics(
            RefreshKind::new().with_processes(
                ProcessRefreshKind::new()
                    .with_cwd(UpdateKind::Always)
                    .with_cpu()
            )
        )
    });

    // Refresh process list
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        ProcessRefreshKind::new()
            .with_cwd(UpdateKind::Always)
            .with_cpu(),
    );

    let mut processes = Vec::new();

    for (pid, process) in system.processes() {
        let name = process.name().to_string_lossy().to_lowercase();

        if name == "opencode" {
            let cpu = process.cpu_usage();
            let cwd = process.cwd().map(|p| p.to_path_buf());
            log::debug!(
                "OpenCode process: pid={}, cpu={:.1}%, cwd={:?}",
                pid.as_u32(),
                cpu,
                cwd
            );
            processes.push(AgentProcess {
                pid: pid.as_u32(),
                cpu_usage: cpu,
                cwd,
            });
        }
    }

    log::debug!("Found {} opencode processes", processes.len());
    processes
}

/// Get OpenCode sessions from JSON files
fn get_opencode_sessions(processes: &[AgentProcess]) -> Vec<Session> {
    let mut sessions = Vec::new();

    // OpenCode data directory: ~/.local/share/opencode/storage/
    // Note: OpenCode uses XDG convention, not macOS Application Support
    let storage_path = match dirs::home_dir() {
        Some(home) => home.join(".local").join("share").join("opencode").join("storage"),
        None => return sessions,
    };

    if !storage_path.exists() {
        log::debug!("OpenCode storage directory does not exist: {:?}", storage_path);
        return sessions;
    }

    // Build cwd -> process map
    let mut cwd_to_process: HashMap<String, &AgentProcess> = HashMap::new();
    for process in processes {
        if let Some(cwd) = &process.cwd {
            cwd_to_process.insert(cwd.to_string_lossy().to_string(), process);
        }
    }

    // Load all projects
    let projects = load_projects(&storage_path);
    log::debug!("Loaded {} OpenCode projects", projects.len());

    // Track which processes have been matched
    let mut matched_pids: std::collections::HashSet<u32> = std::collections::HashSet::new();

    // Match projects to running processes (non-global projects first)
    for project in &projects {
        if project.id == "global" {
            continue; // Handle global separately
        }

        // Check if any process is running in this project's worktree or sandboxes
        let matching_process = cwd_to_process
            .iter()
            .find(|(cwd, _)| {
                // Check if cwd matches the project worktree
                if cwd.as_str() == project.worktree || cwd.starts_with(&format!("{}/", project.worktree)) {
                    return true;
                }
                // Check if cwd matches any sandbox (worktree/branch)
                for sandbox in &project.sandboxes {
                    if cwd.as_str() == sandbox || cwd.starts_with(&format!("{}/", sandbox)) {
                        return true;
                    }
                }
                false
            })
            .map(|(_, p)| *p);

        if let Some(process) = matching_process {
            log::debug!("Project {} matched to process pid={}", project.worktree, process.pid);
            matched_pids.insert(process.pid);
            if let Some(session) = get_latest_session_for_project(&storage_path, project, process) {
                sessions.push(session);
            }
        }
    }

    // For unmatched processes, check global sessions by directory field
    for process in processes {
        if matched_pids.contains(&process.pid) {
            continue;
        }
        if let Some(cwd) = &process.cwd {
            let cwd_str = cwd.to_string_lossy().to_string();
            if let Some(session) = get_global_session_for_directory(&storage_path, &cwd_str, process) {
                log::debug!("Global session matched for directory {} to process pid={}", cwd_str, process.pid);
                sessions.push(session);
            }
        }
    }

    sessions
}

/// Load all project definitions
fn load_projects(storage_path: &PathBuf) -> Vec<OpenCodeProject> {
    let project_dir = storage_path.join("project");
    let mut projects = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&project_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(project) = serde_json::from_str::<OpenCodeProject>(&content) {
                        projects.push(project);
                    }
                }
            }
        }
    }

    projects
}

/// Get the latest session for a project
fn get_latest_session_for_project(
    storage_path: &PathBuf,
    project: &OpenCodeProject,
    process: &AgentProcess,
) -> Option<Session> {
    let session_dir = storage_path.join("session").join(&project.id);

    if !session_dir.exists() {
        return None;
    }

    // Find the most recently updated session file
    let mut latest_session: Option<(OpenCodeSession, u64)> = None;

    if let Ok(entries) = std::fs::read_dir(&session_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(session) = serde_json::from_str::<OpenCodeSession>(&content) {
                        let updated = session.time.updated;
                        if latest_session.as_ref().map(|(_, t)| updated > *t).unwrap_or(true) {
                            latest_session = Some((session, updated));
                        }
                    }
                }
            }
        }
    }

    let (session, _) = latest_session?;

    // Get the last message for status detection and display
    let (last_role, last_message_text, _last_message_time) = get_last_message(storage_path, &session.id);

    // Determine status
    let status = if process.cpu_usage > 5.0 {
        SessionStatus::Processing
    } else if last_role.as_deref() == Some("assistant") {
        SessionStatus::Waiting
    } else if last_role.as_deref() == Some("user") {
        SessionStatus::Processing
    } else {
        SessionStatus::Idle
    };

    // Convert timestamp to ISO string (OpenCode uses milliseconds)
    let updated_secs = session.time.updated / 1000;
    let last_activity_at = chrono::DateTime::from_timestamp(updated_secs as i64, 0)
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    // Use actual process CWD for display (may be sandbox/worktree path)
    let actual_path = process.cwd
        .as_ref()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| project.worktree.clone());

    // Extract project name from actual path
    let project_name = actual_path
        .split('/')
        .filter(|s| !s.is_empty())
        .last()
        .unwrap_or("Unknown")
        .to_string();

    log::info!(
        "OpenCode session: id={}, project={}, status={:?}, last_role={:?}, cpu={:.1}%",
        session.id, project_name, status, last_role, process.cpu_usage
    );

    // Use message text if available, fall back to session title
    let display_message = last_message_text
        .or_else(|| Some(session.title.clone()).filter(|t| !t.is_empty()));

    Some(Session {
        id: session.id,
        agent_type: AgentType::OpenCode,
        project_name,
        project_path: actual_path,
        git_branch: None,
        github_url: None,
        status,
        last_message: display_message,
        last_message_role: last_role,
        last_activity_at,
        pid: process.pid,
        cpu_usage: process.cpu_usage,
        active_subagent_count: 0,
    })
}

/// Get the last message role, time, and text for a session
fn get_last_message(storage_path: &PathBuf, session_id: &str) -> (Option<String>, Option<String>, u64) {
    let message_dir = storage_path.join("message").join(session_id);

    if !message_dir.exists() {
        log::debug!("Message dir does not exist: {:?}", message_dir);
        return (None, None, 0);
    }

    // Collect all messages sorted by created time (descending)
    let mut messages: Vec<(String, String, u64)> = Vec::new(); // (role, message_id, created)

    if let Ok(entries) = std::fs::read_dir(&message_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(msg) = serde_json::from_str::<OpenCodeMessage>(&content) {
                        messages.push((msg.role, msg.id, msg.time.created));
                    }
                }
            }
        }
    }

    // Sort by created time descending (newest first)
    messages.sort_by(|a, b| b.2.cmp(&a.2));

    let message_count = messages.len();

    // Find the first message with displayable text (skip system prompts)
    for (role, message_id, time) in messages {
        if let Some(text) = get_message_text(storage_path, &message_id) {
            log::debug!(
                "Session {} has {} messages, showing: id={}, role={}, created={}, text={:?}",
                session_id, message_count, message_id, role, time, &text[..text.len().min(50)]
            );
            return (Some(role), Some(text), time);
        }
    }

    log::debug!("Session {} has {} messages but no displayable text", session_id, message_count);
    (None, None, 0)
}

/// Get the text content from a message's parts
fn get_message_text(storage_path: &PathBuf, message_id: &str) -> Option<String> {
    let part_dir = storage_path.join("part").join(message_id);

    if !part_dir.exists() {
        return None;
    }

    let mut text_content: Option<String> = None;
    let mut reasoning_content: Option<String> = None;

    // Find the "text" type part (preferred), or "reasoning" as fallback
    if let Ok(entries) = std::fs::read_dir(&part_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(part) = serde_json::from_str::<OpenCodePart>(&content) {
                        if part.part_type == "text" {
                            if let Some(text) = part.text {
                                text_content = Some(text);
                            }
                        } else if part.part_type == "reasoning" && reasoning_content.is_none() {
                            if let Some(text) = part.text {
                                reasoning_content = Some(text);
                            }
                        }
                    }
                }
            }
        }
    }

    // Prefer text content, fall back to reasoning
    let content = text_content.or(reasoning_content)?;

    // Skip system prompts (XML-formatted instructions)
    let trimmed = content.trim();
    if trimmed.starts_with('<') && (trimmed.contains("ultrawork") || trimmed.contains("mode>")) {
        return None;
    }

    // Truncate if too long
    let truncated = if content.len() > 200 {
        format!("{}...", &content[..197])
    } else {
        content
    };

    Some(truncated)
}

/// Get a global session matching a specific directory
fn get_global_session_for_directory(
    storage_path: &PathBuf,
    directory: &str,
    process: &AgentProcess,
) -> Option<Session> {
    let session_dir = storage_path.join("session").join("global");

    if !session_dir.exists() {
        return None;
    }

    // Find sessions matching this directory
    let mut latest_session: Option<(OpenCodeSession, u64)> = None;

    if let Ok(entries) = std::fs::read_dir(&session_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "json").unwrap_or(false) {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(session) = serde_json::from_str::<OpenCodeSession>(&content) {
                        // Check if this session's directory matches or is a parent of the process CWD
                        if directory == session.directory || directory.starts_with(&format!("{}/", session.directory)) {
                            let updated = session.time.updated;
                            if latest_session.as_ref().map(|(_, t)| updated > *t).unwrap_or(true) {
                                latest_session = Some((session, updated));
                            }
                        }
                    }
                }
            }
        }
    }

    let (session, _) = latest_session?;

    // Get the last message for status detection and display
    let (last_role, last_message_text, _last_message_time) = get_last_message(storage_path, &session.id);

    // Determine status
    let status = if process.cpu_usage > 5.0 {
        SessionStatus::Processing
    } else if last_role.as_deref() == Some("assistant") {
        SessionStatus::Waiting
    } else if last_role.as_deref() == Some("user") {
        SessionStatus::Processing
    } else {
        SessionStatus::Idle
    };

    // Convert timestamp to ISO string (OpenCode uses milliseconds)
    let updated_secs = session.time.updated / 1000;
    let last_activity_at = chrono::DateTime::from_timestamp(updated_secs as i64, 0)
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    // Extract project name from session directory
    let project_name = session.directory
        .split('/')
        .filter(|s| !s.is_empty())
        .last()
        .unwrap_or("Unknown")
        .to_string();

    log::info!(
        "OpenCode global session: id={}, directory={}, status={:?}, last_role={:?}, cpu={:.1}%",
        session.id, session.directory, status, last_role, process.cpu_usage
    );

    // Use message text if available, fall back to session title
    let display_message = last_message_text
        .or_else(|| Some(session.title.clone()).filter(|t| !t.is_empty()));

    Some(Session {
        id: session.id,
        agent_type: AgentType::OpenCode,
        project_name,
        project_path: session.directory,
        git_branch: None,
        github_url: None,
        status,
        last_message: display_message,
        last_message_role: last_role,
        last_activity_at,
        pid: process.pid,
        cpu_usage: process.cpu_usage,
        active_subagent_count: 0,
    })
}
