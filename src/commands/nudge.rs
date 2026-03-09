//! `grove nudge` — send a text nudge to an agent via tmux send-keys.
#![allow(dead_code)]
//!
//! Resolves the agent's tmux session from SessionStore (or orchestrator-tmux.json
//! for the orchestrator), checks a debounce window, then sends the message
//! via `tmux send-keys`. Includes retry logic and records a nudge event.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use serde::Serialize;

use crate::config::resolve_project_root;
use crate::db::events::EventStore;
use crate::db::sessions::SessionStore;
use crate::json::json_output;
use crate::logging::print_success;
use crate::types::{AgentState, EventLevel, EventType, InsertEvent};

const DEFAULT_MESSAGE: &str = "Check your mail inbox for new messages.";
const MAX_RETRIES: u32 = 3;
const RETRY_DELAY_MS: u64 = 500;
const DEBOUNCE_MS: u128 = 500;

// ---------------------------------------------------------------------------
// Execute
// ---------------------------------------------------------------------------

pub fn execute(
    agent_name: &str,
    message: Option<&str>,
    from: &str,
    force: bool,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    if agent_name.trim().is_empty() {
        return Err("Missing required argument: <agent-name>".to_string());
    }

    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let root = resolve_project_root(&cwd, project_override).map_err(|e| e.to_string())?;
    let root_str = root.to_string_lossy().to_string();
    let overstory = format!("{root_str}/.overstory");

    let raw_message = message.unwrap_or(DEFAULT_MESSAGE);
    let full_message = format!("[NUDGE from {from}] {raw_message}");

    let result = nudge_agent(&overstory, &root_str, agent_name, &full_message, force);

    // Record event (fire-and-forget)
    record_nudge_event(&overstory, agent_name, from, &full_message, result.delivered);

    if json {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Output {
            agent_name: String,
            delivered: bool,
            reason: Option<String>,
        }
        println!(
            "{}",
            json_output(
                "nudge",
                &Output {
                    agent_name: agent_name.to_string(),
                    delivered: result.delivered,
                    reason: result.reason.clone(),
                }
            )
        );
    } else if result.delivered {
        print_success("Nudge delivered", Some(agent_name));
    } else {
        let reason = result.reason.as_deref().unwrap_or("unknown error");
        return Err(format!("Nudge failed: {reason}"));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Core nudge logic
// ---------------------------------------------------------------------------

struct NudgeResult {
    delivered: bool,
    reason: Option<String>,
}

fn nudge_agent(
    overstory_dir: &str,
    project_root: &str,
    agent_name: &str,
    message: &str,
    force: bool,
) -> NudgeResult {
    // Resolve tmux session for this agent
    let tmux_session = match resolve_tmux_session(overstory_dir, project_root, agent_name) {
        Some(s) if !s.is_empty() => s,
        _ => {
            return NudgeResult {
                delivered: false,
                reason: Some(format!("No active session for agent \"{agent_name}\"")),
            };
        }
    };

    // Check debounce (unless forced)
    if !force {
        let state_path = format!("{overstory_dir}/nudge-state.json");
        if is_debounced(&state_path, agent_name) {
            return NudgeResult {
                delivered: false,
                reason: Some("Debounced: nudge sent too recently".to_string()),
            };
        }
    }

    // Verify session is alive
    if !is_tmux_session_alive(&tmux_session) {
        return NudgeResult {
            delivered: false,
            reason: Some(format!(
                "Tmux session \"{tmux_session}\" is not alive"
            )),
        };
    }

    // Send with retry
    if send_nudge_with_retry(&tmux_session, message) {
        // Record debounce timestamp
        let state_path = format!("{overstory_dir}/nudge-state.json");
        let _ = record_debounce(&state_path, agent_name);

        NudgeResult {
            delivered: true,
            reason: None,
        }
    } else {
        NudgeResult {
            delivered: false,
            reason: Some(format!("Failed to send after {MAX_RETRIES} attempts")),
        }
    }
}

// ---------------------------------------------------------------------------
// Session resolution
// ---------------------------------------------------------------------------

/// Resolve the tmux session name for an agent.
///
/// For regular agents: looks up SessionStore.
/// For "orchestrator": falls back to orchestrator-tmux.json.
fn resolve_tmux_session(
    overstory_dir: &str,
    project_root: &str,
    agent_name: &str,
) -> Option<String> {
    let sessions_db = format!("{overstory_dir}/sessions.db");
    if PathBuf::from(&sessions_db).exists() {
        if let Ok(store) = SessionStore::new(&sessions_db) {
            if let Ok(Some(session)) = store.get_by_name(agent_name) {
                if session.state != AgentState::Zombie
                    && session.state != AgentState::Completed
                    && !session.tmux_session.is_empty()
                {
                    return Some(session.tmux_session);
                }
            }
        }
    }

    // Fallback for orchestrator: check orchestrator-tmux.json
    if agent_name == "orchestrator" {
        return load_orchestrator_tmux_session(project_root);
    }

    None
}

/// Load the orchestrator's tmux session from orchestrator-tmux.json.
fn load_orchestrator_tmux_session(project_root: &str) -> Option<String> {
    let reg_path = format!("{project_root}/.overstory/orchestrator-tmux.json");
    let text = fs::read_to_string(&reg_path).ok()?;
    let val: serde_json::Value = serde_json::from_str(&text).ok()?;
    val.get("tmuxSession")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

// ---------------------------------------------------------------------------
// Debounce
// ---------------------------------------------------------------------------

fn is_debounced(state_path: &str, agent_name: &str) -> bool {
    let text = match fs::read_to_string(state_path) {
        Ok(t) => t,
        Err(_) => return false,
    };
    let state: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return false,
    };
    if let Some(last_ts) = state.get(agent_name).and_then(|v| v.as_u64()) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let elapsed = now.saturating_sub(last_ts) as u128;
        return elapsed < DEBOUNCE_MS;
    }
    false
}

fn record_debounce(state_path: &str, agent_name: &str) -> Result<(), String> {
    let mut state: serde_json::Map<String, serde_json::Value> =
        if let Ok(text) = fs::read_to_string(state_path) {
            serde_json::from_str(&text).unwrap_or_default()
        } else {
            serde_json::Map::new()
        };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    state.insert(agent_name.to_string(), serde_json::json!(now));
    let text = serde_json::to_string_pretty(&state).map_err(|e| e.to_string())?;
    fs::write(state_path, text).map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tmux helpers
// ---------------------------------------------------------------------------

fn is_tmux_session_alive(session_name: &str) -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", session_name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Send a nudge to a tmux session with retry logic.
///
/// Sends the message text, then after 500ms sends an empty Enter to ensure
/// submission (Claude Code's TUI may absorb the first Enter during re-render).
fn send_nudge_with_retry(tmux_session: &str, message: &str) -> bool {
    for attempt in 1..=MAX_RETRIES {
        let ok = Command::new("tmux")
            .args(["send-keys", "-t", tmux_session, message, "Enter"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if ok {
            // Follow-up Enter after short delay to ensure submission
            thread::sleep(Duration::from_millis(500));
            let _ = Command::new("tmux")
                .args(["send-keys", "-t", tmux_session, "", "Enter"])
                .output();
            return true;
        }

        if attempt < MAX_RETRIES {
            thread::sleep(Duration::from_millis(RETRY_DELAY_MS));
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Event recording
// ---------------------------------------------------------------------------

fn record_nudge_event(
    overstory_dir: &str,
    agent_name: &str,
    from: &str,
    message: &str,
    delivered: bool,
) {
    let events_db = format!("{overstory_dir}/events.db");
    let run_id = read_current_run_id(overstory_dir);

    let data = serde_json::json!({
        "type": "nudge",
        "from": from,
        "message": message,
        "delivered": delivered,
    });

    let event = InsertEvent {
        run_id,
        agent_name: agent_name.to_string(),
        session_id: None,
        event_type: EventType::Custom,
        tool_name: None,
        tool_args: None,
        tool_duration_ms: None,
        level: EventLevel::Info,
        data: Some(data.to_string()),
    };

    // Best-effort: never propagate event recording errors
    if let Ok(store) = EventStore::new(&events_db) {
        let _ = store.insert(&event);
    }
}

fn read_current_run_id(overstory_dir: &str) -> Option<String> {
    let path = format!("{overstory_dir}/current-run.txt");
    let text = fs::read_to_string(&path).ok()?;
    let trimmed = text.trim().to_string();
    if trimmed.is_empty() { None } else { Some(trimmed) }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    #[test]
    fn test_execute_empty_agent_name() {
        let result = execute("", None, "orchestrator", false, false, Some(Path::new("/tmp")));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Missing required argument"));
    }

    #[test]
    fn test_execute_no_sessions_db() {
        // Without a sessions.db the nudge should fail gracefully
        let result = execute(
            "test-agent",
            None,
            "orchestrator",
            false,
            false,
            Some(Path::new("/tmp")),
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("Nudge failed") || err.contains("No active session"));
    }

    #[test]
    fn test_is_debounced_no_file() {
        let result = is_debounced("/nonexistent/path/nudge-state.json", "agent-x");
        assert!(!result);
    }

    #[test]
    fn test_is_debounced_old_timestamp() {
        let dir = TempDir::new().unwrap();
        let state_path = dir.path().join("nudge-state.json");
        // Write a timestamp from 10 seconds ago (well past 500ms debounce)
        let old_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            - 10_000;
        let state = serde_json::json!({ "my-agent": old_ts });
        fs::write(&state_path, state.to_string()).unwrap();
        let result = is_debounced(state_path.to_str().unwrap(), "my-agent");
        assert!(!result);
    }

    #[test]
    fn test_is_debounced_recent_timestamp() {
        let dir = TempDir::new().unwrap();
        let state_path = dir.path().join("nudge-state.json");
        // Write a timestamp from right now
        let now_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let state = serde_json::json!({ "my-agent": now_ts });
        fs::write(&state_path, state.to_string()).unwrap();
        let result = is_debounced(state_path.to_str().unwrap(), "my-agent");
        assert!(result);
    }

    #[test]
    fn test_record_debounce_creates_file() {
        let dir = TempDir::new().unwrap();
        let state_path = dir.path().join("nudge-state.json");
        record_debounce(state_path.to_str().unwrap(), "my-agent").unwrap();
        assert!(state_path.exists());
        let text = fs::read_to_string(&state_path).unwrap();
        let val: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(val.get("my-agent").is_some());
    }

    #[test]
    fn test_record_debounce_updates_existing() {
        let dir = TempDir::new().unwrap();
        let state_path = dir.path().join("nudge-state.json");
        let initial = serde_json::json!({ "other-agent": 12345 });
        fs::write(&state_path, initial.to_string()).unwrap();
        record_debounce(state_path.to_str().unwrap(), "my-agent").unwrap();
        let text = fs::read_to_string(&state_path).unwrap();
        let val: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(val.get("other-agent").is_some());
        assert!(val.get("my-agent").is_some());
    }

    #[test]
    fn test_is_tmux_session_alive_nonexistent() {
        let result = is_tmux_session_alive("grove-test-nudge-nonexistent-xyz");
        assert!(!result);
    }

    #[test]
    fn test_read_current_run_id_no_file() {
        let result = read_current_run_id("/nonexistent/overstory");
        assert!(result.is_none());
    }

    #[test]
    fn test_read_current_run_id_with_content() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("current-run.txt"), b"run-abc-123\n").unwrap();
        let result = read_current_run_id(dir.path().to_str().unwrap());
        assert_eq!(result, Some("run-abc-123".to_string()));
    }

    #[test]
    fn test_read_current_run_id_empty_file() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("current-run.txt"), b"   \n").unwrap();
        let result = read_current_run_id(dir.path().to_str().unwrap());
        assert!(result.is_none());
    }

    #[test]
    fn test_load_orchestrator_tmux_no_file() {
        let result = load_orchestrator_tmux_session("/nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_load_orchestrator_tmux_with_file() {
        let dir = TempDir::new().unwrap();
        let overstory_dir = dir.path().join(".overstory");
        fs::create_dir_all(&overstory_dir).unwrap();
        let reg_path = overstory_dir.join("orchestrator-tmux.json");
        fs::write(&reg_path, r#"{"tmuxSession":"overstory-main-orch"}"#).unwrap();
        let result = load_orchestrator_tmux_session(dir.path().to_str().unwrap());
        assert_eq!(result, Some("overstory-main-orch".to_string()));
    }
}
