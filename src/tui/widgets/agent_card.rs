//! Agent card widget — visual card block for each agent.
use ratatui::{layout::Rect, Frame};
use crate::types::{AgentSession, TokenSnapshot, StoredEvent};

#[allow(dead_code)]
pub fn render_card(
    f: &mut Frame,
    session: &AgentSession,
    snapshot: Option<&TokenSnapshot>,
    latest_event: Option<&StoredEvent>,
    tmux_line: &str,
    area: Rect,
    selected: bool,
) {
    // Builder 2 implements this
    let _ = (f, session, snapshot, latest_event, tmux_line, area, selected);
}
