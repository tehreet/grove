//! `grove monitor` — persistent Tier 2 monitor agent lifecycle.
//!
//! Subcommands:
//!   start      — spawn monitor daemon (PID file + log file)
//!   stop       — SIGTERM the monitor, update session
//!   status     — show monitor state (PID file + session DB)
//!   foreground — run monitor event loop in foreground (internal, called by daemon child)

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::config::{load_config, resolve_project_root};
use crate::db::sessions::SessionStore;
use crate::json::{json_error, json_output};
use crate::logging::brand_bold;
use crate::types::{AgentSession, AgentState};
use crate::watchdog::poll_once;

const MONITOR_AGENT: &str = "monitor";
const PID_FILE: &str = ".overstory/monitor.pid";
const LOG_DIR: &str = ".overstory/logs";
const LOG_FILE: &str = ".overstory/logs/monitor.log";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn pid_file_path(root: &Path) -> PathBuf {
    root.join(PID_FILE)
}

fn log_file_path(root: &Path) -> PathBuf {
    root.join(LOG_FILE)
}

/// Read PID from the monitor.pid file. Returns None if absent or unparseable.
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

// ---------------------------------------------------------------------------
// foreground (internal)
// ---------------------------------------------------------------------------

/// Run the monitor poll loop in the foreground. Called by the daemon child.
///
/// Polls session health on `tier0_interval_ms` and logs results. Runs until
/// SIGTERM or SIGINT.
pub fn execute_foreground(project_override: Option<&Path>) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let root = resolve_project_root(&cwd, project_override).map_err(|e| e.to_string())?;
    let root_str = root.to_string_lossy().to_string();
    let sessions_db = format!("{root_str}/.overstory/sessions.db");

    // Mark session as working
    if let Ok(store) = SessionStore::new(&sessions_db) {
        let _ = store.update_state(MONITOR_AGENT, AgentState::Working);
        let _ = store.update_last_activity(MONITOR_AGENT);
    }

    let config = load_config(&root, project_override).unwrap_or_default();
    let watchdog_cfg = config.watchdog;
    let interval = Duration::from_millis(watchdog_cfg.tier0_interval_ms);

    eprintln!(
        "[monitor] starting poll loop, interval={}ms",
        watchdog_cfg.tier0_interval_ms
    );

    loop {
        {
            let now_ms = chrono::Utc::now().timestamp_millis() as u64;
            if let Ok(store) = SessionStore::new(&sessions_db) {
                let results = poll_once(&store, &watchdog_cfg, &root, now_ms);
                for (name, status) in &results {
                    eprintln!("[monitor] {} → {:?}", name, status);
                }
            }
        }
        thread::sleep(interval);
    }
}

// ---------------------------------------------------------------------------
// start
// ---------------------------------------------------------------------------

