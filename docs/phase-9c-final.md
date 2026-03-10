# Phase 9C: AI-Assisted Merge Tiers 3-4 + Watchdog Triage

## Prerequisites
Phase 9E must be complete first: `AgentRuntime` trait must have `build_print_command`.

## Goal
Add LLM-powered merge conflict resolution (tiers 3 and 4) and AI-based watchdog triage.

## File Scope

- `src/merge/resolver.rs` — add tier 3 (AI resolve) and tier 4 (reimagine)
- `src/watchdog/mod.rs` — add `src/watchdog/triage.rs`, wire tier 1 logic into poll_once
- `src/watchdog/triage.rs` — NEW FILE: AI failure classification

DO NOT modify main.rs, config.rs, types.rs, or any runtime adapter files.

## Changes

### 1. `src/merge/resolver.rs` — Add Tier 3 + Tier 4

After the existing tier 2 auto-resolve section, add:

**Tier 3 — AI Resolve:**

The resolver already has `MergeResolverOptions` with `ai_resolve_enabled` and `reimagine_enabled`. Use them.

Add a function `try_ai_resolve(conflict_files, repo_root, print_cmd_builder) -> Result<AutoResolveResult, String>` where `print_cmd_builder` is a `&dyn Fn(&str) -> Vec<String>`.

For each conflict file:
1. Read the file content (which has conflict markers)
2. Build a prompt: "You are a merge resolver. The following file has git conflict markers. Resolve the conflicts by keeping the best of both sides. Return ONLY the resolved file content, no explanation, no markdown fences, just the raw file content.\n\nFile: {filename}\n\n{content}"  
3. Run the print command via `std::process::Command::new(&argv[0]).args(&argv[1..]).output()`
4. Use the stdout as the resolved content (trim trailing whitespace)
5. Verify the resolved content has no conflict markers (`!content.contains("<<<<<<< ")`)
6. Write the resolved content back and `git add` the file

**Tier 4 — Reimagine:**

Similar to tier 3, but with a different prompt that reads BOTH full branch versions and asks LLM to rewrite:
1. Get the canonical version: `git show HEAD:{file}`
2. Get the incoming version: `git show {branch}:{file}` (use entry.branch_name)
3. Prompt: "You are merging two versions of a file. Rewrite it to incorporate all changes from both versions. Return ONLY the file content.\n\nCANONICAL VERSION:\n{canonical}\n\nINCOMING VERSION:\n{incoming}"
4. Same write-back and verify logic

**Wiring in `MergeResolver::resolve`:**

After tier 2 fails (when `!tier2.success`), check options and add:

```rust
// 7. Tier 3: AI resolve (if enabled)
if self.options.ai_resolve_enabled {
    if let Some(ref rt) = self.print_runtime {
        let tier3 = try_ai_resolve(&tier2.remaining_conflicts, repo_root, rt.as_ref())?;
        if tier3.success {
            // ... return success with ResolutionTier::AiResolve
        }
        // Tier 4: Reimagine (if enabled)
        if self.options.reimagine_enabled {
            let tier4 = try_reimagine(&tier3.remaining_conflicts, repo_root, rt.as_ref(), &entry.branch_name)?;
            if tier4.success {
                // ... return success with ResolutionTier::Reimagine  
            }
        }
    }
}
```

**Update `MergeResolver` struct** to hold an optional print runtime:
```rust
pub struct MergeResolver {
    options: MergeResolverOptions,
    print_runtime: Option<Box<dyn crate::runtimes::AgentRuntime>>,
}

impl MergeResolver {
    pub fn new(options: MergeResolverOptions) -> Self {
        Self { options, print_runtime: None }
    }
    
    pub fn with_runtime(mut self, rt: Box<dyn crate::runtimes::AgentRuntime>) -> Self {
        self.print_runtime = Some(rt);
        self
    }
}
```

**Add to `ResolutionTier` enum in `src/types.rs`** (check if already there first):
If `AiResolve` and `Reimagine` variants don't exist, add them. If they do, don't change them.

**Graceful degradation:** If `build_print_command` process fails (command not found, exit != 0, empty output), log a warning and return `remaining_conflicts` unchanged (fall through to abort).

### 2. `src/watchdog/triage.rs` — NEW FILE

