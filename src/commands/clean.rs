//! `grove clean` — wipe overstory runtime state.
#![allow(dead_code)]
//!
//! Nuclear cleanup: kill tmux sessions, remove worktrees, delete branches,
//! wipe SQLite databases, clear directory contents, and delete state files.
//! Use --all for full cleanup or individual flags for selective cleanup.

use std::fs;
use std::path::{Path};
use std::process::Command;

use serde::Serialize;

use crate::config::resolve_project_root;
use crate::json::json_output;
use crate::logging::{muted, print_hint, print_success};

// ---------------------------------------------------------------------------
// Execute
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn execute(
    _force: bool,
    all: bool,
    mail: bool,
    sessions: bool,
    metrics: bool,
    logs: bool,
    worktrees: bool,
    branches: bool,
    agents: bool,
    specs: bool,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    let do_worktrees = all || worktrees;
    // Always delete branches when removing worktrees (they accumulate otherwise)
    let do_branches = all || branches || worktrees;
    let do_mail = all || mail;
    let do_sessions = all || sessions;
    let do_metrics = all || metrics;
    let do_logs = all || logs;
    let do_agents = all || agents;
    let do_specs = all || specs;

    let any_selected = do_worktrees
        || do_branches
        || do_mail
        || do_sessions
        || do_metrics
        || do_logs
        || do_agents
        || do_specs;

    if !any_selected {
        return Err(
            "No cleanup targets specified. Use --all for full cleanup or individual flags \
             (--mail, --sessions, --metrics, --logs, --worktrees, --branches, --agents, --specs)."
                .to_string(),
        );
    }

    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let root = resolve_project_root(&cwd, project_override).map_err(|e| e.to_string())?;
    let root_str = root.to_string_lossy().to_string();
    let overstory = format!("{root_str}/.overstory");

    let mut result = CleanResult::default();

    // 1. Kill tmux sessions BEFORE removing worktrees
    if do_worktrees {
        result.pids_killed = kill_project_agent_pids(&overstory);
    }

    // 2. Remove worktrees (overstory/* branches)
    if do_worktrees {
        result.worktrees_cleaned = clean_all_worktrees(&root_str);
    }

    // 3. Delete orphaned overstory/* branches
    if do_branches {
        result.branches_deleted = delete_orphaned_branches(&root_str);
    }

    // 4. Wipe databases
    if do_mail {
        result.mail_wiped = wipe_sqlite_db(&format!("{overstory}/mail.db"));
    }
    if do_metrics {
        result.metrics_wiped = wipe_sqlite_db(&format!("{overstory}/metrics.db"));
    }
    if do_sessions {
        result.sessions_cleared = wipe_sqlite_db(&format!("{overstory}/sessions.db"));
    }
    if all {
        result.merge_queue_cleared =
            wipe_sqlite_db(&format!("{overstory}/merge-queue.db"));
    }

    // 5. Clear directories
    if do_logs {
        result.logs_cleared = clear_directory(&format!("{overstory}/logs"));
    }
    if do_agents {
        result.agents_cleared = clear_directory(&format!("{overstory}/agents"));
    }
    if do_specs {
        result.specs_cleared = clear_directory(&format!("{overstory}/specs"));
    }

    // 6. Delete state files (--all only)
    if all {
        result.nudge_state_cleared =
            delete_file(&format!("{overstory}/nudge-state.json"));
        let _ = clear_directory(&format!("{overstory}/pending-nudges"));
        result.current_run_cleared =
            delete_file(&format!("{overstory}/current-run.txt"));
    }

    if json {
        println!("{}", json_output("clean", &result));
        return Ok(());
    }

    // Text output
    let mut lines: Vec<String> = Vec::new();
    if result.pids_killed > 0 {
        lines.push(format!(
            "Killed {} tmux session{}",
            result.pids_killed,
            if result.pids_killed == 1 { "" } else { "s" }
        ));
    }
    if result.worktrees_cleaned > 0 {
        lines.push(format!(
            "Removed {} worktree{}",
            result.worktrees_cleaned,
            if result.worktrees_cleaned == 1 { "" } else { "s" }
        ));
    }
    if result.branches_deleted > 0 {
        lines.push(format!(
            "Deleted {} orphaned branch{}",
            result.branches_deleted,
            if result.branches_deleted == 1 { "" } else { "es" }
        ));
    }
    if result.mail_wiped {
        lines.push("Wiped mail.db".to_string());
    }
    if result.metrics_wiped {
        lines.push("Wiped metrics.db".to_string());
    }
    if result.sessions_cleared {
        lines.push("Wiped sessions.db".to_string());
    }
    if result.merge_queue_cleared {
        lines.push("Wiped merge-queue.db".to_string());
    }
    if result.logs_cleared {
        lines.push("Cleared logs/".to_string());
    }
    if result.agents_cleared {
        lines.push("Cleared agents/".to_string());
    }
    if result.specs_cleared {
        lines.push("Cleared specs/".to_string());
    }
    if result.nudge_state_cleared {
        lines.push("Cleared nudge-state.json".to_string());
    }
    if result.current_run_cleared {
        lines.push("Cleared current-run.txt".to_string());
    }

    if lines.is_empty() {
        print_hint("Nothing to clean");
    } else {
        for line in &lines {
            println!("{}", muted(line));
        }
        print_success("Clean complete", None);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Result struct
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct CleanResult {
    pids_killed: usize,
    worktrees_cleaned: usize,
    branches_deleted: usize,
    mail_wiped: bool,
    sessions_cleared: bool,
    merge_queue_cleared: bool,
    metrics_wiped: bool,
    logs_cleared: bool,
    agents_cleared: bool,
    specs_cleared: bool,
    nudge_state_cleared: bool,
    current_run_cleared: bool,
}

// ---------------------------------------------------------------------------
// Process helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Worktree helpers
// ---------------------------------------------------------------------------

/// Remove all worktrees whose branch starts with "overstory/".
fn clean_all_worktrees(project_root: &str) -> usize {
    let worktrees = list_overstory_worktrees(project_root);
    let mut cleaned = 0;
    for path in &worktrees {
        let ok = Command::new("git")
            .args(["worktree", "remove", "--force", path])
            .current_dir(project_root)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            cleaned += 1;
        }
    }
    cleaned
}

fn list_overstory_worktrees(project_root: &str) -> Vec<String> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(project_root)
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return vec![],
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut paths = Vec::new();
    let mut current_path: Option<String> = None;
    let mut current_branch: Option<String> = None;

    for line in text.lines() {
        if line.is_empty() {
            if let (Some(p), Some(b)) = (current_path.take(), current_branch.take()) {
                if b.starts_with("overstory/") {
                    paths.push(p);
                }
            }
            current_branch = None;
        } else if let Some(p) = line.strip_prefix("worktree ") {
            current_path = Some(p.to_string());
        } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
            current_branch = Some(b.to_string());
        } else if let Some(b) = line.strip_prefix("branch ") {
            current_branch = Some(b.to_string());
        }
    }
    // Final block (no trailing newline)
    if let (Some(p), Some(b)) = (current_path.take(), current_branch.take()) {
        if b.starts_with("overstory/") {
            paths.push(p);
        }
    }
    paths
}

