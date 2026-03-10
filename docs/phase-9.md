# Phase 9: Complete Overhaul — Remove tmux, Add Runtime Adapters, Close All Gaps

## Context

Grove was built as a Rust rewrite of overstory with the explicit architectural goal of eliminating tmux for agent spawning. However, the codebase still has **167 tmux references across 20 files**. Additionally, grove only supports Claude Code as a runtime, while overstory supports 7. This phase achieves true feature parity and architectural integrity.

This is the largest phase. It has 5 sub-phases that can be dispatched independently.

---

## Phase 9A: Eliminate tmux — Headless-Only Agent Management

### Goal
Remove ALL tmux usage from grove. Agents are child processes with PID tracking. No tmux dependency.

### Changes by File

#### `src/commands/sling.rs`
- **Remove** `spawn_tmux()` function entirely (~50 lines)
- **Remove** `use crate::worktree::tmux;`
- **Remove** the `if ctx.headless || runtime.is_headless()` branch — ALL spawns are now headless
- The `headless` flag on SlingOptions becomes deprecated/ignored (always headless)
- `spawn_headless()` becomes the ONLY spawn path
- Output no longer prints "Tmux: ..." line

#### `src/commands/nudge.rs`
- **Remove** all tmux send-keys logic (resolve_tmux_session, is_tmux_session_alive, send_nudge_with_retry)
- **Replace with:** Send nudge via mail. `grove nudge agent-name --message "text"` → `grove mail send --to agent-name --from operator --subject "Nudge" --body "text" --type nudge`
- Agents check mail via hooks (SessionStart, PostToolUse). Nudge arrives as mail, not keystroke injection.
- Keep --force flag for bypassing debounce

#### `src/commands/stop.rs`
- **Remove** `is_tmux_session_alive()`, `kill_tmux_session()`
- **Replace with:** Kill PID directly. `kill(pid, SIGTERM)`. If PID doesn't die after 5s, `kill(pid, SIGKILL)`.
- Update session state to Zombie/Completed after kill
- For headless agents (tmux_session is empty): already uses PID kill — just make this the ONLY path

#### `src/commands/clean.rs`
- **Remove** `kill_project_tmux_sessions()`, `list_tmux_sessions()`, `kill_tmux_session()`
- **Replace with:** Get all active sessions from DB, kill each PID. No tmux enumeration needed.
- Remove tmux_killed counter from CleanResult

#### `src/commands/status.rs`
- **Remove** `list_tmux_sessions()` function
- **Remove** `tmux_sessions` from StatusOutput struct
- **Replace with:** The JSON output keeps `tmuxSessions: []` (always empty array) for backward compat with tools that parse it
- The text output removes the "Tmux: N" line

#### `src/commands/doctor.rs`
- **Remove** `check_tmux()` — tmux is no longer required
- Replace with `check_process_spawn()` that verifies `kill -0 $$` works (basic PID checking)

#### `src/commands/worktree_cmd.rs`
- **Remove** `is_tmux_alive()`, `kill_tmux_session()`
- Worktree clean kills PIDs from sessions.db, not tmux sessions

#### `src/tui/app.rs`
- **Remove** `capture_tmux()` function
- **Rename** `capture_agent_output()` to be the ONLY output capture function
- It reads from log files: `.overstory/logs/<agent>/<timestamp>/stdout.log`
- Falls back to "(no output — agent log not found)" if no log file

#### `src/tui/views/overview.rs`
- Replace `capture_tmux()` calls with `capture_agent_output()`
- No tmux session name parameter — use agent name + project root to find log files

#### `src/watchdog/mod.rs`
- **Remove** `is_tmux_alive()` function
- Health check uses ONLY `is_pid_alive()` — already works for headless agents
- `kill_agent()` removes tmux kill-session call, uses only PID kill

#### `src/worktree/tmux.rs`
- **DELETE THIS FILE ENTIRELY**
- Remove `pub mod tmux;` from `src/worktree/mod.rs`

