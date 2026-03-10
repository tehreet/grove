//! `grove coordinator` — persistent coordinator event loop.
//!
//! Subcommands:
//!   start   — register session, spawn daemon process, run event loop
//!   stop    — SIGTERM the coordinator, update session
//!   status  — show coordinator info (PID file + log tail + session DB)
//!   send    — insert a message into the coordinator's mailbox
//!   logs    — tail the coordinator log file

use std::fs;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::config::{load_config, resolve_project_root};
use crate::coordinator::event_loop::{run, LoopContext};
use crate::db::mail::MailStore;
use crate::db::sessions::SessionStore;
use crate::json::{json_error, json_output};
use crate::logging::brand_bold;
use crate::types::{AgentSession, AgentState, InsertMailMessage, MailMessageType, MailPriority};

const COORDINATOR_AGENT: &str = "coordinator";
const PID_FILE: &str = ".overstory/coordinator.pid";
const LOG_DIR: &str = ".overstory/logs";
const LOG_FILE: &str = ".overstory/logs/coordinator.log";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn pid_file_path(root: &Path) -> PathBuf {
    root.join(PID_FILE)
}

fn log_file_path(root: &Path) -> PathBuf {
    root.join(LOG_FILE)
}

/// Read PID from the coordinator.pid file. Returns None if file absent or unparseable.
fn read_pid_file(root: &Path) -> Option<u32> {
    fs::read_to_string(pid_file_path(root))
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
}

