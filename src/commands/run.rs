//! `grove run` — manage coordinator runs.
//!
//! Subcommands:
//!   (default)   Show current run status
//!   list        List recent runs
//!   complete    Mark current run as completed
//!   show <id>   Show run details with agents

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::config::load_config;
use crate::db::sessions::{RunStore, SessionStore};
use crate::json::json_output;
use crate::logging::{accent, muted, print_error, print_hint, print_success};
use crate::types::{Run, RunStatus};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const SEPARATOR: &str = "──────────────────────────────────────────────────────────────────────";

fn run_duration_ms(run: &Run) -> i64 {
    let start = DateTime::parse_from_rfc3339(&run.started_at)
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0);
    let end = run
        .completed_at
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.timestamp_millis())
        .unwrap_or_else(|| Utc::now().timestamp_millis());
    (end - start).max(0)
}

fn format_duration(ms: i64) -> String {
    let secs = ms / 1000;
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {:02}s", secs / 60, secs % 60)
    } else {
        format!("{}h {:02}m", secs / 3600, (secs % 3600) / 60)
    }
}

fn read_current_run_id(overstory_dir: &str) -> Option<String> {
    let path = format!("{overstory_dir}/current-run.txt");
    std::fs::read_to_string(&path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

// ---------------------------------------------------------------------------
// JSON output types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunWithDuration {
    id: String,
    started_at: String,
    completed_at: Option<String>,
    agent_count: u32,
    coordinator_session_id: Option<String>,
    status: String,
    duration: String,
}

impl RunWithDuration {
    fn from_run(run: &Run) -> Self {
        let dur = format_duration(run_duration_ms(run));
        RunWithDuration {
            id: run.id.clone(),
            started_at: run.started_at.clone(),
            completed_at: run.completed_at.clone(),
            agent_count: run.agent_count,
            coordinator_session_id: run.coordinator_session_id.clone(),
            status: format!("{:?}", run.status).to_lowercase(),
            duration: dur,
        }
    }
}

// ---------------------------------------------------------------------------
// Execute: show current run
// ---------------------------------------------------------------------------

pub fn execute_current(json: bool, project_override: Option<&Path>) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = load_config(&cwd, project_override).map_err(|e| e.to_string())?;
    let overstory = format!("{}/.overstory", config.project.root);

    // Try sessions.db first via RunStore active run, then fall back to current-run.txt
    let sessions_db = format!("{overstory}/sessions.db");
    let run_id: Option<String> = if PathBuf::from(&sessions_db).exists() {
        let run_store = RunStore::new(&sessions_db).map_err(|e| e.to_string())?;
        run_store
            .get_active_run()
            .ok()
            .flatten()
            .map(|r| r.id)
            .or_else(|| read_current_run_id(&overstory))
    } else {
        read_current_run_id(&overstory)
    };

    let run_id = match run_id {
        Some(id) => id,
        None => {
            if json {
                println!(
                    "{}",
                    json_output(
                        "run",
                        &serde_json::json!({"run": null, "message": "No active run"})
                    )
                );
            } else {
                print_hint("No active run");
            }
            return Ok(());
        }
    };

    if !PathBuf::from(&sessions_db).exists() {
        if json {
            println!(
                "{}",
                json_output(
                    "run",
                    &serde_json::json!({"run": null, "message": format!("Run {run_id} not found in store")})
                )
            );
        } else {
            println!("Run {} not found in store", accent(&run_id));
        }
        return Ok(());
    }

    let run_store = RunStore::new(&sessions_db).map_err(|e| e.to_string())?;
    let run = match run_store.get_run(&run_id).map_err(|e| e.to_string())? {
        Some(r) => r,
        None => {
            if json {
                println!(
                    "{}",
                    json_output(
                        "run",
                        &serde_json::json!({"run": null, "message": format!("Run {run_id} not found in store")})
                    )
                );
            } else {
                println!("Run {} not found in store", accent(&run_id));
            }
            return Ok(());
        }
    };

    let duration = format_duration(run_duration_ms(&run));

    if json {
        let rwd = RunWithDuration::from_run(&run);
        println!(
            "{}",
            json_output(
                "run",
                &serde_json::json!({"run": rwd, "duration": duration})
            )
        );
    } else {
        println!("{}", accent("Current Run"));
        println!("{}", muted(SEPARATOR));
        println!("  ID:       {}", accent(&run.id));
        println!("  Status:   {:?}", run.status);
        println!("  Started:  {}", run.started_at);
        println!("  Agents:   {}", run.agent_count);
        println!("  Duration: {}", duration);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Execute: list runs
// ---------------------------------------------------------------------------

pub fn execute_list(limit: u32, json: bool, project_override: Option<&Path>) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = load_config(&cwd, project_override).map_err(|e| e.to_string())?;
    let sessions_db = format!("{}/.overstory/sessions.db", config.project.root);

    if !PathBuf::from(&sessions_db).exists() {
        if json {
            println!(
                "{}",
                json_output("run list", &serde_json::json!({"runs": []}))
            );
        } else {
            print_hint("No runs recorded yet");
        }
        return Ok(());
    }

    let run_store = RunStore::new(&sessions_db).map_err(|e| e.to_string())?;
    let mut runs = run_store
        .list_runs(Some(crate::types::ListRunsOpts {
            limit: Some(limit as i64),
            ..Default::default()
        }))
        .map_err(|e| e.to_string())?;

    // Fallback: if the runs table is empty (overstory never populates it),
    // derive run records from the sessions table by grouping on run_id.
    if runs.is_empty() {
        let session_store = SessionStore::new(&sessions_db).map_err(|e| e.to_string())?;
        runs = session_store
            .derive_runs_from_sessions(limit as i64)
            .map_err(|e| e.to_string())?;
    }

    if json {
        let with_duration: Vec<RunWithDuration> =
            runs.iter().map(RunWithDuration::from_run).collect();
        println!(
            "{}",
            json_output("run list", &serde_json::json!({"runs": with_duration}))
        );
        return Ok(());
    }

    if runs.is_empty() {
        print_hint("No runs recorded yet");
        return Ok(());
    }

    println!("{}", accent("Recent Runs"));
    println!("{}", muted(SEPARATOR));
    println!("{:<36} {:<10} {:<7} Duration", "ID", "Status", "Agents");
    println!("{}", muted(SEPARATOR));

    for run in &runs {
        let id_display = if run.id.len() > 35 {
            format!("{}...", &run.id[..32])
        } else {
            format!("{:<36}", run.id)
        };
        let status = format!("{:?}", run.status).to_lowercase();
        let duration = format_duration(run_duration_ms(run));
        println!(
            "{} {:<10} {:<7} {}",
            accent(&id_display),
            status,
            run.agent_count,
            duration
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Execute: show run
// ---------------------------------------------------------------------------

pub fn execute_show(
    run_id: &str,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = load_config(&cwd, project_override).map_err(|e| e.to_string())?;
    let sessions_db = format!("{}/.overstory/sessions.db", config.project.root);

    if !PathBuf::from(&sessions_db).exists() {
        let msg = format!("Run {run_id} not found");
        if json {
            println!(
                "{}",
                json_output("run show", &serde_json::json!({"error": msg}))
            );
        } else {
            print_error(&msg, None);
        }
        return Err(msg);
    }

    let run_store = RunStore::new(&sessions_db).map_err(|e| e.to_string())?;
    let session_store = SessionStore::new(&sessions_db).map_err(|e| e.to_string())?;

    let run = match run_store.get_run(run_id).map_err(|e| e.to_string())? {
        Some(r) => r,
        None => {
            let msg = format!("Run {run_id} not found");
            if json {
                println!(
                    "{}",
                    json_output("run show", &serde_json::json!({"error": msg}))
                );
            } else {
                print_error(&msg, None);
            }
            return Err(msg);
        }
    };

    let agents = session_store
        .get_by_run(run_id)
        .map_err(|e| e.to_string())?;
    let duration = format_duration(run_duration_ms(&run));

    if json {
        let rwd = RunWithDuration::from_run(&run);
        println!(
            "{}",
            json_output(
                "run show",
                &serde_json::json!({"run": rwd, "duration": duration, "agents": agents})
            )
        );
        return Ok(());
    }

    println!("{}", accent("Run Details"));
    println!("{}", muted(SEPARATOR));
    println!("  ID:       {}", accent(&run.id));
    println!("  Status:   {:?}", run.status);
    println!("  Started:  {}", run.started_at);
    if let Some(ref ended) = run.completed_at {
        println!("  Ended:    {}", ended);
    }
    println!("  Agents:   {}", run.agent_count);
    println!("  Duration: {}", duration);

    if agents.is_empty() {
        println!("\nNo agents recorded for this run.");
    } else {
        println!("\nAgents ({}):", agents.len());
        println!("{}", muted(SEPARATOR));
        for agent in &agents {
            let agent_start = DateTime::parse_from_rfc3339(&agent.started_at)
                .map(|dt| dt.timestamp_millis())
                .unwrap_or(0);
            let agent_end = match agent.state {
                crate::types::AgentState::Completed | crate::types::AgentState::Zombie => {
                    DateTime::parse_from_rfc3339(&agent.last_activity)
                        .map(|dt| dt.timestamp_millis())
                        .unwrap_or_else(|_| Utc::now().timestamp_millis())
                }
                _ => Utc::now().timestamp_millis(),
            };
            let agent_dur = format_duration((agent_end - agent_start).max(0));
            let state = format!("{:?}", agent.state).to_lowercase();
            println!(
                "  {} [{}] {} | {}",
                accent(&agent.agent_name),
                agent.capability,
                state,
                agent_dur
            );
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Execute: complete current run
// ---------------------------------------------------------------------------

pub fn execute_complete(json: bool, project_override: Option<&Path>) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = load_config(&cwd, project_override).map_err(|e| e.to_string())?;
    let overstory = format!("{}/.overstory", config.project.root);
    let sessions_db = format!("{overstory}/sessions.db");

    let run_id = match read_current_run_id(&overstory) {
        Some(id) => id,
        None => {
            // Also try to get it from RunStore
            if PathBuf::from(&sessions_db).exists() {
                let run_store = RunStore::new(&sessions_db).map_err(|e| e.to_string())?;
                match run_store
                    .get_active_run()
                    .map_err(|e| e.to_string())?
                    .map(|r| r.id)
                {
                    Some(id) => id,
                    None => {
                        let msg = "No active run to complete";
                        if json {
                            println!(
                                "{}",
                                json_output("run complete", &serde_json::json!({"error": msg}))
                            );
                        } else {
                            print_error(msg, None);
                        }
                        return Err(msg.to_string());
                    }
                }
            } else {
                let msg = "No active run to complete";
                if json {
                    println!(
                        "{}",
                        json_output("run complete", &serde_json::json!({"error": msg}))
                    );
                } else {
                    print_error(msg, None);
                }
                return Err(msg.to_string());
            }
        }
    };

    if PathBuf::from(&sessions_db).exists() {
        let run_store = RunStore::new(&sessions_db).map_err(|e| e.to_string())?;
        run_store
            .complete_run(&run_id, RunStatus::Completed)
            .map_err(|e| e.to_string())?;
    }

    // Delete current-run.txt
    let current_run_file = format!("{overstory}/current-run.txt");
    let _ = std::fs::remove_file(&current_run_file);

    if json {
        println!(
            "{}",
            json_output(
                "run complete",
                &serde_json::json!({"runId": run_id, "status": "completed"})
            )
        );
    } else {
        print_success("Run completed", Some(&run_id));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration_seconds() {
        assert_eq!(format_duration(0), "0s");
        assert_eq!(format_duration(45_000), "45s");
    }

    #[test]
    fn test_format_duration_minutes() {
        assert_eq!(format_duration(90_000), "1m 30s");
    }

    #[test]
    fn test_format_duration_hours() {
        assert_eq!(format_duration(3_661_000), "1h 01m");
    }

    #[test]
    fn test_read_current_run_id_missing() {
        let id = read_current_run_id("/nonexistent/path");
        assert!(id.is_none());
    }

    #[test]
    fn test_execute_current_no_db() {
        let result = execute_current(false, Some(Path::new("/tmp/grove-test")));
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_list_no_db() {
        let result = execute_list(10, false, Some(Path::new("/tmp/grove-test")));
        assert!(result.is_ok());
    }
}
