use log::{debug, trace, warn};
use serde::{Deserialize, Serialize};
use sysinfo::System;
use std::path::PathBuf;

use crate::agent::host::detect_host;
use crate::session::Client;

/// Represents a running Claude Code process
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ClaudeProcess {
    pub pid: u32,
    pub cwd: Option<PathBuf>,
    pub cpu_usage: f32,
    pub memory: u64,
    /// Terminal CLI, Claude desktop app, or IDE extension
    #[serde(default)]
    pub client: Client,
    /// App to activate when focusing a session without a terminal
    #[serde(default)]
    pub app_name: Option<String>,
}

/// Check whether a command line is a Claude Code agent process.
///
/// Matches the `claude` CLI (native binary or npm shim) and Claude Code run through
/// node (`.../claude-code/cli.js`, `.../claude-agent-sdk/cli.js`), which is how
/// desktop apps and SDK hosts usually start it. Excludes the Claude desktop app's
/// own main executable (`Claude.app/Contents/MacOS/Claude`), which also ends in `/claude`
/// when lowercased.
pub fn is_claude_command(cmd: &[String]) -> bool {
    let Some(first) = cmd.first() else { return false };
    if first.contains(".app/Contents/MacOS/") {
        return false;
    }
    let first_lower = first.to_lowercase();
    if first_lower == "claude" || first_lower.ends_with("/claude") {
        return true;
    }
    // node .../@anthropic-ai/claude-code/cli.js [args]
    let is_node = first_lower == "node" || first_lower.ends_with("/node") || first_lower.ends_with("/bun");
    is_node
        && cmd.get(1).map(|arg| {
            let a = arg.to_lowercase();
            a.ends_with("/cli.js") && (a.contains("claude-code") || a.contains("claude-agent-sdk"))
        }).unwrap_or(false)
}

/// Check if a process is orphaned by examining its parent chain.
/// A process is considered orphaned if its parent shell has been reparented
/// to launchd/init (PID 1), indicating the original terminal was closed.
///
/// Parent chain for healthy sessions: claude → shell → terminal_emulator
/// Parent chain for orphaned sessions: claude → shell (PPID=1) → launchd
pub fn is_orphaned_process(system: &System, process: &sysinfo::Process) -> bool {
    let parent_pid = match process.parent() {
        Some(pid) => pid,
        None => return true, // No parent at all - definitely orphaned
    };

    // If parent is PID 1 directly, process is orphaned
    if parent_pid.as_u32() == 1 {
        return true;
    }

    // Check grandparent - if parent's parent is PID 1, terminal was closed
    // and the parent shell was reparented to launchd/init
    if let Some(parent_process) = system.process(parent_pid) {
        if let Some(grandparent_pid) = parent_process.parent() {
            if grandparent_pid.as_u32() == 1 {
                return true;
            }
        }
    } else {
        // Parent process doesn't exist in the system - orphaned
        return true;
    }

    false
}

/// Find all running Claude Code processes using the shared system snapshot.
/// Filters out sub-agent processes (whose parent is also a Claude process)
/// and orphaned processes (whose terminal has been closed).
pub fn find_claude_processes(system: &System) -> Vec<ClaudeProcess> {
    use std::collections::HashSet;
    use sysinfo::Pid;

    debug!("=== Starting process discovery ===");

    let total_processes = system.processes().len();
    trace!("Total system processes: {}", total_processes);

    // First pass: collect all Claude process PIDs
    let mut claude_pids: HashSet<Pid> = HashSet::new();
    for (pid, process) in system.processes() {
        let cmd: Vec<String> = process.cmd().iter().map(|s| s.to_string_lossy().to_string()).collect();
        if is_claude_command(&cmd) {
            claude_pids.insert(*pid);
        }
    }

    let mut processes = Vec::new();

    // Second pass: collect Claude processes, excluding sub-agents and orphans
    for (pid, process) in system.processes() {
        let cmd: Vec<String> = process.cmd().iter().map(|s| s.to_string_lossy().to_string()).collect();
        let process_name = process.name().to_string_lossy();

        let is_claude = is_claude_command(&cmd);

        // Exclude our own app
        let is_our_app = process_name.contains("claude-sessions")
            || process_name.contains("tauri-temp")
            || process_name.contains("agent-sessions");

        if is_claude {
            let cwd = process.cwd().map(|p| p.to_path_buf());

            if is_our_app {
                trace!("Skipping our own app: pid={}, name={}", pid.as_u32(), process_name);
                continue;
            }

            // Check if parent is also a Claude process (indicates sub-agent)
            if let Some(parent_pid) = process.parent() {
                if claude_pids.contains(&parent_pid) {
                    debug!(
                        "Skipping sub-agent process: pid={}, parent_pid={}, cwd={:?}",
                        pid.as_u32(),
                        parent_pid.as_u32(),
                        cwd
                    );
                    continue;
                }

                // Check if parent is Zed's external agent (claude-code-acp)
                // These are auto-spawned by Zed and not user-initiated terminal sessions
                if let Some(parent_process) = system.process(parent_pid) {
                    let parent_cmd: String = parent_process.cmd()
                        .iter()
                        .map(|s| s.to_string_lossy())
                        .collect::<Vec<_>>()
                        .join(" ");
                    if parent_cmd.contains("claude-code-acp") {
                        debug!(
                            "Skipping Zed external agent: pid={}, parent_pid={}, cwd={:?}",
                            pid.as_u32(),
                            parent_pid.as_u32(),
                            cwd
                        );
                        continue;
                    }
                }
            }

            // Sessions started by the Claude desktop app or an IDE extension have no
            // terminal: the app's main process is parented to launchd, so the orphan
            // check below would wrongly drop them.
            let host = detect_host(system, *pid);

            // Check if process is orphaned (terminal was closed)
            if host.client == Client::Cli && is_orphaned_process(system, process) {
                warn!(
                    "Skipping orphaned process: pid={}, cwd={:?}, cpu={:.1}% (parent shell reparented to launchd)",
                    pid.as_u32(),
                    cwd,
                    process.cpu_usage()
                );
                continue;
            }

            debug!(
                "Found Claude process: pid={}, host={:?}, cwd={:?}, cpu={:.1}%, mem={}MB",
                pid.as_u32(),
                host.client,
                cwd,
                process.cpu_usage(),
                process.memory() / 1024 / 1024
            );

            processes.push(ClaudeProcess {
                pid: pid.as_u32(),
                cwd,
                cpu_usage: process.cpu_usage(),
                memory: process.memory(),
                client: host.client,
                app_name: host.app_name,
            });
        }
    }

    debug!("Process discovery complete: found {} Claude processes (excluding sub-agents and orphans)", processes.len());
    processes
}