#### `src/worktree/mod.rs`
- Remove `pub mod tmux;`

#### `src/types.rs`
- Keep `tmux_session: String` field in AgentSession for DB backward compat
- It will always be empty string `""` for grove-spawned agents
- Add doc comment: `/// Backward compat with overstory. Always empty for grove agents.`

#### `src/db/sessions.rs`
- Keep `tmux_session` column in schema — DB compat with overstory
- All inserts write empty string

#### `src/commands/init.rs`
- Remove text mentioning tmux in the CLAUDE.md template

### Verification

```bash
# Zero tmux binary calls remaining (excluding tests and resolver string literals)
grep -rn 'Command::new("tmux")' src/ --include="*.rs" | grep -v test | grep -v resolver
# Should return 0 lines

# worktree/tmux.rs deleted
[ ! -f src/worktree/tmux.rs ] && echo "PASS" || echo "FAIL"

# All tests pass
cargo build && cargo test && cargo clippy -- -D warnings

# Headless sling works (already proven)
grove sling test --headless --capability builder --name test-1 --skip-task-check --no-scout-check --files README.md
grove status | grep test-1

# Stop kills PID not tmux
grove stop test-1
```

### Acceptance Criteria
1. `grep -rn 'Command::new("tmux")' src/` returns ONLY test code and merge resolver string literals
2. `src/worktree/tmux.rs` does not exist
3. `grove sling` always spawns headless (no --headless flag needed)
4. `grove stop` kills by PID
5. `grove nudge` sends via mail
6. `grove clean` kills PIDs not tmux sessions
7. `grove doctor` does not check for tmux
8. TUI reads from log files only
9. All existing tests pass

---

## Phase 9B: Runtime Adapters — Codex, Gemini, Copilot

### Goal
Add the 3 most useful runtime adapters so grove works with Claude, Codex, and Gemini.

### New Files

#### `src/runtimes/codex.rs` (~200 lines)
```rust
pub struct CodexRuntime;

impl AgentRuntime for CodexRuntime {
    fn id(&self) -> &str { "codex" }
    fn instruction_path(&self) -> &str { "AGENTS.md" }
    fn is_headless(&self) -> bool { true }
    
    fn build_headless_command(&self, opts: &SpawnOpts) -> Vec<String> {
        // codex exec --full-auto --ephemeral "Read AGENTS.md..."
        let mut cmd = vec!["codex".into(), "exec".into(), "--full-auto".into(), "--ephemeral".into()];
        // Only add --model if it's not a manifest alias (sonnet/opus/haiku)
        let aliases = ["sonnet", "opus", "haiku"];
        if !aliases.contains(&opts.model.as_str()) {
            cmd.extend(["--model".into(), opts.model.clone()]);
        }
        cmd.push(format!("Read {} for your task assignment and begin immediately.", opts.instruction_path));
        cmd
    }
    
    fn build_interactive_command(&self, opts: &SpawnOpts) -> String {
        // Not used — grove is headless only now
        format!("codex --full-auto 'Read {} and begin.'", opts.instruction_path)
    }
    
    fn deploy_config(&self, worktree: &Path, overlay_content: &str, _hooks: &HooksDef) -> Result<(), String> {
        // Write overlay to AGENTS.md (not .claude/CLAUDE.md)
        let agents_path = worktree.join("AGENTS.md");
        std::fs::write(&agents_path, overlay_content)
            .map_err(|e| format!("Failed to write AGENTS.md: {e}"))?;
        Ok(())
    }
    
    fn detect_ready(&self, _pane_content: &str) -> ReadyState {
        ReadyState { phase: ReadyPhase::Ready, detail: None }
    }
    
    fn build_env(&self, model: &ResolvedModel) -> HashMap<String, String> {
        model.env.clone().unwrap_or_default()
            .into_iter().collect()
    }
}
```

