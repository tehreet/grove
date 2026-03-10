//! `grove watch` — Tier 0 mechanical watchdog daemon.
//!
//! Modes:
//!   (default)    — run one health check, print results, exit
//!   --background — daemonize (PID file + log file), run poll loop
//!   --foreground — internal: run continuous poll loop (used by --background daemon)

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde::Serialize;

use crate::config::{load_config, resolve_project_root};
use crate::db::sessions::SessionStore;
use crate::json::{json_error, json_output};
use crate::logging::brand_bold;
use crate::watchdog::HealthStatus;

const PID_FILE: &str = ".overstory/watchdog.pid";
const LOG_FILE: &str = ".overstory/logs/watchdog.log";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn pid_file_path(root: &Path) -> PathBuf {
    root.join(PID_FILE)
}

fn log_file_path(root: &Path) -> PathBuf {
    root.join(LOG_FILE)
}

fn read_pid_file(root: &Path) -> Option<u32> {
    fs::read_to_string(pid_file_path(root))
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
}

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
// Execute
// ---------------------------------------------------------------------------

/// Main entry point for `grove watch`.
///
/// - `foreground`: run continuous poll loop (internal, spawned by --background)
/// - `background`: daemonize, write PID file, spawn foreground child
/// - default: run one health check and exit
pub fn execute(
    interval: Option<u64>,
    background: bool,
    foreground: bool,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let root = resolve_project_root(&cwd, project_override).map_err(|e| e.to_string())?;
    let root_str = root.to_string_lossy().to_string();

    let config = load_config(&root, project_override).unwrap_or_default();
    let watchdog_cfg = config.watchdog;

    let interval_ms = interval.unwrap_or(watchdog_cfg.tier0_interval_ms);

    let sessions_db = format!("{root_str}/.overstory/sessions.db");

    if foreground {
        return run_foreground(&root, &sessions_db, &watchdog_cfg, interval_ms);
    }

    if background {
        return start_background(&root, &root_str, interval, json);
    }

    // Default: one-shot health check
    run_once(&root, &sessions_db, &watchdog_cfg, interval_ms, json)
}

// ---------------------------------------------------------------------------
// One-shot mode
// ---------------------------------------------------------------------------

