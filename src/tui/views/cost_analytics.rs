//! Cost analytics view — per-agent costs, burn rate, projections.
#![allow(dead_code)]

use chrono::Utc;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Cell, Paragraph, Row, Table},
    Frame,
};

use crate::tui::app::App;
use crate::tui::theme::MUTED_GRAY;

// Dracula theme color constants
const BRAND_PRIMARY: Color = Color::Rgb(255, 85, 170);
const ACCENT_CYAN: Color = Color::Rgb(139, 233, 253);
const ACCENT_GREEN: Color = Color::Rgb(80, 250, 123);
const ACCENT_YELLOW: Color = Color::Rgb(241, 250, 140);
const ACCENT_ORANGE: Color = Color::Rgb(255, 184, 108);
const MUTED: Color = MUTED_GRAY;

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();

    let block = Block::new()
        .title(" COST ANALYTICS ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BRAND_PRIMARY));

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.snapshots.is_empty() {
        let empty = Paragraph::new("  No cost data available — waiting for agent metrics")
            .style(Style::default().fg(MUTED));
        f.render_widget(empty, inner);
        return;
    }

    // Split: top 45% for bars + burn rate, bottom fills rest for table
    let chunks = Layout::vertical([Constraint::Percentage(45), Constraint::Fill(1)]).split(inner);

    render_bars_and_rate(f, app, chunks[0]);
    render_cost_table(f, app, chunks[1]);
}

fn render_bars_and_rate(f: &mut Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    render_cost_bars(f, app, chunks[0]);
    render_burn_rate(f, app, chunks[1]);
}

fn render_cost_bars(f: &mut Frame, app: &App, area: Rect) {
    // Aggregate latest cost per agent (take max cost snapshot per agent)
    let mut agent_costs: Vec<(String, f64)> = {
        let mut map: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
        for snap in &app.snapshots {
            let cost = snap.estimated_cost_usd.unwrap_or(0.0);
            let entry = map.entry(snap.agent_name.clone()).or_insert(0.0);
            if cost > *entry {
                *entry = cost;
            }
        }
        let mut v: Vec<(String, f64)> = map.into_iter().collect();
        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        v
    };
    agent_costs.truncate(10); // max 10 agents shown

    let max_cost = agent_costs
        .first()
        .map(|(_, c)| *c)
        .unwrap_or(1.0)
        .max(0.001);
    let bar_max: usize = area.width.saturating_sub(24) as usize;

    let mut lines: Vec<Line> = vec![Line::from(Span::styled(
        " Per-Agent Costs",
        Style::default().fg(MUTED).add_modifier(Modifier::BOLD),
    ))];

    for (agent, cost) in &agent_costs {
        let bar_len = ((cost / max_cost) * bar_max as f64).round() as usize;
        let bar_len = bar_len.max(1);
        let bar = "█".repeat(bar_len);

        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(bar, Style::default().fg(ACCENT_GREEN)),
            Span::raw(" "),
            Span::styled(truncate(agent, 14), Style::default().fg(BRAND_PRIMARY)),
            Span::raw("  "),
            Span::styled(format!("${:.2}", cost), Style::default().fg(ACCENT_ORANGE)),
        ]));
    }

    let p = Paragraph::new(lines);
    f.render_widget(p, area);
}

fn render_burn_rate(f: &mut Frame, app: &App, area: Rect) {
    let total_cost = app.total_cost;

    // Find earliest session start
    let now = Utc::now();
    let earliest = app
        .sessions
        .iter()
        .filter_map(|s| {
            chrono::DateTime::parse_from_rfc3339(&s.started_at)
                .ok()
                .map(|dt| dt.with_timezone(&Utc))
        })
        .min();

    let elapsed_mins = earliest
        .map(|start| now.signed_duration_since(start).num_seconds().max(1) as f64 / 60.0)
        .unwrap_or(1.0);

    let burn_rate = if elapsed_mins > 0.0 {
        total_cost / elapsed_mins
    } else {
        0.0
    };
    let projected = burn_rate * 2.0 * elapsed_mins;

    let lines: Vec<Line> = vec![
        Line::from(Span::styled(
            " Burn Rate",
            Style::default().fg(MUTED).add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                format!("${:.4}/min", burn_rate),
                Style::default().fg(ACCENT_YELLOW),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Projected: ", Style::default().fg(MUTED)),
            Span::styled(
                format!("${:.2}", projected),
                Style::default().fg(ACCENT_YELLOW),
            ),
        ]),
        Line::from(Span::raw("")),
        Line::from(Span::styled(
            " Session Total",
            Style::default().fg(MUTED).add_modifier(Modifier::BOLD),
        )),
        Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("${:.4}", total_cost),
                Style::default()
                    .fg(ACCENT_ORANGE)
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    let p = Paragraph::new(lines);
    f.render_widget(p, area);
}

fn render_cost_table(f: &mut Frame, app: &App, area: Rect) {
    // Aggregate per-agent: pick snapshot with highest cost
    let mut by_agent: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (i, snap) in app.snapshots.iter().enumerate() {
        let cost = snap.estimated_cost_usd.unwrap_or(0.0);
        let entry = by_agent.entry(snap.agent_name.clone()).or_insert(i);
        if cost > app.snapshots[*entry].estimated_cost_usd.unwrap_or(0.0) {
            *entry = i;
        }
    }

    let mut rows_data: Vec<&crate::types::TokenSnapshot> =
        by_agent.values().map(|&i| &app.snapshots[i]).collect();
    rows_data.sort_by(|a, b| {
        b.estimated_cost_usd
            .unwrap_or(0.0)
            .partial_cmp(&a.estimated_cost_usd.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Find capability from sessions
    let cap_map: std::collections::HashMap<&str, &str> = app
        .sessions
        .iter()
        .map(|s| (s.agent_name.as_str(), s.capability.as_str()))
        .collect();

    let header_style = Style::default().fg(MUTED).add_modifier(Modifier::BOLD);
    let header = Row::new(vec![
        Cell::from("Agent").style(header_style),
        Cell::from("Cap").style(header_style),
        Cell::from("In").style(header_style),
        Cell::from("Out").style(header_style),
        Cell::from("Cache").style(header_style),
        Cell::from("$").style(header_style),
    ]);

    let rows: Vec<Row> = rows_data
        .iter()
        .map(|snap| {
            let cap = cap_map
                .get(snap.agent_name.as_str())
                .copied()
                .unwrap_or("-");
            Row::new(vec![
                Cell::from(truncate(&snap.agent_name, 16))
                    .style(Style::default().fg(BRAND_PRIMARY)),
                Cell::from(truncate(cap, 10)).style(Style::default().fg(ACCENT_CYAN)),
                Cell::from(format_tokens(snap.input_tokens))
                    .style(Style::default().fg(Color::White)),
                Cell::from(format_tokens(snap.output_tokens))
                    .style(Style::default().fg(Color::White)),
                Cell::from(format_tokens(
                    snap.cache_read_tokens + snap.cache_creation_tokens,
                ))
                .style(Style::default().fg(Color::White)),
                Cell::from(format!("${:.4}", snap.estimated_cost_usd.unwrap_or(0.0)))
                    .style(Style::default().fg(ACCENT_ORANGE)),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Min(16),
            Constraint::Min(10),
            Constraint::Min(8),
            Constraint::Min(8),
            Constraint::Min(8),
            Constraint::Min(8),
        ],
    )
    .header(header)
    .block(
        Block::new()
            .title(" SESSION COST TABLE ")
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(MUTED)),
    );

    f.render_widget(table, area);
}

fn format_tokens(n: i64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        format!("{n}")
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}
