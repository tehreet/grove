//! `grove inspect` — deep agent inspection: session, events, mail, metrics.

use std::path::Path;

use colored::Colorize;
use serde::Serialize;

use crate::config::load_config;
use crate::db::events::EventStore;
use crate::db::mail::MailStore;
use crate::db::metrics::MetricsStore;
use crate::db::sessions::SessionStore;
use crate::json::json_output;
use crate::logging::muted;
use crate::types::{AgentSession, MailFilters, MailMessage, SessionMetrics, StoredEvent};

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct InspectOutput {
    agent_name: String,
    session: Option<AgentSession>,
    recent_events: Vec<StoredEvent>,
    sent_mail: Vec<MailMessage>,
    received_mail: Vec<MailMessage>,
    metrics: Option<SessionMetrics>,
}

// ---------------------------------------------------------------------------
// Execute
// ---------------------------------------------------------------------------

pub fn execute(
    agent_name: &str,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = load_config(&cwd, project_override).map_err(|e| e.to_string())?;
    let root = &config.project.root;
    let overstory = format!("{root}/.overstory");

    // --- Session ---
    let sessions_db = format!("{overstory}/sessions.db");
    let session: Option<AgentSession> = if std::path::PathBuf::from(&sessions_db).exists() {
        SessionStore::new(&sessions_db)
            .ok()
            .and_then(|s| s.get_by_name(agent_name).ok().flatten())
    } else {
        None
    };

    // --- Recent events ---
    let events_db = format!("{overstory}/events.db");
    let recent_events: Vec<StoredEvent> = if std::path::PathBuf::from(&events_db).exists() {
        EventStore::new(&events_db)
            .ok()
            .and_then(|store| store.get_feed(Some(agent_name), None, None, Some(20)).ok())
            .unwrap_or_default()
    } else {
        vec![]
    };

    // --- Mail ---
    let mail_db = format!("{overstory}/mail.db");
    let (sent_mail, received_mail): (Vec<MailMessage>, Vec<MailMessage>) =
        if std::path::PathBuf::from(&mail_db).exists() {
            let sent = MailStore::new(&mail_db)
                .ok()
                .and_then(|store| {
                    store
                        .get_all(Some(MailFilters {
                            from_agent: Some(agent_name.to_string()),
                            limit: Some(10),
                            ..Default::default()
                        }))
                        .ok()
                })
                .unwrap_or_default();
            let received = MailStore::new(&mail_db)
                .ok()
                .and_then(|store| {
                    store
                        .get_all(Some(MailFilters {
                            to_agent: Some(agent_name.to_string()),
                            limit: Some(10),
                            ..Default::default()
                        }))
                        .ok()
                })
                .unwrap_or_default();
            (sent, received)
        } else {
            (vec![], vec![])
        };

    // --- Metrics ---
    let metrics_db = format!("{overstory}/metrics.db");
    let metrics: Option<SessionMetrics> = if std::path::PathBuf::from(&metrics_db).exists() {
        MetricsStore::new(&metrics_db)
            .ok()
            .and_then(|store| store.get_sessions_by_agent(agent_name).ok())
            .and_then(|sessions| sessions.into_iter().next())
    } else {
        None
    };

    if json {
        let out = InspectOutput {
            agent_name: agent_name.to_string(),
            session,
            recent_events,
            sent_mail,
            received_mail,
            metrics,
        };
        println!("{}", json_output("inspect", &out));
    } else {
        print_inspect(agent_name, session.as_ref(), &recent_events, &sent_mail, &received_mail, metrics.as_ref());
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Text formatting
// ---------------------------------------------------------------------------

fn print_inspect(
    agent_name: &str,
    session: Option<&AgentSession>,
    events: &[StoredEvent],
    sent: &[MailMessage],
    received: &[MailMessage],
    metrics: Option<&SessionMetrics>,
) {
    println!("{}", format!("Agent: {}", agent_name).bold());
    println!("{}", muted("─────────────────────────────────────────────"));

    // Session
    if let Some(s) = session {
        println!("{}", "Session".bold());
        println!("  State:      {}", state_colored(&s.state.to_string()));
        println!("  Task:       {}", s.task_id);
        println!("  Capability: {}", s.capability);
        println!("  Branch:     {}", s.branch_name);
        println!("  Started:    {}", &s.started_at);
        if let Some(ref p) = s.parent_agent {
            println!("  Parent:     {}", p);
        }
    } else {
        println!("{}", muted("  No session record found"));
    }

    // Metrics
    if let Some(m) = metrics {
        println!();
        println!("{}", "Token Usage".bold());
        let total = m.input_tokens + m.output_tokens + m.cache_read_tokens;
        println!("  Input:      {}", m.input_tokens);
        println!("  Output:     {}", m.output_tokens);
        println!("  Cache read: {}", m.cache_read_tokens);
        println!("  Total:      {}", total);
        if let Some(cost) = m.estimated_cost_usd {
            println!("  Cost:       ${:.4}", cost);
        }
    }

    // Recent events
    println!();
    println!("{}", format!("Recent Events ({})", events.len()).bold());
    if events.is_empty() {
        println!("{}", muted("  No events"));
    } else {
        for ev in events.iter().take(10) {
            let ts = &ev.created_at;
            let short_ts = if ts.len() >= 19 { &ts[..19] } else { ts.as_str() };
            let tool = ev
                .tool_name
                .as_ref()
                .map(|t| format!(" [{}]", t))
                .unwrap_or_default();
            let et = serde_json::to_value(ev.event_type)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| format!("{:?}", ev.event_type));
            println!("  {} {}{}", muted(short_ts), et, tool);
        }
    }

    // Mail
    println!();
    println!("{}", format!("Mail — {} sent, {} received", sent.len(), received.len()).bold());
    for msg in sent.iter().take(5) {
        println!("  → {} | {}", msg.to.cyan(), msg.subject);
    }
    for msg in received.iter().take(5) {
        println!("  ← {} | {}", msg.from.cyan(), msg.subject);
    }
}

fn state_colored(state: &str) -> colored::ColoredString {
    match state {
        "working" => state.green(),
        "stalled" | "zombie" => state.yellow(),
        "completed" => state.dimmed(),
        _ => state.normal(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_inspect_no_overstory_dir() {
        // Should not panic even without .overstory/
        let result = execute("test-agent", false, Some(Path::new("/tmp")));
        let _ = result;
    }

    #[test]
    fn test_inspect_json_no_overstory_dir() {
        let result = execute("test-agent", true, Some(Path::new("/tmp")));
        let _ = result;
    }
}
