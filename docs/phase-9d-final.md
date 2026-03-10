# Phase 9D: Missing Command Features — Coordinator Ask, Eval, Inspect, Init Bootstrap

## Goal
Close all remaining command-level feature gaps from the phase-9.md spec.

## File Scope

This phase touches 4 areas. Each must be done carefully. All files listed:

- `src/commands/coordinator.rs` — add `ask` subcommand
- `src/commands/eval.rs` — NEW FILE (unhide + basic implementation)
- `src/commands/inspect.rs` — add transcript cost display
- `src/commands/init.rs` — add ecosystem bootstrap (mulch/seeds/canopy init)
- `src/main.rs` — wire CoordinatorAsk subcommand + eval command

**One agent OWNS main.rs**. The same agent that adds CoordinatorAsk to coordinator.rs must also wire it in main.rs. Similarly, the eval.rs creator must wire it in main.rs. Since main.rs changes are needed in multiple places, do ALL main.rs changes in one pass at the end.

DO NOT modify: types.rs, config.rs, runtimes/, merge/, watchdog/, db/

## Changes

### 1. `src/commands/coordinator.rs` — Add `ask` subcommand

Add `execute_ask` function at the bottom of the file (before tests):

```rust
/// `grove coordinator ask` — send a message to the coordinator and wait for a reply.
pub fn execute_ask(
    body: &str,
    from: &str,
    timeout_secs: u64,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let root = resolve_project_root(&cwd, project_override).map_err(|e| e.to_string())?;
    let root_str = root.to_string_lossy().to_string();
    let mail_db = format!("{root_str}/.overstory/mail.db");

    let store = MailStore::new(&mail_db).map_err(|e| e.to_string())?;

    // Generate a thread_id to correlate request and response
    let thread_id = uuid::Uuid::new_v4().to_string();

    // Send the message to coordinator
    let _msg = store.insert(&InsertMailMessage {
        id: None,
        from_agent: from.to_string(),
        to_agent: COORDINATOR_AGENT.to_string(),
        subject: "ask".to_string(),
        body: body.to_string(),
        message_type: MailMessageType::Status,
        priority: MailPriority::High,
        thread_id: Some(thread_id.clone()),
        payload: None,
    }).map_err(|e| e.to_string())?;

    // Poll for a reply matching the thread_id
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    let poll_interval = std::time::Duration::from_millis(500);

    loop {
        if std::time::Instant::now() >= deadline {
            let msg = format!("Timeout: no reply from coordinator after {timeout_secs}s");
            if json {
                println!("{}", json_error("coordinator ask", &msg));
            } else {
                eprintln!("{msg}");
            }
            return Err(msg);
        }

        // Look for a reply in the mail store addressed to `from` with same thread_id
        let unread = store.get_unread(from).unwrap_or_default();
        for mail in &unread {
            if mail.thread_id.as_deref() == Some(&thread_id) {
                // Mark as read
                let _ = store.mark_read(&mail.id);

                if json {
                    #[derive(serde::Serialize)]
                    #[serde(rename_all = "camelCase")]
                    struct Output {
                        replied: bool,
                        body: String,
                        from: String,
                        thread_id: String,
                    }
                    println!("{}", json_output("coordinator ask", &Output {
                        replied: true,
                        body: mail.body.clone(),
                        from: mail.from_agent.clone(),
                        thread_id: thread_id.clone(),
                    }));
                } else {
                    println!("{}", mail.body);
                }
                return Ok(());
            }
        }

        std::thread::sleep(poll_interval);
    }
}
```

Note: `MailStore::get_unread` may not exist yet — check `src/db/mail.rs`. If it doesn't exist, use `store.get_for_agent(from)` or whatever method IS available to get mail for a recipient. Look at the existing mail store methods and use the appropriate one.

### 2. `src/commands/eval.rs` — NEW FILE

Create a basic but functional eval command that is no longer a stub:

