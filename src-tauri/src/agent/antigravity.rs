//! Google Antigravity sessions: the `agy` CLI and the Antigravity desktop app.
//!
//! The CLI keeps its data under `$ANTIGRAVITY_HOME` (default `~/.gemini/antigravity-cli`):
//! - `brain/<conversation>/.system_generated/logs/transcript.jsonl`: one JSON record
//!   per step, `{type, content, created_at, source, status, step_index, tool_calls}`,
//!   where `type` is USER_INPUT, PLANNER_RESPONSE, GENERIC (tool output),
//!   SYSTEM_MESSAGE or ERROR_MESSAGE.
//! - `history.jsonl`: `{conversationId, workspace}` lines mapping conversations to folders.
//! - `annotations/<conversation>.pbtxt`: text protobuf with the conversation `title`.
//!
//! The desktop app uses `~/.gemini/antigravity` with the same `brain/` layout. Its
//! conversation store itself is an undocumented binary format, so for the app we read
//! the same transcript files when present and otherwise fall back to the brain
//! folder's artifacts (task.md etc.) and their modification times. The format is not
//! publicly specified and may change between releases.

use super::common::{
    file_age, modified_time, path_within, project_name_from_path, read_tail_lines,
    system_time_to_iso, truncate_message, HOST_ACTIVE_WINDOW, HOST_STALE_AFTER,
};
use super::host::{detect_host, find_app_process};
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

pub struct AntigravityDetector;

impl AgentDetector for AntigravityDetector {
    fn name(&self) -> &'static str {
        "Antigravity"
    }

    fn agent_type(&self) -> AgentType {
        AgentType::Antigravity
    }

    fn find_processes(&self, system: &sysinfo::System) -> Vec<AgentProcess> {
        find_antigravity_processes(system)
    }

    fn find_sessions(&self, processes: &[AgentProcess]) -> Vec<Session> {
        if processes.is_empty() {
            return Vec::new();
        }
        let (cli_root, app_root) = match roots() {
            Some(r) => r,
            None => return Vec::new(),
        };
        get_antigravity_sessions(&cli_root, &app_root, processes)
    }
}

/// (CLI data root, desktop app data root)
fn roots() -> Option<(PathBuf, PathBuf)> {
    let home = dirs::home_dir()?;
    let cli = std::env::var("ANTIGRAVITY_HOME")
        .ok()
        .filter(|h| !h.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".gemini").join("antigravity-cli"));
    Some((cli, home.join(".gemini").join("antigravity")))
}

pub fn is_agy_command(name: &str, cmd: &[String]) -> bool {
    if name == "agy" {
        return true;
    }
    cmd.first()
        .map(|f| f == "agy" || f.ends_with("/agy"))
        .unwrap_or(false)
}

fn find_antigravity_processes(system: &sysinfo::System) -> Vec<AgentProcess> {
    let agy_pids: HashSet<sysinfo::Pid> = system
        .processes()
        .iter()
        .filter(|(_, p)| {
            let cmd: Vec<String> = p.cmd().iter().map(|s| s.to_string_lossy().to_string()).collect();
            is_agy_command(&p.name().to_string_lossy(), &cmd)
        })
        .map(|(pid, _)| *pid)
        .collect();

    let mut processes = Vec::new();
    for pid in &agy_pids {
        let Some(process) = system.process(*pid) else { continue };
        if process.parent().map(|pp| agy_pids.contains(&pp)).unwrap_or(false) {
            continue; // helper spawned by agy
        }
        let host = detect_host(system, *pid);
        if host.client == Client::Cli && is_orphaned_process(system, process) {
            continue;
        }
        processes.push(AgentProcess {
            pid: pid.as_u32(),
            cpu_usage: process.cpu_usage(),
            cwd: process.cwd().map(|p| p.to_path_buf()),
            client: host.client,
            app_name: host.app_name,
            is_host: false,
        });
    }

    // The desktop app hosts all of its conversations in one process tree
    if let Some((pid, process)) = find_app_process(system, "Antigravity") {
        processes.push(AgentProcess {
            pid: pid.as_u32(),
            cpu_usage: process.cpu_usage(),
            cwd: None,
            client: Client::Desktop,
            app_name: Some("Antigravity".to_string()),
            is_host: true,
        });
    }
    processes
}

/// What the conversation is doing, from its transcript
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptState {
    pub status: SessionStatus,
    pub last_message: Option<String>,
    pub last_role: Option<String>,
    pub last_timestamp: Option<String>,
    /// Conversation was spawned by another conversation (a subagent)
    pub is_subagent: bool,
}

