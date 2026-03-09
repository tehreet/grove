# Phase 1: Read-Only Commands

Implement the first working commands. These read databases and print formatted output. No mutations, no subprocesses. After this phase, `grove status`, `grove mail`, `grove costs`, and `grove doctor` produce real output.

## Prerequisites

Phase 0 is complete. Available in `src/`:
- `types.rs` — all shared types with serde derives
- `config.rs` — YAML config loader (use `load_config()`)
- `errors.rs` — thiserror error types
- `db/` — SessionStore, MailStore, EventStore, MetricsStore, MergeQueueStore (all tested)
- `logging/mod.rs` — brand colors, print helpers, format_duration, format_relative_time
- `json.rs` — json_output() envelope helper
- `main.rs` — clap CLI skeleton with all commands stubbed

## Critical Requirement: --json Compatibility

Every command that supports `--json` must produce output structurally compatible with overstory's `--json` output. The envelope format is:

```json
{
  "success": true,
  "command": "<command-name>",
  ...data fields spread at top level
}
```

Use `json_output()` from `src/json.rs` for this. The data fields vary per command — see reference TypeScript for each.

## Architecture Note

Commands should be implemented as functions in their `src/commands/` file that take the parsed CLI args and return `Result<()>`. The main.rs match arms call into these functions. Each command file should:

1. Load config via `load_config()`
2. Open the relevant database(s) via the stores in `src/db/`
3. Query data
4. Format and print (text mode or JSON mode)

Make each command module a separate file with `pub fn execute(args, json: bool, quiet: bool) -> Result<()>`.

## Deliverables

### 1. `src/commands/status.rs` — System status overview

Reference: `reference/status.ts` (665 lines)

This is the most-used command. It shows agents, worktrees, tmux sessions, and summary counts.

**What it reads:**
- sessions.db — all sessions (agents), sorted by state priority (working > booting > stalled > zombie > completed)
- git — list worktrees via `git worktree list --porcelain`
- tmux — list sessions via `tmux list-sessions -F "#{session_name}"` (if tmux is available)
- mail.db — count unread messages
- merge-queue.db — count pending entries
- metrics.db — count recent sessions

**Text output format (match overstory):**
```
Overstory Status
──────────────────────────────────────────────────────────────────────
Run: run-2026-03-09... │ 3 agents │ $12.50 │ 15:04:05

  > builder-types       builder    working    grove-c146      2m 30s   $3.50
  > config-builder      builder    completed  grove-c37a      5m 12s   $4.20
  ~ types-lead          lead       booting    grove-c146      0m 03s

Worktrees: 3 │ Tmux: 5 │ Unread mail: 2 │ Merge queue: 0
```

**JSON output format:**
```json
{
  "success": true,
  "command": "status",
  "currentRunId": "run-...",
  "agents": [
    {
      "id": "session-...",
      "agentName": "builder-types",
      "capability": "builder",
      "worktreePath": "/home/.../worktrees/builder-types",
      "branchName": "overstory/builder-types/grove-c146",
      "taskId": "grove-c146",
      "tmuxSession": "overstory-grove-builder-types",
      "state": "working",
      "pid": 12345,
      "parentAgent": "types-lead",
      "depth": 2,
      "runId": "run-...",
      "startedAt": "2026-03-09T18:54:25.684Z",
      "lastActivity": "2026-03-09T19:11:54.821Z",
      "escalationLevel": 0,
      "stalledSince": null,
      "transcriptPath": null
    }
  ],
  "worktrees": [
    { "path": "/home/.../grove", "head": "abc123", "branch": "main" }
  ],
  "tmuxSessions": [
    { "name": "overstory-grove-coordinator", "pid": 12345 }
  ],
  "unreadMailCount": 2,
  "mergeQueueCount": 0,
  "recentMetricsCount": 13
}
```

