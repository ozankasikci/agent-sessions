//! OpenAI Codex sessions: the `codex` CLI and the Codex desktop app / IDE extension.
//!
//! Codex writes one rollout file per conversation:
//! `$CODEX_HOME/sessions/YYYY/MM/DD/rollout-<timestamp>-<id>.jsonl` (CODEX_HOME
//! defaults to `~/.codex`). Every line is `{"timestamp", "type", "payload"}`. The
//! first line is a `session_meta` record with the conversation id, cwd, originator
//! and git info. Later lines are `response_item` (messages, tool calls, tool output),
//! `event_msg` (turn lifecycle: task_started, task_complete, turn_aborted, approvals)
//! and `turn_context` records.
//!
//! The CLI runs one conversation per process from its launch directory, so it is
//! matched to rollouts by cwd. The desktop app and the IDE extension run a single
//! long-lived `codex app-server` that hosts any number of conversations, so while
//! one is running we list its recently active conversations.

use super::common::{
    file_age, modified_time, project_name_from_path, read_first_line, read_tail_lines,
    system_time_to_iso, truncate_message, HOST_ACTIVE_WINDOW, HOST_STALE_AFTER,
};
use super::host::detect_host;
use super::{AgentDetector, AgentProcess};
use crate::process::is_orphaned_process;
use crate::session::{get_github_url, AgentType, Client, Session, SessionStatus};
use once_cell::sync::Lazy;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

pub struct CodexDetector;

impl AgentDetector for CodexDetector {
    fn name(&self) -> &'static str {
        "Codex"
    }

    fn agent_type(&self) -> AgentType {
        AgentType::Codex
    }

    fn find_processes(&self, system: &sysinfo::System) -> Vec<AgentProcess> {
        find_codex_processes(system)
    }

    fn find_sessions(&self, processes: &[AgentProcess]) -> Vec<Session> {
        if processes.is_empty() {
            return Vec::new();
        }
        match codex_home() {
            Some(home) => get_codex_sessions(&home.join("sessions"), processes),
            None => Vec::new(),
        }
    }
}

/// Rollouts older than this are not considered when matching a CLI process
const CLI_MATCH_WINDOW: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// A subagent rollout counts as active when written within this window
const SUBAGENT_ACTIVE_WINDOW: Duration = Duration::from_secs(30);

/// Subcommands that do not run a conversation
const NON_SESSION_SUBCOMMANDS: &[&str] = &[
    "login", "logout", "completion", "apply", "a", "sandbox", "debug", "mcp", "help",
    "features", "cloud", "generate-ts", "-V", "--version", "-h", "--help",
];

/// Subcommands that host many conversations in one process
const HOST_SUBCOMMANDS: &[&str] = &["app-server", "mcp-server"];

fn codex_home() -> Option<PathBuf> {
    if let Ok(home) = std::env::var("CODEX_HOME") {
        if !home.is_empty() {
            return Some(PathBuf::from(home));
        }
    }
    dirs::home_dir().map(|h| h.join(".codex"))
}

/// Is this command line the native codex runtime (not the npm launcher or the app itself)?
pub fn is_codex_command(name: &str, cmd: &[String]) -> bool {
    let Some(first) = cmd.first() else {
        return name == "codex";
    };
    // The desktop app's own executable (Codex.app/Contents/MacOS/Codex)
    if first.contains(".app/Contents/MacOS/") {
        return false;
    }
    let first_lower = first.to_lowercase();
    first_lower == "codex" || first_lower.ends_with("/codex") || name == "codex"
}

/// First non-flag argument after the executable
pub fn codex_subcommand(cmd: &[String]) -> Option<&str> {
    cmd.iter().skip(1).map(|s| s.as_str()).find(|a| !a.starts_with('-') || *a == "-V" || *a == "-h")
}

