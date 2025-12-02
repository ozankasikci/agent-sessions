use sysinfo::System;
use std::collections::HashMap;
use std::path::PathBuf;
use std::fs;

fn main() {
    println!("=== DEBUG: Finding Claude Processes (fixed) ===\n");

    let mut system = System::new_all();
    system.refresh_all();

    let mut claude_processes: Vec<(u32, Option<PathBuf>, f32)> = Vec::new();

    for (pid, process) in system.processes() {
        let cmd = process.cmd();

        let is_claude = if let Some(first_arg) = cmd.first() {
            let first_arg_str = first_arg.to_string_lossy().to_lowercase();
            first_arg_str == "claude" || first_arg_str.ends_with("/claude")
        } else {
            false
        };

        let is_our_app = process.name().to_string_lossy().contains("claude-sessions")
            || process.name().to_string_lossy().contains("tauri-temp");

        if is_claude && !is_our_app {
            let cwd = process.cwd().map(|p| p.to_path_buf());
            println!("Found Claude process:");
            println!("  PID: {}", pid.as_u32());
            println!("  CWD: {:?}", cwd);
            println!("  CPU: {}%", process.cpu_usage());
            println!();

            claude_processes.push((pid.as_u32(), cwd, process.cpu_usage()));
        }
    }

    println!("Total Claude processes found: {}\n", claude_processes.len());

    // Build cwd -> process map
    let mut cwd_to_pid: HashMap<String, u32> = HashMap::new();
    for (pid, cwd, _) in &claude_processes {
        if let Some(cwd) = cwd {
            let cwd_str = cwd.to_string_lossy().to_string();
            cwd_to_pid.insert(cwd_str.clone(), *pid);
        }
    }

    println!("=== CWD -> PID Mapping ===\n");
    for (cwd, pid) in &cwd_to_pid {
        println!("{} -> PID {}", cwd, pid);
    }

    println!("\n=== DEBUG: Checking Project Matches ===\n");

    let claude_dir = dirs::home_dir()
        .map(|h| h.join(".claude").join("projects"))
        .unwrap_or_default();

    if let Ok(entries) = fs::read_dir(&claude_dir) {
        for entry in entries.flatten().take(15) {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let dir_name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");

            let project_path = convert_dir_name_to_path(dir_name);

            if let Some(pid) = cwd_to_pid.get(&project_path) {
                println!("MATCH: {} -> {}", dir_name, project_path);
                println!("       PID: {}", pid);
            }
        }
    }
}

fn convert_dir_name_to_path(dir_name: &str) -> String {
    let name = dir_name.strip_prefix('-').unwrap_or(dir_name);
    let parts: Vec<&str> = name.split('-').collect();

    if parts.is_empty() {
        return String::new();
    }

    let projects_idx = parts.iter().position(|&p| p == "Projects" || p == "UnityProjects");

    if let Some(idx) = projects_idx {
        let path_parts = &parts[..=idx];
        let project_parts = &parts[idx + 1..];

        let mut path = String::from("/");
        path.push_str(&path_parts.join("/"));

        if !project_parts.is_empty() {
            path.push('/');
            path.push_str(&project_parts.join("-"));
        }

        path
    } else {
        format!("/{}", name.replace('-', "/"))
    }
}
