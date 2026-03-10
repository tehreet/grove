//! `grove status` — system status overview.
//!
//! Shows agents, worktrees, tmux sessions, and summary counts.

use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::{DateTime, Utc};
use colored::Colorize;
use serde::Serialize;

use crate::config::load_config;
use crate::db::mail::MailStore;
use crate::db::merge_queue::MergeQueue;
use crate::types::MergeEntryStatus;
use crate::db::metrics::MetricsStore;
use crate::db::sessions::{RunStore, SessionStore};
use crate::json::json_output;
use crate::logging::{brand_bold, muted};
use crate::types::{AgentSession, AgentState, MailFilters};

// ---------------------------------------------------------------------------
// Output types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeInfo {
    pub path: String,
    pub head: String,
    pub branch: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TmuxSessionInfo {
    pub name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StatusOutput {
    current_run_id: Option<String>,
    agents: Vec<AgentSession>,
    worktrees: Vec<WorktreeInfo>,
    tmux_sessions: Vec<TmuxSessionInfo>,
    unread_mail_count: usize,
    merge_queue_count: usize,
    recent_metrics_count: i64,
}

// ---------------------------------------------------------------------------
// Execute
// ---------------------------------------------------------------------------

pub fn execute(
    agent: Option<String>,
    run: Option<String>,
    _compact: bool,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let config = load_config(&cwd, project_override).map_err(|e| e.to_string())?;
    let root = &config.project.root;
    let overstory = format!("{root}/.overstory");

    // --- Sessions ---
    let sessions_db = format!("{overstory}/sessions.db");
    let mut sessions: Vec<AgentSession> = if PathBuf::from(&sessions_db).exists() {
        let store = SessionStore::new(&sessions_db).map_err(|e| e.to_string())?;
        if let Some(ref run_id) = run {
            store.get_by_run(run_id).map_err(|e| e.to_string())?
        } else {
            store.get_all().map_err(|e| e.to_string())?
        }
    } else {
        vec![]
    };

    // Apply agent name filter
    if let Some(ref name) = agent {
        sessions.retain(|s| &s.agent_name == name);
    }

    // Sort by state priority: working=0, booting=1, stalled=2, zombie=3, completed=4
    sessions.sort_by_key(|s| state_priority(&s.state));

    // --- Current run ---
    let current_run_id: Option<String> = if PathBuf::from(&sessions_db).exists() {
        let run_store = RunStore::new(&sessions_db).map_err(|e| e.to_string())?;
        run_store.get_active_run()
            .ok()
            .flatten()
            .map(|r| r.id)
            .or_else(|| run.clone())
    } else {
        run.clone()
    };

    // --- Git worktrees ---
    let worktrees = list_git_worktrees(root);

    // --- Tmux sessions ---
    let tmux_sessions = list_tmux_sessions();

    // --- Unread mail count ---
    let mail_db = format!("{overstory}/mail.db");
    let unread_mail_count: usize = if PathBuf::from(&mail_db).exists() {
        MailStore::new(&mail_db)
            .ok()
            .and_then(|store| {
                store.get_all(Some(MailFilters {
                    to_agent: Some("orchestrator".to_string()),
                    unread: Some(true),
                    ..Default::default()
                })).ok()
            })
            .map(|msgs| msgs.len())
            .unwrap_or(0)
    } else {
        0
    };

    // --- Merge queue count ---
    let mq_db = format!("{overstory}/merge-queue.db");
    let merge_queue_count: usize = if PathBuf::from(&mq_db).exists() {
        MergeQueue::new(&mq_db)
            .ok()
            .and_then(|q| q.list(Some(MergeEntryStatus::Pending)).ok())
            .map(|entries| entries.len())
            .unwrap_or(0)
    } else {
        0
    };

    // --- Recent metrics count ---
    let metrics_db = format!("{overstory}/metrics.db");
    let recent_metrics_count: i64 = if PathBuf::from(&metrics_db).exists() {
        MetricsStore::new(&metrics_db)
            .ok()
            .and_then(|store| store.count_sessions().ok())
            .unwrap_or(0)
    } else {
        0
    };

    if json {
        let output = StatusOutput {
            current_run_id: current_run_id.clone(),
            agents: sessions.clone(),
            worktrees: worktrees.clone(),
            tmux_sessions: tmux_sessions.clone(),
            unread_mail_count,
            merge_queue_count,
            recent_metrics_count,
        };
        println!("{}", json_output("status", &output));
    } else {
        print_status_text(
            &sessions,
            &worktrees,
            &tmux_sessions,
            current_run_id.as_deref(),
            unread_mail_count,
            merge_queue_count,
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Text formatting
// ---------------------------------------------------------------------------

const SEPARATOR: &str = "──────────────────────────────────────────────────────────────────────";

fn print_status_text(
    sessions: &[AgentSession],
    worktrees: &[WorktreeInfo],
    tmux_sessions: &[TmuxSessionInfo],
    current_run_id: Option<&str>,
    unread_mail_count: usize,
    merge_queue_count: usize,
) {
    println!("{}", brand_bold("Overstory Status"));
    println!("{}", muted(SEPARATOR));

    // Run header line
    let now = Utc::now();
    let time_str = now.format("%H:%M:%S").to_string();
    if let Some(run_id) = current_run_id {
        let agent_count = sessions.len();
        let total_cost: f64 = sessions.iter()
            .filter_map(|_s| None::<f64>) // cost from sessions.db not available
            .sum();
        let cost_str = if total_cost > 0.0 {
            format!("${:.2}", total_cost)
        } else {
            String::new()
        };

        let run_display = if run_id.len() > 30 {
            format!("{}...", &run_id[..27])
        } else {
            run_id.to_string()
        };

        let mut header = format!("Run: {} │ {} agents", run_display, agent_count);
        if !cost_str.is_empty() {
            header.push_str(&format!(" │ {}", cost_str));
        }
        header.push_str(&format!(" │ {}", time_str));
        println!("{}", muted(&header));
    } else {
        println!("{}", muted(&format!("No active run │ {}", time_str)));
    }
    println!();

    // Agent rows
    if sessions.is_empty() {
        println!("{}", muted("  No agents found"));
    } else {
        for session in sessions {
            println!("{}", format_agent_row(session));
        }
    }

    println!();
    println!(
        "{}",
        muted(&format!(
            "Worktrees: {} │ Tmux: {} │ Unread mail: {} │ Merge queue: {}",
            worktrees.len(),
            tmux_sessions.len(),
            unread_mail_count,
            merge_queue_count,
        ))
    );
}

fn format_agent_row(session: &AgentSession) -> String {
    let icon = state_icon(&session.state);
    let state_str = state_str(&session.state);
    let duration = compute_duration(session);

    // Fixed-width columns
    let name = pad_right(&session.agent_name, 20);
    let cap = pad_right(&session.capability, 10);
    let state_col = pad_right(state_str, 10);
    let task = pad_right(&session.task_id, 14);
    let dur = pad_right(&duration, 10);

    let row = format!("  {} {}  {}  {}  {}{}", icon, name, cap, state_col, task, dur);

    // Color by state
    match session.state {
        AgentState::Working => format!("{}", row.green()),
        AgentState::Stalled | AgentState::Zombie => format!("{}", row.yellow()),
        AgentState::Completed => format!("{}", row.dimmed()),
        AgentState::Booting => row,
    }
}

fn pad_right(s: &str, width: usize) -> String {
    if s.len() >= width {
        s[..width].to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - s.len()))
    }
}

fn state_icon(state: &AgentState) -> &'static str {
    match state {
        AgentState::Working => ">",
        AgentState::Booting => "~",
        AgentState::Stalled | AgentState::Zombie => "!",
        AgentState::Completed => " ",
    }
}

fn state_str(state: &AgentState) -> &'static str {
    match state {
        AgentState::Working => "working",
        AgentState::Booting => "booting",
        AgentState::Stalled => "stalled",
        AgentState::Zombie => "zombie",
        AgentState::Completed => "completed",
    }
}

