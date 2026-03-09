//! `grove trace` — chronological event timeline for an agent or task.

use std::path::Path;

use colored::Colorize;
use serde::Serialize;

use crate::config::load_config;
use crate::db::events::EventStore;
use crate::db::mail::MailStore;
use crate::json::json_output;
use crate::logging::muted;
use crate::types::{EventLevel, MailFilters, MailMessage, StoredEvent};

// ---------------------------------------------------------------------------
// Timeline item — unified event + mail view
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum TimelineItem {
    Event(StoredEvent),
    Mail(MailMessage),
}

impl TimelineItem {
    fn created_at(&self) -> &str {
        match self {
            TimelineItem::Event(e) => &e.created_at,
            TimelineItem::Mail(m) => &m.created_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TraceOutput {
    subject: String,
    items: Vec<TimelineItem>,
    count: usize,
}

// ---------------------------------------------------------------------------
// Execute
// ---------------------------------------------------------------------------

pub fn execute(
    subject: &str,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = load_config(&cwd, project_override).map_err(|e| e.to_string())?;
    let root = &config.project.root;
    let overstory = format!("{root}/.overstory");

    let events_db = format!("{overstory}/events.db");
    let mail_db = format!("{overstory}/mail.db");

    // Determine if subject looks like a task ID (contains a dash and hex, e.g. grove-8517)
    // For agent-name subjects, query by agent. For task IDs, also include data search.
    let is_task_id = subject.starts_with("grove-")
        || subject.chars().any(|c| c.is_ascii_digit())
            && !subject.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');

    // Collect events
    let mut items: Vec<TimelineItem> = Vec::new();

    if std::path::PathBuf::from(&events_db).exists() {
        let store = EventStore::new(&events_db).map_err(|e| e.to_string())?;

        // Always try agent-name lookup
        let agent_events = store
            .get_feed(Some(subject), None, None, Some(100))
            .map_err(|e| e.to_string())?;
        for ev in agent_events {
            items.push(TimelineItem::Event(ev));
        }

        // Also search by task ID if it looks like one or no agent events found
        if is_task_id || items.is_empty() {
            let task_events = store
                .get_by_task(subject, Some(100))
                .map_err(|e| e.to_string())?;
            for ev in task_events {
                // Deduplicate by ID
                if !items.iter().any(|i| matches!(i, TimelineItem::Event(e) if e.id == ev.id)) {
                    items.push(TimelineItem::Event(ev));
                }
            }
        }
    }

    // Collect mail
    if std::path::PathBuf::from(&mail_db).exists() {
        let store = MailStore::new(&mail_db).map_err(|e| e.to_string())?;

        // Mail sent by subject (agent)
        let sent = store
            .get_all(Some(MailFilters {
                from_agent: Some(subject.to_string()),
                limit: Some(50),
                ..Default::default()
            }))
            .map_err(|e| e.to_string())?;
        for m in sent {
            items.push(TimelineItem::Mail(m));
        }

        // Mail received by subject (agent)
        let received = store
            .get_all(Some(MailFilters {
                to_agent: Some(subject.to_string()),
                limit: Some(50),
                ..Default::default()
            }))
            .map_err(|e| e.to_string())?;
        for m in received {
            // Deduplicate by ID
            if !items.iter().any(|i| matches!(i, TimelineItem::Mail(msg) if msg.id == m.id)) {
                items.push(TimelineItem::Mail(m));
            }
        }
    }

    // Sort by created_at ascending
    items.sort_by(|a, b| a.created_at().cmp(b.created_at()));

    if json {
        let count = items.len();
        let out = TraceOutput { subject: subject.to_string(), items, count };
        println!("{}", json_output("trace", &out));
    } else {
        print_trace(subject, &items);
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Text formatting
// ---------------------------------------------------------------------------

fn print_trace(subject: &str, items: &[TimelineItem]) {
    println!("{}", format!("Trace: {}", subject).bold());
    println!("{}", muted("─────────────────────────────────────────────"));

    if items.is_empty() {
        println!("{}", muted("  No events or mail found"));
        return;
    }

    for item in items {
        match item {
            TimelineItem::Event(ev) => {
                let ts = &ev.created_at;
                let short_ts = if ts.len() >= 19 { &ts[..19] } else { ts.as_str() };
                let level_icon = match ev.level {
                    EventLevel::Error => "✗".red().to_string(),
                    EventLevel::Warn => "!".yellow().to_string(),
                    EventLevel::Debug => "·".dimmed().to_string(),
                    EventLevel::Info => "·".dimmed().to_string(),
                };
                let et = serde_json::to_value(ev.event_type)
                    .ok()
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| format!("{:?}", ev.event_type));
                let tool = ev
                    .tool_name
                    .as_ref()
                    .map(|t| format!(" [{}]", t))
                    .unwrap_or_default();
                println!(
                    "  {} {} {} {}{}",
                    muted(short_ts),
                    level_icon,
                    et.bold(),
                    ev.agent_name.dimmed(),
                    tool,
                );
            }
            TimelineItem::Mail(msg) => {
                let ts = &msg.created_at;
                let short_ts = if ts.len() >= 19 { &ts[..19] } else { ts.as_str() };
                println!(
                    "  {} {} {} → {} | {}",
                    muted(short_ts),
                    "✉".cyan(),
                    msg.from.cyan(),
                    msg.to.cyan(),
                    msg.subject,
                );
            }
        }
    }
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
    fn test_trace_get_by_task() {
        let store = EventStore::new(":memory:").unwrap();
        let mut ev = make_event("builder-1");
        ev.data = Some(r#"{"task_id":"grove-1234"}"#.to_string());
        store.insert(&ev).unwrap();
        store.insert(&make_event("other-agent")).unwrap();

        let found = store.get_by_task("grove-1234", None).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].agent_name, "builder-1");
    }

    #[test]
    fn test_trace_no_overstory_dir() {
        use std::path::Path;
        let result = super::execute("test-agent", false, Some(Path::new("/tmp")));
        let _ = result;
    }
}
