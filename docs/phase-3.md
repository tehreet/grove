# Phase 3: Process Management + Agent Spawning

This is where grove diverges from overstory architecturally. Overstory spawns agents via tmux screen-scraping with sleep escalation and beacon retry loops. Grove spawns agents as direct child processes with stdin/stdout pipes.

## Context

Phases 0-2 are complete. We have 11,779 lines of Rust, 247 tests, 12 working commands. Grove can read/write all `.overstory/` databases interoperably with overstory.

What's still stubbed: sling, coordinator, supervisor, monitor, agents, worktree, log, logs, watch, trace, feed, errors, replay, run, group, ecosystem, metrics (display), eval, update, upgrade, completions.

This phase implements the core: **sling** (agent spawning), **worktree management** (git worktrees), **overlay generation** (CLAUDE.md), **runtime adapters** (claude, codex, gemini, etc.), **log** (session lifecycle), and **watchdog** (health monitoring).

## Architecture Decision: How Grove Spawns Agents

### What overstory does (reference/sling.ts, reference/tmux.ts):

1. Create git worktree + branch
2. Write overlay (CLAUDE.md) to worktree  
3. Create tmux session running the runtime CLI
4. Poll `tmux capture-pane` every 500ms for 30s waiting for TUI to render
5. Sleep 1s buffer
6. Send beacon (task prompt) via `tmux send-keys`
7. Send follow-up Enters at 1s, 2s, 3s, 5s delays
8. Verify beacon received — up to 5 resend attempts with 2s sleeps between

Total: 11+ seconds of sleep, screen-scraping, retry loops.

### What grove does:

**For headless runtimes (claude --print, codex --quiet, etc.):**
```
1. Create git worktree + branch
2. Write overlay to worktree
3. Spawn runtime as child process with stdin/stdout/stderr pipes
4. Write beacon to stdin, close stdin
5. Parse NDJSON events from stdout in real-time
6. Track process lifecycle via PID
```

