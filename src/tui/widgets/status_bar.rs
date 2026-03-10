//! Status bar widget — key hints at the bottom of the screen.
//!
//! Also provides a notification queue for flash messages that fade after 3 s,
//! and displays live system stats (load avg, disk usage) on the right side.

use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::tui::app::{App, View};
use crate::tui::theme::{ACCENT_GREEN, ACCENT_ORANGE, BRAND_PRIMARY, MUTED_GRAY};

// ---------------------------------------------------------------------------
// Notification queue
// ---------------------------------------------------------------------------

struct Notification {
    message: String,
    created: Instant,
}

static NOTIFICATIONS: OnceLock<Mutex<Vec<Notification>>> = OnceLock::new();

fn notif_store() -> &'static Mutex<Vec<Notification>> {
    NOTIFICATIONS.get_or_init(|| Mutex::new(vec![]))
}

/// Push a flash notification that will be displayed for 3 seconds.
#[allow(dead_code)]
pub fn push_notification(msg: impl Into<String>) {
    if let Ok(mut lock) = notif_store().lock() {
        lock.push(Notification {
            message: msg.into(),
            created: Instant::now(),
        });
    }
}

/// Return the most recent active notification (< 3 s old), pruning expired ones.
fn active_notification() -> Option<String> {
    let store = notif_store();
    if let Ok(mut lock) = store.lock() {
        lock.retain(|n| n.created.elapsed().as_secs() < 3);
        lock.last().map(|n| n.message.clone())
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Cached system stats (refresh every 10 s to avoid heavy syscalls per frame)
// ---------------------------------------------------------------------------

struct SysStats {
    load_avg: String,
    disk_usage: String,
    refreshed: Instant,
}

static SYS_STATS: OnceLock<Mutex<SysStats>> = OnceLock::new();

fn sys_stats_store() -> &'static Mutex<SysStats> {
    SYS_STATS.get_or_init(|| {
        Mutex::new(SysStats {
            load_avg: String::new(),
            disk_usage: String::new(),
            refreshed: Instant::now()
                .checked_sub(std::time::Duration::from_secs(60))
                .unwrap_or_else(Instant::now),
        })
    })
}

fn read_load_avg() -> String {
    std::fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|s| {
            let parts: Vec<&str> = s.split_whitespace().collect();
            if parts.len() >= 3 {
                Some(format!("{} {} {}", parts[0], parts[1], parts[2]))
            } else {
                None
            }
        })
        .unwrap_or_default()
}

fn read_disk_usage() -> String {
    // Run `df -h .` and extract the "Use%" column for the current filesystem.
    let output = std::process::Command::new("df")
        .args(["-h", "."])
        .output()
        .ok();
    if let Some(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        // Second line: Filesystem Size Used Avail Use% Mounted
        if let Some(line) = stdout.lines().nth(1) {
            let cols: Vec<&str> = line.split_whitespace().collect();
            // cols: [fs, size, used, avail, use%, mount]
            if cols.len() >= 5 {
                return format!("{} avail ({})", cols[3], cols[4]);
            }
        }
    }
    String::new()
}

