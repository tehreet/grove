//! `grove worktree` — manage agent worktrees.
//!
//! Subcommands:
//!   list          List worktrees with agent status
//!   clean         Remove completed worktrees

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

use crate::config::load_config;
use crate::db::sessions::SessionStore;
use crate::json::json_output;
use crate::logging::{accent, muted, print_hint, print_success, print_warning};

// ---------------------------------------------------------------------------
// Worktree info (reuse from status module if available, else define locally)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeEntry {
    pub path: String,
    pub branch: String,
    pub head: String,
    pub agent_name: Option<String>,
    pub state: Option<String>,
    pub task_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Git helpers
// ---------------------------------------------------------------------------

fn list_git_worktrees_raw(project_root: &str) -> Vec<(String, String, String)> {
    // Returns (path, head, branch) tuples
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(project_root)
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return vec![],
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let mut result = Vec::new();
    let mut path: Option<String> = None;
    let mut head: Option<String> = None;
    let mut branch: Option<String> = None;

    for line in text.lines() {
        if line.is_empty() {
            if let (Some(p), Some(h)) = (path.take(), head.take()) {
                result.push((p, h, branch.take().unwrap_or_else(|| "(detached)".to_string())));
            }
            branch = None;
        } else if let Some(p) = line.strip_prefix("worktree ") {
            path = Some(p.to_string());
        } else if let Some(h) = line.strip_prefix("HEAD ") {
            head = Some(h.to_string());
        } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
            branch = Some(b.to_string());
        } else if let Some(b) = line.strip_prefix("branch ") {
            branch = Some(b.to_string());
        }
    }
    // Final block
    if let (Some(p), Some(h)) = (path.take(), head.take()) {
        result.push((p, h, branch.unwrap_or_else(|| "(detached)".to_string())));
    }

    result
}

fn is_branch_merged(project_root: &str, branch: &str, canonical: &str) -> bool {
    // Check if branch is an ancestor of canonical
    let output = Command::new("git")
        .args(["merge-base", "--is-ancestor", branch, canonical])
        .current_dir(project_root)
        .output();

    matches!(output, Ok(o) if o.status.success())
}


fn remove_git_worktree(project_root: &str, path: &str) -> Result<(), String> {
    let output = Command::new("git")
        .args(["worktree", "remove", "--force", path])
        .current_dir(project_root)
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    Ok(())
}

fn delete_branch(project_root: &str, branch: &str) -> Result<(), String> {
    let output = Command::new("git")
        .args(["branch", "-D", branch])
        .current_dir(project_root)
        .output()
        .map_err(|e| e.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).into_owned());
    }
    Ok(())
}


// ---------------------------------------------------------------------------
// Execute: list
// ---------------------------------------------------------------------------