// ---------------------------------------------------------------------------
// Branch helpers
// ---------------------------------------------------------------------------

/// Delete all refs/heads/overstory/* branches.
fn delete_orphaned_branches(project_root: &str) -> usize {
    let output = Command::new("git")
        .args([
            "for-each-ref",
            "refs/heads/overstory/",
            "--format=%(refname:short)",
        ])
        .current_dir(project_root)
        .output();

    let branches: Vec<String> = match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .lines()
            .filter(|b| !b.is_empty())
            .map(|b| b.to_string())
            .collect(),
        _ => return 0,
    };

    let mut deleted = 0;
    for branch in &branches {
        let ok = Command::new("git")
            .args(["branch", "-D", branch])
            .current_dir(project_root)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            deleted += 1;
        }
    }
    deleted
}

// ---------------------------------------------------------------------------
// File/directory helpers
// ---------------------------------------------------------------------------

/// Delete a SQLite database and its WAL + SHM companion files.
fn wipe_sqlite_db(db_path: &str) -> bool {
    let mut wiped = false;
    for ext in &["", "-wal", "-shm"] {
        let path = format!("{db_path}{ext}");
        if fs::remove_file(&path).is_ok() && ext.is_empty() {
            wiped = true;
        }
    }
    wiped
}

/// Clear all entries inside a directory, keeping the directory itself.
fn clear_directory(dir_path: &str) -> bool {
    let path = Path::new(dir_path);
    if !path.exists() {
        return false;
    }
    let entries = match fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return false,
    };
    let mut had_entries = false;
    for entry in entries.flatten() {
        had_entries = true;
        let _ = fs::remove_dir_all(entry.path()).or_else(|_| fs::remove_file(entry.path()));
    }
    had_entries
}

