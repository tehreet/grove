//! Header bar widget — full-width, shows run ID, agent count, cost, time, git info.

use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::app::App;
use crate::tui::theme::{ACCENT_AMBER, BRAND_GREEN, HEADER_BG, MUTED_GRAY};

// ---------------------------------------------------------------------------
// Cached git info (refresh every 30 s — git calls are slow)
// ---------------------------------------------------------------------------

struct GitInfo {
    branch: String,
    commit: String,
    refreshed: Instant,
}

static GIT_INFO: OnceLock<Mutex<GitInfo>> = OnceLock::new();

fn git_info_store() -> &'static Mutex<GitInfo> {
    GIT_INFO.get_or_init(|| {
        Mutex::new(GitInfo {
            branch: String::new(),
            commit: String::new(),
            refreshed: Instant::now()
                .checked_sub(std::time::Duration::from_secs(60))
                .unwrap_or_else(Instant::now),
        })
    })
}

fn run_git(args: &[&str]) -> Option<String> {
    std::process::Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

fn get_git_info() -> (String, String) {
    let store = git_info_store();
    if let Ok(mut info) = store.lock() {
        if info.refreshed.elapsed().as_secs() >= 30 {
            info.branch = run_git(&["rev-parse", "--abbrev-ref", "HEAD"])
                .unwrap_or_default();
            info.commit = run_git(&["log", "-1", "--pretty=%h"])
                .unwrap_or_default();
            info.refreshed = Instant::now();
        }
        (info.branch.clone(), info.commit.clone())
    } else {
        (String::new(), String::new())
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let now = chrono::Local::now();
    let time_str = now.format("%H:%M:%S").to_string();

    let active_count = app.active_agent_count();
    let total_count = app.sessions.len();
    let cost = app.total_cost_display();

    let run_display = match &app.run_id {
        Some(id) => {
            let short = if id.len() > 28 {
                format!("{}…", &id[..27])
            } else {
                id.clone()
            };
            short
        }
        None => "no active run".to_string(),
    };

    let separator = Span::styled(" │ ", Style::default().fg(MUTED_GRAY));

    let mut spans = vec![
        Span::styled(
            " grove dashboard",
            Style::default()
                .fg(BRAND_GREEN)
                .add_modifier(Modifier::BOLD),
        ),
        separator.clone(),
        Span::styled("run: ", Style::default().fg(MUTED_GRAY)),
        Span::styled(run_display, Style::default().fg(ACCENT_AMBER)),
        separator.clone(),
        Span::styled(
            format!("{} agents", active_count),
            Style::default().fg(BRAND_GREEN),
        ),
    ];

    if total_count > active_count {
        spans.push(Span::styled(
            format!("/{}", total_count),
            Style::default().fg(MUTED_GRAY),
        ));
    }

    if !cost.is_empty() {
        spans.push(separator.clone());
        spans.push(Span::styled(cost, Style::default().fg(ACCENT_AMBER)));
    }

    // Git branch + commit
    let (branch, commit) = get_git_info();
    if !branch.is_empty() {
        spans.push(separator.clone());
        spans.push(Span::styled("⎇ ", Style::default().fg(MUTED_GRAY)));
        spans.push(Span::styled(
            branch,
            Style::default().fg(Color::Rgb(130, 180, 255)),
        ));
        if !commit.is_empty() {
            spans.push(Span::styled(
                format!(" @{}", commit),
                Style::default().fg(MUTED_GRAY),
            ));
        }
    }

    spans.push(separator.clone());
    spans.push(Span::styled(
        time_str,
        Style::default().fg(MUTED_GRAY),
    ));

    if app.filter_mode {
        spans.push(separator.clone());
        spans.push(Span::styled(
            format!("filter: {}_", app.filter_text),
            Style::default().fg(ACCENT_AMBER).add_modifier(Modifier::BOLD),
        ));
    } else if !app.filter_text.is_empty() {
        spans.push(separator.clone());
        spans.push(Span::styled(
            format!("/{}", app.filter_text),
            Style::default().fg(ACCENT_AMBER),
        ));
    }

    let header = Paragraph::new(Line::from(spans))
        .style(Style::default().bg(HEADER_BG));

    f.render_widget(header, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_git_info_does_not_panic() {
        let (branch, commit) = get_git_info();
        // In a git repo these should be non-empty; outside they may be empty.
        // Either way the function must not panic.
        let _ = (branch, commit);
    }

    #[test]
    fn test_git_info_cached() {
        // Call twice — second call should hit cache and return same values.
        let (b1, c1) = get_git_info();
        let (b2, c2) = get_git_info();
        assert_eq!(b1, b2);
        assert_eq!(c1, c2);
    }
}
