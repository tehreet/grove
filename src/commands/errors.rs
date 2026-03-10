//! `grove errors` — aggregated error events from events.db.

use std::path::Path;

use colored::Colorize;
use serde::Serialize;

use crate::config::load_config;
use crate::db::events::EventStore;
use crate::json::json_output;
use crate::logging::muted;
use crate::types::StoredEvent;

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentErrors {
    agent_name: String,
    count: i64,
    latest: StoredEvent,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ErrorsOutput {
    groups: Vec<AgentErrors>,
    total: usize,
}

// ---------------------------------------------------------------------------
// Execute
// ---------------------------------------------------------------------------

pub fn execute(
    agent: Option<String>,
    limit: Option<usize>,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = load_config(&cwd, project_override).map_err(|e| e.to_string())?;
    let root = &config.project.root;
    let events_db = format!("{root}/.overstory/events.db");

    if !std::path::PathBuf::from(&events_db).exists() {
        if json {
            let out = ErrorsOutput {
                groups: vec![],
                total: 0,
            };
            println!("{}", json_output("errors", &out));
        } else {
            println!("{}", muted("No events.db found"));
        }
        return Ok(());
    }

    let store = EventStore::new(&events_db).map_err(|e| e.to_string())?;
    let grouped = store
        .get_errors_grouped(agent.as_deref(), limit)
        .map_err(|e| e.to_string())?;

    let groups: Vec<AgentErrors> = grouped
        .into_iter()
        .map(|(agent_name, count, latest)| AgentErrors {
            agent_name,
            count,
            latest,
        })
        .collect();

    if json {
        let total = groups.iter().map(|g| g.count as usize).sum();
        let out = ErrorsOutput { total, groups };
        println!("{}", json_output("errors", &out));
    } else {
        if groups.is_empty() {
            println!("{}", muted("No errors found"));
        } else {
            println!("{}", "Errors by agent".bold());
            println!("{}", muted("─────────────────────────────────────────────"));
            for g in &groups {
                let ts = &g.latest.created_at;
                let short_ts = if ts.len() >= 19 {
                    &ts[..19]
                } else {
                    ts.as_str()
                };
                let data = g
                    .latest
                    .data
                    .as_ref()
                    .map(|d| {
                        if d.len() > 80 {
                            format!("{}...", &d[..80])
                        } else {
                            d.clone()
                        }
                    })
                    .unwrap_or_default();
                println!(
                    "  {} {} {} (latest: {})",
                    g.agent_name.cyan(),
                    format!("{} errors", g.count).red().bold(),
                    muted(&format!("@ {}", short_ts)),
                    data,
                );
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::db::events::EventStore;
    use crate::types::{EventLevel, EventType, InsertEvent};

    fn error_event(agent: &str) -> InsertEvent {
        InsertEvent {
            run_id: None,
            agent_name: agent.to_string(),
            session_id: None,
            event_type: EventType::Error,
            tool_name: None,
            tool_args: None,
            tool_duration_ms: None,
            level: EventLevel::Error,
            data: Some("something went wrong".to_string()),
        }
    }

    #[test]
    fn test_errors_grouped_basic() {
        let store = EventStore::new(":memory:").unwrap();
        store.insert(&error_event("agent-a")).unwrap();
        store.insert(&error_event("agent-a")).unwrap();
        store.insert(&error_event("agent-b")).unwrap();

        let grouped = store.get_errors_grouped(None, None).unwrap();
        assert_eq!(grouped.len(), 2);
        // agent-a should come first (2 errors)
        assert_eq!(grouped[0].0, "agent-a");
        assert_eq!(grouped[0].1, 2);
        assert_eq!(grouped[1].0, "agent-b");
        assert_eq!(grouped[1].1, 1);
    }

    #[test]
    fn test_errors_grouped_agent_filter() {
        let store = EventStore::new(":memory:").unwrap();
        store.insert(&error_event("agent-a")).unwrap();
        store.insert(&error_event("agent-b")).unwrap();

        let grouped = store.get_errors_grouped(Some("agent-a"), None).unwrap();
        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[0].0, "agent-a");
    }

    #[test]
    fn test_errors_grouped_empty() {
        let store = EventStore::new(":memory:").unwrap();
        let grouped = store.get_errors_grouped(None, None).unwrap();
        assert!(grouped.is_empty());
    }
}
