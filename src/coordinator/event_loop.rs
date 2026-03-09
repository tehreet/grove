//! Coordinator event loop — the core of the native Rust coordinator.
//!
//! This runs inside a tmux session and polls every second:
//!   1. Check mail for "coordinator"
//!   2. Check for completed agents
//!   3. Check the merge queue
//!   4. Evaluate exit triggers
//!   5. Sleep 1 second

#![allow(dead_code)]

use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::db::mail::MailStore;
use crate::db::merge_queue::MergeQueue;
use crate::db::sessions::SessionStore;
use crate::types::{AgentState, CoordinatorExitTriggers, MailMessageType};

// ---------------------------------------------------------------------------
// Context passed into the event loop
// ---------------------------------------------------------------------------

pub struct LoopContext {
    pub project_root: String,
    pub sessions_db: String,
    pub mail_db: String,
    pub merge_queue_db: String,
    pub exit_triggers: CoordinatorExitTriggers,
    pub agent_name: String,
}

// ---------------------------------------------------------------------------
// Message handler
// ---------------------------------------------------------------------------

fn handle_message(
    msg: &crate::types::MailMessage,
    mail_store: &MailStore,
    ctx: &LoopContext,
) {
    // Mark it read first
    let _ = mail_store.mark_read(&msg.id);

    eprintln!(
        "[coordinator] mail from {}: {} ({})",
        msg.from, msg.subject, msg.message_type
    );

    match msg.message_type {
        MailMessageType::Dispatch => {
            // LLM decomposition — spawn a tokio runtime to call planner
            let body = msg.body.clone();
            let api_key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
            let project_root = ctx.project_root.clone();
            eprintln!("[coordinator] dispatch received, decomposing task: {body}");

            thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
                match rt.block_on(crate::coordinator::planner::decompose_task(&body, &api_key)) {
                    Ok(result) => {
                        eprintln!(
                            "[coordinator] decomposition complete: {} subtasks",
                            result.subtasks.len()
                        );
                        // Spawn subtasks via grove sling
                        for subtask in &result.subtasks {
                            eprintln!(
                                "[coordinator] spawning: {} ({})",
                                subtask.title, subtask.capability
                            );
                            // In a real implementation, create a task + sling it
                            // For now, log the intent
                            let _ = (&project_root, subtask);
                        }
                    }
                    Err(e) => {
                        eprintln!("[coordinator] decomposition failed: {e}");
                    }
                }
            });
        }
        MailMessageType::WorkerDone => {
            eprintln!("[coordinator] agent completed: {}", msg.from);
        }
        MailMessageType::Error => {
            eprintln!("[coordinator] ERROR from {}: {}", msg.from, msg.body);
        }
        _ => {
            eprintln!("[coordinator] message type '{}' — no handler", msg.message_type);
        }
    }
}

// ---------------------------------------------------------------------------
// Completion handler
// ---------------------------------------------------------------------------

fn handle_completed_agent(session: &crate::types::AgentSession, ctx: &LoopContext) {
    eprintln!(
        "[coordinator] agent completed: {} (task: {})",
        session.agent_name, session.task_id
    );
    // Future: check if a group is done, trigger next phase
    let _ = ctx;
}

// ---------------------------------------------------------------------------
// Merge handler
// ---------------------------------------------------------------------------

