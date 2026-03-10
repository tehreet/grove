//! TUI view modules.

pub mod agent_detail;
pub mod event_log;
pub mod help;
pub mod overview;
pub mod split_terminal;
pub mod terminal;

use ratatui::Frame;

use crate::tui::app::{App, View};

/// Top-level render dispatcher — calls the active view.
pub fn render(f: &mut Frame, app: &mut App) {
    match app.current_view {
        View::Overview => overview::render(f, app),
        View::AgentDetail => agent_detail::render(f, app),
        View::EventLog => event_log::render(f, app),
        View::Terminal => terminal::render(f, app),
        View::SplitTerminal => split_terminal::render(f, app),
    }

    if app.show_help {
        help::render(f, app);
    }
}