pub fn execute_list(json: bool, project_override: Option<&Path>) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = load_config(&cwd, project_override).map_err(|e| e.to_string())?;
    let root = &config.project.root;
    let overstory = format!("{root}/.overstory");
    let sessions_db = format!("{overstory}/sessions.db");

    let worktrees = list_git_worktrees_raw(root);
    let overstory_wts: Vec<_> = worktrees.iter().filter(|(_, _, b)| b.starts_with("overstory/")).collect();

    let sessions: Vec<crate::types::AgentSession> = if PathBuf::from(&sessions_db).exists() {
        SessionStore::new(&sessions_db)
            .map_err(|e| e.to_string())?
            .get_all()
            .map_err(|e| e.to_string())?
    } else {
        vec![]
    };

    if json {
        let entries: Vec<WorktreeEntry> = overstory_wts
            .iter()
            .map(|(path, head, branch)| {
                let session = sessions.iter().find(|s| &s.worktree_path == path);
                WorktreeEntry {
                    path: path.clone(),
                    branch: branch.clone(),
                    head: head.clone(),
                    agent_name: session.map(|s| s.agent_name.clone()),
                    state: session.map(|s| format!("{:?}", s.state).to_lowercase()),
                    task_id: session.map(|s| s.task_id.clone()),
                }
            })
            .collect();
        println!("{}", json_output("worktree list", &serde_json::json!({"worktrees": entries})));
        return Ok(());
    }

    if overstory_wts.is_empty() {
        print_hint("No agent worktrees found");
        return Ok(());
    }

    println!("Agent worktrees: {}\n", overstory_wts.len());
    for (path, _, branch) in &overstory_wts {
        let session = sessions.iter().find(|s| &s.worktree_path == path);
        let state = session.map(|s| format!("{:?}", s.state).to_lowercase()).unwrap_or_else(|| "unknown".to_string());
        let agent = session.map(|s| s.agent_name.as_str()).unwrap_or("?");
        let task = session.map(|s| s.task_id.as_str()).unwrap_or("?");
        println!("  {}", accent(branch));
        println!("    Agent: {} | State: {} | Task: {}", agent, state, task);
        println!("    Path: {}", muted(path));
        println!();
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Execute: clean
// ---------------------------------------------------------------------------

pub fn execute_clean(
    all: bool,
    force: bool,
    completed_only: bool,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = load_config(&cwd, project_override).map_err(|e| e.to_string())?;
    let root = &config.project.root;
    let overstory = format!("{root}/.overstory");
    let sessions_db = format!("{overstory}/sessions.db");
    let canonical = &config.project.canonical_branch;

    let worktrees = list_git_worktrees_raw(root);
    let overstory_wts: Vec<_> = worktrees
        .into_iter()
        .filter(|(_, _, b)| b.starts_with("overstory/"))
        .collect();

    let sessions: Vec<crate::types::AgentSession> = if PathBuf::from(&sessions_db).exists() {
        SessionStore::new(&sessions_db)
            .map_err(|e| e.to_string())?
            .get_all()
            .map_err(|e| e.to_string())?
    } else {
        vec![]
    };

    let mut cleaned: Vec<String> = vec![];
    let mut failed: Vec<String> = vec![];
    let mut skipped: Vec<String> = vec![];

    for (path, _, branch) in &overstory_wts {
        let session = sessions.iter().find(|s| &s.worktree_path == path);

        // --completed: only process done/zombie agents
        if completed_only && !all {
            if let Some(s) = session {
                match s.state {
                    crate::types::AgentState::Completed | crate::types::AgentState::Zombie => {}
                    _ => continue,
                }
            }
        }

        let is_lead = session.map(|s| s.capability == "lead").unwrap_or(false);

        // Skip non-force, non-lead unmerged branches
        if !force && !is_lead && !branch.is_empty() {
            let merged = is_branch_merged(root, branch, canonical);
            if !merged {
                skipped.push(branch.clone());
                continue;
            }
        }

        // Kill agent process if alive (PID-based, no tmux)
        if let Some(s) = session {
            if let Some(pid) = s.pid {
                if crate::watchdog::is_pid_alive(pid) {
                    let _ = std::process::Command::new("kill")
                        .args(["-15", &pid.to_string()])
                        .output();
                }
            }
        }

        // Warn about force-deleting unmerged branches
        if force && !is_lead && !branch.is_empty() && !json {
            let merged = is_branch_merged(root, branch, canonical);
            if !merged {
                print_warning("Force-deleting unmerged branch", Some(branch));
            }
        }

        // Remove worktree
        match remove_git_worktree(root, path) {
            Ok(()) => {
                // Delete branch too
                if !branch.is_empty() {
                    let _ = delete_branch(root, branch);
                }
                cleaned.push(branch.clone());
                if !json {
                    print_success("Removed", Some(branch));
                }
            }
            Err(e) => {
                failed.push(branch.clone());
                if !json {
                    print_warning(&format!("Failed to remove {branch}"), Some(&e));
                }
            }
        }
    }

    // Mark cleaned sessions as zombie
    if PathBuf::from(&sessions_db).exists() && !cleaned.is_empty() {
        if let Ok(store) = SessionStore::new(&sessions_db) {
            for branch in &cleaned {
                let session = sessions.iter().find(|s| &s.branch_name == branch);
                if let Some(s) = session {
                    let _ = store.update_state(&s.agent_name, crate::types::AgentState::Zombie);
                }
            }
        }
    }

    if json {
        println!(
            "{}",
            json_output(
                "worktree clean",
                &serde_json::json!({
                    "cleaned": cleaned,
                    "failed": failed,
                    "skipped": skipped,
                })
            )
        );
    } else if cleaned.is_empty() && failed.is_empty() && skipped.is_empty() {
        print_hint("No worktrees to clean");
    } else {
        if !cleaned.is_empty() {
            print_success(
                &format!("Cleaned {} worktree{}", cleaned.len(), if cleaned.len() == 1 { "" } else { "s" }),
                None,
            );
        }
        if !failed.is_empty() {
            print_warning(
                &format!("Failed to clean {} worktree{}", failed.len(), if failed.len() == 1 { "" } else { "s" }),
                None,
            );
        }
        if !skipped.is_empty() {
            print_warning(
                &format!("Skipped {} worktree{} with unmerged branches", skipped.len(), if skipped.len() == 1 { "" } else { "s" }),
                Some("Use --force to delete unmerged branches"),
            );
            for branch in &skipped {
                println!("  {}", branch);
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_execute_list_no_db() {
        let result = execute_list(false, Some(Path::new("/tmp/grove-test")));
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_list_json_no_db() {
        let result = execute_list(true, Some(Path::new("/tmp/grove-test")));
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_clean_no_db() {
        let result = execute_clean(false, false, true, false, Some(Path::new("/tmp/grove-test")));
        assert!(result.is_ok());
    }
}
