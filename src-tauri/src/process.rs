use serde::{Deserialize, Serialize};
use sysinfo::System;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeProcess {
    pub pid: u32,
    pub cwd: Option<PathBuf>,
    pub cpu_usage: f32,
    pub memory: u64,
}

pub fn find_claude_processes() -> Vec<ClaudeProcess> {
    let mut system = System::new_all();
    system.refresh_all();

    let mut processes = Vec::new();

    for (pid, process) in system.processes() {
        // Claude Code runs as a node process with "claude" as the first command argument
        // We need to check the command line, not the process name
        let cmd = process.cmd();

        // Check if first argument is "claude" or contains "claude" in the command
        let is_claude = if let Some(first_arg) = cmd.first() {
            let first_arg_str = first_arg.to_string_lossy().to_lowercase();
            // Match "claude" as standalone command (not claude-sessions or other variants)
            first_arg_str == "claude" || first_arg_str.ends_with("/claude")
        } else {
            false
        };

        // Also exclude our own app
        let is_our_app = process.name().to_string_lossy().contains("claude-sessions")
            || process.name().to_string_lossy().contains("tauri-temp");

        if is_claude && !is_our_app {
            let cwd = process.cwd().map(|p| p.to_path_buf());

            processes.push(ClaudeProcess {
                pid: pid.as_u32(),
                cwd,
                cpu_usage: process.cpu_usage(),
                memory: process.memory(),
            });
        }
    }

    processes
}

pub fn get_process_cpu_usage(pid: u32) -> Option<f32> {
    let mut system = System::new_all();
    system.refresh_all();

    let pid = sysinfo::Pid::from_u32(pid);
    system.process(pid).map(|p| p.cpu_usage())
}
