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
        let name = process.name().to_string_lossy().to_lowercase();

        if name.contains("claude") && !name.contains("claude-sessions") {
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