**For interactive runtimes that NEED a TUI (when headless isn't available):**
```
1. Create git worktree + branch  
2. Write overlay to worktree
3. Create tmux session (just like overstory — we still support tmux)
4. Send beacon via tmux send-keys
5. Poll for readiness using runtime-specific detectReady
```

Grove keeps tmux as a fallback for interactive runtimes but DEFAULTS to headless mode where supported. The key insight: Claude Code, Codex, and Gemini all support headless `-p` mode. Only Pi currently requires interactive TUI.

## Deliverables

### 1. `src/worktree/mod.rs` — Git Worktree Management

Reference: `reference/worktree-manager.ts`, `reference/tmux.ts`

**Git operations** (via `std::process::Command` shelling out to `git`):
- `create_worktree(repo_root, base_branch, branch_name, worktree_path)` — `git worktree add -b <branch> <path> <base>`
- `remove_worktree(worktree_path)` — `git worktree remove --force <path>`
- `list_worktrees(repo_root)` → Vec of (path, branch, HEAD)
- `prune_worktrees(repo_root)` — `git worktree prune`
- `delete_branch(repo_root, branch_name)` — `git branch -D <branch>`

**Tmux operations** (for interactive runtime fallback):
- `create_tmux_session(name, command)` — `tmux new-session -d -s <n> <cmd>`
- `kill_tmux_session(name)` — `tmux kill-session -t <n>`
- `send_tmux_keys(name, keys)` — `tmux send-keys -t <n> "<keys>" Enter`
- `capture_tmux_pane(name)` → Option<String>
- `is_tmux_session_alive(name)` → bool
- `list_tmux_sessions()` → Vec<TmuxSession>

Put git ops in `src/worktree/git.rs` and tmux ops in `src/worktree/tmux.rs`.

### 2. `src/process/mod.rs` — Direct Process Management

This is NEW — doesn't exist in overstory. This is grove's replacement for the tmux spawn path.

**src/process/spawn.rs:**
```rust
pub struct ManagedProcess {
    pub pid: u32,
    pub child: tokio::process::Child,
    pub stdin: Option<ChildStdin>,
    pub stdout: Option<ChildStdout>,
    pub stderr: Option<ChildStderr>,
}

pub async fn spawn_headless(
    binary: &str,
    args: &[String],
    cwd: &Path,
    env: HashMap<String, String>,
) -> Result<ManagedProcess>;

pub fn is_process_alive(pid: u32) -> bool;
pub fn kill_process(pid: u32, force: bool) -> Result<()>;
```

**src/process/monitor.rs:**
```rust
/// Parse NDJSON events from agent stdout in real-time.
/// Tracks token usage and can trigger cost circuit breaker.
pub async fn monitor_agent_stdout(
    stdout: ChildStdout,
    agent_name: &str,
    event_store: &EventStore,
    budget_limit: Option<f64>,
) -> Result<AgentOutcome>;

pub enum AgentOutcome {
    Completed { exit_code: i32 },
    BudgetExceeded { cost_so_far: f64 },
    Crashed { signal: Option<i32> },
}
```

### 3. `src/agents/overlay.rs` — Overlay Template Rendering

Reference: `reference/overlay.ts`

Read `templates/overlay.md.tmpl` and replace `{{VARIABLE}}` placeholders with values:
- `{{AGENT_NAME}}`, `{{CAPABILITY}}`, `{{TASK_ID}}`, `{{BRANCH_NAME}}`
- `{{WORKTREE_PATH}}`, `{{PARENT_AGENT}}`, `{{DEPTH}}`
- `{{FILE_SCOPE}}`, `{{SPEC_PATH}}`, `{{RUN_ID}}`
- `{{QUALITY_GATES}}`, `{{MULCH_EXPERTISE}}`, `{{VERIFICATION_CONFIG}}`

Also render the propulsion principle, cost awareness, failure modes, constraints, and communication protocol sections.

Write the rendered overlay to the runtime's instruction path in the worktree (e.g., `.claude/CLAUDE.md` for Claude runtime).

### 4. `src/agents/manifest.rs` — Agent Manifest

Reference: `reference/manifest.ts`

Parse `.overstory/agent-manifest.json`. This maps agent names to capabilities, models, and custom settings.

### 5. `src/runtimes/mod.rs` — Runtime Adapter Trait + Registry

Reference: `reference/runtimes-types.ts`, `reference/runtimes-registry.ts`, `reference/runtimes-claude.ts`

**The trait:**
```rust
pub trait AgentRuntime: Send + Sync {
    fn id(&self) -> &str;
    fn instruction_path(&self) -> &str;
    fn is_headless(&self) -> bool;

    /// Build command for headless spawn (stdin/stdout mode)
    fn build_headless_command(&self, opts: &SpawnOpts) -> Vec<String>;

    /// Build command for interactive tmux spawn
    fn build_interactive_command(&self, opts: &SpawnOpts) -> String;

    /// Deploy overlay + hooks to worktree
    fn deploy_config(&self, worktree: &Path, overlay: &str, hooks: &HooksDef) -> Result<()>;

    /// Detect TUI readiness from tmux pane content (only for interactive mode)
    fn detect_ready(&self, pane_content: &str) -> ReadyState;

    /// Build env vars for model/provider routing
    fn build_env(&self, model: &ResolvedModel) -> HashMap<String, String>;
}
```

**Implement for Claude runtime** (the primary one):
- `id()` → "claude"
- `instruction_path()` → ".claude/CLAUDE.md"
- `is_headless()` → true (Claude Code supports `-p` mode)
- `build_headless_command()` → `["claude", "-p", "--model", model, "--allowedTools", ...]`
- `build_interactive_command()` → `"claude --model {model} --allowedTools Edit,Bash,Write,..."`
- `deploy_config()` → write CLAUDE.md + settings.local.json with hooks
- `detect_ready()` → check for "❯" and "bypass permissions"

**Stub implementations for other runtimes** (codex, gemini, pi, copilot, sapling, opencode) — just the trait impl with the correct id, instruction_path, and build_command. Full implementations can come later.

**Registry:**
```rust
pub fn get_runtime(name: &str) -> Result<Box<dyn AgentRuntime>>;
pub fn resolve_runtime(config: &OverstoryConfig) -> Result<Box<dyn AgentRuntime>>;
```

### 6. `src/commands/sling.rs` — Agent Spawning (THE HUB)

Reference: `reference/sling.ts` (1,130 lines)

This is the central command. The flow:

```
1. Load config
2. Validate capability, task ID, file scope
3. Generate agent name (if not provided)
4. Create git worktree + branch from canonical
5. Build overlay from template + context
6. Get runtime adapter
7. Deploy overlay + hooks to worktree
8. Register session in sessions.db (state: "booting")
9. Choose spawn path:
   a. If runtime.is_headless() → spawn_headless(), pipe stdin/stdout
   b. Else → create_tmux_session(), send beacon via tmux
10. Update session state to "working"
11. If headless, start background monitor task for NDJSON events
12. Print result (agent name, worktree, branch, tmux/pid)
```

**CLI:**
```
grove sling <task-id> --capability <cap> [--name <n>] [--spec <path>] [--files <csv>]
    [--parent <agent>] [--depth <n>] [--model <m>] [--runtime <r>]
    [--skip-task-check] [--no-scout-check] [--no-directives] [--base-branch <b>]
```

### 7. `src/commands/log.rs` — Session Lifecycle

Reference: `reference/log.ts`

Handles the hooks that fire at session start and session end:
- `grove log session-start --agent <n>` — mark session as working, record start event
- `grove log session-end --agent <n> [--exit-code <n>]` — mark session as completed, record metrics (tokens, cost from transcript), record end event

This is called by the hooks system (settings.local.json) when an agent starts and stops.

### 8. `src/watchdog/mod.rs` — Health Monitoring

Reference: `reference/watchdog-daemon.ts`, `reference/watchdog-triage.ts`

Daemon that polls agent health:
- Check each "working" session: is the PID alive? Is the tmux session alive?
- Detect stale agents (no activity for `staleThresholdMs`)
- Detect zombie agents (no activity for `zombieThresholdMs`)
- Auto-nudge stale agents
- Auto-kill zombie agents
- Record health events

**Not a full implementation** — just the core poll loop and stale/zombie detection. Tier-0 watchdog only.

## File Scope

New files:
- `src/worktree/mod.rs`, `src/worktree/git.rs`, `src/worktree/tmux.rs`
- `src/process/mod.rs`, `src/process/spawn.rs`, `src/process/monitor.rs`
- `src/agents/mod.rs`, `src/agents/overlay.rs`, `src/agents/manifest.rs`
- `src/runtimes/mod.rs`, `src/runtimes/claude.rs`, `src/runtimes/registry.rs`
- `src/commands/sling.rs`
- `src/commands/log.rs`
- `src/watchdog/mod.rs`

Modified files:
- `src/main.rs` — wire sling, log, and other new commands
- `src/commands/mod.rs` — register new modules

## Quality Gates

- `cargo build` — clean
- `cargo test` — all tests pass (existing 247 + new)
- `cargo clippy -- -D warnings`

## Acceptance Criteria

1. `grove sling test-task --capability builder --name test-builder --spec .overstory/specs/test.md --files src/main.rs` creates a worktree, writes overlay, registers session — even if the actual agent runtime isn't available, the worktree + session + overlay should be created
2. The overlay written to `.claude/CLAUDE.md` in the worktree contains the agent name, task ID, file scope, and spec path
3. `grove log session-start --agent test-builder` updates session state to "working"
4. `grove log session-end --agent test-builder --exit-code 0` updates session to "completed" and records metrics
5. `grove status` shows the test-builder session
6. `grove clean --worktrees --force` removes the test worktree
7. Runtime registry returns Claude adapter by default, stubs for others
8. All existing 247 tests still pass
