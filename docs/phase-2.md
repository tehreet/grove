# Phase 2: Write Commands

Build the commands that mutate state — databases, filesystem, tmux sessions, git operations. These are the commands users run to manage their agents and merge their work.

Reference TypeScript is in `reference/`. Read it for behavior, write idiomatic Rust.

Rust toolchain is at `~/.cargo/bin/`. Quality gates: `cargo build`, `cargo test`, `cargo clippy -- -D warnings`.

## Deliverables

### 1. `grove mail send` + `grove mail reply` + `grove mail purge`

**File:** `src/commands/mail.rs` (extend existing)

`grove mail send`:
- Flags: `--to <agent>`, `--subject <text>`, `--body <text>`, `--type <type>` (default: status), `--priority <prio>` (default: normal), `--thread <id>`, `--agent <name>` (sender, defaults to "operator"), `--payload <json>`
- Generates UUID for message ID
- Inserts into mail.db via the MailStore
- Prints confirmation: "✓ Sent msg-{id} to {to}"

`grove mail reply`:
- Flags: `--id <msg-id>` (message to reply to), `--body <text>`, `--agent <name>`
- Looks up original message, uses its thread_id (or creates one from original id)
- Swaps from/to
- Inserts reply

`grove mail purge`:
- Flags: `--agent <name>` (purge for specific agent), `--all` (purge everything)
- Deletes from mail.db
- Prints count of purged messages

### 2. `grove clean`

**File:** `src/commands/clean.rs` (new)

Reference: `reference/clean.ts` (793 lines)

