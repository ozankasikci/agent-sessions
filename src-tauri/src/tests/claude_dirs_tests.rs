use crate::agent::AgentProcess;
use crate::session::claude_dirs::{
    encode_project_dir_name, expand_config_dir, find_session_jsonl, read_pid_session_info,
};
use crate::session::{get_sessions_internal, AgentType};
use std::fs;
use std::path::Path;

fn write_session(config_dir: &Path, cwd: &str, session_id: &str) -> std::path::PathBuf {
    let project_dir = config_dir.join("projects").join(encode_project_dir_name(cwd));
    fs::create_dir_all(&project_dir).unwrap();
    let path = project_dir.join(format!("{session_id}.jsonl"));
    fs::write(
        &path,
        format!(
            r#"{{"sessionId":"{session_id}","cwd":"{cwd}","type":"user","message":{{"role":"user","content":"hello from profile"}},"timestamp":"2024-01-01T00:00:00Z"}}"#
        ) + "\n",
    )
    .unwrap();
    path
}

fn write_pid_file(config_dir: &Path, pid: u32, session_id: &str, cwd: &str) {
    let sessions = config_dir.join("sessions");
    fs::create_dir_all(&sessions).unwrap();
    fs::write(
        sessions.join(format!("{pid}.json")),
        format!(r#"{{"pid":{pid},"sessionId":"{session_id}","cwd":"{cwd}","status":"busy"}}"#),
    )
    .unwrap();
}

#[test]
fn test_expand_config_dir() {
    let home = dirs::home_dir().unwrap();
    assert_eq!(expand_config_dir("~/.claude-work"), Some(home.join(".claude-work")));
    assert_eq!(expand_config_dir("/abs/dir"), Some("/abs/dir".into()));
    assert_eq!(expand_config_dir("  "), None);
}

#[test]
fn test_encode_project_dir_name() {
    assert_eq!(
        encode_project_dir_name("/Users/me/Projects/my_app/.worktree"),
        "-Users-me-Projects-my-app--worktree"
    );
}

#[test]
fn test_read_pid_session_info_rejects_mismatched_pid() {
    let dir = tempfile::tempdir().unwrap();
    write_pid_file(dir.path(), 42, "abc", "/tmp/x");
    assert_eq!(read_pid_session_info(dir.path(), 42).unwrap().session_id, "abc");
    // File named 43.json but claiming pid 42 is ignored
    fs::copy(dir.path().join("sessions/42.json"), dir.path().join("sessions/43.json")).unwrap();
    assert!(read_pid_session_info(dir.path(), 43).is_none());
    assert!(read_pid_session_info(dir.path(), 44).is_none());
}

#[test]
fn test_find_session_jsonl_falls_back_to_scanning() {
    let dir = tempfile::tempdir().unwrap();
    let path = write_session(dir.path(), "/some/cwd", "sess-1");
    assert_eq!(find_session_jsonl(dir.path(), Some("/some/cwd"), "sess-1"), Some(path.clone()));
    // Wrong cwd still finds it by scanning project dirs
    assert_eq!(find_session_jsonl(dir.path(), Some("/other"), "sess-1"), Some(path));
    assert!(find_session_jsonl(dir.path(), None, "missing").is_none());
}

#[test]
fn test_sessions_found_in_non_default_profile_via_pid_file() {
    let profile = tempfile::tempdir().unwrap();
    let cwd = "/nonexistent/profile-test/project";
    let pid = 4_000_001;
    // Older, unrelated session in the same project must not be picked
    write_session(profile.path(), cwd, "older-session");
    write_session(profile.path(), cwd, "wanted-session");
    write_pid_file(profile.path(), pid, "wanted-session", cwd);
    // Make the unrelated file the most recent one so heuristics would pick it
    let older = profile.path().join("projects").join(encode_project_dir_name(cwd)).join("older-session.jsonl");
    filetime::set_file_mtime(&older, filetime::FileTime::now()).unwrap();

    let processes = vec![AgentProcess {
        pid,
        cpu_usage: 0.0,
        cwd: Some(cwd.into()),
        config_dir: Some(profile.path().to_path_buf()),
        ..Default::default()
    }];
    let sessions = get_sessions_internal(&processes, AgentType::Claude);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "wanted-session");
    assert_eq!(sessions[0].pid, pid);
}

#[test]
fn test_sessions_found_in_non_default_profile_without_pid_file() {
    let profile = tempfile::tempdir().unwrap();
    let cwd = "/nonexistent/profile-test/heuristic";
    write_session(profile.path(), cwd, "heuristic-session");

    let processes = vec![AgentProcess {
        pid: 4_000_002,
        cpu_usage: 0.0,
        cwd: Some(cwd.into()),
        config_dir: Some(profile.path().to_path_buf()),
        ..Default::default()
    }];
    let sessions = get_sessions_internal(&processes, AgentType::Claude);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, "heuristic-session");
}

#[test]
fn test_profile_label() {
    use crate::session::claude_dirs::{config_dir_for_jsonl, profile_label};
    let home = dirs::home_dir().unwrap();
    assert_eq!(profile_label(&home.join(".claude")), "default");
    assert_eq!(profile_label(&home.join(".claude-celesta")), "celesta");
    assert_eq!(profile_label(Path::new("/opt/work-claude")), "work-claude");
    assert_eq!(
        config_dir_for_jsonl(Path::new("/x/.claude-a/projects/-p/s.jsonl")),
        Some("/x/.claude-a".into())
    );
    assert_eq!(config_dir_for_jsonl(Path::new("/x/other/-p/s.jsonl")), None);
}

#[test]
fn test_profile_label_set_when_multiple_profiles() {
    // A temp profile plus ~/.claude (or any discovered dir) means >1 profile
    let profile = tempfile::tempdir().unwrap();
    let cwd = "/nonexistent/profile-test/label";
    write_session(profile.path(), cwd, "label-session");
    let processes = vec![AgentProcess {
        pid: 4_000_003,
        cpu_usage: 0.0,
        cwd: Some(cwd.into()),
        config_dir: Some(profile.path().to_path_buf()),
        ..Default::default()
    }];
    let sessions = get_sessions_internal(&processes, AgentType::Claude);
    let expected = profile.path().file_name().unwrap().to_str().unwrap().trim_start_matches('.').to_string();
    if crate::session::claude_dirs::discover_claude_dirs().is_empty() {
        assert_eq!(sessions[0].profile, None);
    } else {
        assert_eq!(sessions[0].profile.as_deref(), Some(expected.as_str()));
    }
}
