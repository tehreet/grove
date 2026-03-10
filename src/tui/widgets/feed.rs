//! Feed widget — live event stream panel.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem},
    Frame,
};

use crate::tui::app::{App, Focus};
use crate::tui::theme::{focused_block, unfocused_block, MUTED_GRAY};
use crate::types::{EventType, StoredEvent};

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Feed;
    let title = "FEED (live)";
    let block = if focused {
        focused_block(title)
    } else {
        unfocused_block(title)
    };

    // How many visible lines (minus block borders)
    let inner_height = area.height.saturating_sub(2) as usize;
    let total = app.events.len();

    // Scroll: show latest events at bottom
    let scroll = if total > inner_height {
        // If user hasn't scrolled, show bottom
        let max_scroll = total.saturating_sub(inner_height);
        app.feed_scroll.min(max_scroll)
    } else {
        0
    };

    let visible: Vec<&StoredEvent> = if total > inner_height {
        let start = scroll;
        let end = (start + inner_height).min(total);
        app.events[start..end].iter().collect()
    } else {
        app.events.iter().collect()
    };

    let items: Vec<ListItem> = visible
        .iter()
        .map(|ev| {
            let time_str = ev
                .created_at
                .get(11..19)
                .unwrap_or("??:??:??");

            let agent_color = agent_color_for(&ev.agent_name);

            let event_type_str = event_type_to_str(&ev.event_type);
            let tool_part = if let Some(ref tool) = ev.tool_name {
                let args_preview = ev
                    .tool_args
                    .as_deref()
                    .map(|a| truncate_args(a, 30))
                    .unwrap_or_default();
                let dur = ev
                    .tool_duration_ms
                    .map(|ms| format!(" ({}ms)", ms))
                    .unwrap_or_default();
                format!("{} {}{}", tool, args_preview, dur)
            } else {
                event_type_str
            };

            let line = Line::from(vec![
                Span::styled(
                    format!("{} ", time_str),
                    Style::default().fg(MUTED_GRAY),
                ),
                Span::styled(
                    format!("{} ", truncate(&ev.agent_name, 16)),
                    Style::default()
                        .fg(agent_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(tool_part, Style::default().fg(Color::White)),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = if items.is_empty() {
        List::new(vec![ListItem::new(
            Span::styled("  no events yet", Style::default().fg(MUTED_GRAY)),
        )])
        .block(block)
    } else {
        List::new(items).block(block)
    };

    f.render_widget(list, area);
}

/// Deterministic color per agent name.
fn agent_color_for(name: &str) -> Color {
    let palette = [
        Color::Rgb(100, 180, 255), // light blue
        Color::Rgb(180, 255, 100), // lime
        Color::Rgb(255, 180, 100), // orange
        Color::Rgb(180, 100, 255), // purple
        Color::Rgb(100, 255, 180), // mint
        Color::Rgb(255, 100, 180), // pink
        Color::Rgb(255, 220, 100), // yellow
        Color::Rgb(100, 220, 255), // cyan
    ];
    let hash: usize = name.bytes().fold(0usize, |acc, b| acc.wrapping_add(b as usize));
    palette[hash % palette.len()]
}

fn event_type_to_str(et: &EventType) -> String {
    serde_json::to_string(et)
        .unwrap_or_default()
        .trim_matches('"')
        .to_string()
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max.saturating_sub(1)])
    }
}

fn truncate_args(args: &str, max: usize) -> String {
    // Try to extract the first meaningful arg from JSON
    let cleaned = if args.starts_with('{') || args.starts_with('[') {
        // extract a short preview
        let flat: String = args.chars().filter(|c| *c != '\n').collect();
        flat
    } else {
        args.to_string()
    };
    if cleaned.len() <= max {
        cleaned
    } else {
        format!("{}…", &cleaned[..max.saturating_sub(1)])
    }
}