fn state_priority(state: &AgentState) -> u8 {
    match state {
        AgentState::Working => 0,
        AgentState::Booting => 1,
        AgentState::Stalled => 2,
        AgentState::Zombie => 3,
        AgentState::Completed => 4,
    }
}

fn compute_duration(session: &AgentSession) -> String {
    let start = DateTime::parse_from_rfc3339(&session.started_at)
        .map(|dt| dt.with_timezone(&Utc));
    let end = match session.state {
        AgentState::Completed | AgentState::Zombie => {
            DateTime::parse_from_rfc3339(&session.last_activity)
                .map(|dt| dt.with_timezone(&Utc))
                .ok()
        }
        _ => Some(Utc::now()),
    };

    match (start, end) {
        (Ok(s), Some(e)) => {
            let secs = e.signed_duration_since(s).num_seconds().max(0) as u64;
            format_duration_mmss(secs)
        }
        _ => "?".to_string(),
    }
}

fn format_duration_mmss(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        format!("{}m {:02}s", secs / 60, secs % 60)
    } else {
        format!("{}h {:02}m", secs / 3600, (secs % 3600) / 60)
    }
}

// ---------------------------------------------------------------------------
// Git worktrees
// ---------------------------------------------------------------------------

fn list_git_worktrees(project_root: &str) -> Vec<WorktreeInfo> {
    let output = Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(project_root)
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return vec![],
    };

    let text = String::from_utf8_lossy(&output.stdout);
    parse_git_worktree_list(&text)
}

