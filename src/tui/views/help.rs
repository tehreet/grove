//! Help overlay — shown when user presses `?`.

use ratatui::{
    layout::{Alignment, Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem},
    Frame,
};

use crate::tui::app::App;
use crate::tui::theme::{ACCENT_CYAN, BORDER_FOCUSED, BRAND_PRIMARY, MUTED_GRAY};

pub fn render(f: &mut Frame, _app: &App) {
    let area = f.area();

    // Center the overlay (70% wide, 80% tall, capped)
    let overlay_width = (area.width * 70 / 100).clamp(40, 70);
    let overlay_height = (area.height * 80 / 100).clamp(20, 30);

    let x = (area.width.saturating_sub(overlay_width)) / 2;
    let y = (area.height.saturating_sub(overlay_height)) / 2;

    let overlay_area = Rect::new(x, y, overlay_width, overlay_height);

    // Clear the overlay area
    f.render_widget(Clear, overlay_area);

    let block = Block::new()
        .title(" KEYBOARD SHORTCUTS ")
        .title_alignment(Alignment::Center)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_FOCUSED));

    let inner = overlay_area.inner(Margin {
        horizontal: 1,
        vertical: 1,
    });

    f.render_widget(block, overlay_area);

    let entries = vec![
        (
            "Overview",
            vec![
                ("q", "quit"),
                ("?", "toggle help"),
                ("tab / shift+tab", "cycle panel focus"),
                ("↑↓ / j k", "navigate within panel"),
                ("enter", "open agent detail"),
                ("t", "open terminal view"),
                ("/", "filter agent list"),
                ("r", "force refresh"),
                ("a", "toggle completed agents"),
                ("1", "overview view"),
                ("2", "event log view"),
                ("4 / $", "cost analytics view"),
                ("5", "timeline view"),
            ],
        ),
        (
            "Agent Detail",
            vec![
                ("esc / backspace", "return to overview"),
                ("↑↓ / j k", "scroll"),
                ("t", "open terminal view"),
            ],
        ),
        (
            "Event Log",
            vec![
                ("esc / q", "return to overview"),
                ("↑↓ / j k", "scroll"),
                ("g", "top"),
                ("G", "bottom"),
            ],
        ),
        (
            "Terminal View",
            vec![
                ("esc", "return to overview"),
                ("↑↓ / j k", "scroll"),
                ("g", "top"),
                ("G", "bottom"),
                ("f", "toggle fullscreen"),
                ("s", "enter split view"),
                ("q", "quit"),
            ],
        ),
        (
            "Split Terminal",
            vec![
                ("esc", "return to overview"),
                ("tab", "next panel"),
                ("enter", "open full terminal"),
                ("1-4", "focus panel by number"),
                ("q", "quit"),
            ],
        ),
        (
            "Mail Reader",
            vec![
                ("esc / backspace", "return to overview"),
                ("r", "reply"),
                ("↑↓ / j k", "scroll"),
                ("g", "top"),
                ("G", "bottom"),
                ("q", "quit"),
            ],
        ),
        (
            "Cost Analytics",
            vec![
                ("esc", "return to overview"),
                ("↑↓ / j k", "scroll"),
                ("q", "quit"),
            ],
        ),
        (
            "Timeline",
            vec![
                ("esc", "return to overview"),
                ("↑↓ / j k", "scroll"),
                ("q", "quit"),
            ],
        ),
        ("Anywhere", vec![("ctrl+c", "force quit")]),
    ];

    let mut lines: Vec<ListItem> = vec![
        ListItem::new(Line::from(Span::styled(
            " Press any key to close",
            Style::default().fg(MUTED_GRAY),
        ))),
        ListItem::new(Line::from("")),
    ];

    for (section, keys) in &entries {
        lines.push(ListItem::new(Line::from(Span::styled(
            format!(" {}", section),
            Style::default()
                .fg(BRAND_PRIMARY)
                .add_modifier(Modifier::BOLD),
        ))));
        for (key, desc) in keys {
            lines.push(ListItem::new(Line::from(vec![
                Span::styled(format!("   {:22}", key), Style::default().fg(ACCENT_CYAN)),
                Span::styled(*desc, Style::default()),
            ])));
        }
        lines.push(ListItem::new(Line::from("")));
    }

    let list = List::new(lines);
    f.render_widget(list, inner);
}
