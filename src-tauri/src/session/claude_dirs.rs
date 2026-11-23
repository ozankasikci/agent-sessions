//! Discovery of Claude Code config directories ("profiles").
//!
//! Claude Code stores its data in `~/.claude` by default, but users can run
//! several profiles side by side via `CLAUDE_CONFIG_DIR` (e.g. an alias like
//! `CLAUDE_CONFIG_DIR=~/.claude-work claude`). Each profile has its own
//! `projects/` tree of JSONL transcripts and a `sessions/<pid>.json` file per
//! running process that maps the pid to its session id.

use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

/// The default Claude config directory (`~/.claude`).
pub fn default_claude_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".claude"))
}

/// Expand a leading `~` in a `CLAUDE_CONFIG_DIR` value.
pub fn expand_config_dir(value: &str) -> Option<PathBuf> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if value == "~" {
        return dirs::home_dir();
    }
    if let Some(rest) = value.strip_prefix("~/") {
        return dirs::home_dir().map(|home| home.join(rest));
    }
    Some(PathBuf::from(value))
}

/// Find all Claude config directories we know about: `~/.claude`, any
/// `~/.claude*` directory that contains a `projects/` folder, and this app's
/// own `CLAUDE_CONFIG_DIR` if set.
pub fn discover_claude_dirs() -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    let mut push = |path: PathBuf| {
        if path.join("projects").is_dir() && !found.contains(&path) {
            found.push(path);
        }
    };

    if let Some(default_dir) = default_claude_dir() {
        push(default_dir);
    }

    if let Some(env_dir) = std::env::var("CLAUDE_CONFIG_DIR")
        .ok()
        .and_then(|value| expand_config_dir(&value))
    {
        push(env_dir);
    }

    if let Some(home) = dirs::home_dir() {
        if let Ok(entries) = fs::read_dir(&home) {
            let mut extra: Vec<PathBuf> = entries
                .flatten()
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_str()
                        .map(|name| name.starts_with(".claude"))
                        .unwrap_or(false)
                })
                .map(|entry| entry.path())
                .filter(|path| path.is_dir())
                .collect();
            extra.sort();
            for path in extra {
                push(path);
            }
        }
    }

    found
}

/// Contents of `<config_dir>/sessions/<pid>.json`, written by Claude Code for
/// each running interactive process.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PidSessionInfo {
    pub pid: u32,
    pub session_id: String,
    pub cwd: Option<String>,
}

/// Read the pid file for `pid` in `config_dir`, if present and well-formed.
pub fn read_pid_session_info(config_dir: &Path, pid: u32) -> Option<PidSessionInfo> {
    let path = config_dir.join("sessions").join(format!("{pid}.json"));
    let contents = fs::read_to_string(path).ok()?;
    let info: PidSessionInfo = serde_json::from_str(&contents).ok()?;
    (info.pid == pid && !info.session_id.is_empty()).then_some(info)
}

/// Encode a cwd the way Claude Code names its project directories: every
/// character that is not ASCII alphanumeric becomes `-`.
pub fn encode_project_dir_name(cwd: &str) -> String {
    cwd.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Locate `<session_id>.jsonl` inside `<config_dir>/projects`. Tries the
/// encoded cwd directory first, then falls back to checking every project
/// directory (encoding rules have changed across Claude Code versions).
pub fn find_session_jsonl(config_dir: &Path, cwd: Option<&str>, session_id: &str) -> Option<PathBuf> {
    let projects = config_dir.join("projects");
    let file_name = format!("{session_id}.jsonl");

    if let Some(cwd) = cwd {
        let candidate = projects.join(encode_project_dir_name(cwd)).join(&file_name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }

    fs::read_dir(&projects)
        .ok()?
        .flatten()
        .map(|entry| entry.path().join(&file_name))
        .find(|candidate| candidate.is_file())
}

/// Human-readable label for a profile dir: `~/.claude` -> "default",
/// `~/.claude-work` -> "work", anything else -> its folder name.
pub fn profile_label(config_dir: &Path) -> String {
    if default_claude_dir().as_deref() == Some(config_dir) {
        return "default".to_string();
    }
    let name = config_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    name.strip_prefix(".claude-")
        .or_else(|| name.strip_prefix(".claude_"))
        .or_else(|| name.strip_prefix('.'))
        .filter(|label| !label.is_empty())
        .unwrap_or(name)
        .to_string()
}

/// Find the profile dir a JSONL transcript belongs to
/// (`<config_dir>/projects/<project>/<file>.jsonl`).
pub fn config_dir_for_jsonl(jsonl_path: &Path) -> Option<PathBuf> {
    let projects = jsonl_path.parent()?.parent()?;
    (projects.file_name()? == "projects").then(|| projects.parent().map(Path::to_path_buf))?
}