/// Check whether a process with the given PID is alive via `kill -0`.
fn pid_is_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Return the last `n` lines of a file as a String.
fn tail_file(path: &Path, n: usize) -> String {
    let Ok(file) = fs::File::open(path) else {
        return String::new();
    };
    let reader = io::BufReader::new(file);
    let lines: Vec<String> = reader.lines().map_while(Result::ok).collect();
    lines
        .iter()
        .rev()
        .take(n)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

// ---------------------------------------------------------------------------
// start
// ---------------------------------------------------------------------------

pub fn execute_start(
    _no_attach: bool,
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
    // This is called by the daemon child spawned below.
    if foreground {
        let store = SessionStore::new(&sessions_db).map_err(|e| e.to_string())?;

        // Update session state to "working"
        let _ = store.update_state(COORDINATOR_AGENT, AgentState::Working);
        let _ = store.update_last_activity(COORDINATOR_AGENT);

        // Load config for exit triggers
        let config = load_config(&root, project_override).unwrap_or_default();
        let exit_triggers = config.coordinator.map(|c| c.exit_triggers).unwrap_or(
            crate::types::CoordinatorExitTriggers {
                all_agents_done: false,
                task_tracker_empty: false,
                on_shutdown_signal: true,
            },
        );

        let sessions_db_path = sessions_db.clone();
        let ctx = LoopContext {
            project_root: root_str,
            sessions_db,
            mail_db,
            merge_queue_db,
            exit_triggers,
            agent_name: COORDINATOR_AGENT.to_string(),
            has_received_work: false,
        };

        run(ctx);

        // After loop exits, mark session completed
        if let Ok(store) = SessionStore::new(&sessions_db_path) {
            let _ = store.update_state(COORDINATOR_AGENT, AgentState::Completed);
            let _ = store.update_last_activity(COORDINATOR_AGENT);
        }
        return Ok(());
    }

    // Daemon mode: check if coordinator is already running
    if let Some(pid) = read_pid_file(&root) {
        if pid_is_alive(pid) {
            let msg = format!("Coordinator is already running (PID: {pid})");
            if json {
                println!("{}", json_error("coordinator start", &msg));
            } else {
                eprintln!("{msg}");
            }
            return Err(msg);
        }
    }

    // Create log directory
    let log_dir = root.join(LOG_DIR);
    fs::create_dir_all(&log_dir).map_err(|e| format!("Failed to create log dir: {e}"))?;
    let log_path = log_file_path(&root);
    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("Failed to open log file: {e}"))?;
    let log_file_err = log_file.try_clone().map_err(|e| e.to_string())?;

    // Register session (state = booting)
    let now = chrono::Utc::now().to_rfc3339();
    let session = AgentSession {
        id: uuid::Uuid::new_v4().to_string(),
        agent_name: COORDINATOR_AGENT.to_string(),
        capability: "coordinator".to_string(),
        worktree_path: root_str.clone(),
        branch_name: String::new(),
        task_id: String::new(),
        tmux_session: String::new(),
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

    // Spawn daemon child
    let grove_bin = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "grove".to_string());

    let child = Command::new(&grove_bin)
        .args([
            "coordinator",
            "start",
            "--foreground",
            "--project",
            &root_str,
        ])
        .stdout(log_file)
        .stderr(log_file_err)
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to spawn coordinator daemon: {e}"))?;

    let pid = child.id();

    // Write PID file
    fs::write(pid_file_path(&root), pid.to_string())
        .map_err(|e| format!("Failed to write PID file: {e}"))?;

    // Update session with PID
    if let Ok(store) = SessionStore::new(&sessions_db) {
        let mut s = session.clone();
        s.pid = Some(pid as i64);
        let _ = store.upsert(&s);
    }

    // Detach child — we don't wait on it
    drop(child);

    if json {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Output {
            started: bool,
            agent_name: String,
            pid: u32,
            log_file: String,
        }
        println!(
            "{}",
            json_output(
                "coordinator start",
                &Output {
                    started: true,
                    agent_name: COORDINATOR_AGENT.to_string(),
                    pid,
                    log_file: log_path.to_string_lossy().to_string(),
                }
            )
        );
    } else {
        println!("{} coordinator started, PID: {pid}", brand_bold("grove"));
        println!("  Log: {}", log_path.display());
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

    // Find PID to kill
    let pid = read_pid_file(&root).or_else(|| {
        // Fall back to session record pid
        SessionStore::new(&sessions_db)
            .ok()
            .and_then(|s| s.get_by_name(COORDINATOR_AGENT).ok())
            .flatten()
            .and_then(|s| s.pid)
            .map(|p| p as u32)
    });

    let Some(pid) = pid else {
        let msg = "Coordinator not running (no PID file or active session found)";
        if json {
            println!("{}", json_error("coordinator stop", msg));
        } else {
            eprintln!("{msg}");
        }
        return Err(msg.to_string());
    };

    let mut killed = false;

    if pid_is_alive(pid) {
        // Send SIGTERM
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .output();
        killed = true;

        // Wait up to 5 seconds for the process to exit
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if !pid_is_alive(pid) {
                break;
            }
            thread::sleep(Duration::from_millis(200));
        }
    }

    // Remove PID file
    let _ = fs::remove_file(pid_file_path(&root));

    // Update session state in DB (best-effort)
    if let Ok(store) = SessionStore::new(&sessions_db) {
        let _ = store.update_state(COORDINATOR_AGENT, AgentState::Completed);
        let _ = store.update_last_activity(COORDINATOR_AGENT);
    }

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
            println!("  Process {pid} terminated (SIGTERM)");
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
        pid: Option<u32>,
        pid_file: Option<String>,
        log_file: Option<String>,
        log_tail: Option<String>,
        started_at: Option<String>,
        last_activity: Option<String>,
        active_agents: usize,
    }

    // Check PID file
    let pid = read_pid_file(&root);
    let alive = pid.map(pid_is_alive).unwrap_or(false);
    let pid_file_str = if pid_file_path(&root).exists() {
        Some(pid_file_path(&root).to_string_lossy().to_string())
    } else {
        None
    };
    let log_path = log_file_path(&root);
    let log_file_str = if log_path.exists() {
        Some(log_path.to_string_lossy().to_string())
    } else {
        None
    };
    let log_tail = log_file_str
        .as_ref()
        .map(|_| tail_file(&log_path, 5))
        .filter(|s| !s.is_empty());

    // Query sessions.db
    let session = if PathBuf::from(&sessions_db).exists() {
        SessionStore::new(&sessions_db)
            .ok()
            .and_then(|s| s.get_by_name(COORDINATOR_AGENT).ok())
            .flatten()
    } else {
        None
    };

    let active_agents = if PathBuf::from(&sessions_db).exists() {
        SessionStore::new(&sessions_db)
            .ok()
            .and_then(|s| s.get_active().ok())
            .map(|v| {
                v.iter()
                    .filter(|s| s.agent_name != COORDINATOR_AGENT)
                    .count()
            })
            .unwrap_or(0)
    } else {
        0
    };

    let running = alive
        || session
            .as_ref()
            .map(|s| s.state == AgentState::Working || s.state == AgentState::Booting)
            .unwrap_or(false);

    let state_str = if alive {
        Some("working".to_string())
    } else {
        session
            .as_ref()
            .map(|s| format!("{:?}", s.state).to_lowercase())
    };

    if json {
        println!(
            "{}",
            json_output(
                "coordinator status",
                &Output {
                    running,
                    state: state_str,
                    pid,
                    pid_file: pid_file_str,
                    log_file: log_file_str,
                    log_tail,
                    started_at: session.as_ref().map(|s| s.started_at.clone()),
                    last_activity: session.as_ref().map(|s| s.last_activity.clone()),
                    active_agents,
                }
            )
        );
    } else {
        println!("{} coordinator status", brand_bold("grove"));
        println!("  Running:     {running}");
        if let Some(state) = &state_str {
            println!("  State:       {state}");
        }
        if let Some(p) = pid {
            println!(
                "  PID:         {p} ({})",
                if alive { "alive" } else { "dead" }
            );
        }
        if let Some(ref s) = session {
            println!("  Started:     {}", s.started_at);
            println!("  Last active: {}", s.last_activity);
        }
        println!("  Active agents (excl. coordinator): {active_agents}");
        if let Some(ref tail) = log_tail {
            println!("  Recent log:");
            for line in tail.lines() {
                println!("    {line}");
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// logs
// ---------------------------------------------------------------------------

pub fn execute_logs(
    follow: bool,
    lines: usize,
    _json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let root = resolve_project_root(&cwd, project_override).map_err(|e| e.to_string())?;
    let log_path = log_file_path(&root);

    if !log_path.exists() {
        if follow {
            eprintln!("Waiting for coordinator log: {}", log_path.display());
        } else {
            eprintln!("No coordinator log found: {}", log_path.display());
            return Ok(());
        }
    }

    if !follow {
        let tail = tail_file(&log_path, lines);
        if tail.is_empty() {
            eprintln!("(log is empty)");
        } else {
            println!("{tail}");
        }
        return Ok(());
    }

    // --follow: poll for new content
    let mut pos: u64 = {
        // Print existing content up to `lines` tail
        let tail = tail_file(&log_path, lines);
        if !tail.is_empty() {
            println!("{tail}");
        }
        log_path.metadata().map(|m| m.len()).unwrap_or(0)
    };

    loop {
        thread::sleep(Duration::from_millis(500));
        if let Ok(meta) = log_path.metadata() {
            let len = meta.len();
            if len > pos {
                if let Ok(mut file) = fs::File::open(&log_path) {
                    use std::io::{Read, Seek, SeekFrom};
                    let _ = file.seek(SeekFrom::Start(pos));
                    let mut buf = String::new();
                    if file.read_to_string(&mut buf).is_ok() {
                        print!("{buf}");
                    }
                    pos = len;
                }
            }
        }
    }
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

/// `grove coordinator ask` — send a message to the coordinator and wait for a reply.
pub fn execute_ask(
    body: &str,
    from: &str,
    timeout_secs: u64,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let root = resolve_project_root(&cwd, project_override).map_err(|e| e.to_string())?;
    let root_str = root.to_string_lossy().to_string();
    let mail_db = format!("{root_str}/.overstory/mail.db");

    let store = MailStore::new(&mail_db).map_err(|e| e.to_string())?;
    let thread_id = uuid::Uuid::new_v4().to_string();

    store
        .insert(&InsertMailMessage {
            id: None,
            from_agent: from.to_string(),
            to_agent: COORDINATOR_AGENT.to_string(),
            subject: "ask".to_string(),
            body: body.to_string(),
            message_type: MailMessageType::Status,
            priority: MailPriority::High,
            thread_id: Some(thread_id.clone()),
            payload: None,
        })
        .map_err(|e| e.to_string())?;

    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let poll_interval = Duration::from_millis(500);

    loop {
        if Instant::now() >= deadline {
            let msg = format!("Timeout: no reply from coordinator after {timeout_secs}s");
            if json {
                println!("{}", json_error("coordinator ask", &msg));
            } else {
                eprintln!("{msg}");
            }
            return Err(msg);
        }

        let unread = store.get_unread(from).unwrap_or_default();
        for mail in &unread {
            if mail.thread_id.as_deref() == Some(thread_id.as_str()) {
                let _ = store.mark_read(&mail.id);

                if json {
                    #[derive(Serialize)]
                    #[serde(rename_all = "camelCase")]
                    struct Output {
                        replied: bool,
                        body: String,
                        from: String,
                        thread_id: String,
                    }

                    println!(
                        "{}",
                        json_output(
                            "coordinator ask",
                            &Output {
                                replied: true,
                                body: mail.body.clone(),
                                from: mail.from.clone(),
                                thread_id: thread_id.clone(),
                            }
                        )
                    );
                } else {
                    println!("{}", mail.body);
                }
                return Ok(());
            }
        }

        thread::sleep(poll_interval);
    }
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

    #[test]
    fn test_ask_times_out_without_reply() {
        let tmpdir = tempfile::tempdir().unwrap();
        let overstory_dir = tmpdir.path().join(".overstory");
        std::fs::create_dir_all(&overstory_dir).unwrap();

        let result = execute_ask("status?", "operator", 0, false, Some(tmpdir.path()));
        assert!(result.is_err());
    }

    #[test]
    fn test_pid_is_alive_self() {
        // Our own PID should always be alive
        let pid = std::process::id();
        assert!(pid_is_alive(pid));
    }

    #[test]
    fn test_pid_is_alive_bogus() {
        // PID 999999999 should not be alive
        assert!(!pid_is_alive(999_999_999));
    }

    #[test]
    fn test_tail_file_nonexistent() {
        let result = tail_file(Path::new("/tmp/nonexistent-grove-test-log.log"), 5);
        assert!(result.is_empty());
    }

    #[test]
    fn test_logs_no_file() {
        let result = execute_logs(
            false,
            10,
            false,
            Some(Path::new("/tmp/grove-coord-test-nonexistent")),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_read_pid_file_nonexistent() {
        let result = read_pid_file(Path::new("/tmp/grove-coord-test-nonexistent"));
        assert!(result.is_none());
    }
}
