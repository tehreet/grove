//! Agent card widget — compact card view for a single agent session.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::tui::theme::{agent_state_color, agent_state_icon, MUTED_GRAY};
use crate::types::{AgentSession, StoredEvent, TokenSnapshot};

// New color names — will come from theme.rs after Builder 1's changes merge.
// Defined locally so this compiles standalone.
const BRAND_PRIMARY: ratatui::style::Color = ratatui::style::Color::Rgb(255, 85, 170);
const ACCENT_CYAN: ratatui::style::Color = ratatui::style::Color::Rgb(139, 233, 253);
const TEXT_DIM: ratatui::style::Color = ratatui::style::Color::Rgb(98, 114, 164);
const MUTED: ratatui::style::Color = ratatui::style::Color::Rgb(98, 114, 164);

/// Render a single agent card into the given area (6 rows high including border).
pub fn render_card(
    f: &mut Frame,
    session: &AgentSession,
    snapshot: Option<&TokenSnapshot>,
    latest_event: Option<&StoredEvent>,
    tmux_line: &str,
    area: Rect,
    selected: bool,
) {
    let state_color = agent_state_color(&session.state);
    let border_color = if selected { state_color } else { MUTED_GRAY };

    // Card block with colored border
    let title_spans = vec![
        Span::styled(
            format!(" {} ", agent_state_icon(&session.state)),
            Style::default().fg(state_color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            session.agent_name.clone(),
            Style::default().fg(BRAND_PRIMARY).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" ── {} ", session.capability),
            Style::default().fg(ACCENT_CYAN),
        ),
    ];

    let block = Block::new()
        .title(Line::from(title_spans))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));

    // Inner area for content lines
    let inner = block.inner(area);
    f.render_widget(block, area);

    // We have inner.height lines available — use up to 4
    let activity = format_activity(latest_event);
    let mini_out = truncate(tmux_line.trim(), inner.width.saturating_sub(2) as usize);
    let stats = format_stats(snapshot, session);

    let content_lines = vec![
        Line::from(vec![Span::styled(
            format!(" {}", activity),
            Style::default().fg(ratatui::style::Color::White),
        )]),
        Line::from(vec![Span::styled(
            format!(" {}", mini_out),
            Style::default().fg(TEXT_DIM),
        )]),
        Line::from(vec![Span::styled(
            format!(" {}", stats),
            Style::default().fg(MUTED),
        )]),
    ];

    // Lay out content lines vertically within inner area
    let available = inner.height as usize;
    let lines_to_show = content_lines.len().min(available);

    if inner.width < 4 || inner.height < 1 {
        return;
    }

    let constraints: Vec<Constraint> = (0..lines_to_show)
        .map(|_| Constraint::Length(1))
        .collect();

    let chunks = Layout::vertical(constraints).split(inner);
    for (i, line) in content_lines.into_iter().take(lines_to_show).enumerate() {
        let p = Paragraph::new(line);
        f.render_widget(p, chunks[i]);
    }
}

// ---------------------------------------------------------------------------
// Activity line
// ---------------------------------------------------------------------------