/// `<USER_REQUEST>text</USER_REQUEST>` -> `text`; plain content is returned as is
fn user_request_text(content: &str) -> Option<String> {
    let text = match (content.find("<USER_REQUEST>"), content.find("</USER_REQUEST>")) {
        (Some(start), Some(end)) if end > start => &content[start + "<USER_REQUEST>".len()..end],
        (Some(_), _) => return None,
        _ if content.trim_start().starts_with('<') => return None, // settings change etc.
        _ => content,
    };
    let text = text.trim();
    if text.is_empty() { None } else { Some(text.to_string()) }
}

fn is_subagent_marker(content: &str) -> bool {
    // A subagent's first step is a system message naming the parent: "sender=<uuid>"
    content
        .find("sender=")
        .map(|i| {
            let id = &content[i + "sender=".len()..];
            id.len() >= 36 && id[..36].chars().all(|c| c.is_ascii_hexdigit() || c == '-')
        })
        .unwrap_or(false)
}

/// Derive status and preview from transcript lines (oldest first)
pub fn parse_transcript_state(lines: &[String]) -> TranscriptState {
    let mut status = SessionStatus::Waiting;
    let mut last_message = None;
    let mut last_role = None;
    let mut last_timestamp = None;
    let mut is_subagent = false;
    let mut pending_tool = false;

    for line in lines {
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        if let Some(ts) = v.get("created_at").or_else(|| v.get("timestamp")).and_then(|t| t.as_str()) {
            last_timestamp = Some(ts.to_string());
        }
        let content = v.get("content").and_then(|c| c.as_str()).unwrap_or("");
        match v.get("type").and_then(|t| t.as_str()).unwrap_or("") {
            "USER_INPUT" => {
                pending_tool = false;
                if let Some(text) = user_request_text(content) {
                    last_message = Some(text);
                    last_role = Some("user".to_string());
                    status = SessionStatus::Thinking;
                }
            }
            "SYSTEM_MESSAGE" => {
                if v.get("step_index").and_then(|i| i.as_u64()) == Some(0) && is_subagent_marker(content) {
                    is_subagent = true;
                }
                status = SessionStatus::Thinking;
            }
            "PLANNER_RESPONSE" => {
                if v.get("status").and_then(|s| s.as_str()) == Some("ERROR") {
                    status = SessionStatus::Waiting;
                    continue;
                }
                let calls: Vec<&str> = v
                    .get("tool_calls")
                    .and_then(|c| c.as_array())
                    .map(|a| a.iter().filter_map(|c| c.get("name").and_then(|n| n.as_str())).collect())
                    .unwrap_or_default();
                let text = content.trim();
                pending_tool = !calls.is_empty();
                if !text.is_empty() {
                    last_message = Some(text.to_string());
                    last_role = Some("assistant".to_string());
                }
                status = if calls.iter().any(|c| *c == "ask_question") {
                    SessionStatus::Waiting
                } else if !calls.is_empty() {
                    SessionStatus::Processing
                } else if !text.is_empty() {
                    SessionStatus::Waiting // final reply, turn is over
                } else {
                    SessionStatus::Thinking // thinking-only step
                };
            }
            "GENERIC" => {
                // Tool output (or the user's answer to ask_question) came back;
                // the model works on it next
                if pending_tool {
                    status = SessionStatus::Thinking;
                    pending_tool = false;
                }
            }
            "ERROR_MESSAGE" => status = SessionStatus::Waiting,
            _ => {}
        }
    }

    TranscriptState {
        status,
        last_message: last_message.map(|m| truncate_message(&m, 100)),
        last_role,
        last_timestamp,
        is_subagent,
    }
}

/// One conversation folder under `brain/`
#[derive(Debug, Clone)]
struct Conversation {
    id: String,
    /// transcript.jsonl when present, else the newest file in the brain folder
    log: PathBuf,
    has_transcript: bool,
    mtime: SystemTime,
}

fn newest_file_in(dir: &Path, depth: usize) -> Option<(PathBuf, SystemTime)> {
    let mut best: Option<(PathBuf, SystemTime)> = None;
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        let Ok(meta) = entry.metadata() else { continue };
        let candidate = if meta.is_dir() {
            if depth == 0 { None } else { newest_file_in(&path, depth - 1) }
        } else {
            meta.modified().ok().map(|m| (path.clone(), m))
        };
        if let Some((p, m)) = candidate {
            if best.as_ref().map(|(_, b)| m > *b).unwrap_or(true) {
                best = Some((p, m));
            }
        }
    }
    best
}

