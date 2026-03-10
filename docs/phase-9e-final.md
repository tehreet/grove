# Phase 9E: Runtime Trait Extensions + Config Wiring

## Goal
Add `build_print_command` and `parse_transcript` methods to the `AgentRuntime` trait, implement them on all 4 adapters (claude, codex, gemini, copilot), and ensure per-capability routing config is fully wired.

These methods are prerequisites for Phase 9C (AI merge tiers + watchdog triage use `build_print_command`).

## File Scope

- `src/runtimes/mod.rs` — add methods to trait
- `src/runtimes/claude.rs` — implement methods
- `src/runtimes/codex.rs` — implement methods
- `src/runtimes/gemini.rs` — implement methods
- `src/runtimes/copilot.rs` — implement methods
- `src/types.rs` — add TranscriptSummary type (if not already there)

DO NOT modify main.rs, config.rs, registry.rs, or any other file.

## Changes

### 1. `src/types.rs` — Add TranscriptSummary (if not already present)

Check if `TranscriptSummary` struct exists. If not, add it near the other output types:

```rust
/// Token usage summary extracted from an agent session transcript.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSummary {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    pub total_cost_usd: f64,
    pub model: Option<String>,
}
```

### 2. `src/runtimes/mod.rs` — Extend AgentRuntime trait

Add these two methods to the `AgentRuntime` trait (after `build_env`):

```rust
/// Build argv for a one-shot AI call (used by merge resolver + watchdog triage).
/// The returned vec is the full argv: ["claude", "-p", "--model", "...", prompt]
/// `prompt` is the full text prompt to send. `model` overrides the default if Some.
fn build_print_command(&self, prompt: &str, model: Option<&str>) -> Vec<String>;

/// Parse a session transcript file and extract token usage.
/// Returns None if the file doesn't exist, can't be read, or format is unrecognized.
fn parse_transcript(&self, path: &std::path::Path) -> Option<crate::types::TranscriptSummary>;
```

### 3. `src/runtimes/claude.rs` — Implement on ClaudeRuntime

```rust
fn build_print_command(&self, prompt: &str, model: Option<&str>) -> Vec<String> {
    // claude -p --print "prompt text" [--model MODEL]
    let mut cmd = vec!["claude".to_string(), "-p".to_string()];
    if let Some(m) = model {
        cmd.push("--model".to_string());
        cmd.push(m.to_string());
    }
    cmd.push(prompt.to_string());
    cmd
}

fn parse_transcript(&self, path: &std::path::Path) -> Option<crate::types::TranscriptSummary> {
    // Claude transcripts are NDJSON files (.jsonl). Each line is a JSON object.
    // Look for "usage" fields: {"type":"result","usage":{"input_tokens":N,"output_tokens":N,...}}
    // Also look for assistant turns with usage metadata.
    let content = std::fs::read_to_string(path).ok()?;
    
    let mut summary = crate::types::TranscriptSummary::default();
    let mut found = false;
    
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        let Ok(val) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        
        // Look for usage in any object
        if let Some(usage) = val.get("usage") {
            found = true;
            summary.input_tokens += usage.get("input_tokens")
                .and_then(|v| v.as_u64()).unwrap_or(0);
            summary.output_tokens += usage.get("output_tokens")
                .and_then(|v| v.as_u64()).unwrap_or(0);
            summary.cache_read_tokens += usage.get("cache_read_input_tokens")
                .and_then(|v| v.as_u64()).unwrap_or(0);
            summary.cache_write_tokens += usage.get("cache_creation_input_tokens")
                .and_then(|v| v.as_u64()).unwrap_or(0);
        }
        // Capture model from assistant messages
        if let Some(model) = val.get("model").and_then(|v| v.as_str()) {
            if !model.is_empty() {
                summary.model = Some(model.to_string());
            }
        }
    }
    
    if found { Some(summary) } else { None }
}
```

### 4. `src/runtimes/codex.rs` — Implement on CodexRuntime

```rust
fn build_print_command(&self, prompt: &str, model: Option<&str>) -> Vec<String> {
    // codex -q "prompt" [--model MODEL]
    // -q flag = quiet/non-interactive output
    let mut cmd = vec!["codex".to_string(), "-q".to_string()];
    if let Some(m) = model {
        // Filter out Anthropic model aliases that Codex doesn't understand
        let anthropic_aliases = ["sonnet", "opus", "haiku", "claude"];
        if !anthropic_aliases.iter().any(|a| m.contains(a)) {
            cmd.push("--model".to_string());
            cmd.push(m.to_string());
        }
    }
    cmd.push(prompt.to_string());
    cmd
}

fn parse_transcript(&self, path: &std::path::Path) -> Option<crate::types::TranscriptSummary> {
    // Codex writes NDJSON to stderr. Lines have {"type":"message","usage":{...}} format.
    let content = std::fs::read_to_string(path).ok()?;
    let mut summary = crate::types::TranscriptSummary::default();
    let mut found = false;
    
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        let Ok(val) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if let Some(usage) = val.get("usage") {
            found = true;
            summary.input_tokens += usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
            summary.output_tokens += usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
        }
        if let Some(model) = val.get("model").and_then(|v| v.as_str()) {
            if !model.is_empty() { summary.model = Some(model.to_string()); }
        }
    }
    
    if found { Some(summary) } else { None }
}
```

### 5. `src/runtimes/gemini.rs` — Implement on GeminiRuntime