fn run_once(
    root: &Path,
    sessions_db: &str,
    config: &crate::types::WatchdogConfig,
    interval_ms: u64,
    json: bool,
) -> Result<(), String> {
    let store = match SessionStore::new(sessions_db) {
        Ok(s) => s,
        Err(_) => {
            // No sessions DB — nothing to check
            if json {
                #[derive(Serialize)]
                #[serde(rename_all = "camelCase")]
                struct Output {
                    checked: u32,
                    results: Vec<serde_json::Value>,
                    interval_ms: u64,
                }
                println!(
                    "{}",
                    json_output(
                        "watch",
                        &Output {
                            checked: 0,
                            results: vec![],
                            interval_ms,
                        }
                    )
                );
            } else {
                println!(
                    "{} no sessions.db found — nothing to check",
                    brand_bold("watch")
                );
            }
            return Ok(());
        }
    };

    let _ = root; // root is used for watchdog pid; not needed in one-shot
    let now_ms = chrono::Utc::now().timestamp_millis() as u64;
    let results = crate::watchdog::poll_once(&store, config, Path::new("."), now_ms);

    if json {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct CheckResult {
            agent: String,
            status: String,
        }
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Output {
            checked: usize,
            results: Vec<CheckResult>,
            interval_ms: u64,
        }
        let check_results: Vec<CheckResult> = results
            .iter()
            .map(|(name, status)| CheckResult {
                agent: name.clone(),
                status: format!("{status:?}").to_lowercase(),
            })
            .collect();
        let checked = check_results.len();
        println!(
            "{}",
            json_output(
                "watch",
                &Output {
                    checked,
                    results: check_results,
                    interval_ms,
                }
            )
        );
    } else {
        println!("{}", brand_bold("Watchdog health check"));
        println!("{}", "─".repeat(70));
        if results.is_empty() {
            println!("  All agents healthy (or no active sessions)");
        } else {
            for (agent, status) in &results {
                let icon = match status {
                    HealthStatus::Alive => "✓",
                    HealthStatus::Stale => "!",
                    HealthStatus::Zombie => "✗",
                    HealthStatus::Dead => "✗",
                };
                println!("  {icon} {agent}: {status:?}");
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Background mode (daemon spawn)
// ---------------------------------------------------------------------------

fn start_background(
    root: &Path,
    root_str: &str,
    interval: Option<u64>,
    json: bool,
) -> Result<(), String> {
    // Check if already running
    if let Some(pid) = read_pid_file(root) {
        if pid_is_alive(pid) {
            let msg = format!("Watchdog already running (PID: {pid})");
            if json {
                println!("{}", json_error("watch", &msg));
            } else {
                eprintln!("{msg}");
            }
            return Err(msg);
        }
        // Stale PID file — remove it
        let _ = fs::remove_file(pid_file_path(root));
    }

    // Ensure log directory exists
    let log_path = log_file_path(root);
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create log dir: {e}"))?;
    }

    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("Failed to open log file: {e}"))?;
    let log_file_err = log_file.try_clone().map_err(|e| e.to_string())?;

    let grove_bin = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "grove".to_string());

    let mut args = vec!["watch", "--foreground", "--project", root_str];
    let interval_str;
    if let Some(iv) = interval {
        interval_str = iv.to_string();
        args.push("--interval");
        args.push(&interval_str);
    }

    let child = Command::new(&grove_bin)
        .args(&args)
        .stdout(log_file)
        .stderr(log_file_err)
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to spawn watchdog daemon: {e}"))?;

    let pid = child.id();

    fs::write(pid_file_path(root), pid.to_string())
        .map_err(|e| format!("Failed to write PID file: {e}"))?;

    drop(child);

    if json {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Output {
            started: bool,
            pid: u32,
            log_file: String,
            pid_file: String,
        }
        println!(
            "{}",
            json_output(
                "watch",
                &Output {
                    started: true,
                    pid,
                    log_file: log_path.to_string_lossy().to_string(),
                    pid_file: pid_file_path(root).to_string_lossy().to_string(),
                }
            )
        );
    } else {
        println!(
            "{} Watchdog started in background, PID: {pid}",
            brand_bold("grove")
        );
        println!("  Log:  {}", log_path.display());
        println!("  PID file: {}", pid_file_path(root).display());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Foreground loop (used internally by daemon child)
// ---------------------------------------------------------------------------

fn run_foreground(
    root: &Path,
    sessions_db: &str,
    config: &crate::types::WatchdogConfig,
    interval_ms: u64,
) -> Result<(), String> {
    // Write our own PID file (overwrite the one written by the spawner with our actual PID)
    fs::write(pid_file_path(root), std::process::id().to_string())
        .map_err(|e| format!("Failed to write PID file: {e}"))?;

    let interval = Duration::from_millis(interval_ms);

    loop {
        if let Ok(store) = SessionStore::new(sessions_db) {
            let now_ms = chrono::Utc::now().timestamp_millis() as u64;
            let results = crate::watchdog::poll_once(&store, config, root, now_ms);
            for (agent, status) in &results {
                let ts = chrono::Utc::now().format("%H:%M:%S");
                eprintln!("[{ts}] watchdog: {agent} → {status:?}");
            }
        }
        std::thread::sleep(interval);
    }
}

// ---------------------------------------------------------------------------
// Stop helper (called by integration layer or stop subcommand)
// ---------------------------------------------------------------------------

/// Stop a running watchdog daemon by reading the PID file and sending SIGTERM.
pub fn execute_stop(json: bool, project_override: Option<&Path>) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let root = resolve_project_root(&cwd, project_override).map_err(|e| e.to_string())?;

    let pid = read_pid_file(&root).ok_or_else(|| "No watchdog PID file found".to_string())?;

    if !pid_is_alive(pid) {
        let _ = fs::remove_file(pid_file_path(&root));
        let msg = "Watchdog is not running (stale PID file removed)";
        if json {
            println!("{}", json_error("watch stop", msg));
        } else {
            println!("{msg}");
        }
        return Ok(());
    }

    Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .output()
        .map_err(|e| format!("Failed to send SIGTERM: {e}"))?;

    let _ = fs::remove_file(pid_file_path(&root));

    if json {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Output {
            stopped: bool,
            pid: u32,
        }
        println!(
            "{}",
            json_output("watch stop", &Output { stopped: true, pid })
        );
    } else {
        println!("{} Watchdog stopped (PID: {pid})", brand_bold("grove"));
    }

    Ok(())
}

/// Show watchdog status.
pub fn execute_status(json: bool, project_override: Option<&Path>) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let root = resolve_project_root(&cwd, project_override).map_err(|e| e.to_string())?;

    let pid = read_pid_file(&root);
    let running = pid.map(pid_is_alive).unwrap_or(false);

    if json {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Output {
            running: bool,
            pid: Option<u32>,
            pid_file: String,
        }
        println!(
            "{}",
            json_output(
                "watch status",
                &Output {
                    running,
                    pid,
                    pid_file: pid_file_path(&root).to_string_lossy().to_string(),
                }
            )
        );
    } else {
        println!("{}", brand_bold("Watchdog status"));
        if running {
            println!("  Status: running (PID: {})", pid.unwrap_or(0));
        } else {
            println!("  Status: not running");
        }
        println!("  PID file: {}", pid_file_path(&root).display());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_root() -> TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".overstory")).unwrap();
        dir
    }

    #[test]
    fn test_pid_file_path() {
        let dir = make_root();
        let p = pid_file_path(dir.path());
        assert!(p.to_string_lossy().ends_with("watchdog.pid"));
    }

    #[test]
    fn test_read_pid_file_missing() {
        let dir = make_root();
        assert!(read_pid_file(dir.path()).is_none());
    }

    #[test]
    fn test_read_pid_file_valid() {
        let dir = make_root();
        fs::write(pid_file_path(dir.path()), "12345\n").unwrap();
        assert_eq!(read_pid_file(dir.path()), Some(12345u32));
    }

    #[test]
    fn test_read_pid_file_invalid() {
        let dir = make_root();
        fs::write(pid_file_path(dir.path()), "not-a-number").unwrap();
        assert!(read_pid_file(dir.path()).is_none());
    }

    #[test]
    fn test_pid_is_alive_self() {
        let my_pid = std::process::id();
        assert!(pid_is_alive(my_pid));
    }

    #[test]
    fn test_pid_is_alive_nonexistent() {
        assert!(!pid_is_alive(999_999_999));
    }

    #[test]
    fn test_execute_status_no_pid_file() {
        let dir = make_root();
        let result = execute_status(false, Some(dir.path()));
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_status_json_no_pid_file() {
        let dir = make_root();
        let result = execute_status(true, Some(dir.path()));
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_stop_no_pid_file() {
        let dir = make_root();
        let result = execute_stop(false, Some(dir.path()));
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No watchdog PID file"));
    }

    #[test]
    fn test_execute_one_shot_no_sessions_db() {
        let dir = make_root();
        // No sessions.db — should return Ok (nothing to check)
        let result = execute(None, false, false, false, Some(dir.path()));
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_one_shot_json_no_sessions_db() {
        let dir = make_root();
        let result = execute(None, false, false, true, Some(dir.path()));
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_stop_stale_pid() {
        let dir = make_root();
        // Write a PID that definitely doesn't exist
        fs::write(pid_file_path(dir.path()), "999999999\n").unwrap();
        let result = execute_stop(false, Some(dir.path()));
        assert!(result.is_ok());
        // PID file should be cleaned up
        assert!(!pid_file_path(dir.path()).exists());
    }
}
