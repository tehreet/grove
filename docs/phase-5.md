# Phase 5: Feature Parity — Remaining Commands

## Context

Phases 0-4 are complete and verified. Grove has 18,948 lines of Rust, 354 tests, 23 working commands. This phase implements the 8 remaining stub commands (excluding dashboard, which is Phase 6, and supervisor, which is deprecated).

After this phase, every overstory command has a working grove equivalent.

## Reference

All reference TypeScript implementations are in `reference/` (logs.ts, replay.ts, metrics.ts, monitor.ts, watch.ts, prime.ts, ecosystem.ts). Run `ov <command> --help` against the overstory repo at `/home/joshf/overstory` to see exact flag names and behavior.

## Deliverables

### 1. `grove logs` — NDJSON log queries

Reference: `reference/logs.ts` (488 lines), `ov logs --help`

```
grove logs [--agent <n>] [--level <level>] [--since <timestamp>] [--until <timestamp>] [--limit <n>] [--json]
```

Query events.db and display as structured log lines. Format: `HH:MM:SS LEVEL event_type [agent_name] toolName=X durationMs=Y args={...}`

- Default limit: 100
- Levels: debug, info, warn, error
- `--agent` filters by agent_name
- `--since` / `--until` filter by created_at timestamp
- `--json` outputs raw StoredEvent JSON array
- Sort by created_at DESC (newest first)

Overstory output format (match this):
```
15:12:34 INF tool.end [builder-name] toolName=Bash durationMs=1601
15:12:35 INF tool.start [lead-name] toolName=Bash args={"command":"..."}
```

### 2. `grove replay` — Interleaved chronological replay

Reference: `reference/replay.ts` (231 lines), `ov replay --help`

```
grove replay [--run <id>] [--agent <n>...] [--since <timestamp>] [--until <timestamp>] [--limit <n>] [--json]
```

Similar to `logs` but interleaves events from multiple agents chronologically. Groups events by date. Includes mail_sent events with full payload display.

- Default limit: 200
- `--agent` can appear multiple times to filter to specific agents
- `--run` filters by run_id
- Shows date headers: `--- 2026-03-09 ---`
- Events formatted with relative time: `1d ago MAIL SENT [agent] data={...}`

Overstory output format (match this):
```
Replay
──────────────────────────────────────────────────────────────────────
5 events

--- 2026-03-08 ---
    1d ago MAIL SENT  [builder-task-1] data={"to":"coordinator","subject":"Worker done",...}
```

### 3. `grove metrics` — Session metrics display

Reference: `reference/metrics.ts` (129 lines), `ov metrics --help`

```
grove metrics [--last <n>] [--json]
```

Read metrics.db sessions table. Display:
- Total sessions count
- Completed count
- Average duration
- Breakdown by capability (count + avg duration per capability)
- Recent sessions list (agent name, capability, task ID, status, duration)

Overstory output format (match this):
```
Session Metrics
──────────────────────────────────────────────────────────────────────

Total sessions: 5
Completed: 5
Avg duration: 4m 33s

By capability:
  builder: 3 sessions (avg 3m 37s)
  lead: 1 sessions (avg 15m 48s)

Recent sessions:
  builder-name [builder] task-id | done | 9m 56s
```

### 4. `grove monitor start/stop/status` — Persistent monitor agent

Reference: `reference/monitor.ts` (402 lines), `ov monitor --help`

```
grove monitor start [--no-attach]
grove monitor stop
grove monitor status [--json]
```

The monitor is a persistent agent (like the coordinator) that provides Tier 2 health monitoring — an LLM-powered watchdog that can reason about agent behavior. For grove, implement the lifecycle management (start/stop/status) using the same daemon pattern as the coordinator (PID file + log file, no tmux):

- `start`: Spawn `grove monitor start --foreground` as background process, write PID to `.overstory/monitor.pid`, log to `.overstory/logs/monitor.log`
- `stop`: Read PID, send SIGTERM, clean up
- `status`: Read PID, check alive, show state from sessions.db

The actual monitoring logic can be minimal for now — a poll loop that runs the watchdog health checks. Full LLM-powered monitoring is a later enhancement.

### 5. `grove watch` — Mechanical watchdog daemon

Reference: `reference/watch.ts` (258 lines), `ov watch --help`

```
grove watch [--interval <ms>] [--background] [--json]
```

Tier 0 mechanical watchdog. Polls agent health on an interval:
- Check each "working" session: is the PID alive?
- Detect stale agents (last_activity older than staleThresholdMs from config)
- Detect zombie agents (last_activity older than zombieThresholdMs from config)
- Update session state for stale/zombie agents
- Log health check events to events.db

`--background` daemonizes (fork + PID file at `.overstory/watchdog.pid`)
Default interval: from config `watchdog.tier0IntervalMs` (30000ms)

Note: `src/watchdog/mod.rs` already exists with basic health check logic from Phase 3. This command wires it to the CLI.

### 6. `grove prime` — Context loading

Reference: `reference/prime.ts` (357 lines), `ov prime --help`

```
grove prime [--agent <n>] [--compact] [--json]
```

Outputs a context summary for an agent or the project:
- Project name, canonical branch, config summary
- Agent manifest (capabilities available)
- Active agents (from sessions.db)
- Recent events summary
- Mulch expertise summary (call `mulch prime` subprocess if mulch is enabled)

`--agent` primes context for a specific agent (includes their mail, events, session info)
`--compact` outputs reduced context (for PreCompact hook — shorter format)
`--json` outputs as JSON

