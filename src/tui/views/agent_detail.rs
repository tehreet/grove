//! Agent detail view — drill-in for a single agent.

use chrono::{DateTime, Utc};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph},
    Frame,
};

use crate::tui::app::App;
use crate::tui::theme::{
    agent_state_color, agent_state_icon, ACCENT_ORANGE, BORDER_UNFOCUSED, BRAND_PRIMARY, MUTED_GRAY,
};
use crate::tui::widgets::status_bar;
use crate::types::AgentSession;

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();

    let layout = Layout::vertical([
        Constraint::Length(1), // back header
        Constraint::Fill(1),   // content
        Constraint::Length(1), // status bar
    ])
    .split(area);

    render_back_header(f, app, layout[0]);
    render_content(f, app, layout[1]);
    status_bar::render(f, app, layout[2]);
}

fn render_back_header(f: &mut Frame, app: &App, area: Rect) {
    let session = match &app.selected_agent {
        Some(s) => s,
        None => return,
    };

    let state_color = agent_state_color(&session.state);
    let icon = agent_state_icon(&session.state);

    let line = Line::from(vec![
        Span::styled(" ← Agent: ", Style::default().fg(MUTED_GRAY)),
        Span::styled(
            session.agent_name.clone(),
            Style::default()
                .fg(ACCENT_ORANGE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" (", Style::default().fg(MUTED_GRAY)),
        Span::styled(&session.capability, Style::default().fg(MUTED_GRAY)),
        Span::styled(") ", Style::default().fg(MUTED_GRAY)),
        Span::styled(icon, Style::default().fg(state_color)),
        Span::styled(
            format!(" {}", format!("{:?}", session.state).to_lowercase()),
            Style::default().fg(state_color),
        ),
    ]);

    f.render_widget(Paragraph::new(line), area);
}

fn render_content(f: &mut Frame, app: &mut App, area: Rect) {
    let session = match app.selected_agent.clone() {
        Some(s) => s,
        None => return,
    };

    // Top: session info | token info
    // Bottom: events | mail
    let top_bottom = Layout::vertical([Constraint::Length(8), Constraint::Fill(1)]).split(area);

    render_top_panels(f, app, top_bottom[0], &session);
    render_bottom_panels(f, app, top_bottom[1], &session.agent_name);
}

fn render_top_panels(f: &mut Frame, app: &App, area: Rect, session: &AgentSession) {
    let panels =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);

    // Session panel
    let started_ago = time_ago(&session.started_at);
    let session_lines = vec![
        Line::from(vec![
            Span::styled("  Task:     ", Style::default().fg(MUTED_GRAY)),
            Span::styled(&session.task_id, Style::default().fg(ACCENT_ORANGE)),
        ]),
        Line::from(vec![
            Span::styled("  Branch:   ", Style::default().fg(MUTED_GRAY)),
            Span::styled(truncate(&session.branch_name, 30), Style::default()),
        ]),
        Line::from(vec![
            Span::styled("  Worktree: ", Style::default().fg(MUTED_GRAY)),
            Span::styled(truncate(&session.worktree_path, 30), Style::default()),
        ]),
        Line::from(vec![
            Span::styled("  State:    ", Style::default().fg(MUTED_GRAY)),
            Span::styled(
                format!("{:?}", session.state).to_lowercase(),
                Style::default().fg(agent_state_color(&session.state)),
            ),
        ]),
        Line::from(vec![
            Span::styled("  PID:      ", Style::default().fg(MUTED_GRAY)),
            Span::styled(
                session
                    .pid
                    .map(|p| p.to_string())
                    .unwrap_or_else(|| "n/a".to_string()),
                Style::default(),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Started:  ", Style::default().fg(MUTED_GRAY)),
            Span::styled(started_ago, Style::default()),
        ]),
    ];

    let session_block = Block::new()
        .title(" SESSION ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_UNFOCUSED));

    f.render_widget(
        Paragraph::new(session_lines).block(session_block),
        panels[0],
    );

    // Token panel
    let snap = app.snapshot_for(&session.agent_name);
    let token_lines = vec![
        Line::from(vec![
            Span::styled("  Input:    ", Style::default().fg(MUTED_GRAY)),
            Span::styled(
                snap.map(|s| format_number(s.input_tokens))
                    .unwrap_or_else(|| "—".to_string()),
                Style::default(),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Output:   ", Style::default().fg(MUTED_GRAY)),
            Span::styled(
                snap.map(|s| format_number(s.output_tokens))
                    .unwrap_or_else(|| "—".to_string()),
                Style::default(),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Cache:    ", Style::default().fg(MUTED_GRAY)),
            Span::styled(
                snap.map(|s| format_number(s.cache_read_tokens))
                    .unwrap_or_else(|| "—".to_string()),
                Style::default(),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Cost:     ", Style::default().fg(MUTED_GRAY)),
            Span::styled(
                snap.and_then(|s| s.estimated_cost_usd)
                    .map(|c| format!("${:.4}", c))
                    .unwrap_or_else(|| "—".to_string()),
                Style::default().fg(ACCENT_ORANGE),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Model:    ", Style::default().fg(MUTED_GRAY)),
            Span::styled(
                snap.and_then(|s| s.model_used.as_deref())
                    .unwrap_or("—")
                    .to_string(),
                Style::default(),
            ),
        ]),
    ];

    let token_block = Block::new()
        .title(" TOKENS ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_UNFOCUSED));

    f.render_widget(Paragraph::new(token_lines).block(token_block), panels[1]);
}

fn render_bottom_panels(f: &mut Frame, app: &mut App, area: Rect, _agent_name: &str) {
    let panels =
        Layout::vertical([Constraint::Percentage(60), Constraint::Percentage(40)]).split(area);

    // Recent events
    let events = &app.agent_detail_events;
    let event_items: Vec<ListItem> = events
        .iter()
        .rev()
        .take(area.height as usize)
        .map(|ev| {
            let time_str = ev.created_at.get(11..19).unwrap_or("??:??:??");
            let et_str = serde_json::to_string(&ev.event_type)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string();
            let tool_label = ev.tool_name.as_deref().unwrap_or(&et_str);
            let dur = ev
                .tool_duration_ms
                .map(|ms| format!(" ({}ms)", ms))
                .unwrap_or_default();
            let args = ev
                .tool_args
                .as_deref()
                .map(|a| format!(" {}", truncate(a, 40)))
                .unwrap_or_default();
            ListItem::new(Line::from(vec![
                Span::styled(format!("{} ", time_str), Style::default().fg(MUTED_GRAY)),
                Span::styled(format!("{}{}{}", tool_label, args, dur), Style::default()),
            ]))
        })
        .collect();

    let events_block = Block::new()
        .title(" RECENT EVENTS ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_UNFOCUSED));

    let events_list = if event_items.is_empty() {
        List::new(vec![ListItem::new(Span::styled(
            "  no events",
            Style::default().fg(MUTED_GRAY),
        ))])
        .block(events_block)
    } else {
        List::new(event_items).block(events_block)
    };

    f.render_widget(events_list, panels[0]);

    // Mail
    let mail_items: Vec<ListItem> = app
        .agent_detail_mail
        .iter()
        .take(area.height as usize)
        .map(|msg| {
            let dot = if !msg.read {
                Span::styled("● ", Style::default().fg(ACCENT_ORANGE))
            } else {
                Span::styled("  ", Style::default())
            };
            ListItem::new(Line::from(vec![
                dot,
                Span::styled(
                    format!("{} → {}: ", truncate(&msg.from, 12), truncate(&msg.to, 12)),
                    Style::default().fg(BRAND_PRIMARY),
                ),
                Span::styled(truncate(&msg.subject, 40), Style::default()),
            ]))
        })
        .collect();

    let mail_block = Block::new()
        .title(" MAIL (sent/received) ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_UNFOCUSED));

    let mail_list = if mail_items.is_empty() {
        List::new(vec![ListItem::new(Span::styled(
            "  no mail",
            Style::default().fg(MUTED_GRAY),
        ))])
        .block(mail_block)
    } else {
        List::new(mail_items).block(mail_block)
    };

    f.render_widget(mail_list, panels[1]);
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}

fn time_ago(ts: &str) -> String {
    let parsed = DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.with_timezone(&Utc));
    let now = Utc::now();
    match parsed {
        Some(dt) => {
            let secs = now.signed_duration_since(dt).num_seconds().max(0) as u64;
            if secs < 60 {
                format!("{}s ago", secs)
            } else if secs < 3600 {
                format!("{}m {}s ago", secs / 60, secs % 60)
            } else {
                format!("{}h {}m ago", secs / 3600, (secs % 3600) / 60)
            }
        }
        None => ts.get(..16).unwrap_or(ts).to_string(),
    }
}

fn format_number(n: i64) -> String {
    // Format with thousand separators
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}
