use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

use crate::process::{find_claude_processes, ClaudeProcess};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub project_name: String,
    pub project_path: String,
    pub git_branch: Option<String>,
    pub status: SessionStatus,
    pub last_message: Option<String>,
    pub last_message_role: Option<String>,
    pub last_activity_at: String,
    pub pid: u32,
    pub cpu_usage: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Waiting,
    Processing,
    Idle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionsResponse {
    pub sessions: Vec<Session>,
    pub total_count: usize,
    pub waiting_count: usize,
}

#[derive(Debug, Deserialize)]
struct JsonlMessage {
    #[serde(rename = "sessionId")]
    session_id: Option<String>,
    #[serde(rename = "gitBranch")]
    git_branch: Option<String>,
    timestamp: Option<String>,
    message: Option<MessageContent>,
}

#[derive(Debug, Deserialize)]
struct MessageContent {
    role: Option<String>,
    content: Option<serde_json::Value>,
}

pub fn get_sessions() -> SessionsResponse {
    let claude_processes = find_claude_processes();
    let mut sessions = Vec::new();

    // Build a map of cwd -> process for matching
    let mut cwd_to_process: HashMap<String, &ClaudeProcess> = HashMap::new();
    for process in &claude_processes {
        if let Some(cwd) = &process.cwd {
            let cwd_str = cwd.to_string_lossy().to_string();
            cwd_to_process.insert(cwd_str, process);
        }
    }

    // Scan ~/.claude/projects for session files
    let claude_dir = dirs::home_dir()
        .map(|h| h.join(".claude").join("projects"))
        .unwrap_or_default();

    if !claude_dir.exists() {
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

            let project_path = dir_name.replace("-", "/");
            let project_path = if project_path.starts_with("/") {
                project_path
            } else {
                format!("/{}", project_path)
            };

            // Check if this project has an active Claude process
            let process = cwd_to_process.get(&project_path);
            if process.is_none() {
                continue; // Skip projects without active processes
            }
            let process = process.unwrap();

            // Find the most recent JSONL file
            if let Some(session) = find_active_session(&path, &project_path, process) {
                sessions.push(session);
            }
        }
    }

    // Sort: waiting first, then processing, then idle
    sessions.sort_by(|a, b| {
        let status_order = |s: &SessionStatus| match s {
            SessionStatus::Waiting => 0,
            SessionStatus::Processing => 1,
            SessionStatus::Idle => 2,
        };
        status_order(&a.status).cmp(&status_order(&b.status))
    });

    let waiting_count = sessions.iter()
        .filter(|s| matches!(s.status, SessionStatus::Waiting))
        .count();
    let total_count = sessions.len();

    SessionsResponse {
        sessions,
        total_count,
        waiting_count,
    }
}

fn find_active_session(project_dir: &PathBuf, project_path: &str, process: &ClaudeProcess) -> Option<Session> {
    // Find the most recently modified JSONL file
    let mut jsonl_files: Vec<_> = fs::read_dir(project_dir)
        .ok()?
        .flatten()
        .filter(|e| {
            e.path().extension()
                .map(|ext| ext == "jsonl")
                .unwrap_or(false)
        })
        .collect();

    jsonl_files.sort_by(|a, b| {
        let time_a = a.metadata().and_then(|m| m.modified()).ok();
        let time_b = b.metadata().and_then(|m| m.modified()).ok();
        time_b.cmp(&time_a)
    });

    let jsonl_path = jsonl_files.first()?.path();

    // Parse the JSONL file to get session info
    let file = File::open(&jsonl_path).ok()?;
    let reader = BufReader::new(file);

    let mut session_id = None;
    let mut git_branch = None;
    let mut last_timestamp = None;
    let mut last_message = None;
    let mut last_role = None;

    // Read last N lines for efficiency
    let lines: Vec<_> = reader.lines().flatten().collect();
    let recent_lines = lines.iter().rev().take(50);

    for line in recent_lines {
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

            if last_message.is_none() {
                if let Some(content) = msg.message {
                    last_role = content.role;
                    last_message = content.content.and_then(|c| {
                        match c {
                            serde_json::Value::String(s) => Some(s),
                            serde_json::Value::Array(arr) => {
                                arr.iter().find_map(|v| {
                                    v.get("text").and_then(|t| t.as_str()).map(String::from)
                                })
                            }
                            _ => None,
                        }
                    });
                }
            }

            if session_id.is_some() && last_message.is_some() {
                break;
            }
        }
    }

    let session_id = session_id?;

    // Determine status
    let status = determine_status(process.cpu_usage, last_role.as_deref(), &last_timestamp);

    // Extract project name from path
    let project_name = project_path
        .split('/')
        .filter(|s| !s.is_empty())
        .last()
        .unwrap_or("Unknown")
        .to_string();

    // Truncate message for preview
    let last_message = last_message.map(|m| {
        if m.len() > 100 {
            format!("{}...", &m[..100])
        } else {
            m
        }
    });

    Some(Session {
        id: session_id,
        project_name,
        project_path: project_path.to_string(),
        git_branch,
        status,
        last_message,
        last_message_role: last_role,
        last_activity_at: last_timestamp.unwrap_or_else(|| "Unknown".to_string()),
        pid: process.pid,
        cpu_usage: process.cpu_usage,
    })
}

fn determine_status(cpu_usage: f32, last_role: Option<&str>, _last_timestamp: &Option<String>) -> SessionStatus {
    // High CPU means actively processing
    if cpu_usage > 5.0 {
        return SessionStatus::Processing;
    }

    // Check last message role
    match last_role {
        Some("assistant") => SessionStatus::Waiting,
        Some("user") => SessionStatus::Processing,
        _ => SessionStatus::Idle,
    }
}
