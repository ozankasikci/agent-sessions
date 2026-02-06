use serde::{Deserialize, Serialize};

/// Type of AI coding agent
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AgentType {
    Claude,
    OpenCode,
}

/// Represents a Claude Code session
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub agent_type: AgentType,
    pub project_name: String,
    pub project_path: String,
    pub git_branch: Option<String>,
    pub github_url: Option<String>,
    pub status: SessionStatus,
    pub last_message: Option<String>,
    pub last_message_role: Option<String>,
    pub last_activity_at: String,
    pub pid: u32,
    pub cpu_usage: f32,
    pub active_subagent_count: usize,
}

/// Status of a Claude Code session
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Waiting,
    Processing,
    Thinking,
    Compacting,
    Idle,
}

/// Response containing all sessions and counts
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionsResponse {
    pub sessions: Vec<Session>,
    pub total_count: usize,
    pub waiting_count: usize,
}

/// Internal struct for parsing JSONL messages
#[derive(Debug, Deserialize)]
pub(crate) struct JsonlMessage {
    #[serde(rename = "sessionId")]
    pub session_id: Option<String>,
    #[serde(rename = "gitBranch")]
    pub git_branch: Option<String>,
    pub cwd: Option<String>,
    pub timestamp: Option<String>,
    #[serde(rename = "type")]
    pub msg_type: Option<String>,
    pub subtype: Option<String>,
    #[serde(rename = "isCompactSummary")]
    pub is_compact_summary: Option<bool>,
    pub message: Option<MessageContent>,
}

/// Internal struct for message content
#[derive(Debug, Deserialize)]
pub(crate) struct MessageContent {
    pub role: Option<String>,
    pub content: Option<serde_json::Value>,
}
