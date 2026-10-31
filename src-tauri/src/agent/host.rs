//! Shared helpers for working out which app hosts an agent process.
//!
//! Terminal sessions have a shell and a terminal emulator above them. Desktop apps
//! (Claude, Codex, Antigravity) and IDE extensions spawn agents directly, with no
//! TTY, and their main process is parented to launchd. Walking the ancestor chain
//! tells the two apart.

use crate::session::Client;
use sysinfo::{Pid, Process, System};

/// Maximum number of ancestors to inspect (guards against cycles in stale snapshots)
const MAX_DEPTH: usize = 16;

/// Desktop apps: (process name or bundle executable name, app name to activate)
const DESKTOP_APPS: &[(&str, &str)] = &[
    ("Claude", "Claude"),
    ("Codex", "Codex"),
    ("ChatGPT", "ChatGPT"),
];

/// Editors whose extensions spawn agents (VS Code family, Electron forks, Zed)
const IDE_MARKERS: &[(&str, &str)] = &[
    ("Code Helper", "Visual Studio Code"),
    // Antigravity is a VS Code fork; agents started from its terminal or extensions
    // are IDE sessions. Its own agent conversations are reported by the Antigravity
    // detector as Desktop.
    ("Antigravity", "Antigravity"),
    ("Code - Insiders", "Visual Studio Code - Insiders"),
    ("Cursor", "Cursor"),
    ("Windsurf", "Windsurf"),
    ("VSCodium", "VSCodium"),
    ("Zed", "Zed"),
];

/// How an agent process is hosted
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostInfo {
    pub client: Client,
    pub app_name: Option<String>,
}

impl HostInfo {
    pub fn cli() -> Self {
        Self { client: Client::Cli, app_name: None }
    }
}

/// Name of the `.app` bundle an executable path lives in, if any.
/// e.g. "/Applications/Claude.app/Contents/MacOS/Claude" -> "Claude"
pub fn app_bundle_name(path: &str) -> Option<&str> {
    let idx = path.find(".app/")?;
    let before = &path[..idx];
    Some(before.rsplit('/').next().unwrap_or(before))
}

/// Classify a single process name / executable path
pub fn classify_name(name: &str, exe: &str) -> Option<HostInfo> {
    let bundle = app_bundle_name(exe);
    for (marker, app) in DESKTOP_APPS {
        // Electron helpers are named "Claude Helper", "Codex Helper (Renderer)" etc.
        let name_matches = name == *marker || name.starts_with(&format!("{} Helper", marker));
        if name_matches || bundle == Some(*marker) {
            return Some(HostInfo { client: Client::Desktop, app_name: Some(app.to_string()) });
        }
    }
    for (marker, app) in IDE_MARKERS {
        if name.starts_with(marker) || bundle.map(|b| b.starts_with(marker)).unwrap_or(false) {
            return Some(HostInfo { client: Client::Ide, app_name: Some(app.to_string()) });
        }
    }
    if bundle.map(|b| b.starts_with("Visual Studio Code")).unwrap_or(false) {
        return Some(HostInfo { client: Client::Ide, app_name: Some("Visual Studio Code".to_string()) });
    }
    None
}

fn process_exe(process: &Process) -> String {
    process
        .cmd()
        .first()
        .map(|s| s.to_string_lossy().to_string())
        .or_else(|| process.exe().map(|p| p.to_string_lossy().to_string()))
        .unwrap_or_default()
}

/// Walk up the parent chain of `pid` and report the first desktop app or IDE found.
/// Returns CLI when no such ancestor exists (normal terminal session).
pub fn detect_host(system: &System, pid: Pid) -> HostInfo {
    let mut current = system.process(pid).and_then(|p| p.parent());
    for _ in 0..MAX_DEPTH {
        let Some(ppid) = current else { break };
        if ppid.as_u32() <= 1 {
            break;
        }
        let Some(parent) = system.process(ppid) else { break };
        let name = parent.name().to_string_lossy().to_string();
        if let Some(info) = classify_name(&name, &process_exe(parent)) {
            return info;
        }
        current = parent.parent();
    }
    HostInfo::cli()
}

/// Find a running desktop app's main process by its name (e.g. "Codex", "Antigravity")
pub fn find_app_process<'a>(system: &'a System, app: &str) -> Option<(&'a Pid, &'a Process)> {
    system.processes().iter().find(|(_, p)| {
        let name = p.name().to_string_lossy();
        if name == app {
            return true;
        }
        // Electron apps' main executable may be named "Electron"; helpers live in
        // nested bundles under Contents/Frameworks, so this matches the main process only
        process_exe(p).contains(&format!("/{}.app/Contents/MacOS/", app))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_name_extraction() {
        assert_eq!(app_bundle_name("/Applications/Claude.app/Contents/MacOS/Claude"), Some("Claude"));
        assert_eq!(
            app_bundle_name("/Applications/Codex.app/Contents/Resources/codex"),
            Some("Codex")
        );
        assert_eq!(app_bundle_name("/usr/local/bin/claude"), None);
    }

    #[test]
    fn classify_desktop_apps() {
        let info = classify_name("Claude", "/Applications/Claude.app/Contents/MacOS/Claude").unwrap();
        assert_eq!(info.client, Client::Desktop);
        assert_eq!(info.app_name.as_deref(), Some("Claude"));

        let info = classify_name("Codex Helper (Renderer)", "").unwrap();
        assert_eq!(info.app_name.as_deref(), Some("Codex"));

        let info = classify_name("Code Helper (Plugin)", "").unwrap();
        assert_eq!(info.client, Client::Ide);
    }

    #[test]
    fn classify_terminals_as_none() {
        assert!(classify_name("zsh", "/bin/zsh").is_none());
        assert!(classify_name("iTerm2", "/Applications/iTerm.app/Contents/MacOS/iTerm2").is_none());
        assert!(classify_name("tmux", "tmux").is_none());
        // A process merely called "claude" (the CLI) is not the desktop app
        assert!(classify_name("claude", "/usr/local/bin/claude").is_none());
    }
}