Overstory output format (match this):
```
# Overstory Context

## Project: grove
Canonical branch: main
Max concurrent agents: 25
Max depth: 2

## Agent Manifest
- **scout** [haiku]: explore, research
- **builder** [sonnet]: implement, refactor, fix
```

### 7. `grove ecosystem` — Ecosystem tool info

Reference: `reference/ecosystem.ts` (292 lines), `ov ecosystem --help`

```
grove ecosystem [--json]
```

Shows status of all os-eco ecosystem tools:
- overstory/grove: version, doctor summary
- mulch: version (run `mulch --version`)
- seeds: version (run `sd --version`)
- canopy: version (run `cn --version`)

For each tool, check if the binary exists in PATH, get version, show status.

Overstory output format (match this):
```
os-eco Ecosystem
══════════════════════════════════════════════════════════════════════

  - overstory (ov) / grove
    Version: 0.1.0
    Doctor:  14 passed, 0 warn

  - mulch (ml)
    Version: 0.6.3

  - seeds (sd)
    Version: 0.2.5

  - canopy (cn)
    Version: 0.2.2
```

### 8. `grove supervisor` — Deprecated

```
grove supervisor
```

Print: "grove supervisor: deprecated. Use `grove coordinator` instead." and exit 0. Don't implement any logic.

## File Scope

New files:
- `src/commands/logs.rs`
- `src/commands/replay.rs`
- `src/commands/metrics_cmd.rs` (to avoid collision with src/db/metrics.rs)
- `src/commands/monitor.rs`
- `src/commands/watch_cmd.rs` (to avoid collision with reserved word)
- `src/commands/prime.rs`
- `src/commands/ecosystem.rs`

Modified files:
- `src/commands/mod.rs` — register new modules
- `src/main.rs` — wire all commands, remove all not_yet_implemented stubs for these commands
- `src/commands/stop.rs` — may need update to handle supervisor deprecation

## Quality Gates

- `cargo build` — clean compilation
- `cargo test` — all existing 354 tests pass + new tests for each command
- `cargo clippy -- -D warnings`

## Verification Commands

```bash
G=./target/debug/grove

# 1. logs
$G logs --limit 5 2>&1 | head -8
$G logs --limit 5 2>&1 | grep -q "tool" && echo "PASS: logs shows events" || echo "FAIL"
$G logs --json --limit 3 2>&1 | python3 -c "import sys,json; d=json.load(sys.stdin); print(f'JSON ok: {len(d)} items')" 2>&1

# 2. replay
$G replay --limit 5 2>&1 | head -8
$G replay --limit 5 2>&1 | grep -q "Replay\|events" && echo "PASS: replay works" || echo "FAIL"

# 3. metrics
$G metrics 2>&1 | head -10
$G metrics 2>&1 | grep -qi "total sessions\|session" && echo "PASS: metrics shows data" || echo "FAIL"
$G metrics --json 2>&1 | python3 -c "import sys,json; json.load(sys.stdin); print('JSON ok')" 2>&1

# 4. monitor
$G monitor start --no-attach 2>&1
sleep 3
[ -f .overstory/monitor.pid ] && echo "PASS: monitor PID file" || echo "FAIL"
$G monitor status 2>&1 | head -3
$G monitor stop 2>&1

# 5. watch
$G watch --json 2>&1 | head -3
# Note: watch in non-background mode runs one check and prints results

# 6. prime
$G prime 2>&1 | head -10
$G prime 2>&1 | grep -qi "project\|grove" && echo "PASS: prime shows context" || echo "FAIL"
$G prime --json 2>&1 | python3 -c "import sys,json; json.load(sys.stdin); print('JSON ok')" 2>&1

# 7. ecosystem
$G ecosystem 2>&1 | head -10
$G ecosystem 2>&1 | grep -qi "mulch\|seeds\|canopy" && echo "PASS: ecosystem shows tools" || echo "FAIL"

# 8. supervisor deprecated
$G supervisor 2>&1 | grep -qi "deprecated" && echo "PASS: supervisor deprecated" || echo "FAIL"

# 9. NO STUBS REMAINING for Phase 5 commands
for cmd in logs "replay --limit 1" metrics "monitor status" watch prime ecosystem supervisor; do
  result=$($G $cmd 2>&1 | head -1)
  echo "$result" | grep -q "not yet implemented" && echo "FAIL: grove $cmd still stub" || echo "OK: grove $cmd"
done

# Quality gates
cargo build && echo "BUILD PASS" || echo "BUILD FAIL"
cargo test && echo "TEST PASS" || echo "TEST FAIL"
cargo clippy -- -D warnings && echo "CLIPPY PASS" || echo "CLIPPY FAIL"
```

## Acceptance Criteria

1. All 8 commands produce meaningful output (not stubs)
2. `grove logs` output format matches overstory's `ov logs` (timestamps, levels, agent names, tool info)
3. `grove replay` interleaves events chronologically with date headers
4. `grove metrics` shows session counts, durations, capability breakdown
5. `grove monitor` uses daemon mode (PID file, log file, no tmux)
6. `grove watch` runs health checks and detects stale/zombie agents
7. `grove prime` outputs project context summary matching overstory's format
8. `grove ecosystem` detects installed os-eco tools and shows versions
9. All `--json` flags produce valid JSON with "success" and "command" fields
10. All existing 354 tests still pass + new tests per command
11. Zero `not_yet_implemented` stubs remain for Phase 5 commands