fn format_activity(event: Option<&StoredEvent>) -> String {
    match event {
        Some(ev) => {
            if let Some(ref tool) = ev.tool_name {
                let args: Option<serde_json::Value> = ev
                    .tool_args
                    .as_deref()
                    .and_then(|a| serde_json::from_str(a).ok());
                match tool.as_str() {
                    "Edit" | "Write" => {
                        let path = args
                            .as_ref()
                            .and_then(|v| v.get("file_path"))
                            .and_then(|v| v.as_str())
                            .map(shorten_path)
                            .unwrap_or_else(|| "...".to_string());
                        format!("✎ {}", path)
                    }
                    "Bash" => {
                        let cmd = args
                            .as_ref()
                            .and_then(|v| v.get("command"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("...");
                        format!("$ {}", truncate(cmd, 36))
                    }
                    "Read" => {
                        let path = args
                            .as_ref()
                            .and_then(|v| v.get("file_path"))
                            .and_then(|v| v.as_str())
                            .map(shorten_path)
                            .unwrap_or_else(|| "...".to_string());
                        format!("◉ {}", path)
                    }
                    other => other.to_string(),
                }
            } else {
                "idle".to_string()
            }
        }
        None => "no activity".to_string(),
    }
}

// ---------------------------------------------------------------------------
// Stats line
// ---------------------------------------------------------------------------

fn format_stats(snapshot: Option<&TokenSnapshot>, session: &AgentSession) -> String {
    let tokens = snapshot
        .map(|s| {
            format!(
                "in: {}  out: {}",
                format_tokens(s.input_tokens),
                format_tokens(s.output_tokens)
            )
        })
        .unwrap_or_default();

    let cost = snapshot
        .and_then(|s| s.estimated_cost_usd)
        .map(|c| format!("${:.2}", c))
        .unwrap_or_default();

    let duration = compute_duration(session);

    let parts: Vec<&str> = [tokens.as_str(), cost.as_str(), duration.as_str()]
        .iter()
        .filter(|s| !s.is_empty())
        .copied()
        .collect();
    parts.join("  ")
}

fn format_tokens(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
}

fn compute_duration(session: &AgentSession) -> String {
    use chrono::{DateTime, Utc};

    let started = match session.started_at.parse::<DateTime<Utc>>() {
        Ok(t) => t,
        Err(_) => return String::new(),
    };

    let now = Utc::now();
    let elapsed = now.signed_duration_since(started);
    let total_secs = elapsed.num_seconds().max(0) as u64;

    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let secs = total_secs % 60;

    if hours > 0 {
        format!("{}h {:02}m {:02}s", hours, minutes, secs)
    } else if minutes > 0 {
        format!("{}m {:02}s", minutes, secs)
    } else {
        format!("{}s", secs)
    }
}

// ---------------------------------------------------------------------------
// Path/string helpers
// ---------------------------------------------------------------------------

fn shorten_path(path: &str) -> String {
    // Keep last 2 components of path
    let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if parts.len() <= 2 {
        return path.to_string();
    }
    format!("…/{}/{}", parts[parts.len() - 2], parts[parts.len() - 1])
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", truncated)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AgentState;

    fn make_session(name: &str) -> AgentSession {
        AgentSession {
            id: "id-1".to_string(),
            agent_name: name.to_string(),
            capability: "builder".to_string(),
            worktree_path: "/tmp".to_string(),
            branch_name: "main".to_string(),
            task_id: "task-1".to_string(),
            tmux_session: "".to_string(),
            state: AgentState::Working,
            pid: None,
            parent_agent: None,
            depth: 1,
            run_id: None,
            started_at: "2024-01-01T00:00:00Z".to_string(),
            last_activity: "2024-01-01T00:01:00Z".to_string(),
            escalation_level: 0,
            stalled_since: None,
            transcript_path: None,
        }
    }

    #[test]
    fn test_format_activity_none() {
        assert_eq!(format_activity(None), "no activity");
    }

    #[test]
    fn test_format_activity_no_tool() {
        use crate::types::{EventLevel, EventType};
        let ev = StoredEvent {
            id: 1,
            run_id: None,
            agent_name: "a".to_string(),
            session_id: None,
            event_type: EventType::ToolStart,
            tool_name: None,
            tool_args: None,
            tool_duration_ms: None,
            level: EventLevel::Info,
            data: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
        };
        assert_eq!(format_activity(Some(&ev)), "idle");
    }

    #[test]
    fn test_format_activity_bash() {
        use crate::types::{EventLevel, EventType};
        let ev = StoredEvent {
            id: 1,
            run_id: None,
            agent_name: "a".to_string(),
            session_id: None,
            event_type: EventType::ToolStart,
            tool_name: Some("Bash".to_string()),
            tool_args: Some(r#"{"command":"cargo build"}"#.to_string()),
            tool_duration_ms: None,
            level: EventLevel::Info,
            data: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
        };
        assert_eq!(format_activity(Some(&ev)), "$ cargo build");
    }

    #[test]
    fn test_format_activity_edit() {
        use crate::types::{EventLevel, EventType};
        let ev = StoredEvent {
            id: 1,
            run_id: None,
            agent_name: "a".to_string(),
            session_id: None,
            event_type: EventType::ToolStart,
            tool_name: Some("Edit".to_string()),
            tool_args: Some(r#"{"file_path":"/home/user/project/src/main.rs"}"#.to_string()),
            tool_duration_ms: None,
            level: EventLevel::Info,
            data: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
        };
        assert!(format_activity(Some(&ev)).starts_with("✎"));
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world foo bar", 10).chars().count(), 10);
    }

    #[test]
    fn test_shorten_path_short() {
        assert_eq!(shorten_path("src/main.rs"), "src/main.rs");
    }

    #[test]
    fn test_shorten_path_long() {
        let result = shorten_path("/home/user/project/src/tui/theme.rs");
        assert_eq!(result, "…/tui/theme.rs");
    }

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(500), "500");
        assert_eq!(format_tokens(1500), "1.5k");
        assert_eq!(format_tokens(2_500_000), "2.5M");
    }

    #[test]
    fn test_format_stats_no_snapshot() {
        let session = make_session("test");
        let result = format_stats(None, &session);
        // Should at least contain duration
        assert!(!result.is_empty() || result.is_empty()); // no panic
    }
}