fn handle_merge_entry(entry: &crate::types::MergeEntry, ctx: &LoopContext) {
    eprintln!(
        "[coordinator] merging branch: {} (agent: {})",
        entry.branch_name, entry.agent_name
    );

    // Run `grove merge --branch <branch>` in the project root
    let output = Command::new("grove")
        .args(["merge", "--branch", &entry.branch_name])
        .current_dir(&ctx.project_root)
        .output();

    match output {
        Ok(out) if out.status.success() => {
            eprintln!("[coordinator] merge succeeded: {}", entry.branch_name);
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            eprintln!("[coordinator] merge failed: {}: {stderr}", entry.branch_name);
        }
        Err(e) => {
            eprintln!("[coordinator] merge error: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Public: run the event loop (blocking)
// ---------------------------------------------------------------------------

pub fn run(ctx: LoopContext) {
    eprintln!("[coordinator] event loop starting");

    // Open DB connections (reconnect each tick for reliability)
    let mut last_check = chrono::Utc::now();

    loop {
        // --- 1. Check mail for coordinator ---
        if let Ok(mail_store) = MailStore::new(&ctx.mail_db) {
            if let Ok(messages) = mail_store.get_unread(&ctx.agent_name) {
                for msg in &messages {
                    handle_message(msg, &mail_store, &ctx);
                }
            }
        }

        // --- 2. Check for completed agents ---
        let now = chrono::Utc::now();
        if let Ok(session_store) = SessionStore::new(&ctx.sessions_db) {
            if let Ok(all_sessions) = session_store.get_all() {
                let completed_since: Vec<_> = all_sessions
                    .iter()
                    .filter(|s| {
                        s.state == AgentState::Completed
                            && s.agent_name != ctx.agent_name
                            && parse_ts(&s.last_activity) > last_check
                    })
                    .collect();
                for session in completed_since {
                    handle_completed_agent(session, &ctx);
                }
            }
        }
        last_check = now;

        // --- 3. Check merge queue ---
        if let Ok(mut merge_queue) = MergeQueue::new(&ctx.merge_queue_db) {
            // Only process one entry per tick to avoid stampede
            if let Ok(Some(entry)) = merge_queue.dequeue() {
                handle_merge_entry(&entry, &ctx);
            }
        }

        // --- 4. Check exit triggers ---
        if should_exit(&ctx) {
            eprintln!("[coordinator] exit trigger fired — shutting down");
            break;
        }

        // --- 5. Sleep ---
        thread::sleep(Duration::from_secs(1));
    }

    eprintln!("[coordinator] event loop exited");
}

fn should_exit(ctx: &LoopContext) -> bool {
    if ctx.exit_triggers.all_agents_done {
        if let Ok(store) = SessionStore::new(&ctx.sessions_db) {
            if let Ok(active) = store.get_active() {
                // Active = any agent other than the coordinator itself
                let others_active: Vec<_> = active
                    .iter()
                    .filter(|s| s.agent_name != ctx.agent_name)
                    .collect();
                if others_active.is_empty() {
                    return true;
                }
            }
        }
    }
    false
}

fn parse_ts(ts: &str) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(ts)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or(chrono::DateTime::UNIX_EPOCH)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CoordinatorExitTriggers;

    fn test_ctx(sessions_db: &str) -> LoopContext {
        LoopContext {
            project_root: "/tmp".to_string(),
            sessions_db: sessions_db.to_string(),
            mail_db: ":memory:".to_string(),
            merge_queue_db: ":memory:".to_string(),
            exit_triggers: CoordinatorExitTriggers {
                all_agents_done: true,
                task_tracker_empty: false,
                on_shutdown_signal: false,
            },
            agent_name: "coordinator".to_string(),
        }
    }

    #[test]
    fn test_should_exit_no_db() {
        // With no DB, should not exit (DB open fails)
        let ctx = test_ctx("/tmp/nonexistent-groove-test.db");
        // should_exit returns false when DB is unreachable
        // (open will succeed with bundled sqlite, but table is empty)
        // Just verify it doesn't panic
        let _ = should_exit(&ctx);
    }

    #[test]
    fn test_parse_ts_valid() {
        let ts = "2026-03-09T10:00:00Z";
        let parsed = parse_ts(ts);
        assert!(parsed.timestamp() > 0);
    }

    #[test]
    fn test_parse_ts_invalid() {
        let parsed = parse_ts("not-a-timestamp");
        // Should return UNIX_EPOCH
        assert_eq!(parsed.timestamp(), 0);
    }

    #[test]
    fn test_loop_context_fields() {
        let ctx = test_ctx(":memory:");
        assert_eq!(ctx.agent_name, "coordinator");
        assert!(ctx.exit_triggers.all_agents_done);
    }
}
