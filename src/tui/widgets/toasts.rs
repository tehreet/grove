//! Toast notification overlay widget.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Clear, Paragraph},
    Frame,
};

/// A single toast notification.
pub struct Toast {
    pub message: String,
    pub color: Color,
    pub created: std::time::Instant,
}

impl Toast {
    pub fn new(message: impl Into<String>, color: Color) -> Self {
        Toast {
            message: message.into(),
            color,
            created: std::time::Instant::now(),
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created.elapsed().as_secs() >= 3
    }
}

/// Render toast notifications overlaid in top-right corner.
/// Takes a slice of toasts (caller manages the Vec).
pub fn render_toasts(f: &mut Frame, toasts: &[Toast]) {
    let active: Vec<&Toast> = toasts
        .iter()
        .filter(|t| !t.is_expired())
        .rev()
        .take(3)
        .collect();

    if active.is_empty() {
        return;
    }

    let area = f.area();
    let toast_width: u16 = 42;
    let x = area.width.saturating_sub(toast_width + 1);

    let bg = Color::Rgb(40, 42, 54); // HEADER_BG

    for (i, toast) in active.iter().enumerate() {
        let y = 1 + (i as u16 * 2);
        if y + 1 >= area.height {
            break;
        }

        let toast_area = Rect::new(x, y, toast_width, 1);
        let widget = Paragraph::new(Line::from(vec![Span::styled(
            format!(" {} ", toast.message),
            Style::default()
                .fg(toast.color)
                .add_modifier(Modifier::BOLD),
        )]))
        .style(Style::default().bg(bg));

        f.render_widget(Clear, toast_area);
        f.render_widget(widget, toast_area);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toast_not_expired_immediately() {
        let toast = Toast::new("hello", Color::Green);
        assert!(!toast.is_expired());
    }

    #[test]
    fn test_toast_new_stores_message() {
        let toast = Toast::new("test message", Color::Red);
        assert_eq!(toast.message, "test message");
        assert_eq!(toast.color, Color::Red);
    }
}