The nuclear cleanup command. Flags:
- `--sessions` — wipe sessions.db
- `--mail` — wipe mail.db  
- `--events` — wipe events.db
- `--metrics` — wipe metrics.db
- `--merge-queue` — wipe merge-queue.db
- `--worktrees` — remove all .overstory/worktrees/*, delete git worktrees, delete branches
- `--tmux` — kill all overstory-* tmux sessions
- `--all` — all of the above
- `--force` — skip confirmation prompt

Steps for `--worktrees`:
1. List directories in `.overstory/worktrees/`
2. For each: `git worktree remove --force <path>`
3. Delete the branch: `git branch -D <branch>`
4. Clean up session store entries

Steps for `--tmux`:
1. List tmux sessions matching `overstory-{project}-*`
2. Kill each: `tmux kill-session -t <name>`
3. Log synthetic session-end events

Steps for database wipes:
1. Delete the .db file
2. Recreate with empty schema via the store's `create_tables()`

Print summary: "✓ Clean complete" with counts.

### 3. `grove stop`

**File:** `src/commands/stop.rs` (new)

Reference: `reference/stop.ts` (250 lines)

Stop a running agent. Args: `<agent-name>` or `--all`.
- Flags: `--force` (SIGKILL instead of SIGTERM), `--grace <ms>` (grace period, default 2000)

Steps:
1. Look up agent in sessions.db
2. If PID exists, send SIGTERM (or SIGKILL with --force)
3. Wait grace period
4. If still alive, send SIGKILL
5. Kill tmux session: `tmux kill-session -t <session-name>`
6. Update session state to "completed" in sessions.db
7. Log session-end event

### 4. `grove nudge`

**File:** `src/commands/nudge.rs` (new)

Reference: `reference/nudge.ts` (280 lines)

Send text to an agent's tmux session. Args: `<agent-name> <message>`.
- Runs: `tmux send-keys -t <tmux-session> "<message>" Enter`
- Verify session exists first
- Print confirmation

### 5. `grove merge`

**File:** `src/commands/merge.rs` (new) + `src/merge/resolver.rs` (new)

Reference: `reference/merge-resolver.ts` (1,125 lines)

`grove merge` subcommands:
- `grove merge <branch>` — merge a specific branch
- `grove merge --all` — merge all pending from queue
- `grove merge --list` — list merge queue

The resolver implements 3 tiers (AI-resolve is Phase 5, skip for now):

**Tier 1: Clean merge**
```
git merge --no-edit <branch>
```
If exit code 0 → success.

**Tier 2: Auto-resolve with content-displacement detection**
When git merge produces conflicts:
1. List conflicted files: `git diff --name-only --diff-filter=U`
2. For each file, read conflict markers
3. Parse `<<<<<<<`, `=======`, `>>>>>>>` sections
4. Keep incoming (theirs) changes
5. BUT — compare "ours" content against the base: if lines from "ours" that existed before the merge are being dropped, flag as `ContentDisplaced`
6. Stage resolved files: `git add <file>`
7. Commit: `git commit --no-edit`

**Return type must be the enum from architecture doc:**
```rust
pub enum MergeOutcome {
    Clean { merged_files: Vec<String> },
    Resolved { tier: ResolutionTier, resolutions: Vec<ConflictResolution> },
    ContentDisplaced { tier: ResolutionTier, displaced: Vec<DisplacedHunk>, resolutions: Vec<ConflictResolution> },
    Failed { attempted_tiers: Vec<ResolutionTier>, reason: String },
}
```

When `ContentDisplaced` is returned, print a warning showing what was lost. Do NOT silently succeed.

Update merge-queue.db status after merge.

### 6. `grove init`

**File:** `src/commands/init.rs` (new)

Reference: `reference/init.ts` (900 lines)

Initialize `.overstory/` in the current project:
1. Create directory structure: `.overstory/`, `.overstory/worktrees/`, `.overstory/specs/`, `.overstory/logs/`, `.overstory/agent-defs/`
2. Write default `config.yaml` with project name from `--name` flag or directory name
3. Copy agent definitions from embedded assets (build.rs embeds `agents/*.md`)
4. Write `agent-manifest.json`
5. Write `.overstory/.gitignore` (ignore *.db, worktrees/, logs/)
6. Run `git add .overstory/ && git commit -m "chore: initialize overstory"`
7. Print success with next steps

Flags: `--name <project-name>`, `--yes` (skip confirmation)

### 7. `grove hooks install/uninstall`

**File:** `src/commands/hooks.rs` (new)

Reference: `reference/hooks.ts` (280 lines)

Manage Claude Code lifecycle hooks in `.claude/settings.local.json`:
- `grove hooks install` — write hooks config that calls `grove log` on session start/end
- `grove hooks uninstall` — remove the hooks config
- `grove hooks status` — show current hook state

### 8. `grove spec write`

**File:** `src/commands/spec.rs` (new)

Reference: `reference/spec.ts` (130 lines)

Write a spec file:
- `grove spec write <task-id> --body <text>` — writes to `.overstory/specs/<task-id>.md`
- `grove spec write <task-id> --file <path>` — copies file content
- `grove spec read <task-id>` — prints spec content

## File Scope

New files:
- `src/commands/clean.rs`
- `src/commands/stop.rs`
- `src/commands/nudge.rs`
- `src/commands/init.rs`
- `src/commands/hooks.rs`
- `src/commands/spec.rs`
- `src/merge/mod.rs`
- `src/merge/resolver.rs`

Modified files:
- `src/commands/mail.rs` (add send/reply/purge)
- `src/commands/mod.rs` (register new modules)
- `src/main.rs` (wire new commands)
- `src/types.rs` (add MergeOutcome, DisplacedHunk, ConflictResolution if not present)

## Quality Gates

- `cargo build` — clean compilation
- `cargo test` — all tests pass (write tests for: mail send roundtrip, clean wipes db, merge resolver tiers, init creates structure, spec write/read)
- `cargo clippy -- -D warnings` — no warnings

## Acceptance Criteria

1. `grove mail send --to agent1 --subject "test" --body "hello"` inserts into mail.db and `grove mail list` shows it
2. `grove clean --sessions --force` wipes sessions.db and recreates empty schema
3. `grove stop <agent>` kills the process and tmux session, updates session state
4. `grove nudge <agent> "hello"` sends text to tmux
5. `grove merge <branch>` performs clean merge or auto-resolve with content-displacement detection
6. `grove init --name testproject --yes` creates full `.overstory/` structure
7. `grove hooks install` writes correct hooks JSON
8. `grove spec write task-123 --body "do the thing"` creates the spec file
9. Merge resolver NEVER returns success when content was displaced — always surfaces the warning
