use log::{debug, trace};
use serde::{Deserialize, Serialize};
use sysinfo::{ProcessRefreshKind, RefreshKind, System};
use std::path::PathBuf;

/// Represents a running Claude Code process
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ClaudeProcess {
    pub pid: u32,
    pub cwd: Option<PathBuf>,
    pub cpu_usage: f32,
    pub memory: u64,
}

/// Find all running Claude Code processes on the system
pub fn find_claude_processes() -> Vec<ClaudeProcess> {
    debug!("=== Starting process discovery ===");

    // Refresh process info - use Always to detect newly spawned processes
    let mut system = System::new_with_specifics(
        RefreshKind::new().with_processes(
            ProcessRefreshKind::new()
                .with_cmd(sysinfo::UpdateKind::Always)
                .with_cwd(sysinfo::UpdateKind::Always)
                .with_cpu()
                .with_memory()
        )
    );
    system.refresh_processes(sysinfo::ProcessesToUpdate::All);

    let total_processes = system.processes().len();
    trace!("Total system processes: {}", total_processes);

    let mut processes = Vec::new();

    for (pid, process) in system.processes() {
        // Claude Code runs as a node process with "claude" as the first command argument
        let cmd = process.cmd();
        let process_name = process.name().to_string_lossy();

        let is_claude = if let Some(first_arg) = cmd.first() {
            let first_arg_str = first_arg.to_string_lossy().to_lowercase();
            first_arg_str == "claude" || first_arg_str.ends_with("/claude")
        } else {
            false
        };

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

            debug!(
                "Found Claude process: pid={}, cwd={:?}, cpu={:.1}%, mem={}MB",
                pid.as_u32(),
                cwd,
                process.cpu_usage(),
                process.memory() / 1024 / 1024
            );

            processes.push(ClaudeProcess {
                pid: pid.as_u32(),
                cwd,
                cpu_usage: process.cpu_usage(),
                memory: process.memory(),
            });
        }
    }

    debug!("Process discovery complete: found {} Claude processes", processes.len());
    processes
}
