//! Overview view — main dashboard layout with agent cards + mini-terminal.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::app::App;
use crate::tui::theme::{agent_state_icon, unfocused_block, MUTED_GRAY};
use crate::tui::widgets::{agent_card, feed, header, mail_list, status_bar};
use crate::types::AgentState;

// New color names — will come from theme.rs after Builder 1's changes merge.
// Defined locally so this compiles standalone.
const MUTED: ratatui::style::Color = ratatui::style::Color::Rgb(98, 114, 164);

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();

    let outer = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Fill(1),   // main body
        Constraint::Length(1), // merge bar
        Constraint::Length(1), // status bar
    ])
    .split(area);

    header::render(f, app, outer[0]);
    render_body(f, app, outer[1]);
    render_merge_bar(f, app, outer[2]);
    status_bar::render(f, app, outer[3]);
}

fn render_body(f: &mut Frame, app: &mut App, area: Rect) {
    let body = Layout::vertical([
        Constraint::Percentage(45), // cards
        Constraint::Fill(1),        // feed + mail
        Constraint::Length(7),      // mini-terminal
    ])
    .split(area);

    render_cards(f, app, body[0]);
    render_feed_mail(f, app, body[1]);
    render_mini_terminal(f, app, body[2]);
}

fn render_cards(f: &mut Frame, app: &mut App, area: Rect) {
    let visible = app.visible_sessions();
    let card_height: u16 = 6;
    let cols: u16 = if area.width >= 120 { 2 } else { 1 };
    let card_width = area.width / cols;

    let mut row: u16 = 0;
    let mut col: u16 = 0;
    let selected_idx = app.table_state.selected().unwrap_or(0);

    for (i, session) in visible.iter().enumerate() {
        if area.y + row + card_height > area.y + area.height {
            break;
        }

        let card_area = Rect::new(
            area.x + col * card_width,
            area.y + row,
            card_width,
            card_height,
        );

        let snapshot = app.snapshot_for(&session.agent_name);
        let latest_event = app
            .events
            .iter()
            .rev()
            .find(|e| e.agent_name == session.agent_name);
        let tmux_lines = crate::tui::app::capture_agent_output(&session.tmux_session, &session.agent_name, ".");
        let last_line = tmux_lines.last().map(|s| s.as_str()).unwrap_or("");

        agent_card::render_card(
            f,
            session,
            snapshot,
            latest_event,
            last_line,
            card_area,
            i == selected_idx,
        );

        col += 1;
        if col >= cols {
            col = 0;
            row += card_height;
        }
    }
}

fn render_feed_mail(f: &mut Frame, app: &mut App, area: Rect) {
    let panels = Layout::horizontal([
        Constraint::Percentage(60),
        Constraint::Percentage(40),
    ])
    .split(area);

    feed::render(f, app, panels[0]);
    mail_list::render(f, app, panels[1]);
}

fn render_mini_terminal(f: &mut Frame, app: &App, area: Rect) {
    let active: Vec<&crate::types::AgentSession> = app
        .sessions
        .iter()
        .filter(|s| s.state == AgentState::Working || s.state == AgentState::Booting)
        .collect();

    if active.is_empty() {
        let block = unfocused_block("TERMINAL");
        let p = Paragraph::new("  no active agents")
            .style(Style::default().fg(MUTED_GRAY))
            .block(block);
        f.render_widget(p, area);
        return;
    }

    // Rotate every 5 seconds
    let idx = (app.tick_count / 5) as usize % active.len();
    let agent = active[idx];

    let title = format!(
        "TERMINAL — {} {}",
        agent_state_icon(&agent.state),
        agent.agent_name
    );
    let block = unfocused_block(&title);

    let lines = crate::tui::app::capture_agent_output(&agent.tmux_session, &agent.agent_name, ".");
    let display_lines: Vec<Line> = lines
        .iter()
        .rev()
        .take(5)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(|l| {
            Line::styled(
                l.clone(),
                Style::default().fg(ratatui::style::Color::Rgb(98, 114, 164)),
            )
        })
        .collect();

    let p = Paragraph::new(display_lines).block(block);
    f.render_widget(p, area);
}

fn render_merge_bar(f: &mut Frame, app: &App, area: Rect) {
    let pending = app.merge_entries.len();
    let sessions = app.metric_session_count;
    let cost_str = app.total_cost_display();

    let cost_part = if cost_str.is_empty() {
        String::new()
    } else {
        format!(", {} total", cost_str)
    };

    let text = format!(
        " MERGE: {} pending │ METRICS: {} sessions{}",
        pending, sessions, cost_part
    );

    let bar = Paragraph::new(Line::from(vec![Span::styled(
        text,
        Style::default().fg(MUTED),
    )]));

    f.render_widget(bar, area);
}
