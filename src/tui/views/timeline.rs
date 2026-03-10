//! Timeline / Gantt view — horizontal bars per agent.
#![allow(dead_code)]

use chrono::{DateTime, Utc};
use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::tui::app::App;
use crate::tui::theme::{agent_state_color, MUTED_GRAY};
use crate::types::{AgentSession, AgentState};

// Dracula theme color constants
const BRAND_PRIMARY: Color = Color::Rgb(255, 85, 170);
const ACCENT_GREEN: Color = Color::Rgb(80, 250, 123);
const ACCENT_YELLOW: Color = Color::Rgb(241, 250, 140);
const ACCENT_RED: Color = Color::Rgb(255, 85, 85);
const ACCENT_PURPLE: Color = Color::Rgb(189, 147, 249);
const MUTED: Color = MUTED_GRAY;

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();

    let block = Block::new()
        .title(" TIMELINE ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BRAND_PRIMARY));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.sessions.is_empty() {
        let empty = Paragraph::new("  No sessions — start a run to see the timeline")
            .style(Style::default().fg(MUTED));
        f.render_widget(empty, inner);
        return;
    }

    let now = Utc::now();
    let run_start = app
        .sessions
        .iter()
        .filter_map(|s| {
            DateTime::parse_from_rfc3339(&s.started_at)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        })
        .min()
        .unwrap_or(now);

    let total_secs = now
        .signed_duration_since(run_start)
        .num_seconds()
        .max(60) as f64;

    let name_width: usize = 14;
    let bar_width = (inner.width as usize).saturating_sub(name_width + 4);

    let mut lines: Vec<Line> = vec![];

    lines.push(render_time_axis(total_secs, name_width, bar_width));
    lines.push(Line::from(""));

    for session in &app.sessions.clone() {
        lines.push(render_agent_row(
            session, run_start, now, name_width, bar_width,
        ));
    }

    lines.push(Line::from(""));
    lines.push(render_legend());

    let p = Paragraph::new(lines);
    f.render_widget(p, inner);
}

fn render_time_axis(total_secs: f64, name_width: usize, bar_width: usize) -> Line<'static> {
    // Choose interval based on total duration
    let total_mins = total_secs / 60.0;
    let interval_mins: f64 = if total_mins < 10.0 {
        2.0
    } else if total_mins < 30.0 {
        5.0
    } else if total_mins < 120.0 {
        15.0
    } else {
        30.0
    };

    let interval_secs = interval_mins * 60.0;

    // Build a label line: "  <name_width spaces>  <axis labels>"
    let prefix = " ".repeat(name_width + 2);
    let mut axis = vec![' '; bar_width];

    let mut t = 0.0f64;
    while t <= total_secs {
        let col = ((t / total_secs) * bar_width as f64) as usize;
        let label = if t < 60.0 {
            "0m".to_string()
        } else {
            format!("{}m", (t / 60.0).round() as u64)
        };
        for (i, ch) in label.chars().enumerate() {
            if col + i < bar_width {
                axis[col + i] = ch;
            }
        }
        t += interval_secs;
    }

    let axis_str: String = axis.into_iter().collect();
    let full = format!("{prefix}{axis_str}");

    Line::from(Span::styled(full, Style::default().fg(MUTED)))
}

fn render_agent_row(
    session: &AgentSession,
    run_start: DateTime<Utc>,
    now: DateTime<Utc>,
    name_width: usize,
    bar_width: usize,
) -> Line<'static> {
    let total_secs = now
        .signed_duration_since(run_start)
        .num_seconds()
        .max(1) as f64;

    let agent_start = DateTime::parse_from_rfc3339(&session.started_at)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or(run_start);

    let agent_end = match session.state {
        AgentState::Completed | AgentState::Zombie => {
            DateTime::parse_from_rfc3339(&session.last_activity)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or(now)
        }
        _ => now,
    };

    let start_col = ((agent_start
        .signed_duration_since(run_start)
        .num_seconds()
        .max(0) as f64
        / total_secs)
        * bar_width as f64) as usize;

    let end_col = ((agent_end
        .signed_duration_since(run_start)
        .num_seconds()
        .max(0) as f64
        / total_secs)
        * bar_width as f64) as usize;

    let end_col = end_col.min(bar_width);

    let state_color = agent_state_color(&session.state);
    let icon = state_icon(&session.state);

    let name = format!(
        "{:width$}",
        truncate(&session.agent_name, name_width),
        width = name_width
    );
    let gap = " ".repeat(start_col);
    let bar = "█".repeat(end_col.saturating_sub(start_col).max(1));
    let trail = " ".repeat(bar_width.saturating_sub(end_col));

    Line::from(vec![
        Span::styled(name, Style::default().fg(BRAND_PRIMARY)),
        Span::raw("  "),
        Span::raw(gap),
        Span::styled(bar, Style::default().fg(state_color)),
        Span::raw(trail),
        Span::styled(
            format!(" {icon}"),
            Style::default().fg(state_color),
        ),
    ])
}

fn render_legend() -> Line<'static> {
    Line::from(vec![
        Span::styled("  ▶ ", Style::default().fg(ACCENT_GREEN)),
        Span::styled("Working  ", Style::default().fg(MUTED)),
        Span::styled("◌ ", Style::default().fg(ACCENT_YELLOW)),
        Span::styled("Booting  ", Style::default().fg(MUTED)),
        Span::styled("⚠ ", Style::default().fg(ACCENT_RED)),
        Span::styled("Stalled  ", Style::default().fg(MUTED)),
        Span::styled("☠ ", Style::default().fg(ACCENT_RED)),
        Span::styled("Zombie  ", Style::default().fg(MUTED)),
        Span::styled("✔ ", Style::default().fg(ACCENT_PURPLE)),
        Span::styled("Done", Style::default().fg(MUTED)),
    ])
}

/// State-specific icons matching the spec's legend.
fn state_icon(state: &AgentState) -> &'static str {
    match state {
        AgentState::Working => "▶",
        AgentState::Booting => "◌",
        AgentState::Stalled => "⚠",
        AgentState::Zombie => "☠",
        AgentState::Completed => "✔",
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}
