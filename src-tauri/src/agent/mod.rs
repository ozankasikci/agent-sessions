pub mod claude;
pub mod opencode;

use crate::session::{Session, SessionsResponse, AgentType};
use std::sync::Mutex;
use sysinfo::{ProcessRefreshKind, RefreshKind, System};

/// Common process info shared across agent types
#[derive(Debug, Clone)]
pub struct AgentProcess {
    pub pid: u32,
    pub cpu_usage: f32,
    pub cwd: Option<std::path::PathBuf>,
}

// Shared System instance — refreshed once per poll cycle, used by all detectors.
// Previously each detector maintained its own System, causing two full process table
// scans per poll. Now we scan once and share the snapshot.
static SHARED_SYSTEM: Mutex<Option<System>> = Mutex::new(None);

/// Trait for detecting and parsing agent sessions
pub trait AgentDetector: Send + Sync {
    /// Human-readable name of the agent
    fn name(&self) -> &'static str;

    /// The agent type for tagging sessions
    fn agent_type(&self) -> AgentType;

    /// Find running processes for this agent using the shared system snapshot
    fn find_processes(&self, system: &System) -> Vec<AgentProcess>;

    /// Parse sessions from data files, matched to running processes
    fn find_sessions(&self, processes: &[AgentProcess]) -> Vec<Session>;
}

/// Get all sessions from all registered agent detectors
pub fn get_all_sessions() -> SessionsResponse {
    use std::collections::HashSet;
    use crate::session::{status_sort_priority, cleanup_stale_status_entries};

    let detectors: Vec<Box<dyn AgentDetector>> = vec![
        Box::new(claude::ClaudeDetector),
        Box::new(opencode::OpenCodeDetector),
    ];

    // Phase 1: Refresh shared system once, discover all processes
    let all_processes: Vec<Vec<AgentProcess>> = {
        let mut system_guard = SHARED_SYSTEM.lock().unwrap();
        let system = system_guard.get_or_insert_with(|| {
            log::debug!("Initializing shared System instance");
            System::new_with_specifics(
                RefreshKind::new().with_processes(
                    ProcessRefreshKind::new()
                        .with_cmd(sysinfo::UpdateKind::Always)
                        .with_cwd(sysinfo::UpdateKind::Always)
                        .with_cpu()
                        .with_memory()
                )
            )
        });
        system.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::All,
            ProcessRefreshKind::new()
                .with_cmd(sysinfo::UpdateKind::Always)
                .with_cwd(sysinfo::UpdateKind::Always)
                .with_cpu()
                .with_memory()
        );

        detectors.iter().map(|d| d.find_processes(system)).collect()
    }; // System lock released here — file I/O below runs without holding it

    // Phase 2: Find sessions (file I/O, git subprocesses — no system lock held)
    let mut all_sessions = Vec::new();
    for (detector, processes) in detectors.iter().zip(all_processes.iter()) {
        let sessions = detector.find_sessions(processes);
        log::info!("{}: found {} processes, {} sessions",
            detector.name(), processes.len(), sessions.len());
        all_sessions.extend(sessions);
    }

    // Clean up stale status tracking entries for sessions that no longer exist
    let active_ids: HashSet<String> = all_sessions.iter().map(|s| s.id.clone()).collect();
    cleanup_stale_status_entries(&active_ids);

    // Sort by status priority first, then by most recent activity
    all_sessions.sort_by(|a, b| {
        let priority_a = status_sort_priority(&a.status);
        let priority_b = status_sort_priority(&b.status);

        if priority_a != priority_b {
            priority_a.cmp(&priority_b)
        } else {
            b.last_activity_at.cmp(&a.last_activity_at)
        }
    });

    let waiting_count = all_sessions.iter()
        .filter(|s| matches!(s.status, crate::session::SessionStatus::Waiting))
        .count();
    let total_count = all_sessions.len();

    SessionsResponse {
        sessions: all_sessions,
        total_count,
        waiting_count,
    }
}
