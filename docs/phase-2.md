# Phase 2: Write Commands

Phase 1 proved grove can read real `.overstory/` databases. Now we make it write to them.

## Context

Phase 0 and 1 are complete. On main we have:
- `src/types.rs` — all shared types with serde
- `src/config.rs` — YAML config loader
- `src/errors.rs` — thiserror errors
- `src/db/` — all 5 database stores (sessions, mail, events, metrics, merge_queue)
- `src/commands/status.rs` — working, reads real databases
- `src/commands/mail.rs` — `mail list` and `mail check` working
- `src/commands/costs.rs` — working, reads metrics.db
- `src/commands/doctor.rs` — working, checks dependencies
- `src/main.rs` — all 35 commands registered in clap
- `src/logging/` — brand colors, formatters
- `src/json.rs` — JSON output envelope

167 tests passing. Compiles clean.

## Deliverables

### 1. `grove mail send` + `grove mail reply` + `grove mail read` + `grove mail purge`

Reference: `reference/mail-store.ts`, existing `src/commands/mail.rs` and `src/db/mail.rs`

**mail send:**
```
grove mail send --to <agent> --subject <subject> --body <body> [--type <type>] [--priority <priority>] [--from <agent>] [--thread <id>] [--payload <json>]
```
- Inserts into mail.db messages table
- Auto-generates UUID for message ID
- `--from` defaults to "operator" (human user)
- `--type` defaults to "status", valid: status, question, result, error, dispatch, worker_done, merge_ready, merged, merge_failed, escalation, health_check, assign
- `--priority` defaults to "normal", valid: low, normal, high, urgent
- Prints confirmation with message ID

**mail reply:**
```
grove mail reply <message-id> --body <body> [--type <type>]
```
- Reads the original message to get from/to (swaps them), subject (prepends "Re: "), thread_id
- Inserts reply

**mail read:**
```
grove mail read <message-id>
```
- Displays full message (from, to, subject, body, type, priority, time, payload)
- Marks as read

**mail purge:**
```
grove mail purge [--agent <name>] [--all]
```
- Deletes messages. `--agent` purges for one agent, `--all` purges everything.

**mail check --inject:**
```
grove mail check --agent <name> --inject
```
- Returns unread messages formatted for injection into an agent's context (the hook format overstory uses)
- Marks them as read

### 2. `grove clean`

Reference: `reference/` doesn't have clean.ts — read the overstory source at the TypeScript level description from docs/phase-0.md and docs/architecture.md.

```
grove clean [--worktrees] [--sessions] [--mail] [--events] [--metrics] [--merge-queue] [--all] [--force]
```

Nuclear cleanup command. Each flag targets a specific resource:
- `--worktrees` — delete all `.overstory/worktrees/*`, run `git worktree prune`, kill associated tmux sessions
- `--sessions` — delete sessions.db (or truncate the sessions table)
- `--mail` — delete all messages from mail.db
- `--events` — delete all events from events.db
- `--metrics` — delete all from metrics.db
- `--merge-queue` — delete all from merge-queue.db
- `--all` — all of the above
- `--force` — skip confirmation prompt

For worktree cleanup:
1. List all directories in `.overstory/worktrees/`
2. For each, find the associated tmux session (query sessions.db for tmux_session column)
3. Kill the tmux session: `tmux kill-session -t <name>`
4. Remove the worktree directory: `rm -rf`
5. Run `git worktree prune`
6. Delete associated git branches: `git branch -D <branch>`
7. Log synthetic session-end events to sessions.db before deleting

Print summary: "Killed N tmux sessions, Removed N worktrees, Wiped sessions.db" etc.

### 3. `grove stop`

```
grove stop <agent-name> [--force] [--signal <signal>]
```

- Look up agent in sessions.db by agent_name
- Get its PID and tmux_session
- Send SIGTERM to PID (or --signal override)
- If --force, also kill the tmux session
- Update session state to "completed" in sessions.db
- Print confirmation

### 4. `grove nudge`

```
grove nudge <agent-name> <message>
```

- Look up agent's tmux_session in sessions.db
- Send the message text to the tmux session via: `tmux send-keys -t <session> "<message>" Enter`
- Print confirmation

### 5. `grove init`

```
grove init [--name <project-name>] [--yes]
```

- Create `.overstory/` directory structure:
  - `.overstory/config.yaml` (with defaults)
  - `.overstory/agents/` (copy agent definitions)
  - `.overstory/agent-defs/` (copy agent definitions)
  - `.overstory/worktrees/`
  - `.overstory/specs/`
  - `.overstory/logs/`
  - `.overstory/agent-manifest.json` (empty manifest)
  - `.overstory/hooks.json`
  - `.overstory/.gitignore` (ignore *.db, worktrees/)
  - `.overstory/README.md`
- Default config should have:
  - project.name from --name or directory name
  - project.root as absolute path to CWD
  - project.canonicalBranch as "main"
  - Sensible defaults for all other fields
- If --yes, skip confirmation
- Git commit the scaffold

### 6. `grove spec write`

```
grove spec write <task-id> --body <content> [--file <path>]
```

- Write a spec file to `.overstory/specs/<task-id>.md`
- Content from --body or read from --file
- Print path to created spec

### 7. `grove hooks install` / `grove hooks uninstall`

```
grove hooks install [--runtime <runtime>]
grove hooks uninstall
```

- For Claude runtime: write `.claude/settings.local.json` with hooks configuration
- The hooks JSON defines session-start and session-end hooks that call `grove log` commands
- For uninstall: remove the hooks from settings.local.json

## File Scope

New files:
- `src/commands/clean.rs`
- `src/commands/stop.rs`
- `src/commands/nudge.rs`
- `src/commands/init.rs`
- `src/commands/spec.rs`
- `src/commands/hooks.rs`

Modified files:
- `src/commands/mail.rs` — add send, reply, read, purge, check --inject
- `src/commands/mod.rs` — register new modules
- `src/main.rs` — wire new commands to implementations

## Quality Gates

- `cargo build` — clean, zero errors
- `cargo test` — all existing 167 tests still pass + new tests for write operations
- `cargo clippy -- -D warnings` — no warnings

## Testing Strategy

For write commands, tests should:
1. Create a temp directory with `.overstory/` structure
2. Create in-memory or temp-file SQLite databases
3. Run the write operation
4. Query the database to verify the write landed correctly
5. For `clean`, verify files are actually deleted

Integration test: run `grove mail send` then `grove mail list` and verify the sent message appears.

## Acceptance Criteria

1. `grove mail send --to agent1 --subject "test" --body "hello"` inserts a message and prints the ID
2. `grove mail list` shows the newly sent message
3. `grove mail read <id>` displays full message and marks as read
4. `grove clean --all --force` wipes all databases and worktrees cleanly
5. `grove stop <agent>` terminates the agent and updates sessions.db
6. `grove nudge <agent> "keep going"` sends text to the agent's tmux session
7. `grove init --name myproject --yes` creates a valid `.overstory/` directory
8. All writes are compatible — overstory can read what grove writes and vice versa