fn find_codex_processes(system: &sysinfo::System) -> Vec<AgentProcess> {
    let codex_pids: HashSet<sysinfo::Pid> = system
        .processes()
        .iter()
        .filter(|(_, p)| {
            let cmd: Vec<String> = p.cmd().iter().map(|s| s.to_string_lossy().to_string()).collect();
            is_codex_command(&p.name().to_string_lossy(), &cmd)
        })
        .map(|(pid, _)| *pid)
        .collect();

    let mut processes = Vec::new();
    for pid in &codex_pids {
        let Some(process) = system.process(*pid) else { continue };
        let cmd: Vec<String> = process.cmd().iter().map(|s| s.to_string_lossy().to_string()).collect();
        let subcommand = codex_subcommand(&cmd);

        if subcommand.map(|s| NON_SESSION_SUBCOMMANDS.contains(&s)).unwrap_or(false) {
            continue;
        }
        // Helpers spawned by codex itself (sandboxes, nested runs) belong to the parent
        if process.parent().map(|pp| codex_pids.contains(&pp)).unwrap_or(false) {
            continue;
        }

        let is_host = subcommand.map(|s| HOST_SUBCOMMANDS.contains(&s)).unwrap_or(false);
        let host = detect_host(system, *pid);

        if !is_host && host.client == Client::Cli && is_orphaned_process(system, process) {
            log::debug!("Skipping orphaned codex process pid={}", pid.as_u32());
            continue;
        }

        let client = if is_host && host.client == Client::Cli {
            // An app-server we could not attribute to an app: most likely the desktop app
            Client::Desktop
        } else {
            host.client
        };

        log::debug!(
            "Codex process: pid={}, subcommand={:?}, host={:?}, cwd={:?}",
            pid.as_u32(), subcommand, client, process.cwd()
        );

        processes.push(AgentProcess {
            pid: pid.as_u32(),
            cpu_usage: process.cpu_usage(),
            cwd: process.cwd().map(|p| p.to_path_buf()),
            client,
            app_name: host.app_name.or_else(|| (client == Client::Desktop).then(|| "Codex".to_string())),
            is_host,
        });
    }
    processes
}

/// Metadata from a rollout's first line (never changes, so it is cached per file)
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CodexMeta {
    pub id: String,
    pub cwd: String,
    pub originator: String,
    pub source: String,
    pub git_branch: Option<String>,
    pub repository_url: Option<String>,
    pub parent_thread_id: Option<String>,
    pub is_subagent: bool,
}

impl CodexMeta {
    /// Conversations started from a terminal (`codex`, `codex exec`)
    fn is_cli(&self) -> bool {
        matches!(self.source.as_str(), "cli" | "exec")
            || self.originator.contains("cli")
            || self.originator.contains("tui")
            || self.originator.contains("exec")
    }

    /// Started from the desktop app or an IDE extension
    fn is_app(&self) -> bool {
        !self.is_cli()
    }
}

/// Parse the `session_meta` line of a rollout
pub fn parse_session_meta(line: &str) -> Option<CodexMeta> {
    let v: Value = serde_json::from_str(line).ok()?;
    if v.get("type").and_then(|t| t.as_str()) != Some("session_meta") {
        return None;
    }
    let p = v.get("payload")?;
    let s = |key: &str| p.get(key).and_then(|x| x.as_str()).unwrap_or("").to_string();

    let source_value = p.get("source");
    let (source, subagent_parent, is_subagent) = match source_value {
        Some(Value::String(s)) => (s.clone(), None, false),
        Some(Value::Object(obj)) => {
            let sub = obj.get("subagent");
            let parent = sub
                .and_then(|s| s.get("thread_spawn"))
                .and_then(|t| t.get("parent_thread_id"))
                .and_then(|x| x.as_str())
                .map(String::from);
            (String::new(), parent, sub.is_some())
        }
        _ => (String::new(), None, false),
    };
    let parent_thread_id = p
        .get("parent_thread_id")
        .and_then(|x| x.as_str())
        .map(String::from)
        .or(subagent_parent);
    let thread_source_is_sub = p
        .get("thread_source")
        .and_then(|x| x.as_str())
        .map(|ts| ts != "user")
        .unwrap_or(false);

    let git = p.get("git");
    let git_str = |key: &str| {
        git.and_then(|g| g.get(key))
            .and_then(|x| x.as_str())
            .filter(|x| !x.is_empty())
            .map(String::from)
    };

    let id = {
        let id = s("id");
        if id.is_empty() { s("session_id") } else { id }
    };

    Some(CodexMeta {
        id,
        cwd: s("cwd"),
        originator: s("originator").to_lowercase(),
        source: source.to_lowercase(),
        git_branch: git_str("branch"),
        repository_url: git_str("repository_url"),
        is_subagent: is_subagent || parent_thread_id.is_some() || thread_source_is_sub,
        parent_thread_id,
    })
}

