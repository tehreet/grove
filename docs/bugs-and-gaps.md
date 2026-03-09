# Grove Bug & Gap List — Pre-Phase 4

## CRITICAL BUGS (will break Phase 4 coordinator)

### BUG-1: Overlay (CLAUDE.md) written empty [DISPATCHED — fix in progress]
- **Severity:** CRITICAL — agents spawn with zero instructions
- **Location:** `src/runtimes/claude.rs` `deploy_config()` 
- **Root cause:** `deploy_config()` is called with `overlay_content=""` and overwrites the CLAUDE.md that `write_overlay()` just wrote
- **Fix:** Skip CLAUDE.md write in deploy_config when overlay_content is empty

### BUG-2: `mail send --from` flag doesn't exist — uses `--agent` instead
- **Severity:** HIGH — overstory uses `--from`, all agent definitions reference `--from`
- **Location:** `src/main.rs` mail send clap definition
- **Fix:** Add `--from` as an alias for `--agent`, or rename `--agent` to `--from`

### BUG-3: `clean` doesn't delete git branches
- **Severity:** HIGH — stale branches accumulate after every run
- **Location:** `src/commands/clean.rs`
- **Fix:** After removing worktrees, iterate session records and `git branch -D` each branch_name before wiping sessions.db

### BUG-4: `clean --force` flag missing
- **Severity:** MEDIUM — can't skip confirmation prompt programmatically
- **Location:** `src/commands/clean.rs` clap definition
- **Fix:** Add `--force`/`-f` flag that skips any confirmation

### BUG-5: Unknown capability error message is misleading
- **Severity:** LOW — `grove sling test --capability nonexistent` gives hierarchy error instead of "unknown capability"
- **Location:** `src/commands/sling.rs` — hierarchy validation runs before capability validation
- **Fix:** Move capability existence check before hierarchy validation

### BUG-6: `--project /nonexistent` doesn't error
- **Severity:** MEDIUM — silently shows empty status instead of erroring
- **Location:** `src/commands/status.rs` or `src/config.rs`
- **Fix:** Check that project root exists and has .overstory/ before proceeding

### BUG-7: Duplicate agent name not caught before git failure
- **Severity:** LOW — error message is confusing ("git worktree add failed" instead of "agent name already in use")
- **Location:** `src/commands/sling.rs`
- **Fix:** The code checks sessions.db for active agents with same name, but the previous session was completed so the check passes. Also need to check if worktree directory already exists on disk.

## STUB COMMANDS (need implementation for feature parity)

### Priority 1 — Required for coordinator (Phase 4):
- `grove agents` — list agent definitions from manifest
- `grove worktree list` — list active worktrees
- `grove group list/add/status` — task group management (coordinator uses this)

### Priority 2 — Required for observability:
- `grove inspect <agent>` — deep agent inspection (transcript, events, tokens)
- `grove trace <agent>` — chronological event timeline
- `grove feed` — real-time event stream (--follow mode)
- `grove errors` — aggregated error events
- `grove logs` — NDJSON log queries
- `grove replay <agent>` — replay agent session
- `grove metrics` — session metrics display (different from costs)
- `grove monitor` — real-time agent monitoring

### Priority 3 — Nice to have:
- `grove prime` — context loading for agents
- `grove dashboard` — TUI (Phase 5)
- `grove run list/show` — run management
- `grove watch` — file watcher
- `grove ecosystem` — ecosystem tool management
- `grove completions` — shell completions
- `grove update/upgrade` — self-update
- `grove supervisor` — deprecated, low priority

## FEATURE GAPS (grove has the command but behavior differs from overstory)

### GAP-1: `mail list --to` filter may not work correctly
- Needs verification — test showed "No messages" when messages exist

### GAP-2: `grove init` prints "ov hooks install" instead of "grove hooks install"
- The success message references `ov` not `grove`

### GAP-3: `grove doctor` warns about cargo not found when run without ~/.cargo/env
- Need to ensure PATH is inherited correctly or check common cargo locations

### GAP-4: Sling `--from` agent defaults to "orchestrator" not "operator" for auto-dispatch mail
- Overstory may use a different default sender

### GAP-5: `mail send` uses `--agent` flag for sender instead of `--from`
- All overstory agent definitions and docs use `--from`
- This will cause every agent to fail when trying to send mail with `--from`
