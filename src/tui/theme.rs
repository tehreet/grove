//! Grove TUI brand palette and style helpers.
//! Dracula + charm.sh inspired theme.

use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, BorderType, Borders};

// Dracula + charm.sh inspired palette
pub const BRAND_PRIMARY: Color = Color::Rgb(255, 85, 170);
pub const ACCENT_PURPLE: Color = Color::Rgb(189, 147, 249);
pub const ACCENT_CYAN: Color = Color::Rgb(139, 233, 253);
pub const ACCENT_GREEN: Color = Color::Rgb(80, 250, 123);
pub const ACCENT_YELLOW: Color = Color::Rgb(241, 250, 140);
pub const ACCENT_ORANGE: Color = Color::Rgb(255, 184, 108);
pub const ACCENT_RED: Color = Color::Rgb(255, 85, 85);

pub const WORKING_COLOR: Color = ACCENT_GREEN;
pub const BOOTING_COLOR: Color = ACCENT_YELLOW;
pub const STALLED_COLOR: Color = ACCENT_RED;
pub const ZOMBIE_COLOR: Color = Color::Rgb(255, 85, 85);
pub const COMPLETED_COLOR: Color = ACCENT_PURPLE;

pub const MUTED: Color = Color::Rgb(98, 114, 164);
pub const HEADER_BG: Color = Color::Rgb(40, 42, 54);
pub const BORDER_FOCUSED: Color = BRAND_PRIMARY;
pub const BORDER_UNFOCUSED: Color = Color::Rgb(68, 71, 90);
pub const TEXT_PRIMARY: Color = Color::Rgb(248, 248, 242);
#[allow(dead_code)]
pub const TEXT_DIM: Color = Color::Rgb(98, 114, 164);

// Backward-compat aliases so existing imports compile
#[allow(dead_code)]
pub const MUTED_GRAY: Color = MUTED;
#[allow(dead_code)]
pub const BRAND_GREEN: Color = BRAND_PRIMARY;
#[allow(dead_code)]
pub const ACCENT_AMBER: Color = ACCENT_ORANGE;

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
        AgentState::Working => "▶",
        AgentState::Booting => "◌",
        AgentState::Stalled => "⚠",
        AgentState::Zombie => "☠",
        AgentState::Completed => "✔",
    }
}
