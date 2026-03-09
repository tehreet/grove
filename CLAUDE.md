# Grove

Rust rebuild of overstory — multi-agent orchestration for AI coding agents.

**You are building a new Rust CLI, not porting TypeScript line-by-line.** Write idiomatic Rust. Use the reference TypeScript in `reference/` to understand behavior and data contracts, but the code you write should feel native to Rust.

## Tech Stack

- **Language:** Rust 2021 edition, stable toolchain
- **CLI:** clap v4 with derive macros
- **Database:** rusqlite with bundled SQLite (WAL mode, 5s busy timeout)
- **Serialization:** serde + serde_json + serde_yaml
- **Async:** tokio (only where needed — coordinator event loop, TUI, --follow modes)
- **TUI:** ratatui + crossterm
- **Errors:** thiserror for typed errors, anyhow for application-level
- **HTTP:** reqwest (for Claude API calls in planner + AI-resolve)
- **Git:** shelling out to git CLI via std::process::Command (libgit2 via git2 crate for merge operations)

## Architecture

Most commands are synchronous: read config → open database → query → format → print. Only the coordinator (persistent event loop), dashboard (TUI), feed --follow (streaming), and watchdog (daemon) need async.

All database access uses rusqlite synchronously. No async database layer. This matches the original bun:sqlite pattern — synchronous, WAL mode, concurrent access from multiple processes.

## Conventions

- All shared types go in `src/types.rs`
- All errors in `src/errors.rs` using thiserror
- Database stores in `src/db/` — one file per database (sessions.rs, mail.rs, events.rs, metrics.rs, merge_queue.rs)
- Commands in `src/commands/` — one file per command
- Every command that supports `--json` must produce output compatible with the TypeScript version's `--json` output
- Use `colored` crate for terminal colors, matching the brand palette in `reference/color.ts`
- Tests go in the same file as the code (`#[cfg(test)] mod tests`)
- Integration tests in `tests/`

## Quality Gates

```bash
cargo build          # Must compile clean
cargo test           # All tests pass
cargo clippy         # No warnings
cargo fmt --check    # Formatted
```

## Reference Material

- `reference/types.ts` — All 72 shared types. Port these to `src/types.rs` with serde derives.
- `reference/config.ts` — YAML config loader. Port to `src/config.rs`.
- `reference/errors.ts` — Error types. Port to `src/errors.rs`.
- `reference/index.ts` — CLI entry point with all 35 commands. Port to `src/main.rs` with clap.
- `reference/*-store.ts` — Database stores. Port to `src/db/*.rs`.
- `reference/color.ts` + `reference/theme.ts` — Terminal output styling. Port to `src/logging/`.
- `agents/*.md` — Agent definitions (language-agnostic, used as-is).
- `templates/overlay.md.tmpl` — Overlay template (used as-is, rendered with string replace).

## SQLite Schemas

These are the data contracts. Grove reads and writes the same databases as overstory.

**sessions.db:** sessions (id, agent_name, capability, worktree_path, branch_name, task_id, tmux_session, state, pid, parent_agent, depth, run_id, started_at, last_activity, escalation_level, stalled_since, transcript_path) + runs (id, started_at, completed_at, description)

**mail.db:** messages (id, from_agent, to_agent, subject, body, type, priority, thread_id, payload, read, created_at)

**events.db:** events (id, run_id, agent_name, session_id, event_type, tool_name, tool_args, tool_duration_ms, level, data, created_at)

**metrics.db:** sessions (agent_name, task_id, capability, started_at, completed_at, duration_ms, exit_code, merge_result, parent_agent, input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens, estimated_cost_usd, model_used, run_id) + token_snapshots (id, agent_name, input_tokens, output_tokens, cache_read_tokens, cache_creation_tokens, estimated_cost_usd, model_used, run_id, created_at)

**merge-queue.db:** merge_queue (id, branch_name, task_id, agent_name, files_modified, enqueued_at, status, resolved_tier)

<!-- mulch:start -->
## Project Expertise (Mulch)
<!-- mulch-onboard-v:1 -->

This project uses [Mulch](https://github.com/jayminwest/mulch) for structured expertise management.

**At the start of every session**, run:
```bash
mulch prime
```

This injects project-specific conventions, patterns, decisions, and other learnings into your context.
Use `mulch prime --files src/foo.ts` to load only records relevant to specific files.

**Before completing your task**, review your work for insights worth preserving — conventions discovered,
patterns applied, failures encountered, or decisions made — and record them:
```bash
mulch record <domain> --type <convention|pattern|failure|decision|reference|guide> --description "..."
```

Link evidence when available: `--evidence-commit <sha>`, `--evidence-bead <id>`

Run `mulch status` to check domain health and entry counts.
Run `mulch --help` for full usage.
Mulch write commands use file locking and atomic writes — multiple agents can safely record to the same domain concurrently.

### Before You Finish

1. Discover what to record:
   ```bash
   mulch learn
   ```
2. Store insights from this work session:
   ```bash
   mulch record <domain> --type <convention|pattern|failure|decision|reference|guide> --description "..."
   ```
3. Validate and commit:
   ```bash
   mulch sync
   ```
<!-- mulch:end -->

<!-- seeds:start -->
## Issue Tracking (Seeds)
<!-- seeds-onboard-v:1 -->

This project uses [Seeds](https://github.com/jayminwest/seeds) for git-native issue tracking.

**At the start of every session**, run:
```
sd prime
```

This injects session context: rules, command reference, and workflows.

**Quick reference:**
- `sd ready` — Find unblocked work
- `sd create --title "..." --type task --priority 2` — Create issue
- `sd update <id> --status in_progress` — Claim work
- `sd close <id>` — Complete work
- `sd dep add <id> <depends-on>` — Add dependency between issues
- `sd sync` — Sync with git (run before pushing)

### Before You Finish
1. Close completed issues: `sd close <id>`
2. File issues for remaining work: `sd create --title "..."`
3. Sync and push: `sd sync && git push`
<!-- seeds:end -->

<!-- canopy:start -->
## Prompt Management (Canopy)
<!-- canopy-onboard-v:1 -->

This project uses [Canopy](https://github.com/jayminwest/canopy) for git-native prompt management.

**At the start of every session**, run:
```
cn prime
```

This injects prompt workflow context: commands, conventions, and common workflows.

**Quick reference:**
- `cn list` — List all prompts
- `cn render <name>` — View rendered prompt (resolves inheritance)
- `cn emit --all` — Render prompts to files
- `cn update <name>` — Update a prompt (creates new version)
- `cn sync` — Stage and commit .canopy/ changes

**Do not manually edit emitted files.** Use `cn update` to modify prompts, then `cn emit` to regenerate.
<!-- canopy:end -->
