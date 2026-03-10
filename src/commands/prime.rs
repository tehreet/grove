#![allow(dead_code)]

use std::path::Path;
use std::process::Command;

use serde::Serialize;

use crate::config::load_config;
use crate::db::metrics::MetricsStore;
use crate::db::sessions::SessionStore;
use crate::json::json_output;
use crate::types::SessionMetrics;

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PrimeOutput {
    agent: Option<String>,
    compact: bool,
    context: String,
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

pub fn format_manifest(entries: &[serde_json::Value]) -> String {
    if entries.is_empty() {
        return String::new();
    }
    let mut lines = Vec::new();
    for entry in entries {
        let name = entry["name"].as_str().unwrap_or("unknown");
        let model = entry["model"].as_str().unwrap_or("unknown");
        let caps: Vec<&str> = entry["capabilities"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();
        let can_spawn = entry["canSpawn"].as_bool().unwrap_or(false);
        let caps_str = caps.join(", ");
        let spawn_suffix = if can_spawn { " (can spawn)" } else { "" };
        lines.push(format!(
            "- **{}** [{}]: {}{}",
            name, model, caps_str, spawn_suffix
        ));
    }
    lines.join("\n")
}

pub fn format_metrics(sessions: &[SessionMetrics]) -> String {
    if sessions.is_empty() {
        return String::new();
    }
    let mut lines = Vec::new();
    for s in sessions {
        let status = if s.completed_at.is_some() {
            "completed"
        } else {
            "in-progress"
        };
        let duration = format!("{}s", s.duration_ms / 1000);
        lines.push(format!(
            "- {} ({}): {} — {} ({})",
            s.agent_name, s.capability, s.task_id, status, duration
        ));
    }
    lines.join("\n")
}

fn run_mulch_prime() -> Option<String> {
    let output = Command::new("mulch").arg("prime").output().ok()?;
    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout).to_string();
        if !text.trim().is_empty() {
            Some(text)
        } else {
            None
        }
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Orchestrator mode
// ---------------------------------------------------------------------------

fn build_orchestrator_context(project_override: Option<&Path>) -> Result<String, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = load_config(&cwd, project_override).map_err(|e| e.to_string())?;

    let root = config.project.root.clone();

    let mut ctx = String::new();

    // Header
    ctx.push_str("# Overstory Context\n\n");

    // Project section
    ctx.push_str(&format!("## Project: {}\n", config.project.name));
    ctx.push_str(&format!(
        "Canonical branch: {}\n",
        config.project.canonical_branch
    ));
    ctx.push_str(&format!(
        "Max concurrent agents: {}\n",
        config.agents.max_concurrent
    ));
    ctx.push_str(&format!("Max depth: {}\n", config.agents.max_depth));
    ctx.push('\n');

    // Agent manifest
    ctx.push_str("## Agent Manifest\n");
    let manifest_path = std::path::PathBuf::from(&root).join(&config.agents.manifest_path);
    if manifest_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&manifest_path) {
            if let Ok(entries) = serde_json::from_str::<Vec<serde_json::Value>>(&content) {
                let manifest_str = format_manifest(&entries);
                if !manifest_str.is_empty() {
                    ctx.push_str(&manifest_str);
                    ctx.push('\n');
                }
            }
        }
    }
    ctx.push('\n');

    // Recent activity
    ctx.push_str("## Recent Activity\n");
    let overstory = format!("{}/.overstory", root);
    let metrics_db = format!("{}/metrics.db", overstory);
    if std::path::PathBuf::from(&metrics_db).exists() {
        if let Ok(store) = MetricsStore::new(&metrics_db) {
            if let Ok(sessions) = store.get_recent_sessions(Some(5)) {
                let activity = format_metrics(&sessions);
                if !activity.is_empty() {
                    ctx.push_str(&activity);
                    ctx.push('\n');
                }
            }
        }
    }
    ctx.push('\n');

    // Expertise
    if config.mulch.enabled {
        ctx.push_str("## Expertise\n");
        if let Some(mulch_out) = run_mulch_prime() {
            ctx.push_str(&mulch_out);
        }
    }

    Ok(ctx)
}

// ---------------------------------------------------------------------------
// Agent mode
// ---------------------------------------------------------------------------

