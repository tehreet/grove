//! `grove agents` — discover and query agents by capability.
//!
//! Subcommands:
//!   discover   Find active agents with their file scopes

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::load_config;
use crate::db::sessions::SessionStore;
use crate::json::json_output;
use crate::logging::{accent, muted, print_hint};
use crate::types::AgentSession;

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredAgent {
    pub agent_name: String,
    pub capability: String,
    pub state: String,
    pub task_id: String,
    pub branch_name: String,
    pub parent_agent: Option<String>,
    pub depth: u32,
    pub file_scope: Vec<String>,
    pub started_at: String,
    pub last_activity: String,
}

// ---------------------------------------------------------------------------
// File scope extraction
// ---------------------------------------------------------------------------

/// Extract file scope from an agent's overlay instruction file.
/// Looks for the "## File Scope (exclusive ownership)" section in .claude/CLAUDE.md.
fn extract_file_scope(worktree_path: &str) -> Vec<String> {
    // Try known overlay paths
    let paths_to_try = [".claude/CLAUDE.md", "CLAUDE.md"];

    let mut content: Option<String> = None;
    for rel_path in &paths_to_try {
        let full_path = PathBuf::from(worktree_path).join(rel_path);
        if let Ok(text) = fs::read_to_string(&full_path) {
            content = Some(text);
            break;
        }
    }

    let content = match content {
        Some(c) => c,
        None => return vec![],
    };

    let start_marker = "## File Scope (exclusive ownership)";
    let end_marker = "## Expertise";

    let start_idx = match content.find(start_marker) {
        Some(i) => i,
        None => return vec![],
    };
    let end_idx = match content[start_idx..].find(end_marker) {
        Some(i) => start_idx + i,
        None => return vec![],
    };

    let section = &content[start_idx..end_idx];

    if section.contains("No file scope restrictions") {
        return vec![];
    }

    // Extract paths from markdown list items: - `path`
    let mut paths = Vec::new();
    for line in section.lines() {
        let trimmed = line.trim();
        if let Some(inner) = trimmed.strip_prefix("- `") {
            if let Some(path) = inner.strip_suffix('`') {
                paths.push(path.to_string());
            }
        }
    }

    paths
}

// ---------------------------------------------------------------------------
// Execute
// ---------------------------------------------------------------------------

pub fn execute_discover(
    capability: Option<String>,
    include_all: bool,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = load_config(&cwd, project_override).map_err(|e| e.to_string())?;
    let root = &config.project.root;
    let overstory = format!("{root}/.overstory");
    let sessions_db = format!("{overstory}/sessions.db");

    if !PathBuf::from(&sessions_db).exists() {
        if json {
            println!("{}", json_output("agents discover", &serde_json::json!({"agents": []})));
        } else {
            print_hint("No sessions database found");
        }
        return Ok(());
    }

    let store = SessionStore::new(&sessions_db).map_err(|e| e.to_string())?;
    let sessions: Vec<AgentSession> = if include_all {
        store.get_all().map_err(|e| e.to_string())?
    } else {
        store.get_active().map_err(|e| e.to_string())?
    };

    // Filter by capability
    let sessions: Vec<AgentSession> = if let Some(ref cap) = capability {
        sessions.into_iter().filter(|s| &s.capability == cap).collect()
    } else {
        sessions
    };

    // Enrich with file scopes
    let agents: Vec<DiscoveredAgent> = sessions
        .into_iter()
        .map(|s| {
            let file_scope = extract_file_scope(&s.worktree_path);
            let state = format!("{:?}", s.state).to_lowercase();
            DiscoveredAgent {
                agent_name: s.agent_name,
                capability: s.capability,
                state,
                task_id: s.task_id,
                branch_name: s.branch_name,
                parent_agent: s.parent_agent,
                depth: s.depth,
                file_scope,
                started_at: s.started_at,
                last_activity: s.last_activity,
            }
        })
        .collect();

    if json {
        println!("{}", json_output("agents discover", &serde_json::json!({"agents": agents})));
    } else {
        print_agents(&agents);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Text formatting
// ---------------------------------------------------------------------------

fn print_agents(agents: &[DiscoveredAgent]) {
    if agents.is_empty() {
        println!("No agents found.");
        return;
    }

    println!("Found {} agent{}:\n", agents.len(), if agents.len() == 1 { "" } else { "s" });

    for agent in agents {
        let icon = state_icon(&agent.state);
        println!("  {} {} [{}]", icon, accent(&agent.agent_name), agent.capability);
        println!("    State: {} | Task: {}", agent.state, accent(&agent.task_id));
        println!("    Branch: {}", accent(&agent.branch_name));
        let parent = agent.parent_agent.as_deref().map(|p| accent(p).to_string()).unwrap_or_else(|| "none".to_string());
        println!("    Parent: {} | Depth: {}", parent, agent.depth);
        if agent.file_scope.is_empty() {
            println!("    Files: {}", muted("(unrestricted)"));
        } else {
            println!("    Files: {}", agent.file_scope.join(", "));
        }
        println!();
    }
}

fn state_icon(state: &str) -> &'static str {
    match state {
        "working" => ">",
        "booting" => "-",
        "stalled" => "!",
        _ => " ",
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_file_scope_missing_dir() {
        let scope = extract_file_scope("/nonexistent/path");
        assert!(scope.is_empty());
    }

    #[test]
    fn test_state_icon() {
        assert_eq!(state_icon("working"), ">");
        assert_eq!(state_icon("booting"), "-");
        assert_eq!(state_icon("stalled"), "!");
        assert_eq!(state_icon("completed"), " ");
    }

    #[test]
    fn test_execute_discover_no_db() {
        let result = execute_discover(None, false, false, Some(Path::new("/tmp")));
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_discover_json_no_db() {
        let result = execute_discover(None, false, true, Some(Path::new("/tmp")));
        assert!(result.is_ok());
    }
}
