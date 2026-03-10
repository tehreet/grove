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
    transcript_summary: Option<TranscriptSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptSummary {
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
    model: Option<String>,
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

    let transcript_summary = session
        .as_ref()
        .and_then(|s| s.transcript_path.as_deref())
        .and_then(parse_transcript_summary);

    if json {
        let out = InspectOutput {
            agent_name: agent_name.to_string(),
            session,
            recent_events,
            sent_mail,
            received_mail,
            metrics,
            transcript_summary,
        };
        println!("{}", json_output("inspect", &out));
    } else {
        print_inspect(
            agent_name,
            session.as_ref(),
            &recent_events,
            &sent_mail,
            &received_mail,
            metrics.as_ref(),
            transcript_summary.as_ref(),
        );
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
    transcript_summary: Option<&TranscriptSummary>,
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

    if let Some(summary) = transcript_summary {
        println!();
        println!("{}", "Transcript Usage".bold());
        println!("  Input:       {}", summary.input_tokens);
        println!("  Output:      {}", summary.output_tokens);
        if summary.cache_read_tokens > 0 {
            println!("  Cache read:  {}", summary.cache_read_tokens);
        }
        if summary.cache_write_tokens > 0 {
            println!("  Cache write: {}", summary.cache_write_tokens);
        }
        if let Some(model) = &summary.model {
            println!("  Model:       {model}");
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
            let short_ts = if ts.len() >= 19 {
                &ts[..19]
            } else {
                ts.as_str()
            };
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
    println!(
        "{}",
        format!("Mail — {} sent, {} received", sent.len(), received.len()).bold()
    );
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

fn parse_transcript_summary(path: &str) -> Option<TranscriptSummary> {
    let content = std::fs::read_to_string(path).ok()?;
    let mut summary = TranscriptSummary {
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        model: None,
    };
    let mut saw_usage = false;

    for line in content.lines().filter(|line| !line.trim().is_empty()) {
        let value: serde_json::Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => continue,
        };

        if let Some(usage) = value.get("usage") {
            saw_usage |= add_usage(&mut summary, usage);
        }

        if let Some(message) = value.get("message") {
            if let Some(usage) = message.get("usage") {
                saw_usage |= add_usage(&mut summary, usage);
            }
            if summary.model.is_none() {
                summary.model = string_at(message, &["model", "model_name"]);
            }
        }

        if let Some(meta) = value.get("metadata").or_else(|| value.get("meta")) {
            saw_usage |= add_gemini_usage(&mut summary, meta);
            if summary.model.is_none() {
                summary.model = string_at(meta, &["model", "model_name"]);
            }
        }

        if summary.model.is_none() {
            summary.model = string_at(&value, &["model", "model_name"]);
        }
    }

    saw_usage.then_some(summary)
}

fn add_usage(summary: &mut TranscriptSummary, usage: &serde_json::Value) -> bool {
    let input = u64_at(usage, &["input_tokens", "prompt_tokens"]);
    let output = u64_at(usage, &["output_tokens", "completion_tokens"]);
    let cache_read = u64_at(usage, &["cache_read_input_tokens", "cache_read_tokens"]);
    let cache_write = u64_at(
        usage,
        &[
            "cache_creation_input_tokens",
            "cache_write_tokens",
            "cache_creation_tokens",
        ],
    );

    summary.input_tokens += input;
    summary.output_tokens += output;
    summary.cache_read_tokens += cache_read;
    summary.cache_write_tokens += cache_write;

    input > 0 || output > 0 || cache_read > 0 || cache_write > 0
}

fn add_gemini_usage(summary: &mut TranscriptSummary, metadata: &serde_json::Value) -> bool {
    let input = u64_at(metadata, &["promptTokenCount"]);
    let output = u64_at(metadata, &["candidatesTokenCount"]);
    let cache_read = u64_at(metadata, &["cachedContentTokenCount"]);

    summary.input_tokens += input;
    summary.output_tokens += output;
    summary.cache_read_tokens += cache_read;

    input > 0 || output > 0 || cache_read > 0
}

fn u64_at(value: &serde_json::Value, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(|v| v.as_u64()))
        .unwrap_or(0)
}

fn string_at(value: &serde_json::Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(|v| v.as_str()))
        .map(ToString::to_string)
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

    #[test]
    fn test_parse_transcript_summary_claude_usage() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.ndjson");
        std::fs::write(
            &path,
            "{\"type\":\"result\",\"usage\":{\"input_tokens\":100,\"output_tokens\":40,\"cache_read_input_tokens\":8,\"cache_creation_input_tokens\":3},\"model\":\"claude-3-7-sonnet\"}\n",
        )
        .unwrap();

        let summary = parse_transcript_summary(path.to_str().unwrap()).unwrap();
        assert_eq!(summary.input_tokens, 100);
        assert_eq!(summary.output_tokens, 40);
        assert_eq!(summary.cache_read_tokens, 8);
        assert_eq!(summary.cache_write_tokens, 3);
        assert_eq!(summary.model.as_deref(), Some("claude-3-7-sonnet"));
    }

    #[test]
    fn test_parse_transcript_summary_gemini_usage() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.ndjson");
        std::fs::write(
            &path,
            "{\"metadata\":{\"promptTokenCount\":12,\"candidatesTokenCount\":5,\"cachedContentTokenCount\":2,\"model_name\":\"gemini-2.5-pro\"}}\n",
        )
        .unwrap();

        let summary = parse_transcript_summary(path.to_str().unwrap()).unwrap();
        assert_eq!(summary.input_tokens, 12);
        assert_eq!(summary.output_tokens, 5);
        assert_eq!(summary.cache_read_tokens, 2);
        assert_eq!(summary.model.as_deref(), Some("gemini-2.5-pro"));
    }
}
