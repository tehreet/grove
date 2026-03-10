//! Feed widget — live event stream panel.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem},
    Frame,
};

use crate::tui::app::{App, Focus};
use crate::tui::theme::{focused_block, unfocused_block, MUTED_GRAY, TEXT_PRIMARY};
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
            let time_str = ev.created_at.get(11..19).unwrap_or("??:??:??");

            let agent_color = agent_color_for(&ev.agent_name);

            let event_type_str = event_type_to_str(&ev.event_type);
            let tool_part = if let Some(ref tool) = ev.tool_name {
                let (icon, desc) = format_tool_event(tool, ev.tool_args.as_deref());
                let dur = ev
                    .tool_duration_ms
                    .map(|ms| format!(" ({}ms)", ms))
                    .unwrap_or_default();
                format!("{} {}{}", icon, desc, dur)
            } else {
                match &ev.event_type {
                    EventType::SessionStart => "\u{25b6} started".to_string(),
                    EventType::SessionEnd => "\u{23f9} completed".to_string(),
                    EventType::MailSent => "\u{2709} mail".to_string(),
                    _ => event_type_str,
                }
            };

            let line = Line::from(vec![
                Span::styled(format!("{} ", time_str), Style::default().fg(MUTED_GRAY)),
                Span::styled(
                    format!("{} ", truncate(&ev.agent_name, 16)),
                    Style::default()
                        .fg(agent_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(tool_part, Style::default().fg(TEXT_PRIMARY)),
            ]);

            ListItem::new(line)
        })
        .collect();

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

/// Deterministic color per agent name (Dracula-derived palette).
fn agent_color_for(name: &str) -> Color {
    let palette = [
        Color::Rgb(139, 233, 253), // cyan
        Color::Rgb(80, 250, 123),  // green
        Color::Rgb(255, 184, 108), // orange
        Color::Rgb(189, 147, 249), // purple
        Color::Rgb(241, 250, 140), // yellow
        Color::Rgb(255, 85, 170),  // pink
        Color::Rgb(255, 85, 85),   // red
        Color::Rgb(248, 248, 242), // foreground
    ];
    let hash: usize = name
        .bytes()
        .fold(0usize, |acc, b| acc.wrapping_add(b as usize));
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

fn format_tool_event(tool_name: &str, args: Option<&str>) -> (String, String) {
    let args_json: Option<serde_json::Value> = args.and_then(|a| serde_json::from_str(a).ok());

    match tool_name {
        "Bash" => {
            let cmd = args_json
                .as_ref()
                .and_then(|v| v.get("command"))
                .and_then(|v| v.as_str())
                .unwrap_or("...");
            ("$".to_string(), truncate(cmd, 40))
        }
        "Edit" => {
            let path = args_json
                .as_ref()
                .and_then(|v| v.get("file_path"))
                .and_then(|v| v.as_str())
                .map(shorten_path)
                .unwrap_or_else(|| "...".to_string());
            ("\u{270e}".to_string(), path)
        }
        "Read" => {
            let path = args_json
                .as_ref()
                .and_then(|v| v.get("file_path"))
                .and_then(|v| v.as_str())
                .map(shorten_path)
                .unwrap_or_else(|| "...".to_string());
            ("\u{25c9}".to_string(), path)
        }
        "Write" => {
            let path = args_json
                .as_ref()
                .and_then(|v| v.get("file_path"))
                .and_then(|v| v.as_str())
                .map(shorten_path)
                .unwrap_or_else(|| "...".to_string());
            ("\u{2295}".to_string(), path)
        }
        "Grep" | "Glob" => {
            let pattern = args_json
                .as_ref()
                .and_then(|v| v.get("pattern"))
                .and_then(|v| v.as_str())
                .unwrap_or("...");
            ("\u{2315}".to_string(), truncate(pattern, 40))
        }
        _ => {
            let preview = args.map(|a| truncate_args(a, 30)).unwrap_or_default();
            (tool_name.to_string(), preview)
        }
    }
}

fn shorten_path(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() <= 2 {
        path.to_string()
    } else {
        format!("\u{2026}/{}", parts[parts.len() - 2..].join("/"))
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
