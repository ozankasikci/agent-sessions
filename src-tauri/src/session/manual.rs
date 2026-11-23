use chrono::{DateTime, SecondsFormat, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use super::parser::parse_manual_claude_session;
use super::{AgentType, Session, SessionStatus};

const STORE_FILE_NAME: &str = "manual-sessions.json";
const MAX_CANDIDATES_PER_SOURCE: usize = 50;
const TAIL_BYTES: u64 = 512 * 1024;

static STORE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualSessionCandidate {
    pub id: String,
    pub agent_type: AgentType,
    pub project_name: String,
    pub project_path: String,
    pub last_message: Option<String>,
    pub last_message_role: Option<String>,
    pub last_activity_at: String,
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PinnedSession {
    pub file_path: String,
    pub agent_type: AgentType,
    pub session_id: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ManualSessionStore {
    #[serde(default)]
    sessions: Vec<PinnedSession>,
}

pub fn list_candidates(
    config_dir: &Path,
    live_sessions: &[Session],
) -> Result<Vec<ManualSessionCandidate>, String> {
    let home = dirs::home_dir().ok_or_else(|| "Could not determine home directory".to_string())?;
    let pinned = read_pins(config_dir)?;

    let live_keys: HashSet<(AgentType, String)> = live_sessions
        .iter()
        .map(|session| (session.agent_type.clone(), session.id.clone()))
        .collect();
    let pinned_keys: HashSet<(AgentType, String)> = pinned
        .iter()
        .map(|session| (session.agent_type.clone(), session.session_id.clone()))
        .collect();

    let mut candidates = Vec::new();
    let mut seen = HashSet::new();

    let claude_roots: Vec<PathBuf> = super::claude_dirs::discover_claude_dirs()
        .into_iter()
        .map(|dir| dir.join("projects"))
        .collect();
    for path in recent_session_files_in(&claude_roots, AgentType::Claude) {
        if let Some(session) = parse_manual_claude_session(&path) {
            let key = (AgentType::Claude, session.id.clone());
            if !live_keys.contains(&key) && !pinned_keys.contains(&key) && seen.insert(key) {
                candidates.push(candidate_from_session(session, path));
            }
        }
    }

    let codex_root = home.join(".codex").join("sessions");
    for path in recent_session_files(&codex_root, AgentType::Codex) {
        if let Some(session) = parse_codex_session_file(&path) {
            let key = (AgentType::Codex, session.id.clone());
            if !live_keys.contains(&key) && !pinned_keys.contains(&key) && seen.insert(key) {
                candidates.push(candidate_from_session(session, path));
            }
        }
    }

    candidates.sort_by(|a, b| {
        let a_timestamp = DateTime::parse_from_rfc3339(&a.last_activity_at).ok();
        let b_timestamp = DateTime::parse_from_rfc3339(&b.last_activity_at).ok();
        b_timestamp.cmp(&a_timestamp)
    });
    Ok(candidates)
}

pub fn add_manual_session(
    config_dir: &Path,
    file_path: &str,
    agent_type: AgentType,
) -> Result<(), String> {
    if matches!(agent_type, AgentType::OpenCode | AgentType::Antigravity) {
        return Err("Only Claude Code and Codex sessions can be pinned manually".to_string());
    }

    let canonical_path = validate_session_path(Path::new(file_path), &agent_type)?;
    let session = parse_session_for_agent(&canonical_path, &agent_type)
        .ok_or_else(|| "Could not parse the selected session file".to_string())?;

    add_pin_to_store(
        config_dir,
        PinnedSession {
            file_path: canonical_path.to_string_lossy().to_string(),
            agent_type,
            session_id: session.id,
        },
    )
}

pub fn remove_manual_session(
    config_dir: &Path,
    session_id: &str,
    agent_type: AgentType,
) -> Result<(), String> {
    remove_pin_from_store(config_dir, session_id, &agent_type)
}

pub fn load_pinned_sessions(config_dir: &Path) -> Result<Vec<Session>, String> {
    let pins = read_pins(config_dir)?;
    let mut sessions = Vec::new();
    let multiple_profiles = super::claude_dirs::discover_claude_dirs().len() > 1;

    for pin in pins {
        let path = PathBuf::from(&pin.file_path);
        match parse_session_for_agent(&path, &pin.agent_type) {
            Some(mut session) if session.id == pin.session_id => {
                if pin.agent_type == AgentType::Claude && multiple_profiles {
                    session.profile = super::claude_dirs::config_dir_for_jsonl(&path)
                        .map(|dir| super::claude_dirs::profile_label(&dir));
                }
                sessions.push(session)
            }
            Some(_) => log::warn!(
                "Pinned session ID no longer matches file contents: {}",
                pin.file_path
            ),
            None => log::warn!("Could not parse pinned session file: {}", pin.file_path),
        }
    }

    Ok(sessions)
}

fn candidate_from_session(session: Session, path: PathBuf) -> ManualSessionCandidate {
    ManualSessionCandidate {
        id: session.id,
        agent_type: session.agent_type,
        project_name: session.project_name,
        project_path: session.project_path,
        last_message: session.last_message,
        last_message_role: session.last_message_role,
        last_activity_at: session.last_activity_at,
        file_path: path.to_string_lossy().to_string(),
    }
}

fn parse_session_for_agent(path: &PathBuf, agent_type: &AgentType) -> Option<Session> {
    match agent_type {
        AgentType::Claude => parse_manual_claude_session(path),
        AgentType::Codex => parse_codex_session_file(path),
        AgentType::OpenCode | AgentType::Antigravity => None,
    }
}

fn validate_session_path(path: &Path, agent_type: &AgentType) -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or_else(|| "Could not determine home directory".to_string())?;
    let expected_roots: Vec<PathBuf> = match agent_type {
        AgentType::Claude => super::claude_dirs::discover_claude_dirs()
            .into_iter()
            .map(|dir| dir.join("projects"))
            .collect(),
        AgentType::Codex => vec![home.join(".codex").join("sessions")],
        AgentType::OpenCode | AgentType::Antigravity => {
            return Err("Only Claude Code and Codex sessions can be pinned manually".to_string())
        }
    };

    let canonical_roots: Vec<PathBuf> = expected_roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .collect();
    if canonical_roots.is_empty() {
        return Err("Session directory is unavailable".to_string());
    }
    let canonical_path = path
        .canonicalize()
        .map_err(|error| format!("Session file is unavailable: {error}"))?;

    if !canonical_roots.iter().any(|root| canonical_path.starts_with(root)) {
        return Err("Selected session file is outside the supported session directory".to_string());
    }

    if canonical_path
        .extension()
        .and_then(|extension| extension.to_str())
        != Some("jsonl")
    {
        return Err("Selected session file must be a JSONL file".to_string());
    }

    let file_name = canonical_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if *agent_type == AgentType::Claude && file_name.starts_with("agent-") {
        return Err("Claude subagent files cannot be pinned directly".to_string());
    }
    if *agent_type == AgentType::Codex && !file_name.starts_with("rollout-") {
        return Err("Selected file is not a Codex rollout".to_string());
    }

    Ok(canonical_path)
}

fn recent_session_files(root: &Path, agent_type: AgentType) -> Vec<PathBuf> {
    recent_session_files_in(&[root.to_path_buf()], agent_type)
}

fn recent_session_files_in(roots: &[PathBuf], agent_type: AgentType) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for root in roots {
        collect_session_files(root, &agent_type, &mut files);
    }
    files.sort_by(|a, b| b.1.cmp(&a.1));
    files
        .into_iter()
        .take(MAX_CANDIDATES_PER_SOURCE)
        .map(|(path, _)| path)
        .collect()
}

fn collect_session_files(
    directory: &Path,
    agent_type: &AgentType,
    files: &mut Vec<(PathBuf, SystemTime)>,
) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();
        if file_type.is_dir() {
            collect_session_files(&path, agent_type, files);
            continue;
        }
        if !file_type.is_file()
            || path.extension().and_then(|extension| extension.to_str()) != Some("jsonl")
        {
            continue;
        }

        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        let matches_source = match agent_type {
            AgentType::Claude => !file_name.starts_with("agent-"),
            AgentType::Codex => file_name.starts_with("rollout-"),
            AgentType::OpenCode | AgentType::Antigravity => false,
        };
        if !matches_source {
            continue;
        }

        if let Ok(modified) = entry.metadata().and_then(|metadata| metadata.modified()) {
            files.push((path, modified));
        }
    }
}

