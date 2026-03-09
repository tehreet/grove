//! `grove group` — manage task groups for batch coordination.
//!
//! Groups track collections of issues and auto-close when all members close.
//! Storage: `.overstory/groups.json`
//!
//! Subcommands:
//!   create <name> <ids...>   Create a new task group
//!   status [group-id]        Show progress for one or all groups
//!   add <group-id> <ids...>  Add issues to a group
//!   remove <group-id> <ids...> Remove issues from a group
//!   list                     List all groups

use std::path::{Path, PathBuf};

use crate::config::load_config;
use crate::json::json_output;
use crate::logging::{accent, print_hint, print_success};
use crate::types::{TaskGroup, TaskGroupProgress, TaskGroupStatus};

// ---------------------------------------------------------------------------
// Storage helpers
// ---------------------------------------------------------------------------

fn groups_path(project_root: &str) -> String {
    format!("{project_root}/.overstory/groups.json")
}

fn load_groups(project_root: &str) -> Result<Vec<TaskGroup>, String> {
    let path = groups_path(project_root);
    if !PathBuf::from(&path).exists() {
        return Ok(vec![]);
    }
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

fn save_groups(project_root: &str, groups: &[TaskGroup]) -> Result<(), String> {
    let path = groups_path(project_root);
    let json = serde_json::to_string_pretty(groups).map_err(|e| e.to_string())?;
    std::fs::write(&path, format!("{json}\n")).map_err(|e| e.to_string())
}

fn generate_group_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    format!("group-{:08x}", nanos)
}

// ---------------------------------------------------------------------------
// Group progress (local status only — no tracker integration)
// ---------------------------------------------------------------------------

/// Build a progress summary for a group. Without tracker integration, all
/// issues are counted as "open" unless the group is marked completed.
fn group_progress(group: &TaskGroup) -> TaskGroupProgress {
    let total = group.member_issue_ids.len() as u32;
    let (completed, open) = if group.status == TaskGroupStatus::Completed {
        (total, 0)
    } else {
        (0, total)
    };
    TaskGroupProgress {
        group: group.clone(),
        total,
        completed,
        in_progress: 0,
        blocked: 0,
        open,
    }
}

fn print_group_progress(progress: &TaskGroupProgress) {
    let status = if progress.group.status == TaskGroupStatus::Completed {
        "[completed]"
    } else {
        "[active]"
    };
    println!(
        "{} ({}) {}",
        progress.group.name,
        accent(&progress.group.id),
        status
    );
    println!(
        "  Issues: {} total | {} completed | {} in_progress | {} blocked | {} open",
        progress.total,
        progress.completed,
        progress.in_progress,
        progress.blocked,
        progress.open,
    );
    if progress.group.status == TaskGroupStatus::Completed {
        if let Some(ref at) = progress.group.completed_at {
            println!("  Completed: {}", at);
        }
    }
}

// ---------------------------------------------------------------------------
// Execute: create
// ---------------------------------------------------------------------------

