#![allow(dead_code)]

use std::path::Path;
use std::process::Command;

/// Run a git command, return stdout on success, Err(String) on failure.
fn run_git(repo_root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo_root)
        .output()
        .map_err(|e| format!("Failed to execute git: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "git {} failed (exit {}): {}",
            args.join(" "),
            output.status.code().unwrap_or(-1),
            if stderr.trim().is_empty() {
                stdout.trim().to_string()
            } else {
                stderr.trim().to_string()
            }
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Parsed worktree entry from `git worktree list --porcelain`.
#[derive(Debug, Clone)]
pub struct WorktreeEntry {
    pub path: String,
    pub branch: String,
    pub head: String,
}

/// Create a git worktree with a new branch.
/// Runs: git worktree add -b <branch_name> <worktree_path> <base_branch>
pub fn create_worktree(
    repo_root: &Path,
    base_branch: &str,
    branch_name: &str,
    worktree_path: &Path,
) -> Result<(), String> {
    let wt_path = worktree_path
        .to_str()
        .ok_or_else(|| "Invalid worktree path".to_string())?;
    run_git(
        repo_root,
        &["worktree", "add", "-b", branch_name, wt_path, base_branch],
    )?;
    Ok(())
}

/// Remove a git worktree. With force=true, uses --force flag.
/// Runs: git worktree remove [--force] <worktree_path>
pub fn remove_worktree(
    repo_root: &Path,
    worktree_path: &Path,
    force: bool,
) -> Result<(), String> {
    let wt_path = worktree_path
        .to_str()
        .ok_or_else(|| "Invalid worktree path".to_string())?;
    if force {
        run_git(repo_root, &["worktree", "remove", "--force", wt_path])?;
    } else {
        run_git(repo_root, &["worktree", "remove", wt_path])?;
    }
    Ok(())
}

/// Parse `git worktree list --porcelain` output into WorktreeEntry vec.
fn parse_worktree_output(output: &str) -> Vec<WorktreeEntry> {
    let mut entries = Vec::new();
    let mut path = String::new();
    let mut head = String::new();
    let mut branch = String::new();

    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("worktree ") {
            path = rest.to_string();
            head.clear();
            branch.clear();
        } else if let Some(rest) = line.strip_prefix("HEAD ") {
            head = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("branch ") {
            branch = rest
                .strip_prefix("refs/heads/")
                .unwrap_or(rest)
                .to_string();
        } else if line.is_empty() && !path.is_empty() {
            entries.push(WorktreeEntry {
                path: path.clone(),
                branch: branch.clone(),
                head: head.clone(),
            });
            path.clear();
            head.clear();
            branch.clear();
        }
    }
    // Handle last block if not terminated by blank line
    if !path.is_empty() {
        entries.push(WorktreeEntry {
            path,
            branch,
            head,
        });
    }
    entries
}

/// List all worktrees. Parses `git worktree list --porcelain`.
pub fn list_worktrees(repo_root: &Path) -> Result<Vec<WorktreeEntry>, String> {
    let output = run_git(repo_root, &["worktree", "list", "--porcelain"])?;
    Ok(parse_worktree_output(&output))
}

/// Prune stale worktree entries.
/// Runs: git worktree prune
pub fn prune_worktrees(repo_root: &Path) -> Result<(), String> {
    run_git(repo_root, &["worktree", "prune"])?;
    Ok(())
}

/// Force-delete a branch.
/// Runs: git branch -D <branch_name>
pub fn delete_branch(repo_root: &Path, branch_name: &str) -> Result<(), String> {
    run_git(repo_root, &["branch", "-D", branch_name])?;
    Ok(())
}

/// Best-effort rollback: remove worktree + delete branch. Errors are swallowed.
/// Used in catch blocks after failed spawns.
pub fn rollback_worktree(repo_root: &Path, worktree_path: &Path, branch_name: &str) {
    let _ = remove_worktree(repo_root, worktree_path, true);
    let _ = delete_branch(repo_root, branch_name);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_run_git_version() {
        let result = run_git(Path::new("."), &["--version"]);
        assert!(result.is_ok(), "git --version failed: {:?}", result);
    }

    #[test]
    fn test_parse_worktree_output() {
        let sample = "\
worktree /home/user/project
HEAD abc123def456
branch refs/heads/main

worktree /home/user/project/.wt/feature
HEAD 789xyz
branch refs/heads/feature/foo

";
        let entries = parse_worktree_output(sample);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "/home/user/project");
        assert_eq!(entries[0].branch, "main");
        assert_eq!(entries[0].head, "abc123def456");
        assert_eq!(entries[1].path, "/home/user/project/.wt/feature");
        assert_eq!(entries[1].branch, "feature/foo");
        assert_eq!(entries[1].head, "789xyz");
    }

    #[test]
    fn test_parse_worktree_output_empty() {
        let entries = parse_worktree_output("");
        assert!(entries.is_empty());
    }

    #[test]
    fn test_list_worktrees_includes_main() {
        let result = list_worktrees(Path::new("."));
        assert!(result.is_ok(), "list_worktrees failed: {:?}", result);
        let entries = result.unwrap();
        assert!(
            !entries.is_empty(),
            "Expected at least one worktree entry"
        );
    }
}
