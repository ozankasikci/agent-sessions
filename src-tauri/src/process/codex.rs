use log::debug;
use std::path::PathBuf;
use std::sync::Mutex;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System, UpdateKind};

/// Represents a running Codex CLI process
#[derive(Debug, Clone)]
pub struct CodexProcess {
    pub pid: u32,
    pub cwd: Option<PathBuf>,
    pub cpu_usage: f32,
}

// Reuse System instance to get accurate CPU readings (requires previous measurement)
static CODEX_SYSTEM: Mutex<Option<System>> = Mutex::new(None);

/// Find all running Codex CLI processes on the system
pub fn find_codex_processes() -> Vec<CodexProcess> {
    debug!("=== Starting Codex process discovery ===");

    let mut system_guard = CODEX_SYSTEM.lock().unwrap();

    // Initialize system if not already done
    let system = system_guard.get_or_insert_with(|| {
        debug!("Initializing new System instance for Codex");
        System::new_with_specifics(
            RefreshKind::new().with_processes(
                ProcessRefreshKind::new()
                    .with_cmd(UpdateKind::Always)
                    .with_cwd(UpdateKind::Always)
                    .with_cpu(),
            ),
        )
    });

    // Refresh process list
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        ProcessRefreshKind::new()
            .with_cmd(UpdateKind::Always)
            .with_cwd(UpdateKind::Always)
            .with_cpu(),
    );

    let mut processes = Vec::new();

    for (pid, process) in system.processes() {
        let cmd = process.cmd();

        // Check if first argument is "codex" or ends with "/codex"
        let is_codex = if let Some(first_arg) = cmd.first() {
            let first_arg_str = first_arg.to_string_lossy().to_lowercase();
            first_arg_str == "codex" || first_arg_str.ends_with("/codex")
        } else {
            false
        };

        if is_codex {
            let cwd = process.cwd().map(|p| p.to_path_buf());
            let cpu = process.cpu_usage();

            debug!(
                "Found Codex process: pid={}, cpu={:.1}%, cwd={:?}",
                pid.as_u32(),
                cpu,
                cwd
            );

            processes.push(CodexProcess {
                pid: pid.as_u32(),
                cwd,
                cpu_usage: cpu,
            });
        }
    }

    debug!(
        "Codex process discovery complete: found {} processes",
        processes.len()
    );
    processes
}
