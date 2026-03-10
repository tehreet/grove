//! Overview view — main dashboard layout.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::app::App;
use crate::tui::theme::MUTED_GRAY;
use crate::tui::widgets::{agent_table, feed, header, mail_list, status_bar};

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();

    // Outer layout: header | body | merge bar | key hints
    let outer = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Fill(1),   // main body
        Constraint::Length(1), // merge/metrics bar
        Constraint::Length(1), // key hints
    ])
    .split(area);

    header::render(f, app, outer[0]);
    render_body(f, app, outer[1]);
    render_merge_bar(f, app, outer[2]);
    status_bar::render(f, app, outer[3]);
}

fn render_body(f: &mut Frame, app: &mut App, area: Rect) {
    // Body: agent table (top, ~40%) | [feed | mail] (bottom, ~60%)
    let agent_height = (area.height * 40 / 100).max(5);
    let bottom_height = area.height.saturating_sub(agent_height);

    let body = Layout::vertical([
        Constraint::Length(agent_height),
        Constraint::Length(bottom_height),
    ])
    .split(area);

    agent_table::render(f, app, body[0]);
    render_feed_mail(f, app, body[1]);
}

fn render_feed_mail(f: &mut Frame, app: &mut App, area: Rect) {
    // [Feed (60%) | Mail (40%)]
    let panels = Layout::horizontal([
        Constraint::Percentage(60),
        Constraint::Percentage(40),
    ])
    .split(area);

    feed::render(f, app, panels[0]);
    mail_list::render(f, app, panels[1]);
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
        Style::default().fg(MUTED_GRAY),
    )]));

    f.render_widget(bar, area);
}
