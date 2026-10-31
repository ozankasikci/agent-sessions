//! Small helpers shared by the JSONL-based detectors (Codex, Antigravity).

use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;
use std::time::{Duration, SystemTime};

/// How far back from the end of a log file to read when looking for the current state
pub const TAIL_BYTES: u64 = 512 * 1024;

/// Sessions hosted by a long-lived app process (Codex app-server, Antigravity app)
/// are listed when their log was written within this window.
pub const HOST_ACTIVE_WINDOW: Duration = Duration::from_secs(30 * 60);

/// A hosted session still marked as working after this long without a write is
/// assumed to have been interrupted (e.g. the app was quit mid-turn).
pub const HOST_STALE_AFTER: Duration = Duration::from_secs(10 * 60);

/// Read the last `TAIL_BYTES` of a file as lines (oldest first)
pub fn read_tail_lines(path: &Path) -> Vec<String> {
    let Ok(file) = File::open(path) else { return Vec::new() };
    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
    let mut reader = BufReader::new(file);
    if size > TAIL_BYTES {
        if reader.seek(SeekFrom::End(-(TAIL_BYTES as i64))).is_err() {
            return Vec::new();
        }
        // Discard the partial line we landed in
        let mut partial = Vec::new();
        let _ = reader.read_until(b'\n', &mut partial);
    }
    reader
        .split(b'\n')
        .flatten()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .filter(|l| !l.trim().is_empty())
        .collect()
}

/// Read the first line of a file
pub fn read_first_line(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    if line.trim().is_empty() {
        None
    } else {
        Some(line)
    }
}

pub fn modified_time(path: &Path) -> Option<SystemTime> {
    path.metadata().and_then(|m| m.modified()).ok()
}

/// Age of a file's last modification
pub fn file_age(path: &Path) -> Option<Duration> {
    modified_time(path).and_then(|m| SystemTime::now().duration_since(m).ok())
}

/// Format a SystemTime as the ISO-8601 string the frontend expects
pub fn system_time_to_iso(time: SystemTime) -> String {
    let dt: chrono::DateTime<chrono::Utc> = time.into();
    dt.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// Truncate a message for the card preview, on a char boundary
pub fn truncate_message(text: &str, max_chars: usize) -> String {
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() > max_chars {
        format!("{}...", collapsed.chars().take(max_chars).collect::<String>())
    } else {
        collapsed
    }
}

/// Last path component, used as the project name
pub fn project_name_from_path(path: &str) -> String {
    path.split('/')
        .filter(|s| !s.is_empty())
        .last()
        .unwrap_or("Unknown")
        .to_string()
}

/// True when `cwd` is `root` or inside it
pub fn path_within(cwd: &str, root: &str) -> bool {
    let root = root.trim_end_matches('/');
    !root.is_empty() && (cwd == root || cwd.starts_with(&format!("{}/", root)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn truncates_on_char_boundary() {
        assert_eq!(truncate_message("héllo wörld", 5), "héllo...");
        assert_eq!(truncate_message("short", 10), "short");
        assert_eq!(truncate_message("a\n  b", 10), "a b");
    }

    #[test]
    fn path_within_checks_boundaries() {
        assert!(path_within("/a/b", "/a/b"));
        assert!(path_within("/a/b/c", "/a/b/"));
        assert!(!path_within("/a/bc", "/a/b"));
        assert!(!path_within("/a", ""));
    }

    #[test]
    fn tail_skips_partial_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.jsonl");
        let mut f = File::create(&path).unwrap();
        let filler = "x".repeat(1000);
        for i in 0..1000 {
            writeln!(f, "{{\"i\":{},\"pad\":\"{}\"}}", i, filler).unwrap();
        }
        drop(f);
        let lines = read_tail_lines(&path);
        assert!(!lines.is_empty());
        for line in &lines {
            assert!(serde_json::from_str::<serde_json::Value>(line).is_ok());
        }
        assert!(lines.last().unwrap().contains("\"i\":999"));
    }
}
