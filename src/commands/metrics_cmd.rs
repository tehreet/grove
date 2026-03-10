#![allow(dead_code)]

//! `grove metrics` — session metrics overview.
//!
//! Shows session counts, durations, and capability breakdowns from metrics.db.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::load_config;
use crate::db::metrics::MetricsStore;
use crate::json::json_output;
use crate::logging::{brand_bold, muted};
use crate::types::SessionMetrics;

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CapabilityBreakdown {
    capability: String,
    count: usize,
    avg_duration_ms: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MetricsOutput {
    total_sessions: usize,
    completed: usize,
    avg_duration_ms: f64,
    by_capability: Vec<CapabilityBreakdown>,
    recent_sessions: Vec<SessionMetrics>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const SEPARATOR: &str = "──────────────────────────────────────────────────────────────────────";

fn format_duration_ms(ms: i64) -> String {
    if ms == 0 {
        return "0s".to_string();
    }
    if ms < 60_000 {
        return format!("{}s", ms / 1000);
    }
    if ms < 3_600_000 {
        let minutes = ms / 60_000;
        let seconds = (ms % 60_000) / 1000;
        return format!("{}m {}s", minutes, seconds);
    }
    let hours = ms / 3_600_000;
    let minutes = (ms % 3_600_000) / 60_000;
    format!("{}h {}m", hours, minutes)
}

fn session_status(s: &SessionMetrics) -> &'static str {
    if s.completed_at.is_some() {
        "done"
    } else if s.exit_code == Some(1) {
        "failed"
    } else {
        "active"
    }
}

fn compute_by_capability(sessions: &[SessionMetrics]) -> Vec<CapabilityBreakdown> {
    let mut map: HashMap<String, (usize, i64)> = HashMap::new();
    for s in sessions {
        let entry = map.entry(s.capability.clone()).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += s.duration_ms;
    }
    let mut breakdowns: Vec<CapabilityBreakdown> = map
        .into_iter()
        .map(|(cap, (count, total_ms))| CapabilityBreakdown {
            capability: cap,
            count,
            avg_duration_ms: if count > 0 { total_ms as f64 / count as f64 } else { 0.0 },
        })
        .collect();
    breakdowns.sort_by(|a, b| b.count.cmp(&a.count).then(a.capability.cmp(&b.capability)));
    breakdowns
}

// ---------------------------------------------------------------------------
// Execute
// ---------------------------------------------------------------------------

pub fn execute(last: Option<i64>, json: bool, project_override: Option<&Path>) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = load_config(&cwd, project_override).map_err(|e| e.to_string())?;
    let root = &config.project.root;
    let metrics_db = format!("{root}/.overstory/metrics.db");

    if !PathBuf::from(&metrics_db).exists() {
        if json {
            let empty = MetricsOutput {
                total_sessions: 0,
                completed: 0,
                avg_duration_ms: 0.0,
                by_capability: vec![],
                recent_sessions: vec![],
            };
            println!("{}", json_output("metrics", &empty));
        } else {
            println!("{}", brand_bold("Session Metrics"));
            println!("{}", muted(SEPARATOR));
            println!("{}", muted("No metrics data found"));
        }
        return Ok(());
    }

    let store = MetricsStore::new(&metrics_db).map_err(|e| e.to_string())?;

    // Get all sessions for aggregate stats
    let all_sessions = store.get_recent_sessions(None).map_err(|e| e.to_string())?;
    let total = all_sessions.len();
    let completed = all_sessions.iter().filter(|s| s.completed_at.is_some()).count();
    let avg_duration_ms = if total > 0 {
        all_sessions.iter().map(|s| s.duration_ms).sum::<i64>() as f64 / total as f64
    } else {
        0.0
    };
    let by_capability = compute_by_capability(&all_sessions);

    // Get recent sessions with limit
    let limit = last.unwrap_or(20);
    let recent = store
        .get_recent_sessions(Some(limit))
        .map_err(|e| e.to_string())?;

    if json {
        let output = MetricsOutput {
            total_sessions: total,
            completed,
            avg_duration_ms,
            by_capability,
            recent_sessions: recent,
        };
        println!("{}", json_output("metrics", &output));
    } else {
        println!("{}", brand_bold("Session Metrics"));
        println!("{}", muted(SEPARATOR));
        println!();
        println!("Total sessions: {}", total);
        println!("Completed: {}", completed);
        println!("Avg duration: {}", format_duration_ms(avg_duration_ms as i64));
        println!();
        println!("By capability:");
        for cap in &by_capability {
            println!(
                "  {}: {} sessions (avg {})",
                cap.capability,
                cap.count,
                format_duration_ms(cap.avg_duration_ms as i64)
            );
        }
        println!();
        println!("Recent sessions:");
        for s in &recent {
            println!(
                "  {} [{}] {} | {} | {}",
                s.agent_name,
                s.capability,
                s.task_id,
                session_status(s),
                format_duration_ms(s.duration_ms)
            );
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

    fn make_session(
        agent: &str,
        task: &str,
        capability: &str,
        duration_ms: i64,
        completed: bool,
        exit_code: Option<i64>,
    ) -> SessionMetrics {
        SessionMetrics {
            agent_name: agent.to_string(),
            task_id: task.to_string(),
            capability: capability.to_string(),
            started_at: "2024-01-01T00:00:00Z".to_string(),
            completed_at: if completed {
                Some("2024-01-01T01:00:00Z".to_string())
            } else {
                None
            },
            duration_ms,
            exit_code,
            merge_result: None,
            parent_agent: None,
            input_tokens: 100,
            output_tokens: 200,
            cache_read_tokens: 50,
            cache_creation_tokens: 10,
            estimated_cost_usd: Some(0.01),
            model_used: None,
            run_id: None,
        }
    }

    #[test]
    fn test_format_duration_ms() {
        assert_eq!(format_duration_ms(0), "0s");
        assert_eq!(format_duration_ms(45_000), "45s");
        assert_eq!(format_duration_ms(59_999), "59s");
        assert_eq!(format_duration_ms(60_000), "1m 0s");
        assert_eq!(format_duration_ms(273_000), "4m 33s");
        assert_eq!(format_duration_ms(948_000), "15m 48s");
        assert_eq!(format_duration_ms(3_600_000), "1h 0m");
        assert_eq!(format_duration_ms(4_500_000), "1h 15m");
    }

    #[test]
    fn test_session_status_display() {
        let done = make_session("a", "t1", "builder", 1000, true, Some(0));
        assert_eq!(session_status(&done), "done");

        let failed = make_session("a", "t2", "builder", 1000, false, Some(1));
        assert_eq!(session_status(&failed), "failed");

        let active = make_session("a", "t3", "builder", 1000, false, None);
        assert_eq!(session_status(&active), "active");

        // exit_code=0, no completed_at → active
        let active2 = make_session("a", "t4", "builder", 1000, false, Some(0));
        assert_eq!(session_status(&active2), "active");
    }

    #[test]
    fn test_capability_grouping() {
        let sessions = vec![
            make_session("a1", "t1", "builder", 3_000, true, Some(0)),
            make_session("a2", "t2", "builder", 5_000, true, Some(0)),
            make_session("a3", "t3", "builder", 1_000, true, Some(0)),
            make_session("b1", "t4", "lead", 15_000, true, Some(0)),
        ];

        let breakdown = compute_by_capability(&sessions);
        let builder = breakdown.iter().find(|c| c.capability == "builder").unwrap();
        let lead = breakdown.iter().find(|c| c.capability == "lead").unwrap();

        assert_eq!(builder.count, 3);
        assert!((builder.avg_duration_ms - 3_000.0).abs() < 1.0);
        assert_eq!(lead.count, 1);
        assert!((lead.avg_duration_ms - 15_000.0).abs() < 1.0);

        // builder should come first (higher count)
        assert_eq!(breakdown[0].capability, "builder");
    }

    #[test]
    fn test_execute_no_metrics_db() {
        let result = execute(None, false, Some(Path::new("/tmp/grove-test")));
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_json_no_metrics_db() {
        let result = execute(None, true, Some(Path::new("/tmp/grove-test")));
        assert!(result.is_ok());
    }
}
