use crate::process::{find_claude_processes, is_orphaned_process, ClaudeProcess};
use std::path::PathBuf;
use sysinfo::{ProcessRefreshKind, RefreshKind, System};

#[test]
fn test_claude_process_creation() {
    let process = ClaudeProcess {
        pid: 12345,
        cwd: Some(PathBuf::from("/Users/test/Projects/my-project")),
        cpu_usage: 5.5,
        memory: 1024,
    };

    assert_eq!(process.pid, 12345);
    assert_eq!(
        process.cwd,
        Some(PathBuf::from("/Users/test/Projects/my-project"))
    );
    assert_eq!(process.cpu_usage, 5.5);
    assert_eq!(process.memory, 1024);
}

#[test]
fn test_claude_process_without_cwd() {
    let process = ClaudeProcess {
        pid: 99999,
        cwd: None,
        cpu_usage: 0.0,
        memory: 0,
    };

    assert_eq!(process.pid, 99999);
    assert!(process.cwd.is_none());
}

#[test]
fn test_claude_process_clone() {
    let process = ClaudeProcess {
        pid: 12345,
        cwd: Some(PathBuf::from("/test/path")),
        cpu_usage: 10.0,
        memory: 2048,
    };

    let cloned = process.clone();
    assert_eq!(process.pid, cloned.pid);
    assert_eq!(process.cwd, cloned.cwd);
    assert_eq!(process.cpu_usage, cloned.cpu_usage);
    assert_eq!(process.memory, cloned.memory);
}

#[test]
fn test_claude_process_serialization() {
    let process = ClaudeProcess {
        pid: 12345,
        cwd: Some(PathBuf::from("/test/path")),
        cpu_usage: 5.5,
        memory: 1024,
    };

    let json = serde_json::to_string(&process).unwrap();
    assert!(json.contains("12345"));
    assert!(json.contains("5.5"));
}

#[test]
fn test_find_claude_processes_returns_vec() {
    // This test just ensures the function runs without panicking
    // In a real environment, it may or may not find Claude processes
    let mut system = System::new_with_specifics(
        RefreshKind::new().with_processes(
            ProcessRefreshKind::new()
                .with_cmd(sysinfo::UpdateKind::Always)
                .with_cwd(sysinfo::UpdateKind::Always)
                .with_cpu()
                .with_memory()
        )
    );
    system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        ProcessRefreshKind::new()
            .with_cmd(sysinfo::UpdateKind::Always)
            .with_cwd(sysinfo::UpdateKind::Always)
            .with_cpu()
            .with_memory()
    );
    let processes = find_claude_processes(&system);
    // Should return a Vec (possibly empty) - just verify we got a result
    let _ = processes.len();
}

#[test]
fn test_find_claude_processes_excludes_orphans() {
    // Run process discovery and verify no orphaned processes are returned
    // An orphaned process has its parent shell reparented to PID 1 (launchd)
    let mut system = System::new_with_specifics(
        RefreshKind::new().with_processes(
            ProcessRefreshKind::new()
                .with_cmd(sysinfo::UpdateKind::Always)
                .with_cwd(sysinfo::UpdateKind::Always)
                .with_cpu()
                .with_memory()
        )
    );
    system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        ProcessRefreshKind::new()
            .with_cmd(sysinfo::UpdateKind::Always)
            .with_cwd(sysinfo::UpdateKind::Always)
            .with_cpu()
            .with_memory()
    );

    let processes = find_claude_processes(&system);

    // Verify that every returned process is NOT orphaned
    for cp in &processes {
        let pid = sysinfo::Pid::from_u32(cp.pid);
        if let Some(process) = system.process(pid) {
            assert!(
                !is_orphaned_process(&system, process),
                "Process pid={} should not be orphaned but was returned by find_claude_processes",
                cp.pid
            );
        }
    }
}

#[test]
fn test_is_orphaned_process_with_current_process() {
    // The current test process should NOT be orphaned since it's running in a terminal
    let mut system = System::new_with_specifics(
        RefreshKind::new().with_processes(
            ProcessRefreshKind::new()
                .with_cmd(sysinfo::UpdateKind::Always)
                .with_cwd(sysinfo::UpdateKind::Always)
                .with_cpu()
        )
    );
    system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        ProcessRefreshKind::new()
            .with_cmd(sysinfo::UpdateKind::Always)
            .with_cwd(sysinfo::UpdateKind::Always)
            .with_cpu()
    );

    let current_pid = sysinfo::Pid::from_u32(std::process::id());
    if let Some(process) = system.process(current_pid) {
        // The test runner process should not be orphaned
        assert!(
            !is_orphaned_process(&system, process),
            "Current test process should not be detected as orphaned"
        );
    }
}

#[test]
fn test_is_orphaned_process_with_launchd() {
    // PID 1 (launchd) itself should not cause panics
    let mut system = System::new_with_specifics(
        RefreshKind::new().with_processes(
            ProcessRefreshKind::new()
                .with_cmd(sysinfo::UpdateKind::Always)
                .with_cwd(sysinfo::UpdateKind::Always)
                .with_cpu()
        )
    );
    system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        ProcessRefreshKind::new()
            .with_cmd(sysinfo::UpdateKind::Always)
            .with_cwd(sysinfo::UpdateKind::Always)
            .with_cpu()
    );

    // launchd (PID 1) has no parent or parent is 0 - test shouldn't panic
    let pid1 = sysinfo::Pid::from_u32(1);
    if let Some(process) = system.process(pid1) {
        // Just verify the function doesn't panic
        let _ = is_orphaned_process(&system, process);
    }
}
