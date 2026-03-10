//! Header bar widget — full-width, shows run ID, agent count, cost, time.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::app::App;
use crate::tui::theme::{ACCENT_AMBER, BRAND_GREEN, HEADER_BG, MUTED_GRAY};

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let now = chrono::Local::now();
    let time_str = now.format("%H:%M:%S").to_string();

    let active_count = app.active_agent_count();
    let total_count = app.sessions.len();
    let cost = app.total_cost_display();

    let run_display = match &app.run_id {
        Some(id) => {
            let short = if id.len() > 28 {
                format!("{}…", &id[..27])
            } else {
                id.clone()
            };
            short
        }
        None => "no active run".to_string(),
    };

    let separator = Span::styled(" │ ", Style::default().fg(MUTED_GRAY));

    let mut spans = vec![
        Span::styled(
            " grove dashboard",
            Style::default()
                .fg(BRAND_GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        separator.clone(),
        Span::styled("run: ", Style::default().fg(MUTED_GRAY)),
        Span::styled(run_display, Style::default().fg(ACCENT_AMBER)),
        separator.clone(),
        Span::styled(
            format!("{} agents", active_count),
            Style::default().fg(BRAND_GREEN),
        ),
    ];

    if total_count > active_count {
        spans.push(Span::styled(
            format!("/{}", total_count),
            Style::default().fg(MUTED_GRAY),
        ));
    }

    if !cost.is_empty() {
        spans.push(separator.clone());
        spans.push(Span::styled(cost, Style::default().fg(ACCENT_AMBER)));
    }

    spans.push(separator.clone());
    spans.push(Span::styled(
        time_str,
        Style::default().fg(MUTED_GRAY),
    ));

    if app.filter_mode {
        spans.push(separator.clone());
        spans.push(Span::styled(
            format!("filter: {}_", app.filter_text),
            Style::default().fg(ACCENT_AMBER).add_modifier(Modifier::BOLD),
        ));
    } else if !app.filter_text.is_empty() {
        spans.push(separator.clone());
        spans.push(Span::styled(
            format!("/{}", app.filter_text),
            Style::default().fg(ACCENT_AMBER),
        ));
    }

    let header = Paragraph::new(Line::from(spans))
        .style(Style::default().bg(HEADER_BG));

    f.render_widget(header, area);
}
