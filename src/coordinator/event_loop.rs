//! Coordinator event loop — the core of the native Rust coordinator.
//!
//! Polls every second:
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
    /// Set to true once a dispatch message is received; gates allAgentsDone exit trigger.
    pub has_received_work: bool,
}

// ---------------------------------------------------------------------------
// Message handler
// ---------------------------------------------------------------------------

/// Returns true if this message counts as "received work" (i.e. a dispatch).
fn handle_message(
    msg: &crate::types::MailMessage,
    mail_store: &MailStore,
    ctx: &LoopContext,
) -> bool {
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
            let coordinator_name = ctx.agent_name.clone();
            eprintln!("[coordinator] dispatch received, decomposing task: {body}");

            thread::spawn(move || {
                let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
                match rt.block_on(crate::coordinator::planner::decompose_task(&body, &api_key)) {
                    Ok(result) => {
                        eprintln!(
                            "[coordinator] decomposition complete: {} subtasks",
                            result.subtasks.len()
                        );
                        // Spawn subtasks via grove sling --headless
                        for subtask in &result.subtasks {
                            eprintln!(
                                "[coordinator] spawning: {} ({})",
                                subtask.title, subtask.capability
                            );
                            spawn_agent_headless(
                                &subtask.title,
                                &subtask.capability,
                                &coordinator_name,
                                &project_root,
                            );
                        }
                    }
                    Err(e) => {
                        eprintln!("[coordinator] decomposition failed: {e}");
                    }
                }
            });
            true
        }
        MailMessageType::WorkerDone => {
            eprintln!("[coordinator] agent completed: {}", msg.from);
            false
        }
        MailMessageType::Error => {
            eprintln!("[coordinator] ERROR from {}: {}", msg.from, msg.body);
            false
        }
        _ => {
            eprintln!("[coordinator] message type '{}' — no handler", msg.message_type);
            false
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
// Agent spawning
// ---------------------------------------------------------------------------

/// Spawn a headless agent via `grove sling --headless` in the given project root.
fn spawn_agent_headless(
    task_id: &str,
    capability: &str,
    parent_agent: &str,
    project_root: &str,
) {
    eprintln!("[coordinator] sling --headless: task={task_id} capability={capability}");
    let output = Command::new("grove")
        .args([
            "sling",
            task_id,
            "--capability",
            capability,
            "--parent",
            parent_agent,
            "--headless",
            "--skip-task-check",
            "--force-hierarchy",
        ])
        .current_dir(project_root)
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            eprintln!("[coordinator] sling succeeded: {}", stdout.trim());
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            eprintln!("[coordinator] sling failed for {task_id}: {stderr}");
        }
        Err(e) => {
            eprintln!("[coordinator] sling error for {task_id}: {e}");
        }
    }
}

// ---------------------------------------------------------------------------
// Public: run the event loop (blocking)
// ---------------------------------------------------------------------------

pub fn run(mut ctx: LoopContext) {
    eprintln!("[coordinator] event loop starting");

    // Open DB connections (reconnect each tick for reliability)
    let mut last_check = chrono::Utc::now();

    loop {
        // --- 1. Check mail for coordinator ---
        if let Ok(mail_store) = MailStore::new(&ctx.mail_db) {
            if let Ok(messages) = mail_store.get_unread(&ctx.agent_name) {
                for msg in &messages {
                    if handle_message(msg, &mail_store, &ctx) {
                        ctx.has_received_work = true;
                    }
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
    // Only evaluate allAgentsDone after coordinator has received actual work.
    // Without this guard the coordinator exits immediately on start (BUG-1).
    if ctx.exit_triggers.all_agents_done && ctx.has_received_work {
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
            has_received_work: false,
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
    fn test_should_not_exit_without_work() {
        // BUG-1 regression: all_agents_done=true but no work received → must not exit
        let ctx = test_ctx(":memory:");
        assert!(!ctx.has_received_work);
        assert!(!should_exit(&ctx), "should not exit before any work is received");
    }

    #[test]
    fn test_should_exit_after_work_received_no_agents() {
        // Once work is received and no other agents are active, should exit
        let mut ctx = test_ctx(":memory:");
        ctx.has_received_work = true;
        // :memory: DB has no sessions → others_active is empty → should exit
        assert!(should_exit(&ctx), "should exit when work received and no active agents");
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
        assert!(!ctx.has_received_work);
    }
}
