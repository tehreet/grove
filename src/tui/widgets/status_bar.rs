//! Status bar widget — key hints at the bottom of the screen.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::app::{App, View};
use crate::tui::theme::{BRAND_GREEN, MUTED_GRAY};

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let sep = Span::styled("  ", Style::default());

    let key = |k: &'static str| {
        Span::styled(
            format!("[{}]", k),
            Style::default()
                .fg(BRAND_GREEN)
                .add_modifier(Modifier::BOLD),
        )
    };
    let label = |l: &'static str| Span::styled(l, Style::default().fg(MUTED_GRAY));

    let spans: Vec<Span> = match app.current_view {
        View::Overview => {
            vec![
                key("q"), label("quit"), sep.clone(),
                key("?"), label("help"), sep.clone(),
                key("tab"), label("focus"), sep.clone(),
                key("↵"), label("detail"), sep.clone(),
                key("/"), label("filter"), sep.clone(),
                key("r"), label("refresh"), sep.clone(),
                key("a"), label("all agents"), sep.clone(),
                key("2"), label("event log"),
            ]
        }
        View::AgentDetail => {
            vec![
                key("esc"), label("back"), sep.clone(),
                key("↑↓"), label("scroll"), sep.clone(),
                key("q"), label("quit"),
            ]
        }
        View::EventLog => {
            vec![
                key("esc"), label("back"), sep.clone(),
                key("↑↓/jk"), label("scroll"), sep.clone(),
                key("g/G"), label("top/bottom"), sep.clone(),
                key("q"), label("quit"),
            ]
        }
    };

    let bar = Paragraph::new(Line::from(spans));
    f.render_widget(bar, area);
}
