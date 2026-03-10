#![allow(dead_code)]

//! `grove replay` — chronological replay of events from events.db.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::config::load_config;
use crate::db::events::EventStore;
use crate::json::json_output;
use crate::logging::{brand_bold, format_relative_time, muted};
use crate::types::{EventQueryOptions, StoredEvent};

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReplayOutput {
    events: Vec<StoredEvent>,
    count: usize,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const SEPARATOR: &str = "──────────────────────────────────────────────────────────────────────";

// ---------------------------------------------------------------------------
// Execute
// ---------------------------------------------------------------------------

pub fn execute(
    run: Option<String>,
    agents: Vec<String>,
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
            let out = ReplayOutput { events: vec![], count: 0 };
            println!("{}", json_output("replay", &out));
        } else {
            println!("{}", muted("No events.db found"));
        }
        return Ok(());
    }

    let opts = EventQueryOptions {
        since,
        until,
        level: None,
        limit: limit.or(Some(200)),

    };

    let agents_slice: Option<&[String]> = if agents.is_empty() { None } else { Some(&agents) };
    let store = EventStore::new(&events_db).map_err(|e| e.to_string())?;
    let events = store
        .query(None, agents_slice, run.as_deref(), &opts, true)
        .map_err(|e| e.to_string())?;

    if json {
        let out = ReplayOutput { count: events.len(), events };
        println!("{}", json_output("replay", &out));
        return Ok(());
    }

    println!("{}", brand_bold("Replay"));
    println!("{}", muted(SEPARATOR));
    println!("{} events", events.len());

    if events.is_empty() {
        return Ok(());
    }

    println!();

    // Group events by date (YYYY-MM-DD)
    let mut by_date: BTreeMap<String, Vec<&StoredEvent>> = BTreeMap::new();
    for ev in &events {
        let date = extract_date(&ev.created_at);
        by_date.entry(date).or_default().push(ev);
    }

    for (date, group) in &by_date {
        println!("{}", muted(&format!("--- {date} ---")));
        for ev in group {
            println!("{}", format_replay_line(ev));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

fn extract_date(ts: &str) -> String {
    if ts.len() >= 10 {
        ts[..10].to_string()
    } else {
        ts.to_string()
    }
}

fn event_type_upper(et: &crate::types::EventType) -> String {
    serde_json::to_value(et)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_uppercase()))
        .unwrap_or_else(|| format!("{et:?}").to_uppercase())
}

pub fn format_replay_line(ev: &StoredEvent) -> String {
    let rel = format_relative_time(&ev.created_at);
    let et = event_type_upper(&ev.event_type);
    let agent = format!("[{}]", ev.agent_name);

    let mut line = format!("    {rel} {et}  {agent}");

    if let Some(ref data) = ev.data {
        let preview = if data.len() > 80 {
            format!("{}...", &data[..80])
        } else {
            data.clone()
        };
        line.push_str(&format!(" data={preview}"));
    }

    line
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{EventLevel, EventType};

    fn make_stored(agent: &str, et: EventType, ts: &str) -> StoredEvent {
        StoredEvent {
            id: 1,
            run_id: None,
            agent_name: agent.to_string(),
            session_id: None,
            event_type: et,
            tool_name: None,
            tool_args: None,
            tool_duration_ms: None,
            level: EventLevel::Info,
            data: None,
            created_at: ts.to_string(),
        }
    }

    #[test]
    fn test_relative_time() {
        // format_relative_time is tested via logging; verify it returns a string
        let ts = "2020-01-01T00:00:00Z";
        let rel = format_relative_time(ts);
        assert!(rel.contains("ago") || !rel.is_empty());
    }

    #[test]
    fn test_date_grouping() {
        let events = vec![
            make_stored("agent-a", EventType::SessionStart, "2026-03-08T10:00:00Z"),
            make_stored("agent-b", EventType::TurnStart, "2026-03-08T11:00:00Z"),
            make_stored("agent-a", EventType::TurnEnd, "2026-03-09T09:00:00Z"),
        ];

        let mut by_date: std::collections::BTreeMap<String, Vec<&StoredEvent>> =
            std::collections::BTreeMap::new();
        for ev in &events {
            let date = extract_date(&ev.created_at);
            by_date.entry(date).or_default().push(ev);
        }

        assert_eq!(by_date.len(), 2);
        assert_eq!(by_date["2026-03-08"].len(), 2);
        assert_eq!(by_date["2026-03-09"].len(), 1);
    }

    #[test]
    fn test_event_type_uppercase() {
        let ev = make_stored("agent-a", EventType::MailSent, "2026-03-08T10:00:00Z");
        let line = format_replay_line(&ev);
        assert!(line.contains("MAIL_SENT"));
    }
}