static META_CACHE: Lazy<Mutex<HashMap<PathBuf, Option<CodexMeta>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn load_meta(path: &Path) -> Option<CodexMeta> {
    if let Some(cached) = META_CACHE.lock().unwrap().get(path) {
        return cached.clone();
    }
    let meta = read_first_line(path).and_then(|l| parse_session_meta(&l)).map(|mut m| {
        if m.id.is_empty() {
            m.id = rollout_id_from_filename(path).unwrap_or_default();
        }
        m
    });
    // Only cache successful parses: a brand-new file may not have its first line yet
    if meta.is_some() {
        META_CACHE.lock().unwrap().insert(path.to_path_buf(), meta.clone());
    }
    meta
}

/// `rollout-2025-05-14T21-37-50-<uuid>.jsonl` -> `<uuid>`
pub fn rollout_id_from_filename(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    let rest = stem.strip_prefix("rollout-")?;
    // Timestamp is fixed width: YYYY-MM-DDTHH-MM-SS (19 chars) followed by '-'
    if rest.len() > 20 {
        Some(rest[20..].to_string())
    } else {
        None
    }
}

/// All rollout files modified within `window`, newest first
fn list_rollouts(sessions_root: &Path, window: Duration) -> Vec<(PathBuf, SystemTime)> {
    let now = SystemTime::now();
    let cutoff = now.checked_sub(window).unwrap_or(SystemTime::UNIX_EPOCH);
    let mut out = Vec::new();

    // YYYY/MM/DD directories group rollouts by the day a conversation *started*, and a
    // resumed conversation keeps appending to its original file, so every day directory
    // is walked and each file's mtime checked (a stat per file, contents are not read).
    let read_dirs = |p: &Path| -> Vec<PathBuf> {
        fs::read_dir(p)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect()
    };
    for year in read_dirs(sessions_root) {
        for month in read_dirs(&year) {
            for day in read_dirs(&month) {
                for entry in fs::read_dir(&day).into_iter().flatten().flatten() {
                    let path = entry.path();
                    let is_rollout = path.extension().map(|e| e == "jsonl").unwrap_or(false)
                        && path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| n.starts_with("rollout-"))
                            .unwrap_or(false);
                    if !is_rollout {
                        continue;
                    }
                    if let Some(mtime) = entry.metadata().ok().and_then(|m| m.modified().ok()) {
                        if mtime >= cutoff {
                            out.push((path, mtime));
                        }
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| b.1.cmp(&a.1));
    out
}

/// What the conversation is doing, derived from the most recent log records
#[derive(Debug, Clone, PartialEq)]
pub struct CodexState {
    pub status: SessionStatus,
    pub last_message: Option<String>,
    pub last_role: Option<String>,
    pub last_timestamp: Option<String>,
}

/// Internal activity before freshness is taken into account
#[derive(Debug, Clone, Copy, PartialEq)]
enum Activity {
    Unknown,
    Thinking,
    Tool,
    Replying,
    Waiting,
}

/// Text of a `response_item` message (`content: [{type: input_text|output_text, text}]`)
fn message_text(content: &Value) -> Option<String> {
    match content {
        Value::String(s) => Some(s.clone()),
        Value::Array(items) => {
            let text: Vec<&str> = items
                .iter()
                .filter_map(|i| i.get("text").and_then(|t| t.as_str()))
                .collect();
            if text.is_empty() { None } else { Some(text.join("\n")) }
        }
        _ => None,
    }
}

/// Injected context (AGENTS.md, environment, instructions) is not a user message
fn is_injected_context(text: &str) -> bool {
    let t = text.trim_start();
    t.is_empty()
        || t.starts_with('<')
        || t.starts_with("# AGENTS.md")
        || t.starts_with("# Context from my IDE")
}

/// Derive status and preview from rollout lines (oldest first).
/// `file_age` is how long ago the file was last written.
pub fn parse_rollout_state(lines: &[String], file_age: Duration) -> CodexState {
    let mut activity = Activity::Unknown;
    let mut last_message = None;
    let mut last_role = None;
    let mut last_timestamp = None;

    for line in lines {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if let Some(ts) = v.get("timestamp").and_then(|t| t.as_str()) {
            last_timestamp = Some(ts.to_string());
        }
        let record_type = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        let Some(p) = v.get("payload") else { continue };
        let payload_type = p.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match (record_type, payload_type) {
            ("event_msg", "task_started") => activity = Activity::Thinking,
            ("event_msg", "user_message") => {
                activity = Activity::Thinking;
                if let Some(msg) = p.get("message").and_then(|m| m.as_str()) {
                    if !is_injected_context(msg) {
                        last_message = Some(msg.to_string());
                        last_role = Some("user".to_string());
                    }
                }
            }
            ("event_msg", "agent_message") => {
                activity = Activity::Replying;
                if let Some(msg) = p.get("message").and_then(|m| m.as_str()) {
                    last_message = Some(msg.to_string());
                    last_role = Some("assistant".to_string());
                }
            }
            ("event_msg", "task_complete") => {
                activity = Activity::Waiting;
                if let Some(msg) = p.get("last_agent_message").and_then(|m| m.as_str()) {
                    if !msg.is_empty() {
                        last_message = Some(msg.to_string());
                        last_role = Some("assistant".to_string());
                    }
                }
            }
            ("event_msg", "turn_aborted") | ("event_msg", "error") => activity = Activity::Waiting,
            ("event_msg", "exec_approval_request")
            | ("event_msg", "apply_patch_approval_request")
            | ("event_msg", "request_user_input")
            | ("event_msg", "elicitation_request") => activity = Activity::Waiting,
            ("response_item", "reasoning") => activity = Activity::Thinking,
            ("response_item", "function_call")
            | ("response_item", "custom_tool_call")
            | ("response_item", "local_shell_call")
            | ("response_item", "web_search_call") => {
                let name = p.get("name").and_then(|n| n.as_str()).unwrap_or("");
                activity = if name.starts_with("request_user_input") {
                    Activity::Waiting
                } else {
                    Activity::Tool
                };
            }
            ("response_item", "function_call_output") | ("response_item", "custom_tool_call_output") => {
                activity = Activity::Thinking
            }
            ("response_item", "message") => {
                let role = p.get("role").and_then(|r| r.as_str()).unwrap_or("");
                let text = p.get("content").and_then(message_text);
                match role {
                    "assistant" => {
                        activity = Activity::Replying;
                        if let Some(t) = text.filter(|t| !t.trim().is_empty()) {
                            last_message = Some(t);
                            last_role = Some("assistant".to_string());
                        }
                    }
                    "user" => {
                        // Older logs have no user_message event; the response item stands in
                        if let Some(t) = text.filter(|t| !is_injected_context(t)) {
                            activity = Activity::Thinking;
                            last_message = Some(t);
                            last_role = Some("user".to_string());
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    let recently_written = file_age < Duration::from_secs(30);
    let status = match activity {
        Activity::Thinking => SessionStatus::Thinking,
        Activity::Tool => SessionStatus::Processing,
        // The final reply is followed by task_complete; a reply with nothing after it
        // means the turn is still streaming, unless the file has gone quiet.
        Activity::Replying if recently_written => SessionStatus::Thinking,
        Activity::Replying | Activity::Waiting => SessionStatus::Waiting,
        Activity::Unknown if recently_written => SessionStatus::Thinking,
        Activity::Unknown => SessionStatus::Waiting,
    };

    CodexState {
        status,
        last_message: last_message.map(|m| truncate_message(&m, 100)),
        last_role,
        last_timestamp,
    }
}

fn build_session(path: &Path, meta: &CodexMeta, process: &AgentProcess, project_path: &str) -> Session {
    let age = file_age(path).unwrap_or_default();
    let lines = read_tail_lines(path);
    let mut state = parse_rollout_state(&lines, age);

    if process.is_host
        && age > HOST_STALE_AFTER
        && matches!(state.status, SessionStatus::Thinking | SessionStatus::Processing)
    {
        state.status = SessionStatus::Waiting;
    }

    let last_activity_at = state
        .last_timestamp
        .clone()
        .or_else(|| modified_time(path).map(system_time_to_iso))
        .unwrap_or_else(|| "Unknown".to_string());

    let github_url = if !project_path.is_empty() && Path::new(project_path).is_dir() {
        get_github_url(project_path)
    } else {
        None
    }
    .or_else(|| meta.repository_url.as_deref().and_then(normalize_github_url));

    Session {
        id: meta.id.clone(),
        agent_type: AgentType::Codex,
        project_name: project_name_from_path(project_path),
        project_path: project_path.to_string(),
        git_branch: meta.git_branch.clone(),
        github_url,
        status: state.status,
        last_message: state.last_message,
        last_message_role: state.last_role,
        last_activity_at,
        pid: process.pid,
        cpu_usage: process.cpu_usage,
        active_subagent_count: 0,
        client: process.client,
        app_name: process.app_name.clone(),
        shared_process: process.is_host,
    }
}

/// `git@github.com:user/repo.git` / `https://github.com/user/repo.git` -> `https://github.com/user/repo`
pub fn normalize_github_url(url: &str) -> Option<String> {
    let path = url
        .strip_prefix("git@github.com:")
        .or_else(|| url.strip_prefix("https://github.com/"))
        .or_else(|| url.strip_prefix("ssh://git@github.com/"))?;
    let path = path.strip_suffix(".git").unwrap_or(path);
    Some(format!("https://github.com/{}", path))
}

fn get_codex_sessions(sessions_root: &Path, processes: &[AgentProcess]) -> Vec<Session> {
    if !sessions_root.exists() {
        log::debug!("Codex sessions directory does not exist: {:?}", sessions_root);
        return Vec::new();
    }

    let rollouts = list_rollouts(sessions_root, CLI_MATCH_WINDOW);
    let mut claimed: HashSet<PathBuf> = HashSet::new();
    let mut sessions = Vec::new();

    // --- CLI processes: one conversation each, matched by cwd (newest file first) ---
    let mut cli_by_cwd: HashMap<String, Vec<&AgentProcess>> = HashMap::new();
    for p in processes.iter().filter(|p| !p.is_host) {
        if let Some(cwd) = &p.cwd {
            cli_by_cwd.entry(cwd.to_string_lossy().to_string()).or_default().push(p);
        }
    }
    let mut assigned: HashMap<String, usize> = HashMap::new();
    let mut remaining: usize = cli_by_cwd.values().map(|v| v.len()).sum();

    for (path, _) in &rollouts {
        if remaining == 0 {
            break;
        }
        let Some(meta) = load_meta(path) else { continue };
        if meta.is_subagent {
            continue;
        }
        let Some(procs) = cli_by_cwd.get(&meta.cwd) else { continue };
        let idx = assigned.entry(meta.cwd.clone()).or_insert(0);
        if *idx >= procs.len() {
            continue;
        }
        let process = procs[*idx];
        *idx += 1;
        remaining -= 1;
        claimed.insert(path.clone());
        sessions.push(build_session(path, &meta, process, &meta.cwd));
    }

    // --- Host processes (desktop app / IDE app-server): recently active conversations ---
    let hosts: Vec<&AgentProcess> = processes.iter().filter(|p| p.is_host).collect();
    if !hosts.is_empty() {
        let now = SystemTime::now();
        for (path, mtime) in &rollouts {
            if now.duration_since(*mtime).unwrap_or_default() > HOST_ACTIVE_WINDOW {
                break; // sorted newest first
            }
            if claimed.contains(path) {
                continue;
            }
            let Some(meta) = load_meta(path) else { continue };
            if meta.is_subagent || !meta.is_app() {
                continue;
            }
            // Desktop conversations go to the desktop host, extension ones to the IDE host
            let wants_desktop = meta.originator.contains("desktop") || meta.originator.contains("app");
            let process = hosts
                .iter()
                .find(|h| (h.client == Client::Desktop) == wants_desktop)
                .or_else(|| hosts.first())
                .copied()
                .unwrap();
            claimed.insert(path.clone());
            sessions.push(build_session(path, &meta, process, &meta.cwd));
        }
    }

    // --- Subagents: count recently written child rollouts per parent ---
    let now = SystemTime::now();
    let mut active_children: HashMap<String, usize> = HashMap::new();
    for (path, mtime) in &rollouts {
        if now.duration_since(*mtime).unwrap_or_default() > SUBAGENT_ACTIVE_WINDOW {
            break;
        }
        if let Some(parent) = load_meta(path).and_then(|m| m.parent_thread_id) {
            *active_children.entry(parent).or_insert(0) += 1;
        }
    }
    for s in &mut sessions {
        s.active_subagent_count = active_children.get(&s.id).copied().unwrap_or(0);
    }

    sessions
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn line(v: serde_json::Value) -> String {
        v.to_string()
    }

    fn meta_line(id: &str, cwd: &str, originator: &str, source: serde_json::Value) -> String {
        line(serde_json::json!({
            "timestamp": "2026-10-31T10:00:00.000Z",
            "type": "session_meta",
            "payload": {
                "id": id, "cwd": cwd, "originator": originator, "cli_version": "0.150.0",
                "source": source,
                "git": {"branch": "main", "repository_url": "git@github.com:me/repo.git"}
            }
        }))
    }

    fn ev(ts: &str, payload: serde_json::Value) -> String {
        line(serde_json::json!({"timestamp": ts, "type": "event_msg", "payload": payload}))
    }

    fn item(ts: &str, payload: serde_json::Value) -> String {
        line(serde_json::json!({"timestamp": ts, "type": "response_item", "payload": payload}))
    }

    const OLD: Duration = Duration::from_secs(600);
    const FRESH: Duration = Duration::from_secs(1);

    #[test]
    fn parses_session_meta() {
        let m = parse_session_meta(&meta_line("abc", "/p", "codex_cli_rs", serde_json::json!("cli"))).unwrap();
        assert_eq!(m.id, "abc");
        assert_eq!(m.cwd, "/p");
        assert_eq!(m.git_branch.as_deref(), Some("main"));
        assert!(m.is_cli());
        assert!(!m.is_subagent);

        let m = parse_session_meta(&meta_line("d", "/p", "Codex Desktop", serde_json::json!("vscode"))).unwrap();
        assert!(m.is_app());

        let sub = serde_json::json!({"subagent": {"thread_spawn": {"parent_thread_id": "abc"}}});
        let m = parse_session_meta(&meta_line("s", "/p", "codex_cli_rs", sub)).unwrap();
        assert!(m.is_subagent);
        assert_eq!(m.parent_thread_id.as_deref(), Some("abc"));

        assert!(parse_session_meta(&ev("t", serde_json::json!({"type": "task_started"}))).is_none());
    }

    #[test]
    fn rollout_id_from_name() {
        let p = Path::new("/x/rollout-2025-05-14T21-37-50-0199a-uuid.jsonl");
        assert_eq!(rollout_id_from_filename(p).as_deref(), Some("0199a-uuid"));
    }

    #[test]
    fn status_turn_lifecycle() {
        let mut lines = vec![
            meta_line("a", "/p", "codex_cli_rs", serde_json::json!("cli")),
            ev("2026-10-31T10:00:01Z", serde_json::json!({"type": "task_started"})),
            ev("2026-10-31T10:00:01Z", serde_json::json!({"type": "user_message", "message": "fix the bug"})),
        ];
        let s = parse_rollout_state(&lines, OLD);
        assert_eq!(s.status, SessionStatus::Thinking);
        assert_eq!(s.last_message.as_deref(), Some("fix the bug"));
        assert_eq!(s.last_role.as_deref(), Some("user"));

        lines.push(item("2026-10-31T10:00:02Z", serde_json::json!({"type": "function_call", "name": "shell", "arguments": "{}", "call_id": "1"})));
        assert_eq!(parse_rollout_state(&lines, OLD).status, SessionStatus::Processing);

        lines.push(item("2026-10-31T10:00:03Z", serde_json::json!({"type": "function_call_output", "call_id": "1", "output": "ok"})));
        assert_eq!(parse_rollout_state(&lines, OLD).status, SessionStatus::Thinking);

        lines.push(item("2026-10-31T10:00:04Z", serde_json::json!({"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "Done, fixed it."}]})));
        assert_eq!(parse_rollout_state(&lines, FRESH).status, SessionStatus::Thinking);
        assert_eq!(parse_rollout_state(&lines, OLD).status, SessionStatus::Waiting);

        lines.push(ev("2026-10-31T10:00:05Z", serde_json::json!({"type": "task_complete", "last_agent_message": "Done, fixed it."})));
        let s = parse_rollout_state(&lines, FRESH);
        assert_eq!(s.status, SessionStatus::Waiting);
        assert_eq!(s.last_message.as_deref(), Some("Done, fixed it."));
        assert_eq!(s.last_role.as_deref(), Some("assistant"));
        assert_eq!(s.last_timestamp.as_deref(), Some("2026-10-31T10:00:05Z"));
    }

    #[test]
    fn status_approval_and_abort() {
        let lines = vec![
            ev("t1", serde_json::json!({"type": "task_started"})),
            item("t2", serde_json::json!({"type": "function_call", "name": "shell"})),
            ev("t3", serde_json::json!({"type": "exec_approval_request"})),
        ];
        assert_eq!(parse_rollout_state(&lines, FRESH).status, SessionStatus::Waiting);

        let lines = vec![
            ev("t1", serde_json::json!({"type": "task_started"})),
            ev("t2", serde_json::json!({"type": "turn_aborted", "reason": "interrupted"})),
        ];
        assert_eq!(parse_rollout_state(&lines, FRESH).status, SessionStatus::Waiting);

        let lines = vec![item("t1", serde_json::json!({"type": "function_call", "name": "request_user_input"}))];
        assert_eq!(parse_rollout_state(&lines, FRESH).status, SessionStatus::Waiting);
    }

    #[test]
    fn skips_injected_context_messages() {
        let lines = vec![
            item("t1", serde_json::json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": "<environment_context>cwd</environment_context>"}]})),
            item("t2", serde_json::json!({"type": "message", "role": "user", "content": [{"type": "input_text", "text": "# AGENTS.md instructions"}]})),
        ];
        let s = parse_rollout_state(&lines, OLD);
        assert_eq!(s.last_message, None);
        assert_eq!(s.status, SessionStatus::Waiting);
    }

    #[test]
    fn command_detection() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(is_codex_command("codex", &v(&["/opt/homebrew/bin/codex"])));
        assert!(is_codex_command("codex", &v(&["codex", "resume", "--last"])));
        assert!(!is_codex_command("Codex", &v(&["/Applications/Codex.app/Contents/MacOS/Codex"])));
        assert!(!is_codex_command("node", &v(&["node", "/usr/local/lib/node_modules/@openai/codex/bin/codex.js"])));
        assert_eq!(codex_subcommand(&v(&["codex", "--yolo", "app-server"])), Some("app-server"));
        assert_eq!(codex_subcommand(&v(&["codex"])), None);
    }

    #[test]
    fn github_url_normalization() {
        assert_eq!(normalize_github_url("git@github.com:me/repo.git").as_deref(), Some("https://github.com/me/repo"));
        assert_eq!(normalize_github_url("https://github.com/me/repo").as_deref(), Some("https://github.com/me/repo"));
        assert_eq!(normalize_github_url("https://gitlab.com/me/repo"), None);
    }

    #[test]
    fn matches_cli_and_host_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let day = dir.path().join("2026").join("10").join("31");
        fs::create_dir_all(&day).unwrap();
        let write = |name: &str, lines: &[String]| {
            let path = day.join(name);
            let mut f = fs::File::create(&path).unwrap();
            for l in lines {
                writeln!(f, "{}", l).unwrap();
            }
            path
        };
        write("rollout-2026-10-31T10-00-00-cli1.jsonl", &[
            meta_line("cli1", "/work/proj", "codex_cli_rs", serde_json::json!("cli")),
            ev("2026-10-31T10:00:01Z", serde_json::json!({"type": "user_message", "message": "hello"})),
        ]);
        write("rollout-2026-10-31T10-01-00-app1.jsonl", &[
            meta_line("app1", "/work/other", "Codex Desktop", serde_json::json!("vscode")),
            ev("2026-10-31T10:01:01Z", serde_json::json!({"type": "task_complete", "last_agent_message": "all set"})),
        ]);
        write("rollout-2026-10-31T10-02-00-sub1.jsonl", &[
            meta_line("sub1", "/work/other", "Codex Desktop",
                serde_json::json!({"subagent": {"thread_spawn": {"parent_thread_id": "app1"}}})),
        ]);

        let cli = AgentProcess { pid: 10, cwd: Some(PathBuf::from("/work/proj")), ..Default::default() };
        let host = AgentProcess {
            pid: 20, cwd: Some(PathBuf::from("/")), client: Client::Desktop,
            app_name: Some("Codex".into()), is_host: true, ..Default::default()
        };

        // CLI only: the desktop conversation is not shown
        let sessions = get_codex_sessions(dir.path(), &[cli.clone()]);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "cli1");
        assert_eq!(sessions[0].pid, 10);
        assert_eq!(sessions[0].project_name, "proj");
        assert_eq!(sessions[0].github_url.as_deref(), Some("https://github.com/me/repo"));
        assert!(!sessions[0].shared_process);

        // With the app running, its conversation appears too, with the subagent counted
        let mut sessions = get_codex_sessions(dir.path(), &[cli, host]);
        sessions.sort_by(|a, b| a.id.cmp(&b.id));
        let ids: Vec<_> = sessions.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, vec!["app1", "cli1"]);
        assert_eq!(sessions[0].client, Client::Desktop);
        assert_eq!(sessions[0].app_name.as_deref(), Some("Codex"));
        assert!(sessions[0].shared_process);
        assert_eq!(sessions[0].status, SessionStatus::Waiting);
        assert_eq!(sessions[0].last_message.as_deref(), Some("all set"));
        assert_eq!(sessions[0].active_subagent_count, 1);
    }
}