pub fn execute_start(
    foreground: bool,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let root = resolve_project_root(&cwd, project_override).map_err(|e| e.to_string())?;
    let root_str = root.to_string_lossy().to_string();

    // If --foreground: run poll loop directly (called by daemon child)
    if foreground {
        return execute_foreground(project_override);
    }

    let sessions_db = format!("{root_str}/.overstory/sessions.db");

    // Check if already running
    if let Some(pid) = read_pid_file(&root) {
        if pid_is_alive(pid) {
            let msg = format!("Monitor is already running (PID: {pid})");
            if json {
                println!("{}", json_error("monitor start", &msg));
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
        agent_name: MONITOR_AGENT.to_string(),
        capability: "monitor".to_string(),
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
        .args(["monitor", "start", "--foreground", "--project", &root_str])
        .stdout(log_file)
        .stderr(log_file_err)
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to spawn monitor daemon: {e}"))?;

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

    // Detach child
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
                "monitor start",
                &Output {
                    started: true,
                    agent_name: MONITOR_AGENT.to_string(),
                    pid,
                    log_file: log_path.to_string_lossy().to_string(),
                }
            )
        );
    } else {
        println!("{} monitor started, PID: {pid}", brand_bold("grove"));
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
        SessionStore::new(&sessions_db)
            .ok()
            .and_then(|s| s.get_by_name(MONITOR_AGENT).ok())
            .flatten()
            .and_then(|s| s.pid)
            .map(|p| p as u32)
    });

    let Some(pid) = pid else {
        let msg = "Monitor not running (no PID file or active session found)";
        if json {
            println!("{}", json_error("monitor stop", msg));
        } else {
            eprintln!("{msg}");
        }
        return Err(msg.to_string());
    };

    let mut killed = false;

    if pid_is_alive(pid) {
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

    // Update session state (best-effort)
    if let Ok(store) = SessionStore::new(&sessions_db) {
        let _ = store.update_state(MONITOR_AGENT, AgentState::Completed);
        let _ = store.update_last_activity(MONITOR_AGENT);
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
                "monitor stop",
                &Output {
                    stopped: true,
                    agent_name: MONITOR_AGENT.to_string(),
                    killed,
                }
            )
        );
    } else {
        println!("{} monitor stopped", brand_bold("grove"));
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
        started_at: Option<String>,
        last_activity: Option<String>,
    }

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

    let session = if PathBuf::from(&sessions_db).exists() {
        SessionStore::new(&sessions_db)
            .ok()
            .and_then(|s| s.get_by_name(MONITOR_AGENT).ok())
            .flatten()
    } else {
        None
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
                "monitor status",
                &Output {
                    running,
                    state: state_str,
                    pid,
                    pid_file: pid_file_str,
                    log_file: log_file_str,
                    started_at: session.as_ref().map(|s| s.started_at.clone()),
                    last_activity: session.as_ref().map(|s| s.last_activity.clone()),
                }
            )
        );
    } else {
        println!("{} monitor status", brand_bold("grove"));
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
        if !running {
            println!("  (monitor is not running)");
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
    use std::path::Path;

    #[test]
    fn test_status_no_sessions_db() {
        let result = execute_status(
            false,
            Some(Path::new("/tmp/grove-monitor-test-nonexistent")),
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_status_json_no_sessions_db() {
        let result = execute_status(true, Some(Path::new("/tmp/grove-monitor-test-nonexistent")));
        assert!(result.is_ok());
    }

    #[test]
    fn test_stop_no_pid() {
        let result = execute_stop(
            false,
            Some(Path::new("/tmp/grove-monitor-test-nonexistent")),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_stop_json_no_pid() {
        let result = execute_stop(true, Some(Path::new("/tmp/grove-monitor-test-nonexistent")));
        assert!(result.is_err());
    }

    #[test]
    fn test_read_pid_file_nonexistent() {
        let result = read_pid_file(Path::new("/tmp/grove-monitor-test-nonexistent"));
        assert!(result.is_none());
    }

    #[test]
    fn test_pid_is_alive_self() {
        let pid = std::process::id();
        assert!(pid_is_alive(pid));
    }

    #[test]
    fn test_pid_is_alive_bogus() {
        assert!(!pid_is_alive(999_999_999));
    }

    #[test]
    fn test_start_tier2_disabled() {
        // Start should fail when tier2 is disabled (default config)
        let tmpdir = tempfile::tempdir().unwrap();
        let overstory_dir = tmpdir.path().join(".overstory");
        std::fs::create_dir_all(&overstory_dir).unwrap();

        // Default config has tier2_enabled: false
        let result = execute_start(false, false, Some(tmpdir.path()));
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("tier2") || msg.contains("Tier 2") || msg.contains("disabled"));
    }

    #[test]
    fn test_start_tier2_disabled_json() {
        let tmpdir = tempfile::tempdir().unwrap();
        let overstory_dir = tmpdir.path().join(".overstory");
        std::fs::create_dir_all(&overstory_dir).unwrap();

        let result = execute_start(false, true, Some(tmpdir.path()));
        assert!(result.is_err());
    }
}
