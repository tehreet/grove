//! `grove coordinator` — persistent coordinator event loop.
//!
//! Subcommands:
//!   start   — register session, spawn tmux, run event loop
//!   stop    — SIGTERM the coordinator, update session
//!   status  — show coordinator info
//!   send    — insert a message into the coordinator's mailbox

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

use crate::config::{load_config, resolve_project_root};
use crate::coordinator::event_loop::{run, LoopContext};
use crate::db::mail::MailStore;
use crate::db::sessions::SessionStore;
use crate::json::{json_error, json_output};
use crate::logging::brand_bold;
use crate::types::{AgentSession, AgentState, InsertMailMessage, MailMessageType, MailPriority};

const COORDINATOR_AGENT: &str = "coordinator";

// ---------------------------------------------------------------------------
// start
// ---------------------------------------------------------------------------

pub fn execute_start(
    no_attach: bool,
    _profile: Option<&str>,
    foreground: bool,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let root = resolve_project_root(&cwd, project_override).map_err(|e| e.to_string())?;
    let root_str = root.to_string_lossy().to_string();

    let sessions_db = format!("{root_str}/.overstory/sessions.db");
    let mail_db = format!("{root_str}/.overstory/mail.db");
    let merge_queue_db = format!("{root_str}/.overstory/merge-queue.db");

    // If --foreground: run the event loop directly in this process.
    // This is how the tmux session runs the coordinator.
    if foreground {
        let store = SessionStore::new(&sessions_db).map_err(|e| e.to_string())?;

        // Update session state to "working"
        let _ = store.update_state(COORDINATOR_AGENT, AgentState::Working);
        let _ = store.update_last_activity(COORDINATOR_AGENT);

        // Load config for exit triggers
        let config = load_config(&root, project_override).unwrap_or_default();
        let exit_triggers = config
            .coordinator
            .map(|c| c.exit_triggers)
            .unwrap_or(crate::types::CoordinatorExitTriggers {
                all_agents_done: false,
                task_tracker_empty: false,
                on_shutdown_signal: true,
            });

        let sessions_db_path = sessions_db.clone();
        let ctx = LoopContext {
            project_root: root_str,
            sessions_db,
            mail_db,
            merge_queue_db,
            exit_triggers,
            agent_name: COORDINATOR_AGENT.to_string(),
        };

        run(ctx);

        // After loop exits, mark session completed
        if let Ok(store) = SessionStore::new(&sessions_db_path) {
            let _ = store.update_state(COORDINATOR_AGENT, AgentState::Completed);
            let _ = store.update_last_activity(COORDINATOR_AGENT);
        }
        return Ok(());
    }

    // Not foreground: spawn tmux session + register session

    // Check if coordinator is already running
    if let Ok(store) = SessionStore::new(&sessions_db) {
        if let Ok(Some(existing)) = store.get_by_name(COORDINATOR_AGENT) {
            if existing.state == AgentState::Working || existing.state == AgentState::Booting {
                let msg = format!(
                    "Coordinator is already running (state: {:?}, tmux: {})",
                    existing.state, existing.tmux_session
                );
                if json {
                    println!("{}", json_error("coordinator start", &msg));
                } else {
                    eprintln!("{msg}");
                }
                return Err(msg);
            }
        }
    }

    // Determine session name
    let config = load_config(&root, project_override).unwrap_or_default();
    let project_name = config.project.name.replace(' ', "-").to_lowercase();
    let project_name = if project_name.is_empty() {
        "grove".to_string()
    } else {
        project_name
    };
    let tmux_session_name = format!("overstory-{project_name}-coordinator");

    // Register session (state = booting)
    let now = chrono::Utc::now().to_rfc3339();
    let session = AgentSession {
        id: uuid::Uuid::new_v4().to_string(),
        agent_name: COORDINATOR_AGENT.to_string(),
        capability: "coordinator".to_string(),
        worktree_path: root_str.clone(),
        branch_name: String::new(),
        task_id: String::new(),
        tmux_session: tmux_session_name.clone(),
        state: AgentState::Booting,
        pid: None,
        parent_agent: None,
        depth: 0,
        run_id: None,
        started_at: now.clone(),
        last_activity: now,
        escalation_level: 0,
        stalled_since: None,
        transcript_path: None,
    };

    {
        let store = SessionStore::new(&sessions_db).map_err(|e| e.to_string())?;
        store.upsert(&session).map_err(|e| e.to_string())?;
    }

    // Build the command to run inside tmux:
    // grove coordinator start --foreground [--project <root>]
    let grove_bin = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "grove".to_string());

    let tmux_command = format!(
        "{grove_bin} coordinator start --foreground --project {root_str}"
    );

    // Create tmux session
    let tmux_result = Command::new("tmux")
        .args([
            "new-session",
            "-d",
            "-s",
            &tmux_session_name,
            "-c",
            &root_str,
            &tmux_command,
        ])
        .output()
        .map_err(|e| format!("Failed to launch tmux: {e}"))?;

    if !tmux_result.status.success() {
        let stderr = String::from_utf8_lossy(&tmux_result.stderr);
        return Err(format!("Failed to create tmux session: {stderr}"));
    }

    // Get the PID of the pane
    let pid_output = Command::new("tmux")
        .args([
            "list-panes",
            "-t",
            &tmux_session_name,
            "-F",
            "#{pane_pid}",
        ])
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse::<i64>()
                .ok()
        });

    // Update session with PID
    if let Some(pid) = pid_output {
        if let Ok(store) = SessionStore::new(&sessions_db) {
            // Re-upsert with pid
            let mut s = session.clone();
            s.pid = Some(pid);
            let _ = store.upsert(&s);
        }
    }

    if json {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Output {
            started: bool,
            agent_name: String,
            tmux_session: String,
            pid: Option<i64>,
        }
        println!(
            "{}",
            json_output(
                "coordinator start",
                &Output {
                    started: true,
                    agent_name: COORDINATOR_AGENT.to_string(),
                    tmux_session: tmux_session_name.clone(),
                    pid: pid_output,
                }
            )
        );
    } else {
        println!("{} coordinator started", brand_bold("grove"));
        println!("  Tmux session: {tmux_session_name}");
        if let Some(pid) = pid_output {
            println!("  PID: {pid}");
        }
        if !no_attach {
            println!("  Attaching to session (Ctrl-b d to detach)...");
            let _ = Command::new("tmux")
                .args(["attach-session", "-t", &tmux_session_name])
                .status();
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// stop
// ---------------------------------------------------------------------------

pub fn execute_stop(json: bool, project_override: Option<&Path>) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let root = resolve_project_root(&cwd, project_override).map_err(|e| e.to_string())?;
    let root_str = root.to_string_lossy().to_string();
    let sessions_db = format!("{root_str}/.overstory/sessions.db");

    if !PathBuf::from(&sessions_db).exists() {
        let msg = "Coordinator not found: sessions.db does not exist";
        if json {
            println!("{}", json_error("coordinator stop", msg));
        } else {
            eprintln!("{msg}");
        }
        return Err(msg.to_string());
    }

    let store = SessionStore::new(&sessions_db).map_err(|e| e.to_string())?;
    let session = store
        .get_by_name(COORDINATOR_AGENT)
        .map_err(|e| e.to_string())?
        .ok_or("Coordinator session not found")?;

    let mut killed = false;

    // SIGTERM to the tmux session (kills the process inside)
    if !session.tmux_session.is_empty() {
        let alive = Command::new("tmux")
            .args(["has-session", "-t", &session.tmux_session])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);

        if alive {
            // Send SIGTERM to the pane process
            if let Some(pid) = session.pid {
                let _ = Command::new("kill")
                    .args(["-TERM", &pid.to_string()])
                    .output();
                killed = true;
            }
            // Wait briefly then kill the tmux session
            std::thread::sleep(std::time::Duration::from_millis(500));
            let _ = Command::new("tmux")
                .args(["kill-session", "-t", &session.tmux_session])
                .output();
        }
    }

    store
        .update_state(COORDINATOR_AGENT, AgentState::Completed)
        .map_err(|e| e.to_string())?;
    store
        .update_last_activity(COORDINATOR_AGENT)
        .map_err(|e| e.to_string())?;

    if json {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Output {
            stopped: bool,
            agent_name: String,
            killed: bool,
        }
        println!(
            "{}",
            json_output(
                "coordinator stop",
                &Output {
                    stopped: true,
                    agent_name: COORDINATOR_AGENT.to_string(),
                    killed,
                }
            )
        );
    } else {
        println!("{} coordinator stopped", brand_bold("grove"));
        if killed {
            println!("  Process terminated (SIGTERM)");
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// status
// ---------------------------------------------------------------------------

pub fn execute_status(json: bool, project_override: Option<&Path>) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let root = resolve_project_root(&cwd, project_override).map_err(|e| e.to_string())?;
    let root_str = root.to_string_lossy().to_string();
    let sessions_db = format!("{root_str}/.overstory/sessions.db");

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Output {
        running: bool,
        state: Option<String>,
        tmux_session: Option<String>,
        pid: Option<i64>,
        started_at: Option<String>,
        last_activity: Option<String>,
        active_agents: usize,
    }

    if !PathBuf::from(&sessions_db).exists() {
        if json {
            println!(
                "{}",
                json_output(
                    "coordinator status",
                    &Output {
                        running: false,
                        state: None,
                        tmux_session: None,
                        pid: None,
                        started_at: None,
                        last_activity: None,
                        active_agents: 0,
                    }
                )
            );
        } else {
            println!("{} coordinator: not running", brand_bold("grove"));
        }
        return Ok(());
    }

    let store = SessionStore::new(&sessions_db).map_err(|e| e.to_string())?;
    let session = store.get_by_name(COORDINATOR_AGENT).map_err(|e| e.to_string())?;
    let active_agents = store
        .get_active()
        .map(|v| {
            v.iter()
                .filter(|s| s.agent_name != COORDINATOR_AGENT)
                .count()
        })
        .unwrap_or(0);

    match session {
        None => {
            if json {
                println!(
                    "{}",
                    json_output(
                        "coordinator status",
                        &Output {
                            running: false,
                            state: None,
                            tmux_session: None,
                            pid: None,
                            started_at: None,
                            last_activity: None,
                            active_agents,
                        }
                    )
                );
            } else {
                println!("{} coordinator: not started", brand_bold("grove"));
            }
        }
        Some(s) => {
            let running = s.state == AgentState::Working || s.state == AgentState::Booting;
            if json {
                println!(
                    "{}",
                    json_output(
                        "coordinator status",
                        &Output {
                            running,
                            state: Some(format!("{:?}", s.state).to_lowercase()),
                            tmux_session: Some(s.tmux_session.clone()),
                            pid: s.pid,
                            started_at: Some(s.started_at.clone()),
                            last_activity: Some(s.last_activity.clone()),
                            active_agents,
                        }
                    )
                );
            } else {
                println!("{} coordinator status", brand_bold("grove"));
                println!(
                    "  State:       {}",
                    format!("{:?}", s.state).to_lowercase()
                );
                println!("  Tmux:        {}", s.tmux_session);
                if let Some(pid) = s.pid {
                    println!("  PID:         {pid}");
                }
                println!("  Started:     {}", s.started_at);
                println!("  Last active: {}", s.last_activity);
                println!("  Active agents (excl. coordinator): {active_agents}");
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// send
// ---------------------------------------------------------------------------

pub fn execute_send(
    subject: &str,
    body: &str,
    from: &str,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let root = resolve_project_root(&cwd, project_override).map_err(|e| e.to_string())?;
    let root_str = root.to_string_lossy().to_string();
    let mail_db = format!("{root_str}/.overstory/mail.db");

    let store = MailStore::new(&mail_db).map_err(|e| e.to_string())?;

    let msg = store
        .insert(&InsertMailMessage {
            id: None,
            from_agent: from.to_string(),
            to_agent: COORDINATOR_AGENT.to_string(),
            subject: subject.to_string(),
            body: body.to_string(),
            message_type: MailMessageType::Status,
            priority: MailPriority::Normal,
            thread_id: None,
            payload: None,
        })
        .map_err(|e| e.to_string())?;

    if json {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Output {
            sent: bool,
            message_id: String,
            to: String,
            subject: String,
        }
        println!(
            "{}",
            json_output(
                "coordinator send",
                &Output {
                    sent: true,
                    message_id: msg.id.clone(),
                    to: COORDINATOR_AGENT.to_string(),
                    subject: subject.to_string(),
                }
            )
        );
    } else {
        println!(
            "{} sent to coordinator: {} ({})",
            brand_bold("grove"),
            subject,
            msg.id
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_status_no_sessions_db() {
        // Should return Ok with "not running" when no DB
        let result = execute_status(false, Some(Path::new("/tmp/grove-coord-test-nonexistent")));
        assert!(result.is_ok());
    }

    #[test]
    fn test_status_json_no_sessions_db() {
        let result = execute_status(true, Some(Path::new("/tmp/grove-coord-test-nonexistent")));
        assert!(result.is_ok());
    }

    #[test]
    fn test_stop_no_sessions_db() {
        let result = execute_stop(false, Some(Path::new("/tmp/grove-coord-test-nonexistent")));
        assert!(result.is_err());
    }

    #[test]
    fn test_send_message() {
        // Uses in-memory DB via tmpdir
        let tmpdir = tempfile::tempdir().unwrap();
        let overstory_dir = tmpdir.path().join(".overstory");
        std::fs::create_dir_all(&overstory_dir).unwrap();

        let result = execute_send(
            "test subject",
            "test body",
            "operator",
            false,
            Some(tmpdir.path()),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_send_message_json() {
        let tmpdir = tempfile::tempdir().unwrap();
        let overstory_dir = tmpdir.path().join(".overstory");
        std::fs::create_dir_all(&overstory_dir).unwrap();

        let result = execute_send(
            "hello coordinator",
            "please do something",
            "human",
            true,
            Some(tmpdir.path()),
        );
        assert!(result.is_ok());
    }
}
