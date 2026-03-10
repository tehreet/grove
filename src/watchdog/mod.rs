//! Watchdog daemon — health monitoring for running agents.
//!
//! Tier-0: mechanical health poll loop that detects stale and zombie agents,
//! auto-nudges stalled agents, and auto-kills zombies.
#![allow(dead_code)]

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::db::sessions::SessionStore;
use crate::types::{AgentState, WatchdogConfig};

// ---------------------------------------------------------------------------
// Agent health
// ---------------------------------------------------------------------------

/// Result of a single health check for one session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    /// Agent is alive and active.
    Alive,
    /// No activity for staleThresholdMs; should nudge.
    Stale,
    /// No activity for zombieThresholdMs; should kill.
    Zombie,
    /// PID is gone or tmux session is dead.
    Dead,
}

/// Check whether a PID is alive on Linux via /proc.
pub fn is_pid_alive(pid: i64) -> bool {
    std::fs::metadata(format!("/proc/{pid}")).is_ok()
}


/// Determine the health status of an agent session.
///
/// An agent is considered:
/// - Dead: if its PID is gone or tmux session is dead (for tmux-based agents)
/// - Zombie: if last_activity is older than zombie_threshold_ms
/// - Stale: if last_activity is older than stale_threshold_ms
/// - Alive: otherwise
pub fn check_health(
    session: &crate::types::AgentSession,
    config: &WatchdogConfig,
    now_ms: u64,
) -> HealthStatus {
    // Check PID liveness (grove uses direct process spawning, no tmux)
    if let Some(pid) = session.pid {
        if !is_pid_alive(pid) {
            return HealthStatus::Dead;
        }
    }

    // Parse last activity timestamp
    let last_activity_ms = chrono::DateTime::parse_from_rfc3339(&session.last_activity)
        .map(|dt| dt.timestamp_millis() as u64)
        .unwrap_or(0);

    let idle_ms = now_ms.saturating_sub(last_activity_ms);

    if idle_ms >= config.zombie_threshold_ms {
        HealthStatus::Zombie
    } else if idle_ms >= config.stale_threshold_ms {
        HealthStatus::Stale
    } else {
        HealthStatus::Alive
    }
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/// Send a nudge to a stalled agent via `grove nudge`.
pub fn nudge_agent(agent_name: &str, project_root: &Path) -> Result<(), String> {
    let status = Command::new("grove")
        .args(["nudge", agent_name, "--message", "Watchdog: you appear stalled. Please check your mail and continue working."])
        .current_dir(project_root)
        .status()
        .map_err(|e| format!("Failed to run grove nudge: {e}"))?;
    if !status.success() {
        return Err(format!("grove nudge {agent_name} failed"));
    }
    Ok(())
}

/// Kill a zombie agent (mark as zombie in DB, kill PID/tmux).
pub fn kill_agent(
    session: &crate::types::AgentSession,
    store: &SessionStore,
    project_root: &Path,
) -> Result<(), String> {
    // Update state to zombie
    store
        .update_state(&session.agent_name, AgentState::Zombie)
        .map_err(|e| e.to_string())?;

    // Kill PID
    if let Some(pid) = session.pid {
        if is_pid_alive(pid) {
            let _ = Command::new("kill")
                .args(["-15", &pid.to_string()])
                .output();
        }
    }



    // Remove worktree if it exists
    if !session.worktree_path.is_empty() {
        let worktree_path = Path::new(&session.worktree_path);
        if worktree_path.exists() {
            let _ = Command::new("git")
                .args(["worktree", "remove", "--force", &session.worktree_path])
                .current_dir(project_root)
                .output();
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tier-0 poll loop
// ---------------------------------------------------------------------------

/// Run one tick of the tier-0 watchdog poll.
///
/// Scans all active sessions, checks health, and takes action:
/// - Stale → nudge
/// - Zombie → kill
/// - Dead → mark completed (the agent exited on its own)
pub fn poll_once(
    store: &SessionStore,
    config: &WatchdogConfig,
    project_root: &Path,
    now_ms: u64,
) -> Vec<(String, HealthStatus)> {
    let active = match store.get_active() {
        Ok(sessions) => sessions,
        Err(_) => return vec![],
    };

    let mut results = Vec::new();

    for session in &active {
        // Only monitor working sessions
        if session.state != AgentState::Working && session.state != AgentState::Stalled {
            continue;
        }

        let health = check_health(session, config, now_ms);

        match health {
            HealthStatus::Alive => {}
            HealthStatus::Stale => {
                let _ = store.update_state(&session.agent_name, AgentState::Stalled);
                if config.tier0_enabled {
                    let _ = nudge_agent(&session.agent_name, project_root);
                }
                results.push((session.agent_name.clone(), HealthStatus::Stale));
            }
            HealthStatus::Zombie => {
                if config.tier0_enabled {
                    let _ = kill_agent(session, store, project_root);
                }
                results.push((session.agent_name.clone(), HealthStatus::Zombie));
            }
            HealthStatus::Dead => {
                let _ = store.update_state(&session.agent_name, AgentState::Completed);
                results.push((session.agent_name.clone(), HealthStatus::Dead));
            }
        }
    }

    results
}

/// Run the tier-0 watchdog poll loop indefinitely.
///
/// Polls every `config.tier0_interval_ms` milliseconds.
pub fn run_tier0(store: &SessionStore, config: &WatchdogConfig, project_root: &Path) {
    if !config.tier0_enabled {
        return;
    }

    let interval = Duration::from_millis(config.tier0_interval_ms);

    loop {
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        poll_once(store, config, project_root, now_ms);
        std::thread::sleep(interval);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AgentSession, AgentState, WatchdogConfig};

    fn default_config() -> WatchdogConfig {
        WatchdogConfig {
            tier0_enabled: true,
            tier0_interval_ms: 30_000,
            tier1_enabled: false,
            tier2_enabled: false,
            stale_threshold_ms: 300_000,
            zombie_threshold_ms: 600_000,
            nudge_interval_ms: 60_000,
        }
    }

    fn make_session(agent_name: &str, last_activity: &str) -> AgentSession {
        AgentSession {
            id: "test-id".to_string(),
            agent_name: agent_name.to_string(),
            capability: "builder".to_string(),
            worktree_path: "/tmp/wt".to_string(),
            branch_name: "test-branch".to_string(),
            task_id: "task-001".to_string(),
            tmux_session: String::new(),
            state: AgentState::Working,
            pid: None,
            parent_agent: None,
            depth: 1,
            run_id: None,
            started_at: "2025-01-01T00:00:00Z".to_string(),
            last_activity: last_activity.to_string(),
            escalation_level: 0,
            stalled_since: None,
            transcript_path: None,
        }
    }

    #[test]
    fn test_check_health_alive() {
        let config = default_config();
        // Recent activity = alive
        let now = chrono::Utc::now();
        let recent = (now - chrono::Duration::seconds(10)).to_rfc3339();
        let session = make_session("agent-a", &recent);
        let now_ms = now.timestamp_millis() as u64;
        assert_eq!(check_health(&session, &config, now_ms), HealthStatus::Alive);
    }

    #[test]
    fn test_check_health_stale() {
        let config = default_config();
        let now = chrono::Utc::now();
        // 400 seconds ago = > stale_threshold (300s) but < zombie (600s)
        let old = (now - chrono::Duration::seconds(400)).to_rfc3339();
        let session = make_session("agent-b", &old);
        let now_ms = now.timestamp_millis() as u64;
        assert_eq!(check_health(&session, &config, now_ms), HealthStatus::Stale);
    }

    #[test]
    fn test_check_health_zombie() {
        let config = default_config();
        let now = chrono::Utc::now();
        // 700 seconds ago = > zombie_threshold (600s)
        let very_old = (now - chrono::Duration::seconds(700)).to_rfc3339();
        let session = make_session("agent-c", &very_old);
        let now_ms = now.timestamp_millis() as u64;
        assert_eq!(check_health(&session, &config, now_ms), HealthStatus::Zombie);
    }

    #[test]
    fn test_is_pid_alive_self() {
        assert!(is_pid_alive(std::process::id() as i64));
    }

    #[test]
    fn test_is_pid_alive_nonexistent() {
        assert!(!is_pid_alive(999_999_999));
    }

    
    }
