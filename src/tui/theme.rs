//! Grove TUI brand palette and style helpers.

use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, BorderType, Borders};

pub const BRAND_GREEN: Color = Color::Rgb(46, 125, 50);
pub const ACCENT_AMBER: Color = Color::Rgb(255, 183, 77);
pub const MUTED_GRAY: Color = Color::Rgb(120, 120, 110);
pub const WORKING_COLOR: Color = Color::Rgb(76, 175, 80);
pub const BOOTING_COLOR: Color = Color::Rgb(255, 193, 7);
pub const STALLED_COLOR: Color = Color::Rgb(244, 67, 54);
pub const COMPLETED_COLOR: Color = Color::Rgb(0, 188, 212);
pub const BORDER_FOCUSED: Color = BRAND_GREEN;
pub const BORDER_UNFOCUSED: Color = Color::Rgb(58, 58, 58);
pub const HEADER_BG: Color = Color::Rgb(25, 45, 25);
pub const ZOMBIE_COLOR: Color = STALLED_COLOR;

pub fn focused_block(title: &str) -> Block<'_> {
    Block::new()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_FOCUSED))
}

pub fn unfocused_block(title: &str) -> Block<'_> {
    Block::new()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(BORDER_UNFOCUSED))
}

pub fn agent_state_color(state: &crate::types::AgentState) -> Color {
    use crate::types::AgentState;
    match state {
        AgentState::Working => WORKING_COLOR,
        AgentState::Booting => BOOTING_COLOR,
        AgentState::Stalled => STALLED_COLOR,
        AgentState::Zombie => ZOMBIE_COLOR,
        AgentState::Completed => COMPLETED_COLOR,
    }
}

pub fn agent_state_icon(state: &crate::types::AgentState) -> &'static str {
    use crate::types::AgentState;
    match state {
        AgentState::Working => "●",
        AgentState::Booting => "○",
        AgentState::Stalled => "!",
        AgentState::Zombie => "✗",
        AgentState::Completed => "✓",
    }
}

pub fn dimmed() -> Style {
    Style::default().fg(MUTED_GRAY)
}

pub fn bold_green() -> Style {
    Style::default().fg(BRAND_GREEN).add_modifier(Modifier::BOLD)
}

pub fn bold_amber() -> Style {
    Style::default().fg(ACCENT_AMBER).add_modifier(Modifier::BOLD)
}
