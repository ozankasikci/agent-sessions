use serde::{Deserialize, Serialize};
use sysinfo::{ProcessRefreshKind, RefreshKind, System};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaudeProcess {
    pub pid: u32,
    pub cwd: Option<PathBuf>,
    pub cpu_usage: f32,
    pub memory: u64,
}

pub fn find_claude_processes() -> Vec<ClaudeProcess> {
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

    let mut processes = Vec::new();

    for (pid, process) in system.processes() {
        // Claude Code runs as a node process with "claude" as the first command argument
        let cmd = process.cmd();

        let is_claude = if let Some(first_arg) = cmd.first() {
            let first_arg_str = first_arg.to_string_lossy().to_lowercase();
            first_arg_str == "claude" || first_arg_str.ends_with("/claude")
        } else {
            false
        };

        // Exclude our own app
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