fn build_agent_context(
    agent_name: &str,
    compact: bool,
    project_override: Option<&Path>,
) -> Result<String, String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = load_config(&cwd, project_override).map_err(|e| e.to_string())?;

    let root = config.project.root.clone();

    let mut ctx = String::new();

    // Header
    ctx.push_str(&format!("# Agent Context: {}\n\n", agent_name));

    // Identity section
    ctx.push_str("## Identity\n");

    let identity_path = std::path::PathBuf::from(&root)
        .join(".overstory/agents")
        .join(agent_name)
        .join("identity.json");

    if identity_path.exists() {
        if let Ok(content) = std::fs::read_to_string(&identity_path) {
            if let Ok(identity) = serde_json::from_str::<serde_json::Value>(&content) {
                let capability = identity["capability"].as_str().unwrap_or("unknown");
                let sessions_completed = identity["sessionsCompleted"].as_u64().unwrap_or(0);
                ctx.push_str(&format!("Name: {}\n", agent_name));
                ctx.push_str(&format!("Capability: {}\n", capability));
                ctx.push_str(&format!("Sessions completed: {}\n", sessions_completed));
            }
        }
    } else {
        ctx.push_str(&format!("Name: {}\n", agent_name));
        ctx.push_str("New agent - no prior sessions\n");
    }
    ctx.push('\n');

    // Activation section — check if agent has active task
    let sessions_db = format!("{}/.overstory/sessions.db", root);
    if std::path::PathBuf::from(&sessions_db).exists() {
        if let Ok(store) = SessionStore::new(&sessions_db) {
            if let Ok(Some(session)) = store.get_by_name(agent_name) {
                use crate::types::AgentState;
                let is_active =
                    !matches!(session.state, AgentState::Completed | AgentState::Zombie);
                if is_active && !session.task_id.is_empty() {
                    ctx.push_str("## Activation\n");
                    ctx.push_str(&format!("You have a bound task: **{}**\n", session.task_id));
                    ctx.push_str(
                        "Read your overlay at `.claude/CLAUDE.md` and begin working immediately.\n",
                    );
                    ctx.push('\n');
                }
            }
        }
    }

    // Expertise section (skip if compact)
    if !compact && config.mulch.enabled {
        ctx.push_str("## Expertise\n");
        if let Some(mulch_out) = run_mulch_prime() {
            ctx.push_str(&mulch_out);
        }
    }

    Ok(ctx)
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn execute(
    agent: Option<String>,
    compact: bool,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    let context = match &agent {
        None => build_orchestrator_context(project_override)?,
        Some(name) => build_agent_context(name, compact, project_override)?,
    };

    if json {
        let output = PrimeOutput {
            agent: agent.clone(),
            compact,
            context,
        };
        println!("{}", json_output("prime", &output));
    } else {
        print!("{}", context);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_format_manifest_basic() {
        let entries = vec![
            json!({"name": "scout", "model": "haiku", "capabilities": ["explore", "research"], "canSpawn": false}),
            json!({"name": "builder", "model": "sonnet", "capabilities": ["implement", "refactor"], "canSpawn": true}),
        ];
        let result = format_manifest(&entries);
        assert!(result.contains("**scout** [haiku]: explore, research"));
        assert!(result.contains("**builder** [sonnet]: implement, refactor (can spawn)"));
    }

    #[test]
    fn test_format_manifest_empty() {
        let result = format_manifest(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_format_metrics_empty() {
        let result = format_metrics(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_format_metrics_with_sessions() {
        let sessions = vec![
            SessionMetrics {
                agent_name: "builder-1".into(),
                task_id: "task-abc".into(),
                capability: "implement".into(),
                started_at: "2026-01-01T00:00:00Z".into(),
                completed_at: Some("2026-01-01T00:02:00Z".into()),
                duration_ms: 120_000,
                exit_code: Some(0),
                merge_result: None,
                parent_agent: None,
                input_tokens: 1000,
                output_tokens: 500,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                estimated_cost_usd: None,
                model_used: None,
                run_id: None,
            },
            SessionMetrics {
                agent_name: "scout-1".into(),
                task_id: "task-xyz".into(),
                capability: "explore".into(),
                started_at: "2026-01-01T00:00:00Z".into(),
                completed_at: None,
                duration_ms: 45_000,
                exit_code: None,
                merge_result: None,
                parent_agent: None,
                input_tokens: 200,
                output_tokens: 100,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                estimated_cost_usd: None,
                model_used: None,
                run_id: None,
            },
        ];
        let result = format_metrics(&sessions);
        assert!(result.contains("builder-1 (implement): task-abc — completed (120s)"));
        assert!(result.contains("scout-1 (explore): task-xyz — in-progress (45s)"));
    }

    #[test]
    fn test_prime_output_json_structure() {
        let output = PrimeOutput {
            agent: Some("builder-1".into()),
            compact: false,
            context: "# Agent Context: builder-1\n".into(),
        };
        let json_str = json_output("prime", &output);
        let v: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(v["success"], true);
        assert_eq!(v["command"], "prime");
        assert_eq!(v["agent"], "builder-1");
        assert_eq!(v["compact"], false);
        assert!(v["context"].as_str().unwrap().contains("Agent Context"));
    }
}