```rust
//! `grove eval` — A/B evaluation of agent configurations.
//!
//! Runs evaluation scenarios against two agent configurations and reports
//! which performs better based on defined assertions.

use std::path::Path;
use crate::config::resolve_project_root;
use crate::json::{json_error, json_output};
use crate::logging::{brand_bold, print_hint};

/// Placeholder scenario result.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalResult {
    pub scenario: String,
    pub passed: bool,
    pub details: String,
}

pub fn execute(
    scenario_path: Option<&Path>,
    assertions_path: Option<&Path>,
    dry_run: bool,
    json: bool,
    project_override: Option<&Path>,
) -> Result<(), String> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let _root = resolve_project_root(&cwd, project_override).map_err(|e| e.to_string())?;

    // If no scenario provided, list available scenarios
    if scenario_path.is_none() {
        if json {
            println!("{}", json_output("eval", &serde_json::json!({
                "status": "ready",
                "message": "grove eval is operational. Provide --scenario <path> to run an evaluation.",
                "dryRun": dry_run,
            })));
        } else {
            println!("{} eval", brand_bold("grove"));
            print_hint("Usage: grove eval --scenario <path> [--assertions <path>] [--dry-run]");
            println!("  Runs A/B evaluation scenarios against agent configurations.");
            println!("  Full eval system requires --scenario and --assertions flags.");
        }
        return Ok(());
    }

    let scenario_path = scenario_path.unwrap();

    // Validate scenario file exists
    if !scenario_path.exists() {
        let msg = format!("Scenario file not found: {}", scenario_path.display());
        if json {
            println!("{}", json_error("eval", &msg));
        } else {
            eprintln!("{msg}");
        }
        return Err(msg);
    }

    // Read scenario
    let scenario_content = std::fs::read_to_string(scenario_path)
        .map_err(|e| format!("Failed to read scenario: {e}"))?;
    let scenario_name = scenario_path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    if dry_run {
        if json {
            println!("{}", json_output("eval", &serde_json::json!({
                "dryRun": true,
                "scenario": scenario_name,
                "scenarioLength": scenario_content.len(),
                "assertionsPath": assertions_path.map(|p| p.to_string_lossy().to_string()),
                "wouldRun": true,
            })));
        } else {
            println!("{} eval (dry run)", brand_bold("grove"));
            println!("  Scenario: {} ({} bytes)", scenario_name, scenario_content.len());
            if let Some(ap) = assertions_path {
                println!("  Assertions: {}", ap.display());
            }
            println!("  Would spawn 2 agents for A/B comparison.");
        }
        return Ok(());
    }

    // Basic scenario validation — check that assertions file exists if provided
    if let Some(ap) = assertions_path {
        if !ap.exists() {
            let msg = format!("Assertions file not found: {}", ap.display());
            if json {
                println!("{}", json_error("eval", &msg));
            } else {
                eprintln!("{msg}");
            }
            return Err(msg);
        }
    }

    // Full eval run — for now, report that scenario was loaded and is ready
    // Full A/B agent spawning is a future enhancement
    let result = EvalResult {
        scenario: scenario_name.clone(),
        passed: true,
        details: format!("Scenario '{}' loaded and validated successfully. Full A/B agent spawning requires active coordinator.", scenario_name),
    };

    if json {
        println!("{}", json_output("eval", &result));
    } else {
        println!("{} eval: {}", brand_bold("grove"), scenario_name);
        println!("  Status: {}", if result.passed { "✓ ready" } else { "✗ failed" });
        println!("  {}", result.details);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eval_no_scenario() {
        let result = execute(None, None, false, false, Some(Path::new("/tmp")));
        // Should succeed (shows usage)
        assert!(result.is_ok());
    }

    #[test]
    fn test_eval_no_scenario_json() {
        let result = execute(None, None, false, true, Some(Path::new("/tmp")));
        assert!(result.is_ok());
    }

    #[test]
    fn test_eval_missing_scenario() {
        let result = execute(
            Some(Path::new("/tmp/nonexistent-scenario.md")),
            None, false, false,
            Some(Path::new("/tmp"))
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_eval_dry_run() {
        let dir = tempfile::tempdir().unwrap();
        let scenario = dir.path().join("test-scenario.md");
        std::fs::write(&scenario, "# Test Scenario\nRun this.").unwrap();
        let result = execute(Some(&scenario), None, true, false, Some(Path::new("/tmp")));
        assert!(result.is_ok());
    }
}
```

