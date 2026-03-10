//! Split terminal view — shows 2-4 agent terminals simultaneously in a grid.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::Line,
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};

use crate::tui::app::App;
use crate::tui::theme::{agent_state_color, agent_state_icon, BORDER_FOCUSED, BORDER_UNFOCUSED};
use crate::tui::views::terminal::strip_ansi;
use crate::tui::widgets::status_bar;

pub fn render(f: &mut Frame, app: &mut App) {
    let area = f.area();

    let layout = Layout::vertical([
        Constraint::Fill(1),   // panels
        Constraint::Length(1), // status bar
    ])
    .split(area);

    render_panels(f, app, layout[0]);
    status_bar::render(f, app, layout[1]);
}

fn render_panels(f: &mut Frame, app: &App, area: Rect) {
    let count = app.split_agents.len();

    if count == 0 {
        let para = Paragraph::new("No agents available");
        f.render_widget(para, area);
        return;
    }

    if count == 1 {
        render_panel(f, app, area, 0);
        return;
    }

    // 2-4 agents: 2x2 grid (bottom-right empty when count==3)
    let rows =
        Layout::vertical([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);
    let top =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[0]);
    let bottom =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(rows[1]);

    let cells = [top[0], top[1], bottom[0], bottom[1]];
    for (i, &cell) in cells.iter().enumerate() {
        if i < count {
            render_panel(f, app, cell, i);
        }
    }
}

fn render_panel(f: &mut Frame, app: &App, area: Rect, idx: usize) {
    let agent = match app.split_agents.get(idx) {
        Some(a) => a,
        None => return,
    };
    let lines_opt = app.split_lines.get(idx);

    let border_color = if idx == app.split_focus {
        BORDER_FOCUSED
    } else {
        BORDER_UNFOCUSED
    };
    let state_color = agent_state_color(&agent.state);
    let icon = agent_state_icon(&agent.state);

    let title = format!(" {} {} {} ", icon, agent.agent_name, idx + 1);

    let block = Block::new()
        .title(title)
        .title_style(Style::default().fg(state_color))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let inner_height = inner.height as usize;

    let content_lines: Vec<Line> = match lines_opt {
        Some(lines) => {
            let start = lines.len().saturating_sub(inner_height);
            lines
                .iter()
                .skip(start)
                .map(|l| Line::from(strip_ansi(l)))
                .collect()
        }
        None => vec![Line::from("(no output)")],
    };

    f.render_widget(Paragraph::new(content_lines), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::app::App;
    use crate::types::{AgentSession, AgentState};

    fn make_session(name: &str, state: AgentState) -> AgentSession {
        AgentSession {
            id: format!("session-{}", name),
            agent_name: name.to_string(),
            capability: "builder".to_string(),
            worktree_path: "/tmp".to_string(),
            branch_name: "main".to_string(),
            task_id: "task-1".to_string(),
            tmux_session: format!("tmux-{}", name),
            state,
            pid: None,
            parent_agent: None,
            depth: 1,
            run_id: None,
            started_at: "2024-01-01T00:00:00Z".to_string(),
            last_activity: "2024-01-01T00:00:01Z".to_string(),
            escalation_level: 0,
            stalled_since: None,
            transcript_path: None,
        }
    }

    #[test]
    fn test_enter_split_collects_working_agents() {
        let mut app = App::new("/tmp");
        app.sessions = vec![
            make_session("agent-a", AgentState::Working),
            make_session("agent-b", AgentState::Booting),
            make_session("agent-c", AgentState::Completed),
        ];
        app.enter_split();
        // Working + Booting should be included, Completed excluded
        assert_eq!(app.split_agents.len(), 2);
        assert!(app.split_agents.iter().any(|a| a.agent_name == "agent-a"));
        assert!(app.split_agents.iter().any(|a| a.agent_name == "agent-b"));
    }

    #[test]
    fn test_enter_split_caps_at_four() {
        let mut app = App::new("/tmp");
        app.sessions = (0..6)
            .map(|i| make_session(&format!("agent-{}", i), AgentState::Working))
            .collect();
        app.enter_split();
        assert!(app.split_agents.len() <= 4);
    }

    #[test]
    fn test_split_focus_tab_cycles() {
        use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
        let mut app = App::new("/tmp");
        app.split_agents = vec![
            make_session("a", AgentState::Working),
            make_session("b", AgentState::Working),
        ];
        app.split_lines = vec![vec![], vec![]];
        app.current_view = crate::tui::app::View::SplitTerminal;
        assert_eq!(app.split_focus, 0);
        app.handle_key(KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        assert_eq!(app.split_focus, 1);
        app.handle_key(KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        });
        assert_eq!(app.split_focus, 0);
    }
}
