//! Agent table widget — styled scrollable table of agent sessions.

use chrono::{DateTime, Utc};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Cell, Row, Table},
    Frame,
};

use crate::tui::app::App;
use crate::tui::theme::{
    agent_state_color, agent_state_icon, focused_block, unfocused_block, ACCENT_CYAN, MUTED_GRAY,
    TEXT_PRIMARY,
};
use crate::tui::app::Focus;
use ratatui::layout::Constraint;

pub fn render(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Agents;
    let visible = app.visible_sessions();
    let session_count = visible.len();

    let title = if app.show_completed {
        format!("AGENTS ({}) [all]", session_count)
    } else if !app.filter_text.is_empty() {
        format!("AGENTS ({}) [filter: {}]", session_count, app.filter_text)
    } else {
        format!("AGENTS ({})", session_count)
    };

    let block = if focused {
        focused_block(&title)
    } else {
        unfocused_block(&title)
    };

    let header_cells = ["St", "Name", "Capability", "State", "Task", "Duration", "$"]
        .iter()
        .map(|h| {
            Cell::from(*h).style(
                Style::default()
                    .fg(MUTED_GRAY)
                    .add_modifier(Modifier::BOLD),
            )
        });
    let header_row = Row::new(header_cells).height(1);

    let rows: Vec<Row> = visible
        .iter()
        .map(|session| {
            let state_color = agent_state_color(&session.state);
            let icon = agent_state_icon(&session.state);
            let duration = compute_duration(session);
            let cost = app
                .snapshot_for(&session.agent_name)
                .and_then(|s| s.estimated_cost_usd)
                .map(|c| format!("${:.2}", c))
                .unwrap_or_default();

            let state_str = format!("{:?}", session.state).to_lowercase();

            Row::new(vec![
                Cell::from(icon).style(Style::default().fg(state_color)),
                Cell::from(truncate(&session.agent_name, 22))
                    .style(Style::default().fg(state_color)),
                Cell::from(truncate(&session.capability, 10))
                    .style(Style::default().fg(ACCENT_CYAN)),
                Cell::from(state_str).style(Style::default().fg(state_color)),
                Cell::from(truncate(&session.task_id, 16))
                    .style(Style::default().fg(MUTED_GRAY)),
                Cell::from(duration).style(Style::default().fg(MUTED_GRAY)),
                Cell::from(cost).style(Style::default().fg(MUTED_GRAY)),
            ])
            .height(1)
        })
        .collect();

    let widths = [
        Constraint::Length(3),
        Constraint::Length(22),
        Constraint::Length(10),
        Constraint::Length(10),
        Constraint::Length(16),
        Constraint::Length(10),
        Constraint::Length(8),
    ];

    let highlight_style = if focused {
        Style::default()
            .bg(ratatui::style::Color::Rgb(68, 71, 90))
            .fg(TEXT_PRIMARY)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(ratatui::style::Color::Rgb(68, 71, 90)).fg(TEXT_PRIMARY)
    };

    let table = Table::new(rows, widths)
        .header(header_row)
        .block(block)
        .row_highlight_style(highlight_style);

    f.render_stateful_widget(table, area, &mut app.table_state);
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}

fn compute_duration(session: &crate::types::AgentSession) -> String {
    use crate::types::AgentState;
    let start = DateTime::parse_from_rfc3339(&session.started_at)
        .map(|dt| dt.with_timezone(&Utc));
    let end = match session.state {
        AgentState::Completed | AgentState::Zombie => {
            DateTime::parse_from_rfc3339(&session.last_activity)
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
        }
        _ => Some(Utc::now()),
    };

    match (start, end) {
        (Ok(s), Some(e)) => {
            let secs = e.signed_duration_since(s).num_seconds().max(0) as u64;
            format_duration(secs)
        }
        _ => "?".to_string(),
    }
}

fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {:02}s", secs / 60, secs % 60)
    } else {
        format!("{}h {:02}m", secs / 3600, (secs % 3600) / 60)
    }
}