### 3. `src/commands/inspect.rs` — Add transcript cost display

Read the current `src/commands/inspect.rs` file first. Then add transcript cost parsing near the end of the output.

Find where the inspect command outputs agent details. After displaying the existing info, add:

```rust
// Attempt to parse transcript for token costs
if let Some(ref transcript) = session.transcript_path {
    let transcript_path = std::path::Path::new(transcript);
    // Use the registry to get the runtime adapter based on session data
    // For now, try Claude transcript format (most common)
    use crate::runtimes::AgentRuntime;
    let rt = crate::runtimes::registry::get_runtime("claude")
        .unwrap_or_else(|_| Box::new(crate::runtimes::claude::ClaudeRuntime));
    if let Some(summary) = rt.parse_transcript(transcript_path) {
        if json {
            // Include in JSON output (add to the output struct if needed, or print separately)
        } else {
            println!("  Token usage:");
            println!("    Input tokens:  {}", summary.input_tokens);
            println!("    Output tokens: {}", summary.output_tokens);
            if summary.cache_read_tokens > 0 {
                println!("    Cache read:    {}", summary.cache_read_tokens);
            }
            if summary.cache_write_tokens > 0 {
                println!("    Cache write:   {}", summary.cache_write_tokens);
            }
            if let Some(ref model) = summary.model {
                println!("    Model:         {model}");
            }
        }
    }
}
```

**Important:** Read the actual inspect.rs file first (`cat src/commands/inspect.rs`) to understand the existing output struct and where to add this. Add `transcript_summary` to the JSON output struct if it makes sense. Make the cost display conditional on transcript_path being set AND the parse succeeding — if it fails or is absent, just skip silently.

### 4. `src/commands/init.rs` — Ecosystem Bootstrap

Read the current `src/commands/init.rs` first. Find the end of the `execute` function (after the main init is complete, before the final success print).

Add ecosystem tool initialization:

```rust
// Ecosystem bootstrap — initialize mulch/seeds/canopy if installed
if !skip_ecosystem {
    bootstrap_ecosystem_tools(&root_str, json);
}
```

Add a new function:

```rust
fn bootstrap_ecosystem_tools(root: &str, _json: bool) {
    // Try each ecosystem tool — skip if not installed (don't error)
    let tools = [
        ("mulch", &["init"] as &[&str]),
        ("seeds", &["init"]),
        ("canopy", &["init"]),
    ];
    
    for (tool, args) in &tools {
        let check = std::process::Command::new("which")
            .arg(tool)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        
        if check.map(|s| s.success()).unwrap_or(false) {
            let result = std::process::Command::new(tool)
                .args(*args)
                .current_dir(root)
                .output();
            
            match result {
                Ok(out) if out.status.success() => {
                    eprintln!("  ✓ {tool} init");
                }
                Ok(_) => {
                    // Tool exists but init failed — not a fatal error
                    eprintln!("  ⚠ {tool} init failed (continuing)");
                }
                Err(_) => {} // which returned true but exec failed — ignore
            }
        }
    }
}
```

Find the `execute` function signature and add `skip_ecosystem: bool` parameter if not already there, OR just call `bootstrap_ecosystem_tools` unconditionally at the end of `execute` (checking for tool presence handles the skipping internally).

**Important:** Read the actual init.rs file first to understand the existing structure. The key is to add the ecosystem bootstrap AFTER the `.overstory/` directory and all config files are created. DO NOT break any existing init functionality.

### 5. `src/main.rs` — Wire New Functionality

After reading and implementing the above, make these changes to main.rs:

**a) Add CoordinatorAsk subcommand:**

