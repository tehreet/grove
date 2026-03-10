//! `grove nudge` — send a nudge to an agent via mail.
//!
//! Resolves the agent session, checks debounce window, then sends
//! a nudge message via the mail system. The agent receives it through
//! the PostToolUse mail check hook.

use std::path::Path;

use serde::Serialize;

use crate::config::resolve_project_root;
use crate::db::events::EventStore;
use crate::db::mail::MailStore;
use crate::db::sessions::SessionStore;
use crate::json::json_output;
use crate::logging::print_success;
use crate::types::{
    AgentState, EventLevel, EventType, InsertEvent, InsertMailMessage, MailMessageType,
    MailPriority,
};

const DEFAULT_MESSAGE: &str = "Check your mail inbox for new messages.";
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

    let result = nudge_agent(&overstory, agent_name, &full_message, from, force);

    // Record event (fire-and-forget)
    record_nudge_event(
        &overstory,
        agent_name,
        from,
        &full_message,
        result.delivered,
    );

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
// Nudge via mail
// ---------------------------------------------------------------------------

struct NudgeResult {
    delivered: bool,
    reason: Option<String>,
}

fn nudge_agent(
    overstory_dir: &str,
    agent_name: &str,
    message: &str,
    from: &str,
    force: bool,
) -> NudgeResult {
    // Check session exists and is active
    let sessions_db = format!("{overstory_dir}/sessions.db");
    let session = match SessionStore::new(&sessions_db)
        .ok()
        .and_then(|s| s.get_by_name(agent_name).ok().flatten())
    {
        Some(s) => s,
        None => {
            return NudgeResult {
                delivered: false,
                reason: Some(format!("No active session for agent \"{agent_name}\"")),
            };
        }
    };

    // Check agent is in a nudgeable state
    match session.state {
        AgentState::Completed | AgentState::Zombie => {
            return NudgeResult {
                delivered: false,
                reason: Some(format!(
                    "Agent \"{agent_name}\" is {} — cannot nudge",
                    session.state
                )),
            };
        }
        _ => {}
    }

    // Debounce: check last nudge timestamp
    if !force {
        let events_db = format!("{overstory_dir}/events.db");
        if let Ok(store) = EventStore::new(&events_db) {
            if let Ok(events) = store.get_by_agent(
                agent_name,
                Some(crate::types::EventQueryOptions {
                    limit: Some(5),
                    ..Default::default()
                }),
            ) {
                let now = chrono::Utc::now().timestamp_millis() as u128;
                for ev in &events {
                    if ev.event_type == EventType::Custom {
                        if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&ev.created_at) {
                            let ev_ms = ts.timestamp_millis() as u128;
                            if now.saturating_sub(ev_ms) < DEBOUNCE_MS {
                                return NudgeResult {
                                    delivered: false,
                                    reason: Some(
                                        "Debounce: recent nudge already sent. Use --force to bypass."
                                            .to_string(),
                                    ),
                                };
                            }
                        }
                    }
                }
            }
        }
    }

    // Send nudge as mail
    let mail_db = format!("{overstory_dir}/mail.db");
    match MailStore::new(&mail_db) {
        Ok(store) => {
            let msg = InsertMailMessage {
                id: None,
                from_agent: from.to_string(),
                to_agent: agent_name.to_string(),
                subject: "Nudge".to_string(),
                body: message.to_string(),
                priority: MailPriority::High,
                message_type: MailMessageType::Status,
                thread_id: None,
                payload: None,
            };
            match store.insert(&msg) {
                Ok(_) => NudgeResult {
                    delivered: true,
                    reason: None,
                },
                Err(e) => NudgeResult {
                    delivered: false,
                    reason: Some(format!("Failed to send nudge mail: {e}")),
                },
            }
        }
        Err(e) => NudgeResult {
            delivered: false,
            reason: Some(format!("Failed to open mail.db: {e}")),
        },
    }
}

// ---------------------------------------------------------------------------
// Event recording
// ---------------------------------------------------------------------------

fn record_nudge_event(
    overstory_dir: &str,
    agent_name: &str,
    from: &str,
    _message: &str,
    delivered: bool,
) {
    let events_db = format!("{overstory_dir}/events.db");
    if let Ok(store) = EventStore::new(&events_db) {
        let _ = store.insert(&InsertEvent {
            run_id: None,
            agent_name: agent_name.to_string(),
            session_id: None,
            event_type: EventType::Custom,
            tool_name: None,
            tool_args: None,
            tool_duration_ms: None,
            level: EventLevel::Info,
            data: Some(format!(
                r#"{{"action":"nudge","from":"{}","delivered":{}}}"#,
                from, delivered
            )),
        });
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_test_env() -> (TempDir, String) {
        let dir = TempDir::new().unwrap();
        let overstory = dir.path().join(".overstory");
        std::fs::create_dir_all(&overstory).unwrap();
        let sessions_db = overstory.join("sessions.db").to_string_lossy().to_string();
        let store = SessionStore::new(&sessions_db).unwrap();
        // Create a working agent
        let session = crate::types::AgentSession {
            id: "test-1".into(),
            agent_name: "agent-a".into(),
            capability: "builder".into(),
            worktree_path: "/tmp".into(),
            branch_name: "test".into(),
            task_id: "task-1".into(),
            tmux_session: String::new(),
            state: AgentState::Working,
            pid: Some(99999),
            parent_agent: None,
            depth: 0,
            run_id: None,
            started_at: chrono::Utc::now().to_rfc3339(),
            last_activity: chrono::Utc::now().to_rfc3339(),
            escalation_level: 0,
            stalled_since: None,
            transcript_path: None,
        };
        store.upsert(&session).unwrap();
        (dir, overstory.to_string_lossy().to_string())
    }

    #[test]
    fn test_nudge_delivers_via_mail() {
        let (_dir, overstory) = setup_test_env();
        let mail_db = format!("{overstory}/mail.db");
        let _ = MailStore::new(&mail_db).unwrap(); // create the DB

        let result = nudge_agent(&overstory, "agent-a", "Wake up!", "operator", false);
        assert!(result.delivered);

        // Verify mail was sent
        let store = MailStore::new(&mail_db).unwrap();
        let msgs = store.get_unread("agent-a").unwrap();
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].body.contains("Wake up!"));
    }

    #[test]
    fn test_nudge_fails_for_missing_agent() {
        let (_dir, overstory) = setup_test_env();
        let result = nudge_agent(&overstory, "nonexistent", "hello", "operator", false);
        assert!(!result.delivered);
        assert!(result.reason.unwrap().contains("No active session"));
    }

    #[test]
    fn test_nudge_fails_for_completed_agent() {
        let (dir, overstory) = setup_test_env();
        let sessions_db = format!("{overstory}/sessions.db");
        let store = SessionStore::new(&sessions_db).unwrap();
        store
            .update_state("agent-a", AgentState::Completed)
            .unwrap();

        let result = nudge_agent(&overstory, "agent-a", "hello", "operator", false);
        assert!(!result.delivered);
        assert!(result.reason.unwrap().contains("completed"));
    }
}
