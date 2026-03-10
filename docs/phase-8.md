# Phase 8: Native Agent Spawning + Coordinator Integration Test

## Context

Grove's architecture claims direct process spawning without tmux, and a native Rust coordinator event loop. Neither has been tested end-to-end with a real Claude Code agent. This phase proves the architecture works.

## Deliverables

### 1. Fix the TUI Timeline View (key routing)

The timeline view (`View::Timeline`) exists but pressing `5` from overview doesn't navigate to it reliably. Debug and fix the key routing in `app.rs`. The `handle_key_overview` already has `KeyCode::Char('5') => self.current_view = View::Timeline` — verify it actually triggers and the view renders.

### 2. Native Agent Spawning (the big one)

Currently `grove sling` creates a worktree, renders the overlay, then calls `runtimes/claude.rs` to build a spawn command. The runtime builds a tmux session command because that's how overstory works.

**Add a `--headless` path to sling that bypasses tmux:**

When `grove sling --headless` is used (or when no tmux is available):
1. Create worktree (already works)
2. Render overlay (already works)  
3. Spawn `claude --dangerously-skip-permissions -p "Read .claude/CLAUDE.md and begin your task"` as a child process via `process::spawn::spawn_child()`
4. Capture stdout via `process::monitor::Monitor`
5. Write NDJSON events to events.db as they arrive
6. Register the PID in sessions.db
7. The watchdog monitors process health

This means:
- `src/runtimes/claude.rs` gets a `spawn_headless()` method that returns a `std::process::Child`
- `src/commands/sling.rs` gains a `--headless` flag
- When headless, sling spawns the child directly instead of creating a tmux session
- The PID from the child process is stored in the session

### 3. Coordinator Drives a Real Build

Test the coordinator event loop end-to-end:

1. `grove coordinator start --no-attach` (already works as daemon)
2. `grove coordinator send --subject "Test task" --body "..."` sends a task
3. The coordinator's event loop receives the mail
4. The planner (src/coordinator/planner.rs) decomposes the task — this currently calls the Claude API for one-shot planning
5. The coordinator spawns agents via sling (headless)
6. Agents complete their work
7. The coordinator merges their branches
8. The coordinator reports completion

For this test, we can use a simple task that doesn't require AI: have the coordinator spawn a single builder that runs `echo "hello" > test.txt && git add -A && git commit -m "test"`.

### 4. TUI Terminal View — Read from Log Files

Per RETRO-017, the terminal viewer uses `tmux capture-pane`. For headless agents, add a fallback: read from `.overstory/logs/<agent>.log` if no tmux session exists. 

In `app.rs`, change `capture_tmux()` to `capture_agent_output()`:
1. Try tmux capture (backward compat with overstory agents)
2. If no tmux session, read from agent log file
3. If no log file, show "no output available"

### 5. Pre-commit Hook for Conflict Markers (RETRO-023)

Add to `grove hooks install`:
- A git pre-commit hook that rejects commits containing `<<<<<<< ` in .rs files
- The hook is a shell script written to `.git/hooks/pre-commit`

## File Scope

Modified files:
- `src/commands/sling.rs` — add `--headless` flag, direct spawn path
- `src/runtimes/claude.rs` — add `spawn_headless()` returning Child
- `src/coordinator/event_loop.rs` — spawn agents, merge branches, report completion
- `src/coordinator/planner.rs` — ensure planner output feeds into sling
- `src/tui/app.rs` — capture_agent_output fallback, timeline key routing fix
- `src/tui/views/timeline.rs` — verify rendering
- `src/commands/hooks.rs` — add pre-commit conflict marker check

## Verification Commands

```bash
G=./target/debug/grove

# 1. Timeline view
tmux new-session -d -s tui-timeline "$G dashboard"
sleep 2
tmux send-keys -t tui-timeline -l '5'
sleep 1
tmux capture-pane -t tui-timeline -p | head -5
# Should show TIMELINE view, not cost analytics or overview
tmux send-keys -t tui-timeline -l 'q'

# 2. Headless sling
$G sling headless-test --capability builder --name headless-agent \
  --headless --skip-task-check --no-scout-check --files src/main.rs
$G status | grep headless-agent
# Should show agent without tmux session

# 3. Pre-commit hook
$G hooks install --force
echo "<<<<<<< HEAD" >> /tmp/test-conflict.rs
# Hook should reject

# 4. Version
$G --version
# Should show: grove 0.1.0 (hash)

# Quality gates
cargo build && cargo test && cargo clippy -- -D warnings
```

## Acceptance Criteria

1. `grove sling --headless` spawns a Claude Code agent as a direct child process (no tmux)
2. The agent's stdout is captured and written to events.db
3. The agent's PID is tracked in sessions.db
4. The TUI terminal viewer falls back to log files for headless agents
5. The coordinator event loop can receive a task, spawn an agent, and detect completion
6. Timeline view renders when pressing `5`
7. Pre-commit hook rejects conflict markers
8. `grove --version` shows `0.1.0 (hash)`
