//! Mail reader view — full message body with threading and reply support.

use ratatui::{
    layout::{Alignment, Constraint, Layout, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
    Frame,
};

use crate::tui::app::App;
use crate::tui::theme::{
    ACCENT_AMBER, BORDER_FOCUSED, BORDER_UNFOCUSED, BRAND_GREEN, MUTED_GRAY,
};
use crate::types::MailMessage;

pub fn render(f: &mut Frame, app: &App) {
    let area = f.area();

    let has_reply = app.reply_mode;
    let reply_height: u16 = if has_reply { 5 } else { 0 };

    let layout = Layout::vertical([
        Constraint::Length(1),                              // header
        Constraint::Length(4),                              // meta
        Constraint::Fill(1),                                // body + thread
        Constraint::Length(reply_height),                   // reply input (conditional)
        Constraint::Length(1),                              // footer hints
    ])
    .split(area);

    let msg = match app.selected_message.as_ref() {
        Some(m) => m,
        None => return,
    };

    render_header(f, msg, layout[0]);
    render_meta(f, msg, layout[1]);
    render_body(f, app, msg, layout[2]);
    if has_reply {
        render_reply_input(f, app, layout[3]);
    }
    render_footer(f, app, layout[4]);
}

fn render_header(f: &mut Frame, msg: &MailMessage, area: Rect) {
    let line = Line::from(vec![
        Span::styled(" ← ", Style::default().fg(MUTED_GRAY)),
        Span::styled(&msg.subject, Style::default().fg(ACCENT_AMBER).add_modifier(Modifier::BOLD)),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn render_meta(f: &mut Frame, msg: &MailMessage, area: Rect) {
    let time_str = format_time(&msg.created_at);
    let type_str = msg.message_type.to_string();
    let priority_str = msg.priority.to_string();

    let lines = vec![
        Line::from(vec![
            Span::styled("  From:     ", Style::default().fg(MUTED_GRAY)),
            Span::styled(&msg.from, Style::default().fg(BRAND_GREEN)),
        ]),
        Line::from(vec![
            Span::styled("  To:       ", Style::default().fg(MUTED_GRAY)),
            Span::styled(&msg.to, Style::default().fg(BRAND_GREEN)),
        ]),
        Line::from(vec![
            Span::styled("  Type:     ", Style::default().fg(MUTED_GRAY)),
            Span::styled(&type_str, Style::default().fg(ACCENT_AMBER)),
            Span::styled("   Priority: ", Style::default().fg(MUTED_GRAY)),
            Span::styled(&priority_str, Style::default()),
            Span::styled("   Time: ", Style::default().fg(MUTED_GRAY)),
            Span::styled(time_str, Style::default()),
        ]),
        Line::from(vec![
            Span::styled(
                if let Some(tid) = &msg.thread_id {
                    format!("  Thread:   {}", tid)
                } else {
                    String::new()
                },
                Style::default().fg(MUTED_GRAY),
            ),
        ]),
    ];

    let block = Block::new()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(BORDER_UNFOCUSED));

    f.render_widget(Paragraph::new(lines).block(block), area);
}

fn render_body(f: &mut Frame, app: &App, msg: &MailMessage, area: Rect) {
    // Split body area: main message on top, thread replies below (if any)
    let thread_count = app.thread_messages.len();
    let thread_visible = thread_count > 0;

    let (body_area, thread_area) = if thread_visible {
        let parts = Layout::vertical([
            Constraint::Percentage(60),
            Constraint::Percentage(40),
        ])
        .split(area);
        (parts[0], Some(parts[1]))
    } else {
        (area, None)
    };

    // Body paragraph
    let body_block = Block::new()
        .title(" MESSAGE ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_FOCUSED));

    let inner = body_block.inner(body_area);
    let visible_height = inner.height as usize;

    let body_lines: Vec<Line> = msg
        .body
        .lines()
        .map(|l| Line::from(Span::raw(l.to_string())))
        .collect();

    let scroll_offset = app.mail_reader_scroll.min(
        body_lines.len().saturating_sub(visible_height),
    );

    let body_para = Paragraph::new(body_lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll_offset as u16, 0))
        .block(body_block);

    f.render_widget(body_para, body_area);

    // Thread replies panel
    if let Some(t_area) = thread_area {
        render_thread(f, app, msg, t_area);
    }
}

fn render_thread(f: &mut Frame, app: &App, current: &MailMessage, area: Rect) {
    let block = Block::new()
        .title(" THREAD ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_UNFOCUSED));

    let inner = block.inner(area);
    let visible_height = inner.height as usize;

    let mut lines: Vec<Line> = vec![];

    for msg in &app.thread_messages {
        if msg.id == current.id {
            continue; // skip current message in thread list
        }
        let indent = "  ↳ ";
        let time = msg.created_at.get(11..16).unwrap_or("?");
        let from_short = if msg.from.len() > 16 { &msg.from[..16] } else { &msg.from };

        lines.push(Line::from(vec![
            Span::styled(indent, Style::default().fg(MUTED_GRAY)),
            Span::styled(format!("{} ", from_short), Style::default().fg(BRAND_GREEN)),
            Span::styled(format!("[{}]  ", time), Style::default().fg(MUTED_GRAY)),
            Span::styled(
                truncate(&msg.body, area.width.saturating_sub(30) as usize),
                Style::default(),
            ),
        ]));
    }

    let scroll = app.mail_reader_scroll.saturating_sub(
        app.thread_messages.len().saturating_sub(visible_height),
    );

    let para = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll as u16, 0))
        .block(block);

    f.render_widget(para, area);
}

fn render_reply_input(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::new()
        .title(" REPLY ")
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_FOCUSED));

    let inner = block.inner(area).inner(Margin { horizontal: 1, vertical: 0 });

    f.render_widget(block, area);

    // Show cursor blinking at end of text
    let text_with_cursor = format!("{}█", app.reply_text);
    let para = Paragraph::new(Line::from(Span::styled(
        text_with_cursor,
        Style::default(),
    )))
    .wrap(Wrap { trim: false });

    f.render_widget(para, inner);
}

fn render_footer(f: &mut Frame, app: &App, area: Rect) {
    let hints = if app.reply_mode {
        Line::from(vec![
            Span::styled(" [enter] send  ", Style::default().fg(ACCENT_AMBER)),
            Span::styled("[esc] cancel", Style::default().fg(MUTED_GRAY)),
        ])
    } else {
        Line::from(vec![
            Span::styled(" [esc] back  ", Style::default().fg(ACCENT_AMBER)),
            Span::styled("[r] reply  ", Style::default().fg(MUTED_GRAY)),
            Span::styled("[↑↓/jk] scroll", Style::default().fg(MUTED_GRAY)),
        ])
    };

    let para = Paragraph::new(hints).alignment(Alignment::Left);
    f.render_widget(para, area);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}

fn format_time(ts: &str) -> String {
    // e.g. "2024-01-01T00:00:00Z" → "2024-01-01 00:00"
    if ts.len() >= 16 {
        format!("{} {}", &ts[..10], &ts[11..16])
    } else {
        ts.to_string()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_long() {
        let result = truncate("hello world", 8);
        assert!(result.len() <= 10); // truncated + ellipsis
        assert!(result.starts_with("hello w"));
    }

    #[test]
    fn test_truncate_zero_max() {
        assert_eq!(truncate("hello", 0), "");
    }

    #[test]
    fn test_format_time_full() {
        assert_eq!(format_time("2024-01-15T10:30:00Z"), "2024-01-15 10:30");
    }

    #[test]
    fn test_format_time_short() {
        assert_eq!(format_time("short"), "short");
    }
}