#### `src/runtimes/gemini.rs` (~200 lines)
```rust
pub struct GeminiRuntime;

impl AgentRuntime for GeminiRuntime {
    fn id(&self) -> &str { "gemini" }
    fn instruction_path(&self) -> &str { "GEMINI.md" }
    fn is_headless(&self) -> bool { true }
    
    fn build_headless_command(&self, opts: &SpawnOpts) -> Vec<String> {
        // gemini -p "Read GEMINI.md..." --yolo
        vec!["gemini".into(), "-p".into(), 
             format!("Read {} for your task assignment and begin immediately.", opts.instruction_path),
             "--yolo".into()]
    }
    
    fn deploy_config(&self, worktree: &Path, overlay_content: &str, _hooks: &HooksDef) -> Result<(), String> {
        let gemini_path = worktree.join("GEMINI.md");
        std::fs::write(&gemini_path, overlay_content)
            .map_err(|e| format!("Failed to write GEMINI.md: {e}"))?;
        Ok(())
    }
    // ... rest similar pattern
}
```

#### `src/runtimes/copilot.rs` (~200 lines)
```rust
pub struct CopilotRuntime;
// id: "copilot"
// instruction_path: ".github/copilot-instructions.md"
// headless: copilot -p "prompt" --allow-all-tools
// deploy: write to .github/copilot-instructions.md
```

### Modified Files

#### `src/runtimes/mod.rs`
- Add `pub mod codex;`, `pub mod gemini;`, `pub mod copilot;`

#### `src/runtimes/registry.rs`
- Register all 4 runtimes: claude, codex, gemini, copilot
- Add per-capability routing: check `config.runtime.capabilities[capability]` before falling back to `config.runtime.default`

#### `src/agents/overlay.rs`
- The overlay writer currently creates `.claude/` directory. It must use `runtime.instruction_path()` to determine the correct directory and filename.
- For codex: write to `AGENTS.md` (worktree root, no subdirectory)
- For gemini: write to `GEMINI.md`
- For copilot: write to `.github/copilot-instructions.md` (create `.github/` if needed)

### Runtime Trait Extensions

Add to `AgentRuntime` trait:
```rust
/// Build argv for one-shot AI call (used by merge resolver + watchdog triage)
fn build_print_command(&self, prompt: &str, model: Option<&str>) -> Vec<String>;

/// Parse session transcript file into token usage summary
fn parse_transcript(&self, path: &Path) -> Option<TranscriptSummary>;
```

### Verification

```bash
# Codex adapter works
grove sling test-codex --runtime codex --capability builder --name codex-test \
  --skip-task-check --no-scout-check --files README.md
grove status | grep codex-test
[ -f .overstory/worktrees/codex-test/AGENTS.md ] && echo "PASS: AGENTS.md" || echo "FAIL"

# Gemini adapter works  
grove sling test-gemini --runtime gemini --capability builder --name gemini-test \
  --skip-task-check --no-scout-check --files README.md
[ -f .overstory/worktrees/gemini-test/GEMINI.md ] && echo "PASS: GEMINI.md" || echo "FAIL"

# Per-capability routing
# (configure runtime.capabilities.builder = "codex" in config.yaml)
grove sling test-routing --capability builder --name routing-test \
  --skip-task-check --no-scout-check --files README.md
# Should use codex, not claude

# All tests pass
cargo build && cargo test
```

### Acceptance Criteria
1. `grove sling --runtime codex` spawns with `codex exec --full-auto`
2. `grove sling --runtime gemini` spawns with `gemini -p --yolo`
3. `grove sling --runtime copilot` spawns with `copilot -p`
4. Each adapter writes overlay to correct instruction file
5. Per-capability routing works via config
6. `build_print_command` returns correct argv for each runtime
7. All existing tests pass + new adapter unit tests

---

## Phase 9C: AI-Assisted Features — Merge Tiers 3-4, Watchdog Triage