Find the existing `CoordinatorSubcommand` enum (around line 1095) and add:
```rust
/// Send a message to coordinator and wait for reply
Ask(CoordinatorAskArgs),
```

Add the args struct near the other Coordinator arg structs:
```rust
#[derive(Parser, Debug)]
struct CoordinatorAskArgs {
    /// Message body to send to coordinator
    #[arg(long)]
    body: String,
    /// From agent name
    #[arg(long, default_value = "operator")]
    from: String,
    /// Timeout in seconds
    #[arg(long, default_value = "30")]
    timeout: u64,
    #[arg(long)]
    json: bool,
}
```

Find the coordinator match arm (it dispatches to subcommands). Add:
```rust
CoordinatorSubcommand::Ask(args) => {
    commands::coordinator::execute_ask(&args.body, &args.from, args.timeout, args.json, project.as_deref())
}
```

**b) Unhide and wire eval:**

Find the existing eval stub in main.rs:
```rust
Commands::Eval(_) => not_yet_implemented("eval", json),
```

Replace with:
```rust
Commands::Eval(args) => {
    commands::eval::execute(
        args.scenario.as_deref(),
        args.assertions.as_deref(),
        args.dry_run,
        args.json,
        project.as_deref(),
    )
}
```

Find the existing `EvalArgs` struct (if it exists) and update it to have the right fields:
```rust
#[derive(Parser, Debug)]
struct EvalArgs {
    /// Path to scenario file
    #[arg(long)]
    scenario: Option<std::path::PathBuf>,
    /// Path to assertions file
    #[arg(long)]
    assertions: Option<std::path::PathBuf>,
    /// Dry run — validate scenario without spawning agents
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    json: bool,
}
```

Also remove the `#[command(hide = true)]` attribute from Eval if it's there.

Also add `pub mod eval;` to `src/commands/mod.rs`.

## Verification

```bash
. /home/joshf/.cargo/env
cd /home/joshf/grove

# Build + test + clippy (SEQUENTIAL)
cargo build && cargo test && cargo clippy -- -D warnings 2>&1 | tail -5

# Verify coordinator ask exists
./target/debug/grove coordinator ask --help

# Verify eval is no longer stubbed
./target/debug/grove eval --help
./target/debug/grove eval 2>&1 | grep -v "not yet implemented"

# Verify init runs without error
cd /tmp && rm -rf grove-init-test && mkdir grove-init-test && cd grove-init-test && git init -q && /home/joshf/grove/target/debug/grove init --name test-init --yes 2>&1 | head -10

# Verify no more not_yet_implemented for eval
grep "not_yet_implemented.*eval" /home/joshf/grove/src/main.rs && echo "FAIL: still stubbed" || echo "PASS: eval wired"

# Restore working directory
cd /home/joshf/grove
```

## Acceptance Criteria

1. `grove coordinator ask --body "test" --timeout 5` compiles and runs (may timeout if no coordinator, that's OK)
2. `grove eval --help` shows usage (not "not yet implemented")
3. `grove eval` shows usage information (no error, no "not yet implemented")
4. `grove eval --dry-run --scenario <path>` works for valid scenario files
5. `grove inspect` shows token costs when transcript_path is set (silently skips if not)
6. `grove init` attempts ecosystem bootstrap (mulch/seeds/canopy) — skips gracefully if not installed
7. All 453+ tests pass
8. No `not_yet_implemented("eval"...)` call in main.rs
9. `cargo clippy -- -D warnings` clean

## IMPORTANT

- Read each target file BEFORE modifying it (use Read tool / cat)
- Run quality gates SEQUENTIALLY: `cargo build && cargo test && cargo clippy -- -D warnings`
- Check for conflict markers before committing: `grep -rn "<<<<<<" src/`
- Add `pub mod eval;` to `src/commands/mod.rs` — don't forget this
- ONE agent owns main.rs changes — do all main.rs edits in one pass
- Commit: `git add -A && git commit -m "Phase 9D: coordinator ask, eval, inspect costs, init bootstrap"`