```rust
//! Watchdog triage — AI-based failure classification for stalled agents.

use std::path::Path;
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
pub fn find_agent_log(project_root: &Path, agent_name: &str) -> Option<std::path::PathBuf> {
    let logs_base = project_root.join(".overstory/logs").join(agent_name);
    if !logs_base.exists() { return None; }
    
    // Find latest timestamp directory
    let mut entries: Vec<_> = std::fs::read_dir(&logs_base).ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    entries.sort_by_key(|e| e.file_name());
    let latest = entries.last()?;
    
    let stdout = latest.path().join("stdout.log");
    let stderr = latest.path().join("stderr.log");
    
    // Prefer stdout if non-empty, otherwise stderr
    if stdout.exists() && stdout.metadata().ok()?.len() > 0 {
        Some(stdout)
    } else if stderr.exists() {
        Some(stderr)
    } else {
        None
    }
}

/// Run triage: read agent log, call LLM, return verdict.
pub fn triage_agent(
    agent_name: &str,
    project_root: &Path,
    print_cmd: &[String],
) -> TriageVerdict {
    // Read log tail
    let log_path = match find_agent_log(project_root, agent_name) {
        Some(p) => p,
        None => return TriageVerdict::Unknown,
    };
    
    let log_tail = match read_log_tail(&log_path, 50) {
        Some(t) if !t.trim().is_empty() => t,
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
    
    // Build argv: replace last element (prompt) with our prompt
    if print_cmd.is_empty() { return TriageVerdict::Unknown; }
    let mut argv = print_cmd.to_vec();
    // The last element is the prompt placeholder — replace it
    *argv.last_mut().unwrap() = prompt;
    
    let output = match Command::new(&argv[0]).args(&argv[1..]).output() {
        Ok(o) => o,
        Err(_) => return TriageVerdict::Unknown,
    };
    
    if !output.status.success() { return TriageVerdict::Unknown; }
    
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
        let content = (1..=20).map(|i| format!("line {i}")).collect::<Vec<_>>().join("\n");
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
    fn test_triage_unknown_when_no_log() {
        // No log file -> Unknown verdict
        let verdict = triage_agent("fake-agent", Path::new("/tmp/nonexistent"), &["echo".to_string(), "recoverable".to_string()]);
        assert_eq!(verdict, TriageVerdict::Unknown);
    }
}
```

### 3. `src/watchdog/mod.rs` — Wire Tier 1 (Triage)

At the top of mod.rs, add `pub mod triage;`.

The `WatchdogConfig` already has `tier1_enabled`. In `poll_once`, in the `HealthStatus::Zombie` arm, before calling `kill_agent`, check:

```rust
HealthStatus::Zombie => {
    if config.tier0_enabled {
        // Tier 1: AI triage before killing (if enabled and print_cmd available)
        let should_kill = if config.tier1_enabled {
            if let Some(ref print_cmd) = print_cmd_opt {
                let verdict = triage::triage_agent(&session.agent_name, project_root, print_cmd);
                match verdict {
                    triage::TriageVerdict::LongRunning => {
                        // Extend timeout — don't kill, don't nudge
                        false
                    }
                    triage::TriageVerdict::Recoverable | triage::TriageVerdict::Unknown => {
                        // Nudge instead of kill
                        let _ = nudge_agent(&session.agent_name, project_root);
                        false
                    }
                    triage::TriageVerdict::Fatal => true,
                }
            } else {
                true // No print runtime configured, kill as before
            }
        } else {
            true
        };
        
        if should_kill {
            let _ = kill_agent(session, store, project_root);
        }
    }
    results.push((session.agent_name.clone(), HealthStatus::Zombie));
}
```

Update `poll_once` signature to accept optional print_cmd:
```rust
pub fn poll_once(
    store: &SessionStore,
    config: &WatchdogConfig,
    project_root: &Path,
    now_ms: u64,
    print_cmd_opt: Option<&[String]>,
) -> Vec<(String, HealthStatus)>
```

Update `run_tier0` to pass `None` for print_cmd_opt (backward compat):
```rust
poll_once(store, config, project_root, now_ms, None);
```

## Verification

```bash
. /home/joshf/.cargo/env
cd /home/joshf/grove

# Build + test + clippy (SEQUENTIAL — do not parallelize)
cargo build && cargo test && cargo clippy -- -D warnings

# Verify tier 3/4 code exists
grep -n "try_ai_resolve\|try_reimagine\|AiResolve\|Reimagine" src/merge/resolver.rs

# Verify triage module exists
ls src/watchdog/triage.rs && echo "PASS: triage.rs exists" || echo "FAIL"
grep -n "triage_agent\|TriageVerdict" src/watchdog/triage.rs | head -5

# Verify triage wired in watchdog
grep -n "triage\|tier1" src/watchdog/mod.rs
```

## Acceptance Criteria

1. `src/merge/resolver.rs` attempts AI resolution (tier 3) when `ai_resolve_enabled: true` and a print runtime is configured
2. `src/merge/resolver.rs` attempts reimagine (tier 4) when `reimagine_enabled: true`
3. Both tiers degrade gracefully: if the LLM call fails or returns empty output, fall through to abort (no panic, no crash)
4. `src/watchdog/triage.rs` exists with `triage_agent` function and `TriageVerdict` enum
5. `poll_once` accepts `print_cmd_opt: Option<&[String]>` and uses it for tier 1 triage
6. All existing 453 tests pass + new tests for tier 3/4 logic and triage

## IMPORTANT

- Check `src/types.rs` for existing `ResolutionTier` variants before adding — do NOT duplicate
- Run quality gates SEQUENTIALLY: `cargo build && cargo test && cargo clippy -- -D warnings`
- Check for conflict markers before committing: `grep -rn "<<<<<<" src/`
- Commit: `git add -A && git commit -m "Phase 9C: AI merge tiers 3-4 + watchdog triage"`
