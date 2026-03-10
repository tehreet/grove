#![allow(dead_code)]

//! `grove logs` — query and display events from events.db.

use std::path::{Path, PathBuf};

use colored::Colorize;
use serde::Serialize;

use crate::config::load_config;
use crate::db::events::EventStore;
use crate::json::json_output;
use crate::logging::muted;
use crate::types::{EventLevel, EventQueryOptions, StoredEvent};

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LogsOutput {
    events: Vec<StoredEvent>,
    count: usize,
}

// ---------------------------------------------------------------------------
// Execute
// ---------------------------------------------------------------------------

const SEPARATOR: &str = "──────────────────────────────────────────────────────────────────────";

pub fn execute(
    agent: Option<String>,
    level: Option<String>,
    since: Option<String>,
    until: Option<String>,
    limit: Option<i64>,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = load_config(&cwd, project_override).map_err(|e| e.to_string())?;
    let root = &config.project.root;
    let events_db = format!("{root}/.overstory/events.db");

    if !PathBuf::from(&events_db).exists() {
        if json {
            let out = LogsOutput { events: vec![], count: 0 };
            println!("{}", json_output("logs", &out));
        } else {
            println!("{}", muted("No events.db found"));
        }
        return Ok(());
    }

    let parsed_level = parse_level(level.as_deref())?;
    let opts = EventQueryOptions {
        since,
        until,
        level: parsed_level,
        limit,

    };

    let store = EventStore::new(&events_db).map_err(|e| e.to_string())?;
    let events = store
        .query(agent.as_deref(), None, None, &opts, false)
        .map_err(|e| e.to_string())?;

    if json {
        let out = LogsOutput { count: events.len(), events };
        println!("{}", json_output("logs", &out));
    } else {
        if events.is_empty() {
            println!("{}", muted("No events found"));
        } else {
            for ev in &events {
                println!("{}", format_event_line(ev));
            }
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

fn parse_level(s: Option<&str>) -> Result<Option<EventLevel>, String> {
    match s {
        None => Ok(None),
        Some("debug") => Ok(Some(EventLevel::Debug)),
        Some("info") => Ok(Some(EventLevel::Info)),
        Some("warn") => Ok(Some(EventLevel::Warn)),
        Some("error") => Ok(Some(EventLevel::Error)),
        Some(other) => Err(format!("Unknown level: {other}. Use debug, info, warn, or error.")),
    }
}

fn level_abbrev(level: &EventLevel) -> &'static str {
    match level {
        EventLevel::Debug => "DBG",
        EventLevel::Info => "INF",
        EventLevel::Warn => "WRN",
        EventLevel::Error => "ERR",
    }
}

fn event_type_str(et: &crate::types::EventType) -> String {
    // Display with dot notation to match overstory format (tool.start, session.end, etc.)
    serde_json::to_value(et)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.replace('_', ".").to_string()))
        .unwrap_or_else(|| format!("{et:?}"))
}

pub fn format_event_line(ev: &StoredEvent) -> String {
    let ts = &ev.created_at;
    let time_str = if ts.len() >= 19 { &ts[11..19] } else { ts.as_str() };

    let abbrev = level_abbrev(&ev.level);
    let level_colored = match ev.level {
        EventLevel::Error => abbrev.red().to_string(),
        EventLevel::Warn => abbrev.yellow().to_string(),
        EventLevel::Debug => abbrev.dimmed().to_string(),
        EventLevel::Info => abbrev.normal().to_string(),
    };

    let et = event_type_str(&ev.event_type);
    let agent_colored = format!("[{}]", ev.agent_name).cyan().to_string();

    let mut parts = vec![time_str.to_string(), level_colored, et, agent_colored];

    if let Some(ref name) = ev.tool_name {
        parts.push(format!("toolName={name}"));
    }
    if let Some(ms) = ev.tool_duration_ms {
        parts.push(format!("durationMs={ms}"));
    }
    if let Some(ref args) = ev.tool_args {
        let truncated = if args.len() > 80 { format!("{}...", &args[..80]) } else { args.clone() };
        parts.push(format!("args={truncated}"));
    }

    parts.join(" ")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{EventType, InsertEvent};

    fn make_event(agent: &str, level: EventLevel) -> InsertEvent {
        InsertEvent {
            run_id: None,
            agent_name: agent.to_string(),
            session_id: None,
            event_type: EventType::TurnStart,
            tool_name: None,
            tool_args: None,
            tool_duration_ms: None,
            level,
            data: None,
        }
    }

    #[test]
    fn test_level_parse() {
        assert!(matches!(parse_level(None), Ok(None)));
        assert!(matches!(parse_level(Some("debug")), Ok(Some(EventLevel::Debug))));
        assert!(matches!(parse_level(Some("info")), Ok(Some(EventLevel::Info))));
        assert!(matches!(parse_level(Some("warn")), Ok(Some(EventLevel::Warn))));
        assert!(matches!(parse_level(Some("error")), Ok(Some(EventLevel::Error))));
        assert!(parse_level(Some("INVALID")).is_err());
    }

    #[test]
    fn test_level_abbreviation() {
        assert_eq!(level_abbrev(&EventLevel::Debug), "DBG");
        assert_eq!(level_abbrev(&EventLevel::Info), "INF");
        assert_eq!(level_abbrev(&EventLevel::Warn), "WRN");
        assert_eq!(level_abbrev(&EventLevel::Error), "ERR");
    }

    #[test]
    fn test_format_event_line() {
        use crate::db::events::EventStore;

        let store = EventStore::new(":memory:").unwrap();
        let mut ev = make_event("builder-name", EventLevel::Info);
        ev.event_type = EventType::ToolEnd;
        ev.tool_name = Some("Bash".to_string());
        ev.tool_duration_ms = Some(1601);
        store.insert(&ev).unwrap();

        let events = store
            .query(None, None, None, &EventQueryOptions::default(), false)
            .unwrap();
        assert_eq!(events.len(), 1);

        let line = format_event_line(&events[0]);
        assert!(line.contains("INF"));
        assert!(line.contains("builder-name"));
        assert!(line.contains("toolName=Bash"));
        assert!(line.contains("durationMs=1601"));
    }
}