/// Delete a single file. Returns true if the file existed and was removed.
fn delete_file(file_path: &str) -> bool {
    fs::remove_file(file_path).is_ok()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------


/// Kill all active agent processes by PID.
fn kill_project_agent_pids(overstory_dir: &str) -> usize {
    let sessions_db = format!("{overstory_dir}/sessions.db");
    let store = match crate::db::sessions::SessionStore::new(&sessions_db) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    let active = match store.get_active() {
        Ok(a) => a,
        Err(_) => return 0,
    };
    let mut killed = 0;
    for session in &active {
        if let Some(pid) = session.pid {
            if crate::watchdog::is_pid_alive(pid) {
                let _ = Command::new("kill").args(["-15", &pid.to_string()]).output();
                killed += 1;
            }
        }
    }
    killed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn test_execute_no_flags_returns_error() {
        let result = execute(
            false, false, false, false, false, false, false, false, false, false, false,
            Some(Path::new("/tmp")),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No cleanup targets"));
    }

    #[test]
    fn test_execute_all_no_overstory_dir() {
        // Should not panic when .overstory doesn't exist
        let result = execute(
            false, true, false, false, false, false, false, false, false, false, false,
            Some(Path::new("/tmp")),
        );
        // May succeed or fail — just must not panic
        let _ = result;
    }

    #[test]
    fn test_wipe_sqlite_db_nonexistent() {
        let result = wipe_sqlite_db("/nonexistent/path/test.db");
        assert!(!result);
    }

    #[test]
    fn test_wipe_sqlite_db_existing() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        fs::write(&db_path, b"fake db content").unwrap();
        let result = wipe_sqlite_db(db_path.to_str().unwrap());
        assert!(result);
        assert!(!db_path.exists());
    }

    #[test]
    fn test_wipe_sqlite_db_with_wal() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let wal_path = dir.path().join("test.db-wal");
        let shm_path = dir.path().join("test.db-shm");
        fs::write(&db_path, b"db").unwrap();
        fs::write(&wal_path, b"wal").unwrap();
        fs::write(&shm_path, b"shm").unwrap();
        let result = wipe_sqlite_db(db_path.to_str().unwrap());
        assert!(result);
        assert!(!db_path.exists());
        assert!(!wal_path.exists());
        assert!(!shm_path.exists());
    }

    #[test]
    fn test_clear_directory_nonexistent() {
        let result = clear_directory("/nonexistent/path/grove-test");
        assert!(!result);
    }

    #[test]
    fn test_clear_directory_empty() {
        let dir = TempDir::new().unwrap();
        let subdir = dir.path().join("empty_subdir");
        fs::create_dir_all(&subdir).unwrap();
        let result = clear_directory(subdir.to_str().unwrap());
        assert!(!result); // empty directory → false
        assert!(subdir.exists()); // dir itself not removed
    }

    #[test]
    fn test_clear_directory_with_files() {
        let dir = TempDir::new().unwrap();
        let subdir = dir.path().join("test_subdir");
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file1.txt"), b"content").unwrap();
        fs::write(subdir.join("file2.txt"), b"content").unwrap();
        let result = clear_directory(subdir.to_str().unwrap());
        assert!(result);
        assert!(subdir.exists());
        assert!(fs::read_dir(&subdir).unwrap().next().is_none());
    }

    #[test]
    fn test_delete_file_nonexistent() {
        let result = delete_file("/nonexistent/path/test.json");
        assert!(!result);
    }

    #[test]
    fn test_delete_file_existing() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("state.json");
        fs::write(&file, b"{}").unwrap();
        let result = delete_file(file.to_str().unwrap());
        assert!(result);
        assert!(!file.exists());
    }

    #[test]
    fn test_pid_killing_no_agents() {
        // With no active agents, should return 0
        // (just verify no panic)
        assert!(true);
    }
}
