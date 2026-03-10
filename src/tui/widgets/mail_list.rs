//! Mail list widget — recent messages with unread indicators.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem},
    Frame,
};

use crate::tui::app::{App, Focus};
use crate::tui::theme::{
    focused_block, unfocused_block, ACCENT_AMBER, BRAND_GREEN, MUTED_GRAY,
};

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Mail;
    let unread = app.unread_count();
    let title = if unread > 0 {
        format!("MAIL ({} unread)", unread)
    } else {
        "MAIL".to_string()
    };

    let block = if focused {
        focused_block(&title)
    } else {
        unfocused_block(&title)
    };

    let inner_height = area.height.saturating_sub(2) as usize;
    let total = app.messages.len();

    let start = app.mail_scroll.min(total.saturating_sub(inner_height));
    let end = (start + inner_height).min(total);

    let visible = if total == 0 {
        &[][..]
    } else {
        &app.messages[start..end]
    };

    let items: Vec<ListItem> = visible
        .iter()
        .map(|msg| {
            let unread_dot = if !msg.read {
                Span::styled("● ", Style::default().fg(ACCENT_AMBER).add_modifier(Modifier::BOLD))
            } else {
                Span::styled("  ", Style::default())
            };

            let from_to = format!("{} → {}: ", truncate(&msg.from, 12), truncate(&msg.to, 12));
            let subject = truncate(&msg.subject, 28);

            let rel_time = relative_time(&msg.created_at);

            let line = Line::from(vec![
                unread_dot,
                Span::styled(
                    from_to,
                    Style::default().fg(BRAND_GREEN),
                ),
                Span::styled(subject, Style::default()),
                Span::styled(
                    format!(" {}", rel_time),
                    Style::default().fg(MUTED_GRAY),
                ),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = if items.is_empty() {
        List::new(vec![ListItem::new(
            Span::styled("  no mail", Style::default().fg(MUTED_GRAY)),
        )])
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

fn relative_time(ts: &str) -> String {
    let parsed = chrono::DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc));
    let now = chrono::Utc::now();
    match parsed {
        Some(dt) => {
            let secs = now.signed_duration_since(dt).num_seconds();
            if secs < 60 {
                format!("{}s ago", secs)
            } else if secs < 3600 {
                format!("{}m ago", secs / 60)
            } else if secs < 86400 {
                format!("{}h ago", secs / 3600)
            } else {
                format!("{}d ago", secs / 86400)
            }
        }
        None => ts.get(11..16).unwrap_or("?").to_string(),
    }
}