### Goal
Add the LLM-powered features: AI merge conflict resolution and AI failure classification.

### Changes

#### `src/merge/resolver.rs` — Add Tier 3 (AI Resolve) + Tier 4 (Reimagine)

**Tier 3 — AI Resolve:** When auto-resolve fails, call the runtime's `build_print_command` with a prompt containing both sides of the conflict. The LLM returns the resolved content.

**Tier 4 — Reimagine:** When AI resolve also fails (ambiguous merge), the LLM is given the full file from both branches and asked to rewrite it from scratch incorporating both changes.

Both tiers respect `config.merge.aiResolveEnabled` and `config.merge.reimagineEnabled`.

#### `src/watchdog/triage.rs` (NEW FILE ~200 lines)

When an agent is detected as stalled, triage:
1. Reads last 50 lines of the agent's log file
2. Calls `build_print_command` with a classification prompt
3. Returns a verdict: `recoverable` (extend timeout), `fatal` (kill), `long_running` (extend more)
4. The watchdog acts on the verdict

#### `src/watchdog/mod.rs`
- Add tier 1 logic: if stale and tier1_enabled, call triage before killing
- Import and use the new triage module

### Verification

```bash
# Tier 3 merge — create a conflict, verify AI resolution attempted
# (requires ANTHROPIC_API_KEY or runtime API key)
cargo test merge

# Triage — verify module compiles and unit tests pass
cargo test watchdog::triage
```

### Acceptance Criteria
1. Merge with `aiResolveEnabled: true` attempts LLM resolution on conflicts
2. Merge with `reimagineEnabled: true` attempts full file reimagine
3. Watchdog with `tier1Enabled: true` calls triage before killing stalled agents
4. All three features degrade gracefully when no API key is set

---

## Phase 9D: Missing Command Features — Coordinator Ask, Group Close, Eval, Init Bootstrap

### Goal
Close all remaining command-level feature gaps.

### Changes

#### `src/commands/coordinator.rs` — Add `ask` subcommand
- `grove coordinator ask --body "question" --timeout 30`
- Sends mail to coordinator, then polls for a reply matching the thread_id
- Returns the reply body (or timeout error)
- This enables synchronous request-reply with the coordinator

#### `src/commands/group.rs` — Add `close` subcommand + auto-close
- `grove group close <name>` — manually close a group
- Auto-close: when `group add` marks an issue as completed, check if all members are done. If so, auto-close the group.

#### `src/commands/init.rs` — Ecosystem bootstrapping
- After creating `.overstory/`, run ecosystem tool init:
  - `mulch init` if mulch is installed and `--skip-mulch` not set
  - `seeds init` if seeds is installed and `--skip-seeds` not set  
  - `canopy init` if canopy is installed and `--skip-canopy` not set
- Run `onboard` step: inject tool-specific sections into CLAUDE.md

#### `src/commands/eval.rs` (NEW FILE ~400 lines)
- Unhide from CLI
- `grove eval --scenario <path> --assertions <path>`
- Spawns two agents with different configurations (A/B)
- Runs assertions against their outputs
- Reports which performed better
- References overstory's `src/eval/` for behavior

#### `src/commands/inspect.rs` — Transcript parsing
- Use `runtime.parse_transcript()` to extract token counts
- Display cost breakdown per agent in inspect output
- Show model info from transcript

### Verification

```bash
# Coordinator ask
grove coordinator start --no-attach
grove coordinator ask --body "status check" --timeout 10
grove coordinator stop

# Group close
grove group create close-test
grove group close close-test
grove group list | grep "closed"

# Init bootstrap (in fresh dir)
rm -rf /tmp/grove-init-test && mkdir /tmp/grove-init-test && cd /tmp/grove-init-test && git init
grove init --name init-test --yes
# Should attempt mulch init, seeds init if available

# Eval (basic)
grove eval --help  # Should not say "not yet implemented"
```