pub fn execute_create(
    name: &str,
    ids: Vec<String>,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("Group name is required".to_string());
    }
    if ids.is_empty() {
        return Err("At least one issue ID is required".to_string());
    }

    // Check for duplicates
    let mut seen = std::collections::HashSet::new();
    for id in &ids {
        if !seen.insert(id) {
            return Err(format!("Duplicate issue ID: {id}"));
        }
    }

    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = load_config(&cwd, project_override).map_err(|e| e.to_string())?;
    let root = &config.project.root;

    let mut groups = load_groups(root)?;
    let group = TaskGroup {
        id: generate_group_id(),
        name: name.trim().to_string(),
        member_issue_ids: ids,
        status: TaskGroupStatus::Active,
        created_at: chrono::Utc::now().to_rfc3339(),
        completed_at: None,
    };
    groups.push(group.clone());
    save_groups(root, &groups)?;

    if json {
        println!("{}", json_output("group create", &group));
    } else {
        print_success("Created group", Some(&group.name));
        let members: Vec<String> = group.member_issue_ids.iter().map(|id| accent(id).to_string()).collect();
        println!("  Members: {}", members.join(", "));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Execute: status
// ---------------------------------------------------------------------------

pub fn execute_status(
    group_id: Option<String>,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = load_config(&cwd, project_override).map_err(|e| e.to_string())?;
    let root = &config.project.root;
    let groups = load_groups(root)?;

    if let Some(ref gid) = group_id {
        let group = groups
            .iter()
            .find(|g| &g.id == gid)
            .ok_or_else(|| format!("Group \"{gid}\" not found"))?;

        let progress = group_progress(group);
        if json {
            println!("{}", json_output("group status", &progress));
        } else {
            print_group_progress(&progress);
        }
    } else {
        let active: Vec<&TaskGroup> = groups.iter().filter(|g| g.status == TaskGroupStatus::Active).collect();
        if active.is_empty() {
            if json {
                println!("{}", json_output("group status", &serde_json::json!({"groups": []})));
            } else {
                print_hint("No active groups");
            }
            return Ok(());
        }

        let progress_list: Vec<TaskGroupProgress> = active.iter().map(|g| group_progress(g)).collect();
        if json {
            println!("{}", json_output("group status", &serde_json::json!({"groups": progress_list})));
        } else {
            for progress in &progress_list {
                print_group_progress(progress);
                println!();
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Execute: add
// ---------------------------------------------------------------------------

pub fn execute_add(
    group_id: &str,
    ids: Vec<String>,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    if ids.is_empty() {
        return Err("At least one issue ID is required".to_string());
    }

    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = load_config(&cwd, project_override).map_err(|e| e.to_string())?;
    let root = &config.project.root;
    let mut groups = load_groups(root)?;

    let group = groups
        .iter_mut()
        .find(|g| g.id == group_id)
        .ok_or_else(|| format!("Group \"{group_id}\" not found"))?;

    // Check for existing members
    for id in &ids {
        if group.member_issue_ids.contains(id) {
            return Err(format!("Issue \"{id}\" is already a member of group \"{group_id}\""));
        }
    }

    group.member_issue_ids.extend(ids);

    // Reopen if completed
    if group.status == TaskGroupStatus::Completed {
        group.status = TaskGroupStatus::Active;
        group.completed_at = None;
    }

    let group = group.clone();
    save_groups(root, &groups)?;

    if json {
        println!("{}", json_output("group add", &group));
    } else {
        print_success("Added to group", Some(&group.name));
        let members: Vec<String> = group.member_issue_ids.iter().map(|id| accent(id).to_string()).collect();
        println!("  Members: {}", members.join(", "));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Execute: remove
// ---------------------------------------------------------------------------

pub fn execute_remove(
    group_id: &str,
    ids: Vec<String>,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    if ids.is_empty() {
        return Err("At least one issue ID is required".to_string());
    }

    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = load_config(&cwd, project_override).map_err(|e| e.to_string())?;
    let root = &config.project.root;
    let mut groups = load_groups(root)?;

    let group = groups
        .iter_mut()
        .find(|g| g.id == group_id)
        .ok_or_else(|| format!("Group \"{group_id}\" not found"))?;

    // Validate all are members
    for id in &ids {
        if !group.member_issue_ids.contains(id) {
            return Err(format!("Issue \"{id}\" is not a member of group \"{group_id}\""));
        }
    }

    // Ensure not emptying the group
    let remaining: Vec<String> = group
        .member_issue_ids
        .iter()
        .filter(|id| !ids.contains(id))
        .cloned()
        .collect();
    if remaining.is_empty() {
        return Err("Cannot remove all issues from a group".to_string());
    }

    group.member_issue_ids = remaining;
    let group = group.clone();
    save_groups(root, &groups)?;

    if json {
        println!("{}", json_output("group remove", &group));
    } else {
        print_success("Removed from group", Some(&group.name));
        let members: Vec<String> = group.member_issue_ids.iter().map(|id| accent(id).to_string()).collect();
        println!("  Members: {}", members.join(", "));
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
    let groups = load_groups(root)?;

    if groups.is_empty() {
        if json {
            println!("[]");
        } else {
            print_hint("No groups");
        }
        return Ok(());
    }

    if json {
        println!("{}", json_output("group list", &serde_json::json!({"groups": groups})));
    } else {
        for group in &groups {
            let status = if group.status == TaskGroupStatus::Completed { "[completed]" } else { "[active]" };
            println!(
                "{} {} \"{}\" ({} issues)",
                accent(&group.id),
                status,
                group.name,
                group.member_issue_ids.len()
            );
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
    fn test_generate_group_id_format() {
        let id = generate_group_id();
        assert!(id.starts_with("group-"));
        assert_eq!(id.len(), "group-".len() + 8);
    }

    #[test]
    fn test_load_groups_missing_file() {
        let groups = load_groups("/nonexistent/path").unwrap();
        assert!(groups.is_empty());
    }

    #[test]
    fn test_execute_list_no_config() {
        let result = execute_list(false, Some(Path::new("/tmp")));
        // Either works (no groups) or fails with config error — don't panic
        let _ = result;
    }

    #[test]
    fn test_execute_create_validation() {
        let result = execute_create("", vec!["id1".to_string()], false, Some(Path::new("/tmp")));
        assert!(result.is_err());

        let result = execute_create("mygroup", vec![], false, Some(Path::new("/tmp")));
        assert!(result.is_err());
    }

    #[test]
    fn test_group_progress_active() {
        let group = TaskGroup {
            id: "group-abc12345".to_string(),
            name: "test".to_string(),
            member_issue_ids: vec!["a".to_string(), "b".to_string()],
            status: TaskGroupStatus::Active,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            completed_at: None,
        };
        let p = group_progress(&group);
        assert_eq!(p.total, 2);
        assert_eq!(p.open, 2);
        assert_eq!(p.completed, 0);
    }

    #[test]
    fn test_group_progress_completed() {
        let group = TaskGroup {
            id: "group-abc12345".to_string(),
            name: "test".to_string(),
            member_issue_ids: vec!["a".to_string()],
            status: TaskGroupStatus::Completed,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            completed_at: Some("2024-01-02T00:00:00Z".to_string()),
        };
        let p = group_progress(&group);
        assert_eq!(p.total, 1);
        assert_eq!(p.completed, 1);
        assert_eq!(p.open, 0);
    }
}