/// Conversations modified within `window`, newest first
fn list_conversations(root: &Path, window: Duration, allow_fallback: bool) -> Vec<Conversation> {
    let cutoff = SystemTime::now().checked_sub(window).unwrap_or(SystemTime::UNIX_EPOCH);
    let mut out = Vec::new();
    for entry in fs::read_dir(root.join("brain")).into_iter().flatten().flatten() {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Some(id) = dir.file_name().and_then(|n| n.to_str()).map(String::from) else { continue };
        let transcript = dir.join(".system_generated").join("logs").join("transcript.jsonl");
        let found = match modified_time(&transcript) {
            Some(m) => Some((transcript, m, true)),
            None if allow_fallback => newest_file_in(&dir, 3).map(|(p, m)| (p, m, false)),
            None => None,
        };
        if let Some((log, mtime, has_transcript)) = found {
            if mtime >= cutoff {
                out.push(Conversation { id, log, has_transcript, mtime });
            }
        }
    }
    out.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    out
}

/// conversationId -> workspace folder, from history.jsonl (cached by mtime)
static WORKSPACES: Lazy<Mutex<HashMap<PathBuf, (SystemTime, HashMap<String, String>)>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn load_workspaces(root: &Path) -> HashMap<String, String> {
    let path = root.join("history.jsonl");
    let Some(mtime) = modified_time(&path) else { return HashMap::new() };
    if let Some((cached_mtime, map)) = WORKSPACES.lock().unwrap().get(&path) {
        if *cached_mtime == mtime {
            return map.clone();
        }
    }
    let mut map = HashMap::new();
    for line in read_tail_lines(&path) {
        if let Ok(v) = serde_json::from_str::<Value>(&line) {
            let id = v.get("conversationId").and_then(|x| x.as_str());
            let ws = v.get("workspace").and_then(|x| x.as_str());
            if let (Some(id), Some(ws)) = (id, ws) {
                let ws = ws.strip_prefix("file://").unwrap_or(ws);
                map.insert(id.to_string(), ws.to_string());
            }
        }
    }
    WORKSPACES.lock().unwrap().insert(path, (mtime, map.clone()));
    map
}

/// Conversation title from `annotations/<id>.pbtxt` (`title: "..."`)
fn load_title(root: &Path, id: &str) -> Option<String> {
    let raw = fs::read_to_string(root.join("annotations").join(format!("{}.pbtxt", id))).ok()?;
    parse_pbtxt_title(&raw)
}

pub fn parse_pbtxt_title(raw: &str) -> Option<String> {
    let start = raw.find("title:")? + "title:".len();
    let rest = raw[start..].trim_start().strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            }
            '"' => break,
            c => out.push(c),
        }
    }
    if out.trim().is_empty() { None } else { Some(out) }
}

/// First markdown heading / line of an artifact like task.md, as a preview
fn artifact_preview(path: &Path) -> Option<String> {
    if path.extension().map(|e| e != "md").unwrap_or(true) {
        return None;
    }
    let text = fs::read_to_string(path).ok()?;
    text.lines()
        .map(|l| l.trim().trim_start_matches('#').trim())
        .find(|l| !l.is_empty())
        .map(|l| truncate_message(l, 100))
}

fn build_session(
    root: &Path,
    conv: &Conversation,
    state: TranscriptState,
    workspace: Option<&str>,
    process: &AgentProcess,
) -> Session {
    let age = file_age(&conv.log).unwrap_or_default();
    let mut status = state.status;
    if process.is_host
        && age > HOST_STALE_AFTER
        && matches!(status, SessionStatus::Thinking | SessionStatus::Processing)
    {
        status = SessionStatus::Waiting;
    }

    let title = load_title(root, &conv.id);
    let project_path = workspace
        .map(String::from)
        .or_else(|| process.cwd.as_ref().map(|c| c.to_string_lossy().to_string()))
        .unwrap_or_default();
    let project_name = if project_path.is_empty() || project_path == "/" {
        title.clone().unwrap_or_else(|| "Antigravity".to_string())
    } else {
        project_name_from_path(&project_path)
    };
    let github_url = if !project_path.is_empty() && Path::new(&project_path).is_dir() {
        get_github_url(&project_path)
    } else {
        None
    };

    Session {
        id: conv.id.clone(),
        agent_type: AgentType::Antigravity,
        project_name,
        project_path,
        git_branch: None,
        github_url,
        status,
        last_message: state.last_message.or(title),
        last_message_role: state.last_role,
        last_activity_at: state
            .last_timestamp
            .unwrap_or_else(|| system_time_to_iso(conv.mtime)),
        pid: process.pid,
        cpu_usage: process.cpu_usage,
        active_subagent_count: 0,
        client: process.client,
        app_name: process.app_name.clone(),
        shared_process: process.is_host,
    }
}