/// Parse `git worktree list --porcelain` output.
///
/// Each worktree block is separated by a blank line:
/// ```
/// worktree /path/to/repo
/// HEAD abc123
/// branch refs/heads/main
///
/// worktree /path/to/worktree
/// HEAD def456
/// branch refs/heads/feature
/// ```
pub fn parse_git_worktree_list(text: &str) -> Vec<WorktreeInfo> {
    let mut result = Vec::new();
    let mut path: Option<String> = None;
    let mut head: Option<String> = None;
    let mut branch: Option<String> = None;

    for line in text.lines() {
        if line.is_empty() {
            // End of block — flush
            if let (Some(p), Some(h)) = (path.take(), head.take()) {
                result.push(WorktreeInfo {
                    path: p,
                    head: h,
                    branch: branch.take().unwrap_or_else(|| "(detached)".to_string()),
                });
            }
            branch = None;
        } else if let Some(p) = line.strip_prefix("worktree ") {
            path = Some(p.to_string());
        } else if let Some(h) = line.strip_prefix("HEAD ") {
            head = Some(h.to_string());
        } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
            branch = Some(b.to_string());
        } else if let Some(b) = line.strip_prefix("branch ") {
            branch = Some(b.to_string());
        }
    }

    // Final block (no trailing blank line)
    if let (Some(p), Some(h)) = (path.take(), head.take()) {
        result.push(WorktreeInfo {
            path: p,
            head: h,
            branch: branch.unwrap_or_else(|| "(detached)".to_string()),
        });
    }

    result
}

// ---------------------------------------------------------------------------
// Tmux sessions
// ---------------------------------------------------------------------------

fn list_tmux_sessions() -> Vec<TmuxSessionInfo> {
    let output = Command::new("tmux")
        .args(["list-sessions", "-F", "#{session_name}"])
        .output();

    match output {
        Ok(o) if o.status.success() => {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .map(|name| TmuxSessionInfo { name: name.to_string() })
                .collect()
        }
        _ => vec![],
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_priority_order() {
        assert!(state_priority(&AgentState::Working) < state_priority(&AgentState::Booting));
        assert!(state_priority(&AgentState::Booting) < state_priority(&AgentState::Stalled));
        assert!(state_priority(&AgentState::Stalled) < state_priority(&AgentState::Zombie));
        assert!(state_priority(&AgentState::Zombie) < state_priority(&AgentState::Completed));
    }

    #[test]
    fn test_format_duration_mmss() {
        assert_eq!(format_duration_mmss(0), "0s");
        assert_eq!(format_duration_mmss(45), "45s");
        assert_eq!(format_duration_mmss(90), "1m 30s");
        assert_eq!(format_duration_mmss(3661), "1h 01m");
    }

    #[test]
    fn test_parse_git_worktree_list_basic() {
        let input = "worktree /home/user/project\nHEAD abc123\nbranch refs/heads/main\n\nworktree /home/user/project/.overstory/worktrees/agent\nHEAD def456\nbranch refs/heads/feature\n\n";
        let wts = parse_git_worktree_list(input);
        assert_eq!(wts.len(), 2);
        assert_eq!(wts[0].path, "/home/user/project");
        assert_eq!(wts[0].branch, "main");
        assert_eq!(wts[1].branch, "feature");
    }

    #[test]
    fn test_parse_git_worktree_list_detached() {
        let input = "worktree /home/user/project\nHEAD abc123\ndetached\n\n";
        let wts = parse_git_worktree_list(input);
        assert_eq!(wts.len(), 1);
        assert_eq!(wts[0].branch, "(detached)");
    }

    #[test]
    fn test_parse_git_worktree_list_no_trailing_newline() {
        let input = "worktree /home/user/project\nHEAD abc123\nbranch refs/heads/main";
        let wts = parse_git_worktree_list(input);
        assert_eq!(wts.len(), 1);
        assert_eq!(wts[0].branch, "main");
    }

    #[test]
    fn test_pad_right() {
        assert_eq!(pad_right("abc", 5), "abc  ");
        assert_eq!(pad_right("abcdef", 3), "abc");
    }

    #[test]
    fn test_execute_no_overstory_dir() {
        // Should not panic even without .overstory/
        let result = execute(None, None, false, false, Some(Path::new("/tmp")));
        // Either succeeds or returns an error — just shouldn't panic
        let _ = result;
    }

    #[test]
    fn test_execute_json_no_overstory_dir() {
        let result = execute(None, None, false, true, Some(Path::new("/tmp")));
        let _ = result;
    }
}
