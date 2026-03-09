//! `grove stop` — terminate a running agent.
#![allow(dead_code)]
//!
//! Kills the agent's tmux session or process, marks it as completed in
//! SessionStore, and optionally removes its worktree and branch.

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

use crate::config::resolve_project_root;
use crate::db::sessions::SessionStore;
use crate::json::json_output;
use crate::logging::brand_bold;
use crate::types::AgentState;

// ---------------------------------------------------------------------------
// Execute
// ---------------------------------------------------------------------------

pub fn execute(
    agent_name: &str,
    force: bool,
    clean_worktree: bool,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    if agent_name.trim().is_empty() {
        return Err("Missing required argument: <agent-name>".to_string());
    }

    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let root = resolve_project_root(&cwd, project_override).map_err(|e| e.to_string())?;
    let root_str = root.to_string_lossy().to_string();
    let sessions_db = format!("{root_str}/.overstory/sessions.db");

    if !PathBuf::from(&sessions_db).exists() {
        return Err(format!(
            "Agent \"{agent_name}\" not found: sessions.db does not exist"
        ));
    }

    let store = SessionStore::new(&sessions_db).map_err(|e| e.to_string())?;
    let session = store
        .get_by_name(agent_name)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Agent \"{agent_name}\" not found"))?;

    let is_already_completed = session.state == AgentState::Completed;
    let is_zombie = session.state == AgentState::Zombie;

    // Completed agents without --clean-worktree: bail with helpful message
    if is_already_completed && !clean_worktree {
        return Err(format!(
            "Agent \"{agent_name}\" is already completed. Use --clean-worktree to remove its worktree."
        ));
    }

    let is_headless = session.tmux_session.is_empty() && session.pid.is_some();

    let mut tmux_killed = false;
    let mut pid_killed = false;

    if !is_already_completed {
        if is_headless {
            if let Some(pid) = session.pid {
                if is_process_alive(pid) {
                    kill_process(pid);
                    pid_killed = true;
                }
            }
        } else if !session.tmux_session.is_empty()
            && is_tmux_session_alive(&session.tmux_session)
        {
            kill_tmux_session(&session.tmux_session);
            tmux_killed = true;
        }

        store
            .update_state(agent_name, AgentState::Completed)
            .map_err(|e| e.to_string())?;
        store
            .update_last_activity(agent_name)
            .map_err(|e| e.to_string())?;
    }

    let mut worktree_removed = false;
    let mut branch_deleted = false;

    if clean_worktree {
        if !session.worktree_path.is_empty() {
            worktree_removed = remove_worktree(&root_str, &session.worktree_path, force);
        }
        if !session.branch_name.is_empty() {
            branch_deleted = delete_branch(&root_str, &session.branch_name);
        }
    }

    if json {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Output {
            stopped: bool,
            agent_name: String,
            session_id: String,
            capability: String,
            tmux_killed: bool,
            pid_killed: bool,
            worktree_removed: bool,
            branch_deleted: bool,
            force: bool,
            was_zombie: bool,
            was_completed: bool,
        }
        println!(
            "{}",
            json_output(
                "stop",
                &Output {
                    stopped: true,
                    agent_name: agent_name.to_string(),
                    session_id: session.id.clone(),
                    capability: session.capability.clone(),
                    tmux_killed,
                    pid_killed,
                    worktree_removed,
                    branch_deleted,
                    force,
                    was_zombie: is_zombie,
                    was_completed: is_already_completed,
                }
            )
        );
    } else {
        println!("{} {}", brand_bold("Agent stopped:"), agent_name);
        if !is_already_completed {
            if is_headless {
                if pid_killed {
                    println!(
                        "  Process tree killed: PID {}",
                        session.pid.unwrap_or(0)
                    );
                } else {
                    println!(
                        "  Process was already dead (PID {})",
                        session.pid.unwrap_or(0)
                    );
                }
            } else if tmux_killed {
                println!("  Tmux session killed: {}", session.tmux_session);
            } else {
                println!("  Tmux session was already dead");
            }
        }
        if is_zombie {
            println!("  Zombie agent cleaned up (state → completed)");
        }
        if is_already_completed {
            println!("  Agent was already completed (skipped kill)");
        }
        if clean_worktree && worktree_removed {
            println!("  Worktree removed: {}", session.worktree_path);
        }
        if clean_worktree && branch_deleted {
            println!("  Branch deleted: {}", session.branch_name);
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Process helpers
// ---------------------------------------------------------------------------

/// Check whether a process is alive by sending signal 0.
fn is_process_alive(pid: i64) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Send SIGTERM to a process (best-effort).
fn kill_process(pid: i64) {
    let _ = Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .output();
}

// ---------------------------------------------------------------------------
// Tmux helpers
// ---------------------------------------------------------------------------

/// Check whether a tmux session exists.
fn is_tmux_session_alive(session_name: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", session_name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Kill a tmux session (best-effort).
fn kill_tmux_session(session_name: &str) {
    let _ = Command::new("tmux")
        .args(["kill-session", "-t", session_name])
        .output();
}

// ---------------------------------------------------------------------------
// Git helpers
// ---------------------------------------------------------------------------

/// Remove a git worktree (best-effort).
fn remove_worktree(project_root: &str, worktree_path: &str, force: bool) -> bool {
    let mut cmd = Command::new("git");
    cmd.args(["worktree", "remove"]);
    if force {
        cmd.arg("--force");
    }
    cmd.arg(worktree_path).current_dir(project_root);
    cmd.output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Delete a git branch (best-effort).
fn delete_branch(project_root: &str, branch_name: &str) -> bool {
    Command::new("git")
        .args(["branch", "-D", branch_name])
        .current_dir(project_root)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_execute_empty_agent_name() {
        let result = execute("", false, false, false, Some(Path::new("/tmp")));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required argument"));
    }

    #[test]
    fn test_execute_whitespace_agent_name() {
        let result = execute("   ", false, false, false, Some(Path::new("/tmp")));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required argument"));
    }

    #[test]
    fn test_execute_no_sessions_db() {
        let result = execute("test-agent", false, false, false, Some(Path::new("/tmp")));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("not found"));
    }

    #[test]
    fn test_is_process_alive_invalid_pid() {
        // Very high PID should not be alive
        let result = is_process_alive(i64::from(i32::MAX));
        assert!(!result);
    }

    #[test]
    fn test_is_tmux_session_alive_nonexistent() {
        let result = is_tmux_session_alive("grove-test-session-nonexistent-xyz");
        assert!(!result);
    }

    #[test]
    fn test_delete_branch_in_non_git_dir() {
        let result = delete_branch("/tmp", "some-branch");
        assert!(!result);
    }

    #[test]
    fn test_remove_worktree_nonexistent() {
        let result = remove_worktree("/tmp", "/nonexistent/path", true);
        assert!(!result);
    }
}