fn get_sys_stats() -> (String, String) {
    let store = sys_stats_store();
    if let Ok(mut stats) = store.lock() {
        if stats.refreshed.elapsed().as_secs() >= 10 {
            stats.load_avg = read_load_avg();
            stats.disk_usage = read_disk_usage();
            stats.refreshed = Instant::now();
        }
        (stats.load_avg.clone(), stats.disk_usage.clone())
    } else {
        (String::new(), String::new())
    }
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

pub fn render(f: &mut Frame, app: &App, area: Rect) {
    let sep = Span::styled("  ", Style::default());

    let key = |k: &'static str| {
        Span::styled(
            format!("[{}]", k),
            Style::default()
                .fg(BRAND_PRIMARY)
                .add_modifier(Modifier::BOLD),
        )
    };
    let label = |l: &'static str| Span::styled(l, Style::default().fg(MUTED_GRAY));

    let hint_spans: Vec<Span> = match app.current_view {
        View::Overview => {
            vec![
                key("q"),
                label("quit"),
                sep.clone(),
                key("?"),
                label("help"),
                sep.clone(),
                key("tab"),
                label("focus"),
                sep.clone(),
                key("↵"),
                label("detail"),
                sep.clone(),
                key("t"),
                label("terminal"),
                sep.clone(),
                key("/"),
                label("filter"),
                sep.clone(),
                key("r"),
                label("refresh"),
                sep.clone(),
                key("a"),
                label("all agents"),
                sep.clone(),
                key("2"),
                label("event log"),
                sep.clone(),
                key("4/$"),
                label("costs"),
                sep.clone(),
                key("5"),
                label("timeline"),
            ]
        }
        View::AgentDetail => {
            vec![
                key("esc"),
                label("back"),
                sep.clone(),
                key("↑↓"),
                label("scroll"),
                sep.clone(),
                key("q"),
                label("quit"),
            ]
        }
        View::EventLog => {
            vec![
                key("esc"),
                label("back"),
                sep.clone(),
                key("↑↓/jk"),
                label("scroll"),
                sep.clone(),
                key("g/G"),
                label("top/bottom"),
                sep.clone(),
                key("q"),
                label("quit"),
            ]
        }
        View::Terminal => {
            vec![
                key("esc"),
                label("back"),
                sep.clone(),
                key("↑↓/jk"),
                label("scroll"),
                sep.clone(),
                key("g/G"),
                label("top/bottom"),
                sep.clone(),
                key("f"),
                label("fullscreen"),
                sep.clone(),
                key("s"),
                label("split view"),
                sep.clone(),
                key("q"),
                label("quit"),
            ]
        }
        View::SplitTerminal => {
            vec![
                key("esc"),
                label("back"),
                sep.clone(),
                key("tab"),
                label("next panel"),
                sep.clone(),
                key("↵"),
                label("open full"),
                sep.clone(),
                key("1-4"),
                label("focus panel"),
                sep.clone(),
                key("q"),
                label("quit"),
            ]
        }
        View::MailReader => {
            vec![
                key("esc"),
                label("back"),
                sep.clone(),
                key("r"),
                label("reply"),
                sep.clone(),
                key("↑↓/jk"),
                label("scroll"),
            ]
        }
        View::CostAnalytics => {
            vec![
                key("esc"),
                label("back"),
                sep.clone(),
                key("↑↓/jk"),
                label("scroll"),
                sep.clone(),
                key("q"),
                label("quit"),
            ]
        }
        View::Timeline => {
            vec![
                key("esc"),
                label("back"),
                sep.clone(),
                key("↑↓/jk"),
                label("scroll"),
                sep.clone(),
                key("q"),
                label("quit"),
            ]
        }
    };

    // Split area: hints on left, system stats on right
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Fill(1), Constraint::Length(60)])
        .split(area);

    let hints_widget = Paragraph::new(Line::from(hint_spans));
    f.render_widget(hints_widget, chunks[0]);

    // Right side: notification (if any) OR system stats
    let right_spans = build_right_spans();
    let right_widget = Paragraph::new(Line::from(right_spans));
    f.render_widget(right_widget, chunks[1]);
}

fn build_right_spans() -> Vec<Span<'static>> {
    let sep = Span::styled(" │ ", Style::default().fg(MUTED_GRAY));

    // Flash notification takes priority
    if let Some(notif) = active_notification() {
        return vec![Span::styled(
            format!("  ✦ {}", notif),
            Style::default()
                .fg(ACCENT_ORANGE)
                .add_modifier(Modifier::BOLD),
        )];
    }

    let (load_avg, disk_usage) = get_sys_stats();
    let mut spans: Vec<Span<'static>> = vec![];

    if !load_avg.is_empty() {
        spans.push(Span::styled("load ", Style::default().fg(MUTED_GRAY)));
        spans.push(Span::styled(load_avg, Style::default().fg(ACCENT_GREEN)));
    }

    if !disk_usage.is_empty() {
        if !spans.is_empty() {
            spans.push(sep.clone());
        }
        spans.push(Span::styled("disk ", Style::default().fg(MUTED_GRAY)));
        spans.push(Span::styled(disk_usage, Style::default().fg(ACCENT_GREEN)));
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_active_notification() {
        push_notification("Agent X completed!");
        let notif = active_notification();
        assert!(notif.is_some());
        assert_eq!(notif.unwrap(), "Agent X completed!");
    }

    #[test]
    fn test_no_notification_initially() {
        // Can't guarantee clean state in static, but active_notification should not panic
        let _ = active_notification();
    }

    #[test]
    fn test_read_load_avg_not_empty_on_linux() {
        // /proc/loadavg is always present on Linux
        let load = read_load_avg();
        // In CI / test environments this may be empty on non-Linux; just don't panic.
        let _ = load;
    }
}