fn conversation_state(conv: &Conversation) -> TranscriptState {
    if conv.has_transcript {
        return parse_transcript_state(&read_tail_lines(&conv.log));
    }
    // No transcript (desktop app): judge activity by how recently the brain folder changed
    let recent = file_age(&conv.log).map(|a| a < Duration::from_secs(15)).unwrap_or(false);
    TranscriptState {
        status: if recent { SessionStatus::Processing } else { SessionStatus::Waiting },
        last_message: artifact_preview(&conv.log),
        last_role: None,
        last_timestamp: None,
        is_subagent: false,
    }
}

/// Rollouts older than this are not considered when matching a CLI process
const CLI_MATCH_WINDOW: Duration = Duration::from_secs(30 * 24 * 60 * 60);

fn get_antigravity_sessions(cli_root: &Path, app_root: &Path, processes: &[AgentProcess]) -> Vec<Session> {
    let mut sessions = Vec::new();

    // --- agy CLI: match each process to the newest conversation in its folder ---
    let cli_procs: Vec<&AgentProcess> = processes.iter().filter(|p| !p.is_host).collect();
    if !cli_procs.is_empty() && cli_root.exists() {
        let conversations = list_conversations(cli_root, CLI_MATCH_WINDOW, false);
        let workspaces = load_workspaces(cli_root);
        let mut claimed: HashSet<String> = HashSet::new();
        let mut unmatched: Vec<&AgentProcess> = Vec::new();

        for process in &cli_procs {
            let cwd = process.cwd.as_ref().map(|c| c.to_string_lossy().to_string()).unwrap_or_default();
            let found = conversations.iter().find(|c| {
                !claimed.contains(&c.id)
                    && workspaces.get(&c.id).map(|ws| path_within(&cwd, ws) || path_within(ws, &cwd)).unwrap_or(false)
            });
            match found {
                Some(conv) => {
                    claimed.insert(conv.id.clone());
                    let state = conversation_state(conv);
                    let ws = workspaces.get(&conv.id).map(|s| s.as_str());
                    sessions.push(build_session(cli_root, conv, state, ws, process));
                }
                None => unmatched.push(process),
            }
        }

        // No folder information (history.jsonl missing or not yet written):
        // fall back to the newest top-level conversations not already shown.
        for process in unmatched {
            let next = conversations.iter().find(|c| {
                !claimed.contains(&c.id) && !workspaces.contains_key(&c.id)
            });
            let Some(conv) = next else { continue };
            let state = conversation_state(conv);
            if state.is_subagent {
                claimed.insert(conv.id.clone());
                continue;
            }
            claimed.insert(conv.id.clone());
            sessions.push(build_session(cli_root, conv, state, None, process));
        }
    }

    // --- Desktop app: conversations active within the window ---
    if let Some(host) = processes.iter().find(|p| p.is_host) {
        if app_root.exists() {
            let workspaces = load_workspaces(app_root);
            for conv in list_conversations(app_root, HOST_ACTIVE_WINDOW, true) {
                let state = conversation_state(&conv);
                if state.is_subagent {
                    continue;
                }
                let ws = workspaces.get(&conv.id).cloned();
                sessions.push(build_session(app_root, &conv, state, ws.as_deref(), host));
            }
        }
    }

    sessions
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn l(v: Value) -> String {
        v.to_string()
    }

    #[test]
    fn transcript_lifecycle() {
        let mut lines = vec![l(json!({
            "type": "USER_INPUT", "step_index": 0, "created_at": "2026-10-31T10:00:00Z",
            "content": "<USER_REQUEST>\nOptimize the queries\n</USER_REQUEST>\n<USER_SETTINGS_CHANGE>x</USER_SETTINGS_CHANGE>"
        }))];
        let s = parse_transcript_state(&lines);
        assert_eq!(s.status, SessionStatus::Thinking);
        assert_eq!(s.last_message.as_deref(), Some("Optimize the queries"));
        assert_eq!(s.last_role.as_deref(), Some("user"));

        lines.push(l(json!({"type": "PLANNER_RESPONSE", "created_at": "t2", "thinking": "hmm"})));
        assert_eq!(parse_transcript_state(&lines).status, SessionStatus::Thinking);

        lines.push(l(json!({"type": "PLANNER_RESPONSE", "created_at": "t3",
            "tool_calls": [{"name": "run_command", "args": {"CommandLine": "npm test"}}]})));
        assert_eq!(parse_transcript_state(&lines).status, SessionStatus::Processing);

        lines.push(l(json!({"type": "GENERIC", "created_at": "t4", "content": "ok"})));
        assert_eq!(parse_transcript_state(&lines).status, SessionStatus::Thinking);

        lines.push(l(json!({"type": "PLANNER_RESPONSE", "created_at": "t5", "content": "Queries are faster now."})));
        let s = parse_transcript_state(&lines);
        assert_eq!(s.status, SessionStatus::Waiting);
        assert_eq!(s.last_message.as_deref(), Some("Queries are faster now."));
        assert_eq!(s.last_role.as_deref(), Some("assistant"));
        assert_eq!(s.last_timestamp.as_deref(), Some("t5"));
        assert!(!s.is_subagent);
    }

    #[test]
    fn transcript_waiting_on_question_and_error() {
        let lines = vec![l(json!({"type": "PLANNER_RESPONSE", "tool_calls": [{"name": "ask_question"}]}))];
        assert_eq!(parse_transcript_state(&lines).status, SessionStatus::Waiting);

        let lines = vec![
            l(json!({"type": "USER_INPUT", "content": "go"})),
            l(json!({"type": "PLANNER_RESPONSE", "status": "ERROR", "content": "quota"})),
        ];
        assert_eq!(parse_transcript_state(&lines).status, SessionStatus::Waiting);
    }

    #[test]
    fn detects_subagent() {
        let lines = vec![l(json!({
            "type": "SYSTEM_MESSAGE", "step_index": 0,
            "content": "<SYSTEM_MESSAGE>sender=12345678-1234-1234-1234-123456789abc content=do it</SYSTEM_MESSAGE>"
        }))];
        assert!(parse_transcript_state(&lines).is_subagent);
    }

    #[test]
    fn pbtxt_title() {
        assert_eq!(parse_pbtxt_title("title: \"Fix \\\"login\\\" bug\"\nother: 1").as_deref(), Some("Fix \"login\" bug"));
        assert_eq!(parse_pbtxt_title("nothing here"), None);
    }

    #[test]
    fn matches_cli_process_by_workspace() {
        let dir = tempfile::tempdir().unwrap();
        let cli_root = dir.path().join("cli");
        let app_root = dir.path().join("app");
        let write_transcript = |root: &Path, id: &str, lines: &[String]| {
            let logs = root.join("brain").join(id).join(".system_generated").join("logs");
            fs::create_dir_all(&logs).unwrap();
            let mut f = fs::File::create(logs.join("transcript.jsonl")).unwrap();
            for line in lines {
                writeln!(f, "{}", line).unwrap();
            }
        };
        write_transcript(&cli_root, "conv-a", &[l(json!({"type": "USER_INPUT", "content": "hello a"}))]);
        write_transcript(&cli_root, "conv-b", &[l(json!({"type": "PLANNER_RESPONSE", "content": "done b"}))]);
        fs::write(
            cli_root.join("history.jsonl"),
            format!("{}\n{}\n",
                l(json!({"conversationId": "conv-a", "workspace": "/work/a"})),
                l(json!({"conversationId": "conv-b", "workspace": "/work/b"}))),
        ).unwrap();
        fs::create_dir_all(cli_root.join("annotations")).unwrap();
        fs::write(cli_root.join("annotations").join("conv-b.pbtxt"), "title: \"B task\"\n").unwrap();

        // Desktop app conversation without a transcript, only an artifact
        let brain = app_root.join("brain").join("conv-app");
        fs::create_dir_all(&brain).unwrap();
        fs::write(brain.join("task.md"), "# Refactor the router\n- [ ] step").unwrap();

        let proc_b = AgentProcess { pid: 2, cwd: Some(PathBuf::from("/work/b")), ..Default::default() };
        let sessions = get_antigravity_sessions(&cli_root, &app_root, &[proc_b.clone()]);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, "conv-b");
        assert_eq!(sessions[0].project_name, "b");
        assert_eq!(sessions[0].status, SessionStatus::Waiting);
        assert_eq!(sessions[0].last_message.as_deref(), Some("done b"));

        let host = AgentProcess {
            pid: 9, client: Client::Desktop, app_name: Some("Antigravity".into()), is_host: true,
            ..Default::default()
        };
        let sessions = get_antigravity_sessions(&cli_root, &app_root, &[proc_b, host]);
        assert_eq!(sessions.len(), 2);
        let app = sessions.iter().find(|s| s.id == "conv-app").unwrap();
        assert_eq!(app.client, Client::Desktop);
        assert!(app.shared_process);
        assert_eq!(app.project_name, "Antigravity");
        assert_eq!(app.last_message.as_deref(), Some("Refactor the router"));
        assert_eq!(app.status, SessionStatus::Processing); // just written
    }
}