**Implementation notes:**
- Parse `git worktree list --porcelain` output: blocks separated by blank lines, each has `worktree <path>`, `HEAD <sha>`, `branch refs/heads/<name>`
- Parse `tmux list-sessions -F "#{session_name} #{session_id}"` — handle tmux not being installed gracefully (return empty list, don't error)
- State sort priority: working=0, booting=1, stalled=2, zombie=3, completed=4
- Duration: calculate from started_at to now (working/booting/stalled) or started_at to last_activity (completed/zombie)

### 2. `src/commands/mail.rs` — Mail system

Reference: `reference/mail-command.ts` (793 lines)

Subcommands: list, check, read, send, reply, purge. For Phase 1 implement list, check, and read only (read-only). Send/reply are Phase 2.

**`grove mail list`:**
- Lists all messages, newest first
- Flags: `--to <agent>`, `--from <agent>`, `--type <type>`, `--unread`, `--limit <n>`
- Text format: table with from, to, subject (truncated), type, priority, time
- JSON format: `{ "success": true, "command": "mail list", "messages": [...] }`

**`grove mail check --agent <name>`:**
- Returns unread messages for the given agent
- With `--inject`: prints messages in a special format that Claude Code hooks parse (multiline block per message with headers)
- JSON format: `{ "success": true, "command": "mail check", "messages": [...], "count": N }`

**`grove mail read <id>`:**
- Display a single message by ID with full body
- Marks it as read
- JSON format: the full message object

**Inject format (for --inject flag on check):**
```
──────────────────
From: types-lead
Subject: Task assigned
Type: status | Priority: normal
Date: 2026-03-09T19:00:00Z

Full body text here...
──────────────────
```

### 3. `src/commands/costs.rs` — Token costs and spending

Reference: `reference/costs.ts` (702 lines)

Shows token usage and estimated costs from metrics.db.

**What it reads:**
- metrics.db — sessions table (per-agent costs) and token_snapshots table (time-series)

**Modes:**
- Default: summary table of all sessions in current run
- `--agent <name>`: costs for a specific agent
- `--run <id>`: costs for a specific run
- `--live`: show latest token snapshots (live cost tracking)

**Text output:**
```
Cost Summary
──────────────────────────────────────────────────────────────────────
Agent                Cap        In Tok    Out Tok    Cache     Cost
──────────────────────────────────────────────────────────────────────
builder-types        builder    45,230    12,400     38,100    $3.50
config-builder       builder    38,100    10,200     32,000    $4.20
coordinator          coord      92,000    28,300     78,000    $45.80
──────────────────────────────────────────────────────────────────────
Total                           175,330   50,900     148,100   $53.50
```

**JSON format:**
```json
{
  "success": true,
  "command": "costs",
  "sessions": [...],
  "totals": { "inputTokens": N, "outputTokens": N, "cacheReadTokens": N, "estimatedCostUsd": N }
}
```

### 4. `src/commands/doctor.rs` — System health check

Reference: `reference/doctor.ts` (310 lines)

Checks that all dependencies and configuration are healthy.

**Checks to implement:**
1. **git** — `git --version`, verify >= 2.20
2. **tmux** — `tmux -V`, verify installed (warn if missing, don't fail)
3. **Agent runtime** — check configured runtime binary exists (`which claude`, `which pi`, etc.)
4. **.overstory/ directory** — exists, has config.yaml
5. **Databases** — sessions.db, mail.db, events.db, metrics.db, merge-queue.db exist (warn if missing, they're created on first use)
6. **Quality gates** — each gate command is executable (`which` the first word)
7. **Agent manifest** — .overstory/agent-manifest.json exists and parses

**Text output:**
```
Grove Doctor
──────────────────────────────────────────────────────────────────────
  ✓ git 2.43.0
  ✓ tmux 3.4
  ✓ claude (runtime)
  ✓ .overstory/ directory
  ✓ config.yaml
  ⚠ sessions.db (not yet created)
  ✓ mail.db
  ✓ events.db
  ✓ metrics.db
  ✓ merge-queue.db
  ✓ Quality gate: cargo test
  ✓ Quality gate: cargo clippy -- -D warnings
  ✓ agent-manifest.json (10 agents)
──────────────────────────────────────────────────────────────────────
  12 passed, 1 warning, 0 failed
```

**JSON format:**
```json
{
  "success": true,
  "command": "doctor",
  "checks": [
    { "name": "git", "status": "pass", "detail": "2.43.0" },
    { "name": "sessions.db", "status": "warn", "detail": "not yet created" }
  ],
  "summary": { "passed": 12, "warnings": 1, "failed": 0 }
}
```

### 5. Wire commands into main.rs

Update the match arms in `src/main.rs` for status, mail (list/check/read), costs, and doctor to call the real implementations instead of printing "not yet implemented". All other commands keep the stub.

### 6. `src/commands/mod.rs` — Module declarations

Create the mod.rs that exposes all command modules.

## File Scope

New files:
- `src/commands/mod.rs`
- `src/commands/status.rs`
- `src/commands/mail.rs`
- `src/commands/costs.rs`
- `src/commands/doctor.rs`

Modified files:
- `src/main.rs` — wire real command implementations

## Quality Gates

- `cargo build` — clean
- `cargo test` — all tests pass (unit tests for each command + existing 134 tests)
- `cargo clippy -- -D warnings` — no warnings
- `grove status` produces real output when run in a directory with `.overstory/`
- `grove status --json` produces valid JSON matching the schema above
- `grove doctor` checks pass on the current system
- `grove mail list` shows real messages from the grove .overstory/mail.db
- `grove costs` shows real cost data from grove .overstory/metrics.db

## Acceptance Test

Run these from `/home/joshf/grove` (which has a real `.overstory/` from Phase 0):

```bash
grove status                    # Shows agents/worktrees/counts
grove status --json             # Valid JSON with agents array
grove mail list                 # Shows messages from Phase 0 coordination
grove mail list --json          # Valid JSON with messages array
grove mail check --agent coordinator  # Shows unread for coordinator
grove costs                     # Shows token costs from Phase 0 build
grove costs --json              # Valid JSON with sessions and totals
grove doctor                    # All checks pass (git, tmux, .overstory/, dbs)
grove doctor --json             # Valid JSON with checks array
```

Every one of these should produce real output, not stubs or errors.
