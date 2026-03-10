//! `grove log` — session lifecycle hooks for agent sessions.
//!
//! Subcommands:
//!   `grove log session-start --agent <name>` — transition session to "working"
//!   `grove log session-end --agent <name> [--exit-code <n>]` — mark session completed
//!
//! These are called by settings.local.json hooks (SessionStart / Stop) when a
//! Claude Code agent starts and stops.

use std::path::Path;

use crate::config::resolve_project_root;
use crate::db::sessions::SessionStore;
use crate::json::json_output;
use crate::logging::print_success;
use crate::types::AgentState;

// ---------------------------------------------------------------------------
// session-start
// ---------------------------------------------------------------------------

/// Mark a session as "working". Called by the SessionStart hook.
pub fn execute_session_start(
    agent_name: &str,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    if agent_name.trim().is_empty() {
        return Err("--agent is required".to_string());
    }

    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let root = resolve_project_root(&cwd, project_override).map_err(|e| e.to_string())?;
    let sessions_db = root
        .join(".overstory")
        .join("sessions.db")
        .to_string_lossy()
        .to_string();

    let store = SessionStore::new(&sessions_db).map_err(|e| e.to_string())?;

    // Check session exists
    let session = store.get_by_name(agent_name).map_err(|e| e.to_string())?;

    if session.is_none() {
        // Session not found — this can happen if grove status DB doesn't have it yet.
        // Non-fatal: the hook should not crash the agent.
        if !json {
            eprintln!("[grove log] Session not found for agent \"{agent_name}\" — skipping");
        }
        return Ok(());
    }

    store
        .update_state(agent_name, AgentState::Working)
        .map_err(|e| e.to_string())?;
    store
        .update_last_activity(agent_name)
        .map_err(|e| e.to_string())?;

    if json {
        println!(
            "{}",
            json_output(
                "log session-start",
                &serde_json::json!({ "agentName": agent_name, "state": "working" })
            )
        );
    } else {
        print_success("Session started", Some(agent_name));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// session-end
// ---------------------------------------------------------------------------

/// Mark a session as "completed". Called by the Stop hook.
pub fn execute_session_end(
    agent_name: &str,
    exit_code: Option<i32>,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    if agent_name.trim().is_empty() {
        return Err("--agent is required".to_string());
    }

    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let root = resolve_project_root(&cwd, project_override).map_err(|e| e.to_string())?;
    let sessions_db = root
        .join(".overstory")
        .join("sessions.db")
        .to_string_lossy()
        .to_string();

    let store = SessionStore::new(&sessions_db).map_err(|e| e.to_string())?;

    // Check session exists
    let session = store.get_by_name(agent_name).map_err(|e| e.to_string())?;

    if session.is_none() {
        if !json {
            eprintln!("[grove log] Session not found for agent \"{agent_name}\" — skipping");
        }
        return Ok(());
    }

    let final_state = AgentState::Completed;
    store
        .update_state(agent_name, final_state)
        .map_err(|e| e.to_string())?;
    store
        .update_last_activity(agent_name)
        .map_err(|e| e.to_string())?;

    // Record to metrics DB (best-effort — non-fatal on error)
    let _ = record_session_metrics(agent_name, exit_code, &root);

    if json {
        println!(
            "{}",
            json_output(
                "log session-end",
                &serde_json::json!({
                    "agentName": agent_name,
                    "state": "completed",
                    "exitCode": exit_code,
                })
            )
        );
    } else {
        print_success("Session ended", Some(agent_name));
    }

    Ok(())
}

/// Record a session-end entry in metrics.db.
/// Best-effort — errors are silently ignored by the caller.
fn record_session_metrics(
    agent_name: &str,
    exit_code: Option<i32>,
    project_root: &Path,
) -> Result<(), String> {
    use crate::db::metrics::MetricsStore;
    use crate::types::SessionMetrics;

    let metrics_db = project_root
        .join(".overstory")
        .join("metrics.db")
        .to_string_lossy()
        .to_string();

    let sessions_db = project_root
        .join(".overstory")
        .join("sessions.db")
        .to_string_lossy()
        .to_string();

    let session_store = SessionStore::new(&sessions_db).map_err(|e| e.to_string())?;
    let session = session_store
        .get_by_name(agent_name)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Session \"{agent_name}\" not found"))?;

    let metrics_store = MetricsStore::new(&metrics_db).map_err(|e| e.to_string())?;

    let started_at_ms = chrono::DateTime::parse_from_rfc3339(&session.started_at)
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0);
    let completed_at_ms = chrono::Utc::now().timestamp_millis();
    let duration_ms = if started_at_ms > 0 {
        (completed_at_ms - started_at_ms).max(0)
    } else {
        0
    };

    metrics_store
        .record_session(&SessionMetrics {
            agent_name: agent_name.to_string(),
            task_id: session.task_id.clone(),
            capability: session.capability.clone(),
            started_at: session.started_at.clone(),
            completed_at: Some(chrono::Utc::now().to_rfc3339()),
            duration_ms,
            exit_code: exit_code.map(|c| c as i64),
            merge_result: None,
            parent_agent: session.parent_agent.clone(),
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            estimated_cost_usd: None,
            model_used: None,
            run_id: session.run_id.clone(),
        })
        .map_err(|e| e.to_string())?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::sessions::SessionStore;
    use crate::types::{AgentSession, AgentState};
    use tempfile::TempDir;

    fn make_test_session(name: &str) -> AgentSession {
        AgentSession {
            id: uuid::Uuid::new_v4().to_string(),
            agent_name: name.to_string(),
            capability: "builder".to_string(),
            worktree_path: "/tmp/wt".to_string(),
            branch_name: "test-branch".to_string(),
            task_id: "task-001".to_string(),
            tmux_session: String::new(),
            state: AgentState::Booting,
            pid: None,
            parent_agent: None,
            depth: 1,
            run_id: Some("run-001".to_string()),
            started_at: chrono::Utc::now().to_rfc3339(),
            last_activity: chrono::Utc::now().to_rfc3339(),
            escalation_level: 0,
            stalled_since: None,
            transcript_path: None,
        }
    }

    #[test]
    fn test_session_start_updates_state() {
        let dir = TempDir::new().unwrap();
        let overstory = dir.path().join(".overstory");
        std::fs::create_dir_all(&overstory).unwrap();
        let db_path = overstory.join("sessions.db").to_string_lossy().to_string();

        let store = SessionStore::new(&db_path).unwrap();
        let session = make_test_session("test-agent");
        store.upsert(&session).unwrap();

        let result = execute_session_start("test-agent", false, Some(dir.path()));
        assert!(result.is_ok(), "session-start failed: {result:?}");

        let updated = store.get_by_name("test-agent").unwrap().unwrap();
        assert_eq!(updated.state, AgentState::Working);
    }

    #[test]
    fn test_session_end_updates_state() {
        let dir = TempDir::new().unwrap();
        let overstory = dir.path().join(".overstory");
        std::fs::create_dir_all(&overstory).unwrap();
        let db_path = overstory.join("sessions.db").to_string_lossy().to_string();

        let store = SessionStore::new(&db_path).unwrap();
        let mut session = make_test_session("test-agent-2");
        session.state = AgentState::Working;
        store.upsert(&session).unwrap();

        let result = execute_session_end("test-agent-2", Some(0), false, Some(dir.path()));
        assert!(result.is_ok(), "session-end failed: {result:?}");

        let updated = store.get_by_name("test-agent-2").unwrap().unwrap();
        assert_eq!(updated.state, AgentState::Completed);
    }

    #[test]
    fn test_session_start_missing_agent() {
        let result = execute_session_start("", false, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_session_start_nonexistent_session() {
        let dir = TempDir::new().unwrap();
        let overstory = dir.path().join(".overstory");
        std::fs::create_dir_all(&overstory).unwrap();

        // Don't insert any session — should succeed non-fatally
        let result = execute_session_start("nonexistent-agent", false, Some(dir.path()));
        assert!(result.is_ok());
    }
}
