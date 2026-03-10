//! Single-agent terminal viewer — shows live tmux output for a selected agent.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::tui::app::App;
use crate::tui::theme::{
    agent_state_color, agent_state_icon, ACCENT_ORANGE, BORDER_FOCUSED, BRAND_PRIMARY, MUTED_GRAY,
};
use crate::tui::widgets::status_bar;

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();

    if app.terminal_fullscreen {
        render_content(f, app, area);
        return;
    }

    let layout = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Fill(1),   // content
        Constraint::Length(1), // status bar
    ])
    .split(area);

    render_header(f, app, layout[0]);
    render_content(f, app, layout[1]);
    status_bar::render(f, app, layout[2]);
}

fn render_header(f: &mut Frame, app: &App, area: Rect) {
    let agent = match &app.selected_agent {
        Some(a) => a,
        None => return,
    };

    let state_color = agent_state_color(&agent.state);
    let icon = agent_state_icon(&agent.state);

    let line = Line::from(vec![
        Span::styled(" TERMINAL: ", Style::default().fg(MUTED_GRAY)),
        Span::styled(
            agent.agent_name.clone(),
            Style::default().fg(ACCENT_ORANGE).add_modifier(Modifier::BOLD),
        ),
        Span::styled("  ", Style::default()),
        Span::styled(icon, Style::default().fg(state_color)),
        Span::styled(
            format!(" {}  ", format!("{:?}", agent.state).to_lowercase()),
            Style::default().fg(state_color),
        ),
        Span::styled("[esc]", Style::default().fg(BRAND_PRIMARY).add_modifier(Modifier::BOLD)),
        Span::styled(" back  ", Style::default().fg(MUTED_GRAY)),
        Span::styled("[f]", Style::default().fg(BRAND_PRIMARY).add_modifier(Modifier::BOLD)),
        Span::styled(" fullscreen  ", Style::default().fg(MUTED_GRAY)),
        Span::styled("[s]", Style::default().fg(BRAND_PRIMARY).add_modifier(Modifier::BOLD)),
        Span::styled(" split", Style::default().fg(MUTED_GRAY)),
    ]);

    f.render_widget(Paragraph::new(line), area);
}

fn render_content(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::new()
        .title(if app.terminal_fullscreen { " TERMINAL [f fullscreen off] " } else { " OUTPUT " })
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_FOCUSED));

    let inner_height = area.height.saturating_sub(2) as usize;
    let total_lines = app.terminal_lines.len();

    // Clamp scroll
    if total_lines > inner_height {
        let max_scroll = total_lines - inner_height;
        if app.terminal_scroll > max_scroll {
            app.terminal_scroll = max_scroll;
        }
    } else {
        app.terminal_scroll = 0;
    }

    let start = app.terminal_scroll;
    let lines: Vec<Line> = app
        .terminal_lines
        .iter()
        .skip(start)
        .take(inner_height)
        .map(|l| Line::from(strip_ansi(l)))
        .collect();

    let para = Paragraph::new(lines).block(block);
    f.render_widget(para, area);
}

/// Strip ANSI escape sequences from a string.
pub fn strip_ansi(s: &str) -> String {
    // Simple state-machine strip — avoids regex dependency overhead at call frequency
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Consume '[' if present, then consume until alphabetic char
            if chars.peek() == Some(&'[') {
                chars.next();
                for nc in chars.by_ref() {
                    if nc.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi_removes_color_codes() {
        let input = "\x1b[32mhello\x1b[0m world";
        assert_eq!(strip_ansi(input), "hello world");
    }

    #[test]
    fn test_strip_ansi_no_codes() {
        let input = "plain text";
        assert_eq!(strip_ansi(input), "plain text");
    }

    #[test]
    fn test_strip_ansi_multiple_codes() {
        let input = "\x1b[1m\x1b[33mwarning\x1b[0m: something";
        assert_eq!(strip_ansi(input), "warning: something");
    }

    #[test]
    fn test_strip_ansi_empty() {
        assert_eq!(strip_ansi(""), "");
    }
}
