# Phase 4 Bugfix: Coordinator + Observability Commands

## Context

Phase 4 was built but has 6 bugs found during integration testing. The commands are implemented and wired in main.rs. This bugfix phase addresses the bugs without rebuilding everything.

## What Already Works

- `grove worktree list` — lists worktrees correctly
- `grove group list` — reads groups.json correctly
- `grove feed --limit N` — queries events.db correctly
- `grove errors --limit N` — queries error events correctly
- `grove inspect <agent>` — shows agent details from sessions/events/mail
- `grove trace <agent>` — shows chronological event timeline
- `grove coordinator send` — delivers mail to coordinator mailbox
- All commands are wired in main.rs (no stubs for Phase 4 commands)

## Bugs To Fix

### BUG-1 CRITICAL: Coordinator exits immediately on start

**Current behavior:** `grove coordinator start --foreground` prints "event loop starting", immediately prints "exit trigger fired — shutting down", and exits.

**Root cause:** `should_exit()` in `src/coordinator/event_loop.rs` checks `allAgentsDone` and finds no active agents (besides coordinator), so it returns true on the first tick.

**Fix:** Add a `has_received_work: bool` field to `LoopContext`. Initialize it as `false`. Set it to `true` when the coordinator processes a `dispatch` type message. Only evaluate `allAgentsDone` exit trigger when `has_received_work` is `true`.

```rust
// In should_exit():
if ctx.exit_triggers.all_agents_done && ctx.has_received_work {
    // ... existing check
}
```

**File:** `src/coordinator/event_loop.rs`

### BUG-2: `grove agents` reads sessions.db instead of agent-manifest.json

**Current behavior:** After `grove clean --sessions`, `grove agents` says "No sessions database found" because it queries sessions.db for running agents.

**Expected behavior:** `grove agents` should list agent *definitions* from `.overstory/agent-manifest.json` — the capabilities available in this project (builder, scout, reviewer, lead, etc.) with their properties (file path, can_spawn, model overrides).

**Fix:** Rewrite `src/commands/agents.rs` to read and display the agent manifest. Use `crate::agents::manifest::load_manifest_from_project()` which already exists.

**File:** `src/commands/agents.rs`

### BUG-3: `grove group create` requires issue IDs

**Current behavior:** `grove group create test-group` fails with "At least one issue ID is required".

**Expected behavior:** Creating an empty group should work. Issues can be added later with `grove group add`.

**Fix:** In `src/commands/group.rs`, make the issues parameter optional. Create the group with an empty issues array if none provided.

**File:** `src/commands/group.rs`

### BUG-4: `grove run list` shows nothing

**Current behavior:** `grove run list` says "No runs recorded yet" even though runs exist.

**Fix:** Debug the query in `src/commands/run.rs`. The `runs` table in sessions.db should have records from previous coordinator sessions. Check if the RunStore query is correct, or if runs are being written to a different table/database.

**File:** `src/commands/run.rs`

### BUG-5: Coordinator state shows "completed" immediately

**Current behavior:** After `grove coordinator start --no-attach`, `grove coordinator status` shows state "completed".

**Root cause:** This is a consequence of BUG-1. The foreground process exits immediately, the session-end hook fires, and the state is updated to "completed". Once BUG-1 is fixed, the process will stay alive and state will remain "working".

**Fix:** Fixed by BUG-1. No additional changes needed.

### BUG-6: Replace tmux-based coordinator with daemon mode

**Current behavior:** Coordinator creates a tmux session to run `grove coordinator start --foreground`.

**New behavior:** The coordinator should run as a background daemon process:

1. `grove coordinator start` (or `start --no-attach`):
   - Fork a background child process via `std::process::Command` with `.spawn()`
   - The child runs `grove coordinator start --foreground --project <root>`
   - Write the child PID to `.overstory/coordinator.pid`
   - Redirect stdout/stderr to `.overstory/logs/coordinator.log`
   - Parent prints "coordinator started, PID: N" and exits

2. `grove coordinator status`:
   - Read `.overstory/coordinator.pid`
   - Check if PID is alive (`kill -0`)
   - Read last 5 lines of `.overstory/logs/coordinator.log`
   - Query sessions.db for coordinator session record

3. `grove coordinator stop`:
   - Read `.overstory/coordinator.pid`
   - Send SIGTERM to PID
   - Wait up to 5s for process to exit
   - Remove PID file
   - Update session state to "completed"

