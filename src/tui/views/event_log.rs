//! Event log view — full-screen scrollable event log.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem},
    Frame,
};

use crate::types::EventType;

use crate::tui::app::App;
use crate::tui::theme::{BORDER_FOCUSED, MUTED_GRAY, TEXT_PRIMARY};
use crate::tui::widgets::status_bar;

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();

    let layout = Layout::vertical([Constraint::Fill(1), Constraint::Length(1)]).split(area);

    render_event_list(f, app, layout[0]);
    status_bar::render(f, app, layout[1]);
}

fn render_event_list(f: &mut Frame, app: &mut App, area: Rect) {
    let inner_height = area.height.saturating_sub(2) as usize;
    let total = app.events.len();

    let scroll = app
        .event_log_scroll
        .min(total.saturating_sub(inner_height.max(1)));

    let start = scroll;
    let end = (start + inner_height).min(total);

    let visible = if total > 0 {
        &app.events[start..end]
    } else {
        &[][..]
    };

    let items: Vec<ListItem> = visible
        .iter()
        .map(|ev| {
            let time_str = ev.created_at.get(11..19).unwrap_or("??:??:??");
            let et_str = event_type_to_str(&ev.event_type);
            let tool = ev.tool_name.as_deref().unwrap_or(&et_str);
            let args = ev
                .tool_args
                .as_deref()
                .map(|a| format!(" {}", truncate(a, 50)))
                .unwrap_or_default();
            let dur = ev
                .tool_duration_ms
                .map(|ms| format!(" ({}ms)", ms))
                .unwrap_or_default();

            let agent_color = agent_color_for(&ev.agent_name);

            ListItem::new(Line::from(vec![
                Span::styled(format!(" {} ", time_str), Style::default().fg(MUTED_GRAY)),
                Span::styled(
                    format!("{:<18} ", truncate(&ev.agent_name, 18)),
                    Style::default()
                        .fg(agent_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:<10} ", truncate(tool, 10)),
                    Style::default().fg(TEXT_PRIMARY),
                ),
                Span::styled(format!("{}{}", args, dur), Style::default().fg(MUTED_GRAY)),
            ]))
        })
        .collect();

    let scroll_indicator = if total > 0 {
        format!(" [{}/{}]", scroll + 1, total)
    } else {
        String::new()
    };

    let block = Block::new()
        .title(format!(" EVENT LOG{} ", scroll_indicator))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_FOCUSED));

    let list = if items.is_empty() {
        List::new(vec![ListItem::new(Span::styled(
            "  no events yet",
            Style::default().fg(MUTED_GRAY),
        ))])
        .block(block)
    } else {
        List::new(items).block(block)
    };

    f.render_widget(list, area);
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}

fn event_type_to_str(et: &EventType) -> String {
    serde_json::to_string(et)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string()
}

fn agent_color_for(name: &str) -> Color {
    let palette = [
        Color::Rgb(100, 180, 255),
        Color::Rgb(180, 255, 100),
        Color::Rgb(255, 180, 100),
        Color::Rgb(180, 100, 255),
        Color::Rgb(100, 255, 180),
        Color::Rgb(255, 100, 180),
        Color::Rgb(255, 220, 100),
        Color::Rgb(100, 220, 255),
    ];
    let hash: usize = name
        .bytes()
        .fold(0usize, |acc, b| acc.wrapping_add(b as usize));
    palette[hash % palette.len()]
}
