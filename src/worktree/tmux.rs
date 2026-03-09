#![allow(dead_code)]

use std::path::Path;
use std::process::Command;

/// Run a command, return (stdout, stderr, exit_code).
fn run_command(cmd: &str, args: &[&str]) -> Result<(String, String, i32), String> {
    let output = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to execute {cmd}: {e}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let code = output.status.code().unwrap_or(-1);
    Ok((stdout, stderr, code))
}

/// Parsed tmux session info.
#[derive(Debug, Clone)]
pub struct TmuxSession {
    pub name: String,
    pub pid: u32,
}

/// Create a detached tmux session running a command.
/// Runs: tmux new-session -d -s <name> -c <cwd> <command>
/// Returns the pane PID via: tmux list-panes -t <name> -F "#{pane_pid}"
pub fn create_session(name: &str, cwd: &Path, command: &str) -> Result<u32, String> {
    let cwd_str = cwd
        .to_str()
        .ok_or_else(|| "Invalid cwd path".to_string())?;
    let (_, stderr, code) = run_command(
        "tmux",
        &["new-session", "-d", "-s", name, "-c", cwd_str, command],
    )?;
    if code != 0 {
        return Err(format!("tmux new-session failed (exit {code}): {stderr}"));
    }
    let (stdout, stderr, code) = run_command(
        "tmux",
        &["list-panes", "-t", name, "-F", "#{pane_pid}"],
    )?;
    if code != 0 {
        return Err(format!("tmux list-panes failed (exit {code}): {stderr}"));
    }
    let pid_str = stdout.trim();
    pid_str
        .parse::<u32>()
        .map_err(|e| format!("Failed to parse pane PID '{pid_str}': {e}"))
}

/// Kill a tmux session by name.
/// Runs: tmux kill-session -t <name>
/// If session is already gone ("session not found"), returns Ok silently.
pub fn kill_session(name: &str) -> Result<(), String> {
    let (_, stderr, code) = run_command("tmux", &["kill-session", "-t", name])?;
    if code != 0 {
        if stderr.contains("session not found") || stderr.contains("no server running") {
            return Ok(());
        }
        return Err(format!("tmux kill-session failed (exit {code}): {stderr}"));
    }
    Ok(())
}

/// Send keys to a tmux session.
/// Flattens newlines to spaces (prevents embedded Enter keystrokes).
/// Runs: tmux send-keys -t <name> <keys> Enter
pub fn send_keys(name: &str, keys: &str) -> Result<(), String> {
    let flat_keys = keys.replace('\n', " ");
    let (_, stderr, code) = run_command("tmux", &["send-keys", "-t", name, &flat_keys, "Enter"])?;
    if code != 0 {
        return Err(format!("tmux send-keys failed (exit {code}): {stderr}"));
    }
    Ok(())
}

/// Capture the visible content of a tmux pane.
/// Runs: tmux capture-pane -t <name> -p -S -<lines>
/// Returns None if capture fails.
pub fn capture_pane(name: &str, lines: u32) -> Option<String> {
    let lines_arg = format!("-{lines}");
    let result = run_command(
        "tmux",
        &["capture-pane", "-t", name, "-p", "-S", &lines_arg],
    );
    match result {
        Ok((stdout, _, 0)) => Some(stdout),
        _ => None,
    }
}

/// Check if a tmux session is alive.
/// Runs: tmux has-session -t <name>
/// Returns true if exit code is 0.
pub fn is_session_alive(name: &str) -> bool {
    match run_command("tmux", &["has-session", "-t", name]) {
        Ok((_, _, code)) => code == 0,
        Err(_) => false,
    }
}

/// List all active tmux sessions.
/// Runs: tmux list-sessions -F "#{session_name}:#{pid}"
/// Returns empty vec if no server running.
pub fn list_sessions() -> Result<Vec<TmuxSession>, String> {
    let result = run_command("tmux", &["list-sessions", "-F", "#{session_name}:#{pid}"]);
    match result {
        Ok((stdout, stderr, code)) => {
            if code != 0 {
                // No server running or no sessions — return empty
                if stderr.contains("no server running")
                    || stderr.contains("no sessions")
                    || stdout.trim().is_empty()
                {
                    return Ok(vec![]);
                }
                return Err(format!(
                    "tmux list-sessions failed (exit {code}): {stderr}"
                ));
            }
            let mut sessions = Vec::new();
            for line in stdout.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                if let Some(colon) = line.rfind(':') {
                    let name = line[..colon].to_string();
                    let pid_str = &line[colon + 1..];
                    if let Ok(pid) = pid_str.parse::<u32>() {
                        sessions.push(TmuxSession { name, pid });
                    }
                }
            }
            Ok(sessions)
        }
        Err(e) => {
            // tmux not available or no server
            if e.contains("no server running") || e.contains("Failed to execute tmux") {
                return Ok(vec![]);
            }
            Err(e)
        }
    }
}

/// Get the current tmux session name (if running inside tmux).
/// Checks TMUX env var first, then runs: tmux display-message -p "#{session_name}"
pub fn current_session_name() -> Option<String> {
    if std::env::var("TMUX").is_err() {
        return None;
    }
    match run_command("tmux", &["display-message", "-p", "#{session_name}"]) {
        Ok((stdout, _, 0)) => {
            let name = stdout.trim().to_string();
            if name.is_empty() {
                None
            } else {
                Some(name)
            }
        }
        _ => None,
    }
}

/// Verify tmux is installed and executable.
/// Runs: tmux -V
/// Returns Err if tmux is not available.
pub fn ensure_tmux_available() -> Result<(), String> {
    match run_command("tmux", &["-V"]) {
        Ok((_, _, 0)) => Ok(()),
        Ok((_, stderr, code)) => Err(format!("tmux -V failed (exit {code}): {stderr}")),
        Err(e) => Err(format!("tmux not available: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ensure_tmux_available() {
        let result = ensure_tmux_available();
        // If tmux isn't installed, skip gracefully
        if result.is_err() {
            eprintln!("tmux not available, skipping test");
        }
        // Just assert it doesn't panic
    }

    #[test]
    fn test_list_sessions() {
        let result = list_sessions();
        assert!(result.is_ok(), "list_sessions() should not error: {:?}", result);
    }

    #[test]
    fn test_is_session_alive_nonexistent() {
        assert!(
            !is_session_alive("nonexistent-session-xyz"),
            "Nonexistent session should not be alive"
        );
    }

    #[test]
    fn test_capture_pane_nonexistent() {
        let result = capture_pane("nonexistent-session-xyz", 50);
        assert!(result.is_none(), "capture_pane should return None for nonexistent session");
    }

    #[test]
    fn test_current_session_name() {
        // Just verify it doesn't panic
        let _name = current_session_name();
    }
}