4. `grove coordinator logs`:
   - New subcommand: tail `.overstory/logs/coordinator.log`
   - With `--follow`: use `tail -f` equivalent (poll file for new content)

Remove all tmux references from `src/commands/coordinator.rs`. The coordinator does not need tmux.

**Files:** `src/commands/coordinator.rs`, `src/coordinator/event_loop.rs`

## File Scope

- `src/coordinator/event_loop.rs` — BUG-1 (has_received_work flag)
- `src/commands/agents.rs` — BUG-2 (read manifest instead of sessions)
- `src/commands/group.rs` — BUG-3 (allow empty groups)
- `src/commands/run.rs` — BUG-4 (fix run query)
- `src/commands/coordinator.rs` — BUG-5 (consequence of BUG-1), BUG-6 (daemon mode replacing tmux)
- `src/main.rs` — may need updates for new coordinator logs subcommand

## Quality Gates

- `cargo build` — clean compilation, zero errors
- `cargo test` — all existing tests pass + new tests for bugfixes
- `cargo clippy -- -D warnings` — no warnings

## Verification Commands

Run ALL of these after fixing. Every single one must pass.

```bash
G=./target/debug/grove

# BUG-1: Coordinator stays alive
$G coordinator start --no-attach
sleep 5
PID=$(cat .overstory/coordinator.pid 2>/dev/null)
kill -0 $PID 2>/dev/null && echo "BUG-1 PASS: coordinator alive (PID $PID)" || echo "BUG-1 FAIL: coordinator dead"

# BUG-1 continued: status shows working
STATE=$($G coordinator status --json 2>&1 | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('state','unknown'))" 2>/dev/null)
[ "$STATE" = "working" ] && echo "BUG-1 PASS: state is working" || echo "BUG-1 FAIL: state is $STATE"

# BUG-1 continued: send message, verify coordinator processes it
$G coordinator send --subject "bugfix test" --body "hello"
sleep 3
$G mail check --agent coordinator
$G coordinator stop
sleep 2
kill -0 $PID 2>/dev/null && echo "BUG-1 FAIL: still alive after stop" || echo "BUG-1 PASS: stopped cleanly"

# BUG-2: agents reads manifest
$G agents 2>&1 | head -5
$G agents 2>&1 | grep -qi "builder\|scout\|lead" && echo "BUG-2 PASS" || echo "BUG-2 FAIL: no agent definitions shown"

# BUG-3: empty group creation
$G group create empty-test-group 2>&1
$G group list 2>&1 | grep "empty-test-group" && echo "BUG-3 PASS" || echo "BUG-3 FAIL"

# BUG-4: run list shows runs
$G run list 2>&1
# Note: may need to create a run first via grove sling to have data

# BUG-6: daemon mode (no tmux)
$G coordinator start --no-attach
sleep 3
[ -f .overstory/coordinator.pid ] && echo "BUG-6 PASS: PID file exists" || echo "BUG-6 FAIL: no PID file"
[ -f .overstory/logs/coordinator.log ] && echo "BUG-6 PASS: log file exists" || echo "BUG-6 FAIL: no log file"
tmux list-sessions 2>&1 | grep -q "grove-coordinator" && echo "BUG-6 FAIL: tmux session exists (should not)" || echo "BUG-6 PASS: no tmux session"
$G coordinator stop

# No stubs for Phase 4 commands
for cmd in agents "worktree list" "group list" "run list" "feed --limit 1" "errors --limit 1" "inspect x" "trace x" "coordinator status"; do
  result=$($G $cmd 2>&1 | head -1)
  echo "$result" | grep -q "not yet implemented" && echo "FAIL: grove $cmd still a stub" || echo "OK: grove $cmd"
done

# Cargo quality gates
cargo build && echo "BUILD PASS" || echo "BUILD FAIL"
cargo test && echo "TEST PASS" || echo "TEST FAIL"  
cargo clippy -- -D warnings && echo "CLIPPY PASS" || echo "CLIPPY FAIL"
```

## Acceptance Criteria

1. Coordinator stays alive after `start --no-attach` — process persists, PID file written, log file created
2. Coordinator state is "working" while running, "completed" after stop
3. `grove agents` lists agent definitions from manifest (builder, scout, reviewer, lead, etc.)
4. `grove group create <n>` works without issue IDs
5. `grove run list` shows existing runs
6. No tmux session created for coordinator — uses daemon mode with PID file and log file
7. All verification commands above pass
8. All existing tests still pass
