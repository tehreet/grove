//! `grove feed` — live event stream from events.db.

use std::path::Path;

use colored::Colorize;
use serde::Serialize;

use crate::config::load_config;
use crate::db::events::EventStore;
use crate::json::json_output;
use crate::logging::muted;
use crate::types::{EventLevel, StoredEvent};

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct FeedOutput {
    events: Vec<StoredEvent>,
    count: usize,
}

// ---------------------------------------------------------------------------
// Execute
// ---------------------------------------------------------------------------

pub fn execute(
    follow: bool,
    agent: Option<String>,
    event_type: Option<String>,
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
            let out = FeedOutput {
                events: vec![],
                count: 0,
            };
            println!("{}", json_output("feed", &out));
        } else {
            println!("{}", muted("No events.db found"));
        }
        return Ok(());
    }

    let store = EventStore::new(&events_db).map_err(|e| e.to_string())?;

    if follow {
        // Poll mode: start from max_id and stream new events
        let mut cursor = store.get_max_id().unwrap_or(0);
        loop {
            let events = store
                .get_feed(agent.as_deref(), event_type.as_deref(), Some(cursor), None)
                .map_err(|e| e.to_string())?;
            for ev in &events {
                if json {
                    println!("{}", serde_json::to_string(ev).unwrap_or_default());
                } else {
                    print_event(ev);
                }
                cursor = cursor.max(ev.id);
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    } else {
        let events = store
            .get_feed(agent.as_deref(), event_type.as_deref(), None, limit)
            .map_err(|e| e.to_string())?;
        if json {
            let out = FeedOutput {
                count: events.len(),
                events,
            };
            println!("{}", json_output("feed", &out));
        } else {
            if events.is_empty() {
                println!("{}", muted("No events found"));
            } else {
                for ev in &events {
                    print_event(ev);
                }
            }
        }
        Ok(())
    }
}

fn level_str(level: &EventLevel) -> &'static str {
    match level {
        EventLevel::Debug => "debug",
        EventLevel::Info => "info",
        EventLevel::Warn => "warn",
        EventLevel::Error => "error",
    }
}

fn event_type_str(et: &crate::types::EventType) -> String {
    serde_json::to_value(et)
        .ok()
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| format!("{:?}", et))
}

fn print_event(ev: &StoredEvent) {
    let ts = &ev.created_at;
    let short_ts = if ts.len() >= 19 {
        &ts[..19]
    } else {
        ts.as_str()
    };
    let lvl = level_str(&ev.level);
    let level_colored = match lvl {
        "error" => lvl.red().to_string(),
        "warn" => lvl.yellow().to_string(),
        "debug" => lvl.dimmed().to_string(),
        _ => lvl.normal().to_string(),
    };
    let tool_suffix = ev
        .tool_name
        .as_ref()
        .map(|t| format!(" [{}]", t))
        .unwrap_or_default();
    let data_suffix = ev
        .data
        .as_ref()
        .map(|d| {
            let preview = if d.len() > 60 {
                format!("{}...", &d[..60])
            } else {
                d.clone()
            };
            format!(" {}", preview)
        })
        .unwrap_or_default();
    println!(
        "{} {} {} {}{}{}",
        muted(short_ts),
        ev.agent_name.cyan(),
        level_colored,
        event_type_str(&ev.event_type).bold(),
        tool_suffix,
        muted(&data_suffix),
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::db::events::EventStore;
    use crate::types::{EventLevel, EventType, InsertEvent};

    fn make_event(agent: &str) -> InsertEvent {
        InsertEvent {
            run_id: None,
            agent_name: agent.to_string(),
            session_id: None,
            event_type: EventType::TurnStart,
            tool_name: None,
            tool_args: None,
            tool_duration_ms: None,
            level: EventLevel::Info,
            data: None,
        }
    }

    #[test]
    fn test_feed_get_feed_no_filter() {
        let store = EventStore::new(":memory:").unwrap();
        store.insert(&make_event("agent-a")).unwrap();
        store.insert(&make_event("agent-b")).unwrap();
        let events = store.get_feed(None, None, None, None).unwrap();
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_feed_get_feed_agent_filter() {
        let store = EventStore::new(":memory:").unwrap();
        store.insert(&make_event("agent-a")).unwrap();
        store.insert(&make_event("agent-b")).unwrap();
        let events = store.get_feed(Some("agent-a"), None, None, None).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].agent_name, "agent-a");
    }

    #[test]
    fn test_feed_since_id_cursor() {
        let store = EventStore::new(":memory:").unwrap();
        let id1 = store.insert(&make_event("agent-a")).unwrap();
        store.insert(&make_event("agent-a")).unwrap();
        let events = store.get_feed(None, None, Some(id1), None).unwrap();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn test_feed_limit() {
        let store = EventStore::new(":memory:").unwrap();
        for _ in 0..5 {
            store.insert(&make_event("a")).unwrap();
        }
        let events = store.get_feed(None, None, None, Some(3)).unwrap();
        assert_eq!(events.len(), 3);
    }
}