pub(crate) fn parse_codex_session_file(jsonl_path: &PathBuf) -> Option<Session> {
    let file = File::open(jsonl_path).ok()?;
    let mut reader = BufReader::new(file);
    let mut first_line = String::new();
    reader.read_line(&mut first_line).ok()?;
    let meta: Value = serde_json::from_str(&first_line).ok()?;

    if meta.get("type").and_then(Value::as_str) != Some("session_meta") {
        return None;
    }

    let payload = meta.get("payload")?;
    let session_id = payload.get("id")?.as_str()?.to_string();
    let project_path = payload.get("cwd")?.as_str()?.to_string();
    let project_name = Path::new(&project_path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("Unknown")
        .to_string();

    let mut last_activity_at = None;
    let mut last_message = None;
    let mut last_message_role = None;

    for line in read_tail_lines(jsonl_path).into_iter().rev() {
        let Ok(event) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        if last_activity_at.is_none() {
            last_activity_at = event
                .get("timestamp")
                .and_then(Value::as_str)
                .map(str::to_string);
        }

        if last_message.is_some()
            || event.get("type").and_then(Value::as_str) != Some("response_item")
        {
            continue;
        }

        let Some(message_payload) = event.get("payload") else {
            continue;
        };
        if message_payload.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }

        let text = message_payload
            .get("content")
            .and_then(Value::as_array)
            .and_then(|items| {
                items.iter().find_map(|item| {
                    let content_type = item.get("type").and_then(Value::as_str)?;
                    if content_type == "input_text" || content_type == "output_text" {
                        item.get("text")
                            .and_then(Value::as_str)
                            .filter(|text| !text.is_empty())
                    } else {
                        None
                    }
                })
            });

        if let Some(text) = text {
            last_message = Some(truncate_preview(text));
            last_message_role = message_payload
                .get("role")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
    }

    let fallback_timestamp = payload
        .get("timestamp")
        .and_then(Value::as_str)
        .or_else(|| meta.get("timestamp").and_then(Value::as_str))
        .map(str::to_string)
        .or_else(|| modified_timestamp(jsonl_path));

    let status = if was_modified_recently(jsonl_path) {
        SessionStatus::Processing
    } else if last_message_role.is_some() {
        SessionStatus::Waiting
    } else {
        SessionStatus::Idle
    };

    Some(Session {
        id: session_id,
        agent_type: AgentType::Codex,
        project_name,
        project_path,
        git_branch: None,
        github_url: None,
        status,
        last_message,
        last_message_role,
        last_activity_at: last_activity_at
            .or(fallback_timestamp)
            .unwrap_or_else(|| "Unknown".to_string()),
        pid: 0,
        cpu_usage: 0.0,
        active_subagent_count: 0,
        client: super::Client::Cli,
        app_name: None,
        shared_process: false,
        is_manual: true,
        profile: None,
    })
}

fn read_tail_lines(path: &Path) -> Vec<String> {
    let Ok(file) = File::open(path) else {
        return Vec::new();
    };
    let file_size = file
        .metadata()
        .ok()
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let mut reader = BufReader::new(file);

    if file_size > TAIL_BYTES {
        if reader.seek(SeekFrom::End(-(TAIL_BYTES as i64))).is_ok() {
            let mut partial_line = String::new();
            let _ = reader.read_line(&mut partial_line);
        }
    }

    reader.lines().map_while(Result::ok).collect()
}

fn truncate_preview(text: &str) -> String {
    if text.chars().count() > 100 {
        format!("{}...", text.chars().take(100).collect::<String>())
    } else {
        text.to_string()
    }
}

fn was_modified_recently(path: &Path) -> bool {
    path.metadata()
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .map(|age| age.as_secs_f32() < 3.0)
        .unwrap_or(false)
}

fn modified_timestamp(path: &Path) -> Option<String> {
    let modified = path
        .metadata()
        .and_then(|metadata| metadata.modified())
        .ok()?;
    Some(DateTime::<Utc>::from(modified).to_rfc3339_opts(SecondsFormat::Millis, true))
}

fn store_path(config_dir: &Path) -> PathBuf {
    config_dir.join(STORE_FILE_NAME)
}

fn load_store_unlocked(config_dir: &Path) -> Result<ManualSessionStore, String> {
    let path = store_path(config_dir);
    if !path.exists() {
        return Ok(ManualSessionStore::default());
    }

    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read manual session store: {error}"))?;
    serde_json::from_str(&contents)
        .map_err(|error| format!("Failed to parse manual session store: {error}"))
}

fn save_store_unlocked(config_dir: &Path, store: &ManualSessionStore) -> Result<(), String> {
    fs::create_dir_all(config_dir)
        .map_err(|error| format!("Failed to create app config directory: {error}"))?;
    let contents = serde_json::to_string_pretty(store)
        .map_err(|error| format!("Failed to serialize manual session store: {error}"))?;
    fs::write(store_path(config_dir), contents)
        .map_err(|error| format!("Failed to save manual session store: {error}"))
}

pub(crate) fn read_pins(config_dir: &Path) -> Result<Vec<PinnedSession>, String> {
    let _guard = STORE_LOCK
        .lock()
        .map_err(|_| "Manual session store lock is poisoned".to_string())?;
    Ok(load_store_unlocked(config_dir)?.sessions)
}

pub(crate) fn add_pin_to_store(config_dir: &Path, pin: PinnedSession) -> Result<(), String> {
    let _guard = STORE_LOCK
        .lock()
        .map_err(|_| "Manual session store lock is poisoned".to_string())?;
    let mut store = load_store_unlocked(config_dir)?;

    let already_pinned = store.sessions.iter().any(|existing| {
        existing.agent_type == pin.agent_type
            && (existing.session_id == pin.session_id || existing.file_path == pin.file_path)
    });
    if !already_pinned {
        store.sessions.push(pin);
        save_store_unlocked(config_dir, &store)?;
    }

    Ok(())
}

pub(crate) fn remove_pin_from_store(
    config_dir: &Path,
    session_id: &str,
    agent_type: &AgentType,
) -> Result<(), String> {
    let _guard = STORE_LOCK
        .lock()
        .map_err(|_| "Manual session store lock is poisoned".to_string())?;
    let mut store = load_store_unlocked(config_dir)?;
    let original_len = store.sessions.len();
    store
        .sessions
        .retain(|session| session.session_id != session_id || &session.agent_type != agent_type);

    if store.sessions.len() != original_len {
        save_store_unlocked(config_dir, &store)?;
    }

    Ok(())
}