```rust
fn build_print_command(&self, prompt: &str, model: Option<&str>) -> Vec<String> {
    // gemini -p "prompt" [--model MODEL] --yolo
    let mut cmd = vec!["gemini".to_string(), "-p".to_string()];
    if let Some(m) = model {
        cmd.push("--model".to_string());
        cmd.push(m.to_string());
    }
    cmd.push(prompt.to_string());
    cmd.push("--yolo".to_string());
    cmd
}

fn parse_transcript(&self, path: &std::path::Path) -> Option<crate::types::TranscriptSummary> {
    // Gemini writes NDJSON. Look for usageMetadata fields.
    let content = std::fs::read_to_string(path).ok()?;
    let mut summary = crate::types::TranscriptSummary::default();
    let mut found = false;
    
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        let Ok(val) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if let Some(meta) = val.get("usageMetadata") {
            found = true;
            summary.input_tokens += meta.get("promptTokenCount").and_then(|v| v.as_u64()).unwrap_or(0);
            summary.output_tokens += meta.get("candidatesTokenCount").and_then(|v| v.as_u64()).unwrap_or(0);
        }
        if let Some(model) = val.get("modelVersion").and_then(|v| v.as_str()) {
            if !model.is_empty() { summary.model = Some(model.to_string()); }
        }
    }
    
    if found { Some(summary) } else { None }
}
```

### 6. `src/runtimes/copilot.rs` — Implement on CopilotRuntime

```rust
fn build_print_command(&self, prompt: &str, model: Option<&str>) -> Vec<String> {
    // copilot -p "prompt" [--model MODEL] --allow-all-tools
    let mut cmd = vec!["copilot".to_string(), "-p".to_string()];
    if let Some(m) = model {
        cmd.push("--model".to_string());
        cmd.push(m.to_string());
    }
    cmd.push(prompt.to_string());
    cmd.push("--allow-all-tools".to_string());
    cmd
}

fn parse_transcript(&self, path: &std::path::Path) -> Option<crate::types::TranscriptSummary> {
    // Copilot uses similar NDJSON format, look for usage fields
    let content = std::fs::read_to_string(path).ok()?;
    let mut summary = crate::types::TranscriptSummary::default();
    let mut found = false;
    
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        let Ok(val) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if let Some(usage) = val.get("usage") {
            found = true;
            summary.input_tokens += usage.get("prompt_tokens").or_else(|| usage.get("input_tokens"))
                .and_then(|v| v.as_u64()).unwrap_or(0);
            summary.output_tokens += usage.get("completion_tokens").or_else(|| usage.get("output_tokens"))
                .and_then(|v| v.as_u64()).unwrap_or(0);
        }
    }
    
    if found { Some(summary) } else { None }
}
```

## Unit Tests

Add tests in each adapter file's `#[cfg(test)]` block:

### claude.rs tests:
```rust
#[test]
fn test_build_print_command_no_model() {
    let cmd = ClaudeRuntime.build_print_command("hello world", None);
    assert_eq!(cmd, vec!["claude", "-p", "hello world"]);
}

#[test]
fn test_build_print_command_with_model() {
    let cmd = ClaudeRuntime.build_print_command("hello", Some("claude-sonnet-4-6"));
    assert!(cmd.contains(&"--model".to_string()));
    assert!(cmd.contains(&"claude-sonnet-4-6".to_string()));
}

#[test]
fn test_parse_transcript_empty_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("transcript.jsonl");
    std::fs::write(&path, "").unwrap();
    assert!(ClaudeRuntime.parse_transcript(&path).is_none());
}

#[test]
fn test_parse_transcript_with_usage() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("transcript.jsonl");
    std::fs::write(&path, r#"{"type":"result","usage":{"input_tokens":100,"output_tokens":50}}"#).unwrap();
    let summary = ClaudeRuntime.parse_transcript(&path).unwrap();
    assert_eq!(summary.input_tokens, 100);
    assert_eq!(summary.output_tokens, 50);
}
```

Similar tests for codex, gemini, copilot adapters — test `build_print_command` returns correct binary name and that `parse_transcript` returns None for empty/nonexistent files.

## Verification

```bash
. /home/joshf/.cargo/env
cd /home/joshf/grove

# Build must pass
cargo build 2>&1 | tail -3

# All tests pass
cargo test 2>&1 | grep "test result"

# Clippy clean
cargo clippy -- -D warnings 2>&1 | grep -v "^warning\|Checking\|Finished"

# Verify trait has new methods
grep -n "build_print_command\|parse_transcript" src/runtimes/mod.rs

# Verify all adapters implement them
grep -n "build_print_command\|parse_transcript" src/runtimes/claude.rs src/runtimes/codex.rs src/runtimes/gemini.rs src/runtimes/copilot.rs
```

## Acceptance Criteria

1. `AgentRuntime` trait has `build_print_command` and `parse_transcript` methods
2. All 4 adapters (claude, codex, gemini, copilot) implement both methods
3. `TranscriptSummary` type exists in `src/types.rs`
4. All existing 453 tests still pass + new tests added
5. `cargo clippy -- -D warnings` clean
6. DO NOT break any existing functionality

## IMPORTANT

- Run `cargo build && cargo test && cargo clippy -- -D warnings` SEQUENTIALLY (one command with `&&`) not in parallel — parallel cargo builds deadlock
- Commit after completing all changes: `git add -A && git commit -m "Phase 9E: add build_print_command and parse_transcript to runtime trait"`
- Check for conflict markers before committing: `grep -rn "<<<<<<" src/`