### Acceptance Criteria
1. `grove coordinator ask` sends and waits for reply
2. `grove group close` works, auto-close triggers when all members done
3. `grove init` bootstraps ecosystem tools
4. `grove eval` is functional (even if basic)
5. `grove inspect` shows token costs from transcripts

---

## Phase 9E: Per-Capability Routing + Config Gaps

### Goal
Complete config feature parity.

### Changes

#### `src/runtimes/registry.rs`
Already partially done — add:
```rust
// Check capability-specific runtime first
if let Some(caps) = config.runtime.capabilities {
    if let Some(runtime_name) = caps.get(capability) {
        return get_runtime(runtime_name);
    }
}
```

#### `src/config.rs`
- Parse `runtime.capabilities` map from config YAML
- Parse `runtime.print` (runtime for one-shot AI calls used by merge/triage)
- Parse `merge.aiResolveEnabled` and `merge.reimagineEnabled`

#### `src/types.rs`
- Add `capabilities: Option<HashMap<String, String>>` to RuntimeConfig
- Add `print_runtime: Option<String>` to RuntimeConfig
- Add `ai_resolve_enabled: bool` and `reimagine_enabled: bool` to MergeConfig

### Verification

```bash
# Config with per-capability routing
cat > /tmp/test-config.yaml << 'EOF'
runtime:
  default: claude
  capabilities:
    builder: codex
    lead: claude
EOF
# Verify grove reads this correctly
```

### Acceptance Criteria
1. `runtime.capabilities.builder = "codex"` routes builder agents to codex
2. `merge.aiResolveEnabled` controls tier 3
3. Config validation catches invalid runtime names in capabilities

---

## Dispatch Strategy

| Sub-Phase | Dependencies | Builder Count | Est. Lines |
|-----------|-------------|---------------|-----------|
| 9A (tmux removal) | None | 1 builder (touches many files) | ~500 deletions, ~200 additions |
| 9B (adapters) | 9A (headless-only spawn) | 2 builders (adapters + registry) | ~800 |
| 9C (AI features) | 9B (build_print_command) | 1 builder | ~500 |
| 9D (command gaps) | None | 2 builders | ~800 |
| 9E (config) | 9B (runtime routing) | 1 builder | ~200 |

**Total: ~3,000 lines of changes across 5 sub-phases**

Dispatch order: **9A first** (prerequisite for everything), then **9B + 9D in parallel**, then **9C + 9E**.

---

## Global Verification (after all sub-phases)

```bash
G=./target/debug/grove

# 1. Zero tmux binary calls
grep -rn 'Command::new("tmux")' src/ --include="*.rs" | grep -v test | grep -v resolver
# Must be 0

# 2. tmux.rs deleted
[ ! -f src/worktree/tmux.rs ] && echo "PASS" || echo "FAIL"

# 3. All runtimes registered
$G sling --help | grep runtime
$G sling test --runtime codex --capability builder --name x --skip-task-check --no-scout-check --files README.md 2>&1 | head -1
$G sling test --runtime gemini --capability builder --name y --skip-task-check --no-scout-check --files README.md 2>&1 | head -1

# 4. No stubs
grep "not_yet_implemented" src/main.rs | grep -v "fn \|//\|test"
# Only eval should remain if Phase 9D doesn't complete

# 5. Full test suite
cargo build && cargo test && cargo clippy -- -D warnings

# 6. Headless lifecycle works for each runtime
for rt in claude codex gemini; do
  $G sling verify-$rt --runtime $rt --capability builder --name verify-$rt \
    --skip-task-check --no-scout-check --files README.md
  sleep 3
  $G status | grep verify-$rt
done

# 7. TUI works without tmux
$G dashboard  # Should launch fine, show agents from log files

# 8. Line count
find src -name "*.rs" | xargs wc -l | tail -1
# Target: ~29,000 lines
```
