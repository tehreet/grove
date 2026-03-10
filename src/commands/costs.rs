//! `grove costs` — token costs and spending overview.
//!
//! Shows token usage and estimated costs from metrics.db.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::load_config;
use crate::db::metrics::MetricsStore;
use crate::json::json_output;
use crate::logging::{brand_bold, muted};
use crate::types::{SessionMetrics, TokenSnapshot};

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CostTotals {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_creation_tokens: i64,
    pub estimated_cost_usd: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CostsOutput {
    sessions: Vec<SessionMetrics>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LiveOutput {
    snapshots: Vec<TokenSnapshot>,
}

// ---------------------------------------------------------------------------
// Execute
// ---------------------------------------------------------------------------

pub fn execute(
    agent: Option<String>,
    run: Option<String>,
    live: bool,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = load_config(&cwd, project_override).map_err(|e| e.to_string())?;
    let root = &config.project.root;
    let metrics_db = format!("{root}/.overstory/metrics.db");

    if !PathBuf::from(&metrics_db).exists() {
        if json {
            let empty = CostsOutput {
                sessions: vec![],
            };
            println!("{}", json_output("costs", &empty));
        } else {
            println!("{}", brand_bold("Cost Summary"));
            println!("{}", muted(SEPARATOR));
            println!("{}", muted("  No metrics data found"));
        }
        return Ok(());
    }

    let store = MetricsStore::new(&metrics_db).map_err(|e| e.to_string())?;

    if live {
        let snapshots = store
            .get_latest_snapshots(run.as_deref())
            .map_err(|e| e.to_string())?;

        let totals = totals_from_snapshots(&snapshots);

        if json {
            let output = LiveOutput { snapshots };
            println!("{}", json_output("costs", &output));
        } else {
            print_snapshots_text(&snapshots, &totals);
        }
        return Ok(());
    }

    // Standard mode: session cost table
    let sessions = if let Some(ref name) = agent {
        store
            .get_sessions_by_agent(name)
            .map_err(|e| e.to_string())?
    } else if let Some(ref run_id) = run {
        store
            .get_sessions_by_run(run_id)
            .map_err(|e| e.to_string())?
    } else {
        store
            .get_recent_sessions(None)
            .map_err(|e| e.to_string())?
    };

    let totals = totals_from_sessions(&sessions);

    if json {
        let output = CostsOutput {
            sessions: sessions.clone(),
        };
        println!("{}", json_output("costs", &output));
    } else {
        print_sessions_text(&sessions, &totals);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Totals helpers
// ---------------------------------------------------------------------------

fn totals_from_sessions(sessions: &[SessionMetrics]) -> CostTotals {
    CostTotals {
        input_tokens: sessions.iter().map(|s| s.input_tokens).sum(),
        output_tokens: sessions.iter().map(|s| s.output_tokens).sum(),
        cache_read_tokens: sessions.iter().map(|s| s.cache_read_tokens).sum(),
        cache_creation_tokens: sessions.iter().map(|s| s.cache_creation_tokens).sum(),
        estimated_cost_usd: sessions
            .iter()
            .filter_map(|s| s.estimated_cost_usd)
            .sum(),
    }
}

fn totals_from_snapshots(snapshots: &[TokenSnapshot]) -> CostTotals {
    CostTotals {
        input_tokens: snapshots.iter().map(|s| s.input_tokens).sum(),
        output_tokens: snapshots.iter().map(|s| s.output_tokens).sum(),
        cache_read_tokens: snapshots.iter().map(|s| s.cache_read_tokens).sum(),
        cache_creation_tokens: snapshots.iter().map(|s| s.cache_creation_tokens).sum(),
        estimated_cost_usd: snapshots
            .iter()
            .filter_map(|s| s.estimated_cost_usd)
            .sum(),
    }
}

// ---------------------------------------------------------------------------
// Text formatting
// ---------------------------------------------------------------------------

const SEPARATOR: &str = "──────────────────────────────────────────────────────────────────────";

fn print_sessions_text(sessions: &[SessionMetrics], totals: &CostTotals) {
    println!("{}", brand_bold("Cost Summary"));
    println!("{}", muted(SEPARATOR));

    // Header
    println!(
        "{:<22} {:<10} {:>10} {:>10} {:>10} {:>10}",
        "Agent", "Cap", "In Tok", "Out Tok", "Cache", "Cost"
    );
    println!("{}", muted(SEPARATOR));

    for s in sessions {
        let cost = s
            .estimated_cost_usd
            .map(|c| format!("${:.2}", c))
            .unwrap_or_else(|| "-".to_string());
        println!(
            "{:<22} {:<10} {:>10} {:>10} {:>10} {:>10}",
            truncate(&s.agent_name, 22),
            truncate(&s.capability, 10),
            format_tokens(s.input_tokens),
            format_tokens(s.output_tokens),
            format_tokens(s.cache_read_tokens),
            cost,
        );
    }

    println!("{}", muted(SEPARATOR));
    println!(
        "{:<22} {:<10} {:>10} {:>10} {:>10} {:>10}",
        "Total",
        "",
        format_tokens(totals.input_tokens),
        format_tokens(totals.output_tokens),
        format_tokens(totals.cache_read_tokens),
        format!("${:.2}", totals.estimated_cost_usd),
    );
}

fn print_snapshots_text(snapshots: &[TokenSnapshot], totals: &CostTotals) {
    println!("{}", brand_bold("Live Token Usage"));
    println!("{}", muted(SEPARATOR));

    println!(
        "{:<22} {:>10} {:>10} {:>10} {:>10}",
        "Agent", "In Tok", "Out Tok", "Cache", "Cost"
    );
    println!("{}", muted(SEPARATOR));

    for s in snapshots {
        let cost = s
            .estimated_cost_usd
            .map(|c| format!("${:.2}", c))
            .unwrap_or_else(|| "-".to_string());
        println!(
            "{:<22} {:>10} {:>10} {:>10} {:>10}",
            truncate(&s.agent_name, 22),
            format_tokens(s.input_tokens),
            format_tokens(s.output_tokens),
            format_tokens(s.cache_read_tokens),
            cost,
        );
    }

    println!("{}", muted(SEPARATOR));
    println!(
        "{:<22} {:>10} {:>10} {:>10} {:>10}",
        "Total",
        format_tokens(totals.input_tokens),
        format_tokens(totals.output_tokens),
        format_tokens(totals.cache_read_tokens),
        format!("${:.2}", totals.estimated_cost_usd),
    );
}

fn format_tokens(n: i64) -> String {
    // Format with comma separators
    let s = n.to_string();
    let mut result = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max - 1])
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(999), "999");
        assert_eq!(format_tokens(1000), "1,000");
        assert_eq!(format_tokens(45230), "45,230");
        assert_eq!(format_tokens(1_000_000), "1,000,000");
    }

    #[test]
    fn test_totals_from_sessions_empty() {
        let totals = totals_from_sessions(&[]);
        assert_eq!(totals.input_tokens, 0);
        assert_eq!(totals.estimated_cost_usd, 0.0);
    }

    #[test]
    fn test_totals_from_sessions() {
        let sessions = vec![
            SessionMetrics {
                agent_name: "a".to_string(),
                task_id: "t".to_string(),
                capability: "builder".to_string(),
                started_at: "2024-01-01T00:00:00Z".to_string(),
                completed_at: None,
                duration_ms: 0,
                exit_code: None,
                merge_result: None,
                parent_agent: None,
                input_tokens: 100,
                output_tokens: 200,
                cache_read_tokens: 50,
                cache_creation_tokens: 10,
                estimated_cost_usd: Some(0.50),
                model_used: None,
                run_id: None,
            },
            SessionMetrics {
                agent_name: "b".to_string(),
                task_id: "t".to_string(),
                capability: "lead".to_string(),
                started_at: "2024-01-01T00:00:00Z".to_string(),
                completed_at: None,
                duration_ms: 0,
                exit_code: None,
                merge_result: None,
                parent_agent: None,
                input_tokens: 200,
                output_tokens: 300,
                cache_read_tokens: 75,
                cache_creation_tokens: 20,
                estimated_cost_usd: Some(1.00),
                model_used: None,
                run_id: None,
            },
        ];

        let totals = totals_from_sessions(&sessions);
        assert_eq!(totals.input_tokens, 300);
        assert_eq!(totals.output_tokens, 500);
        assert_eq!(totals.cache_read_tokens, 125);
        assert!((totals.estimated_cost_usd - 1.50).abs() < 0.001);
    }

    #[test]
    fn test_execute_no_metrics_db() {
        let result = execute(None, None, false, false, Some(Path::new("/tmp")));
        let _ = result;
    }

    #[test]
    fn test_execute_json_no_metrics_db() {
        let result = execute(None, None, false, true, Some(Path::new("/tmp")));
        let _ = result;
    }
}
