# Phase 8.5: Headless Lifecycle Completion + End-to-End Verification

## Context

Phase 8 proved headless spawning works — an agent launched as a direct child process, read its overlay, did work, and committed. But the lifecycle is incomplete: session state doesn't transition, stdout isn't captured to log files, process death isn't detected, and the watchdog doesn't monitor headless PIDs.

This phase closes every gap. After this, `grove sling --headless` is production-ready.

## Deliverables

### 1. Headless Session State Transitions

When `grove sling --headless` spawns a child process:

**On spawn (immediately after process starts):**
- Call the equivalent of `grove log session-start --agent <name>` programmatically
- Session transitions from `booting` → `working`

**On process exit (when child's wait() returns):**
- Read the exit code
- If exit 0: call `grove log session-end --agent <name> --exit-code 0` → state becomes `completed`
- If exit non-zero: → state becomes `zombie`
- If process killed/crashed: → state becomes `zombie`

**Implementation:**
In `src/commands/sling.rs`, after spawning the child process with `--headless`:
1. Immediately update session state to `working` via `SessionStore::update_state()`
2. Spawn a background thread (or tokio task) that calls `child.wait()`
3. When wait() returns, update session state to `completed` or `zombie` based on exit code
4. The sling command itself returns immediately after spawning (don't block)

### 2. Stdout Capture to Log File

The headless child process's stdout and stderr must be captured to `.overstory/logs/<agent-name>.log`.

**Implementation:**
- When spawning the child, pipe stdout and stderr
- Spawn a reader thread that reads from the pipes and writes to the log file
- The log file is append-only, one line per output line, prefixed with timestamp
- Format: `[2026-03-10T14:09:27Z] <line>`
- This file is what the TUI terminal viewer reads via `capture_agent_output()`

In `src/commands/sling.rs` headless path:
```rust
let mut child = Command::new("claude")
    .args(["--dangerously-skip-permissions", "-p", &prompt])
    .current_dir(&worktree_path)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()?;

let log_path = format!("{}/.overstory/logs/{}.log", project_root, agent_name);
// Spawn thread to read stdout/stderr and write to log_path
```

### 3. Process Death Detection in Watchdog

The watchdog (`src/watchdog/mod.rs`) currently monitors tmux sessions. Add headless process monitoring:

- For sessions where `tmux_session` is empty (headless agents), check if the PID is still alive using `kill(pid, 0)`
- If the PID is dead and session state is `working`, transition to `zombie`
- This handles cases where the background wait thread didn't fire (e.g., sling process itself died)

### 4. Coordinator Headless Integration

Update the coordinator event loop to use `--headless` when spawning agents:

In `src/coordinator/event_loop.rs`:
- When the coordinator needs to spawn an agent, use `grove sling <task> --headless --capability <cap> --name <name>`
- The coordinator should detect agent completion by polling session state (already does this)
- When all agents complete, the coordinator should merge their branches

### 5. NDJSON Event Emission from Headless Agents

The headless stdout capture should parse Claude Code's NDJSON output and write events to events.db:

- Claude Code outputs NDJSON lines to stdout (tool calls, results, etc.)
- Parse each line as JSON
- Extract event type (tool_start, tool_end, etc.) and write to EventStore
- This feeds the TUI feed panel and event log

This may be complex — as a minimum viable implementation:
- Parse lines that look like `{"type":"tool_use",...}` or `{"type":"tool_result",...}`
- Write a `tool_start` event when tool_use is seen, `tool_end` when tool_result is seen
- If parsing fails, just log the raw line to the log file

## File Scope

**Modified files:**
- `src/commands/sling.rs` — headless lifecycle (state transitions, stdout capture, background wait)
- `src/watchdog/mod.rs` — headless PID monitoring
- `src/coordinator/event_loop.rs` — use --headless for spawning
- `src/process/monitor.rs` — NDJSON parsing from stdout (if not already there)

**Do NOT modify:**
- `src/main.rs` — no new commands
- `src/tui/app.rs` — already has capture_agent_output fallback

## Verification Commands

```bash
G=./target/debug/grove

# 1. Headless lifecycle: spawn, working, completed
$G spec write lifecycle-test --body "Create lifecycle-proof.md with 'lifecycle works'. Commit it."
$G sling lifecycle-test --headless --capability builder --name lifecycle-agent \
  --skip-task-check --no-scout-check \
  --spec .overstory/specs/lifecycle-test.md --files lifecycle-proof.md

# Check state transitions
sleep 5
$G status --json | python3 -c "
import sys,json
d=json.load(sys.stdin)
for a in d['agents']:
    if a['agentName']=='lifecycle-agent':
        print(f'State: {a[\"state\"]}')
"
# Should be 'working' (not 'booting')

# Wait for completion
sleep 60
$G status --json | python3 -c "
import sys,json
d=json.load(sys.stdin)
for a in d['agents']:
    if a['agentName']=='lifecycle-agent':
        print(f'State: {a[\"state\"]}')
"
# Should be 'completed'

# 2. Log file exists
[ -f .overstory/logs/lifecycle-agent.log ] && echo "PASS: log file" || echo "FAIL"
wc -l .overstory/logs/lifecycle-agent.log

# 3. Commit exists
git log --oneline overstory/lifecycle-agent/lifecycle-test -3

# Quality gates
cargo build && cargo test && echo "ALL PASS"
```

## Acceptance Criteria

1. `grove sling --headless` transitions session: booting → working → completed
2. Stdout/stderr written to `.overstory/logs/<agent>.log`
3. Process death detected — zombie if crashed, completed if exit 0
4. TUI terminal viewer shows log file content for headless agents
5. Watchdog detects dead headless processes
6. All existing tests pass
