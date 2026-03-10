//! Watchdog triage — AI-based failure classification for stalled agents.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Verdict from the triage LLM call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriageVerdict {
    /// Agent is recoverable — extend timeout and nudge.
    Recoverable,
    /// Agent has fatally failed — kill it.
    Fatal,
    /// Agent is doing long-running work — extend timeout significantly.
    LongRunning,
    /// Could not determine — treat as recoverable (safe default).
    Unknown,
}

/// Read the last N lines from a log file.
fn read_log_tail(log_path: &Path, lines: usize) -> Option<String> {
    let content = std::fs::read_to_string(log_path).ok()?;
    let collected: Vec<&str> = content.lines().collect();
    let start = collected.len().saturating_sub(lines);
    Some(collected[start..].join("\n"))
}

/// Find the agent's log file (stdout or stderr fallback).
pub fn find_agent_log(project_root: &Path, agent_name: &str) -> Option<PathBuf> {
    let logs_base = project_root.join(".overstory/logs").join(agent_name);
    if !logs_base.exists() {
        return None;
    }

    let mut entries: Vec<_> = std::fs::read_dir(&logs_base)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .collect();
    entries.sort_by_key(|entry| entry.file_name());
    let latest = entries.last()?;

    let stdout = latest.path().join("stdout.log");
    let stderr = latest.path().join("stderr.log");

    if stdout.exists() && stdout.metadata().ok()?.len() > 0 {
        Some(stdout)
    } else if stderr.exists() {
        Some(stderr)
    } else {
        None
    }
}

/// Run triage: read agent log, call LLM, return verdict.
pub fn triage_agent(agent_name: &str, project_root: &Path, print_cmd: &[String]) -> TriageVerdict {
    let log_path = match find_agent_log(project_root, agent_name) {
        Some(path) => path,
        None => return TriageVerdict::Unknown,
    };

    let log_tail = match read_log_tail(&log_path, 50) {
        Some(tail) if !tail.trim().is_empty() => tail,
        _ => return TriageVerdict::Unknown,
    };

    let prompt = format!(
        "You are a watchdog for an AI coding agent. The agent appears stalled. \
        Based on the last 50 lines of its log output, classify its state.\n\
        Respond with EXACTLY one word: 'recoverable', 'fatal', or 'long_running'.\n\
        - recoverable: agent hit a temporary issue, can be nudged to continue\n\
        - fatal: agent is stuck in a loop, has an unrecoverable error, or cannot proceed\n\
        - long_running: agent is doing legitimate slow work (compiling, downloading, etc)\n\n\
        Agent: {agent_name}\n\
        Log tail:\n{log_tail}"
    );

    if print_cmd.is_empty() {
        return TriageVerdict::Unknown;
    }

    let mut argv = print_cmd.to_vec();
    if let Some(last) = argv.last_mut() {
        *last = prompt;
    }

    let output = match Command::new(&argv[0]).args(&argv[1..]).output() {
        Ok(output) => output,
        Err(_) => return TriageVerdict::Unknown,
    };

    if !output.status.success() {
        return TriageVerdict::Unknown;
    }

    let response = String::from_utf8_lossy(&output.stdout).to_lowercase();
    let response = response.trim();

    if response.contains("fatal") {
        TriageVerdict::Fatal
    } else if response.contains("long_running") || response.contains("long-running") {
        TriageVerdict::LongRunning
    } else if response.contains("recoverable") {
        TriageVerdict::Recoverable
    } else {
        TriageVerdict::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_read_log_tail_nonexistent() {
        assert!(read_log_tail(Path::new("/nonexistent/log.txt"), 10).is_none());
    }

    #[test]
    fn test_read_log_tail_returns_last_n() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.log");
        let content = (1..=20)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&path, &content).unwrap();
        let tail = read_log_tail(&path, 5).unwrap();
        assert!(tail.contains("line 20"));
        assert!(!tail.contains("line 1\n"));
    }

    #[test]
    fn test_find_agent_log_missing() {
        assert!(find_agent_log(Path::new("/tmp/nonexistent"), "fake-agent").is_none());
    }

    #[test]
    fn test_find_agent_log_prefers_nonempty_stdout() {
        let dir = TempDir::new().unwrap();
        let log_dir = dir.path().join(".overstory/logs/agent-a/20260310T000000");
        std::fs::create_dir_all(&log_dir).unwrap();
        std::fs::write(log_dir.join("stdout.log"), "work").unwrap();
        std::fs::write(log_dir.join("stderr.log"), "err").unwrap();

        let path = find_agent_log(dir.path(), "agent-a").unwrap();
        assert!(path.ends_with("stdout.log"));
    }

    #[test]
    fn test_triage_unknown_when_no_log() {
        let verdict = triage_agent(
            "fake-agent",
            Path::new("/tmp/nonexistent"),
            &["echo".to_string(), "recoverable".to_string()],
        );
        assert_eq!(verdict, TriageVerdict::Unknown);
    }

    #[test]
    fn test_triage_detects_recoverable() {
        let dir = TempDir::new().unwrap();
        let log_dir = dir.path().join(".overstory/logs/agent-b/20260310T000000");
        std::fs::create_dir_all(&log_dir).unwrap();
        std::fs::write(log_dir.join("stdout.log"), "temporary network error").unwrap();

        let verdict = triage_agent(
            "agent-b",
            dir.path(),
            &[
                "sh".to_string(),
                "-c".to_string(),
                "printf recoverable".to_string(),
                "prompt-placeholder".to_string(),
            ],
        );

        assert_eq!(verdict, TriageVerdict::Recoverable);
    }
}
