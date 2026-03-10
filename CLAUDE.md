# Grove

Rust rebuild of overstory — multi-agent orchestration for AI coding agents.

## What This Is

Grove orchestrates multiple AI coding agents (Claude Code, Codex, Gemini, Copilot) working in parallel on a codebase. Each agent gets its own git worktree, reads instructions from an overlay file, communicates via a SQLite mail system, and merges work back through a tiered conflict resolver.

## Tech Stack

- **Language:** Rust 2021 edition
- **CLI:** clap v4 (derive macros)
- **Database:** rusqlite with bundled SQLite (WAL mode, 5s busy timeout)
- **Serialization:** serde + serde_json + serde_yaml
- **Async:** tokio (coordinator event loop, TUI, streaming modes only)
- **TUI:** ratatui + crossterm
- **HTTP:** reqwest (Claude API calls in planner)

## Architecture

**No tmux.** All agents are direct child processes with stdout/stderr piped to log files. PIDs tracked in sessions.db. A monitor daemon detects process death via `/proc/<pid>`.

**Multi-runtime.** Four real adapters (Claude, Codex, Gemini, Copilot) + three stubs (Pi, Sapling, OpenCode). Each adapter writes its overlay to the correct instruction file:
- Claude → `.claude/CLAUDE.md`
- Codex → `AGENTS.md`
- Gemini → `GEMINI.md`
- Copilot → `.github/copilot-instructions.md`

**Coordinator is a Rust event loop**, not an LLM session. LLM called only for one-shot task decomposition.

Most commands are synchronous: read config → open DB → query → format → print. Only coordinator, dashboard, feed --follow, and watchdog need async.

## Project Layout

```
src/
├── main.rs              # CLI entry point (clap), 35 commands
├── types.rs             # All shared types with serde derives
├── config.rs            # YAML config loader
├── errors.rs            # Typed errors (thiserror)
├── json.rs              # JSON output helpers
├── commands/            # One file per command
├── db/                  # SQLite stores (sessions, mail, events, metrics, merge_queue)
├── runtimes/            # Runtime adapters (claude, codex, gemini, copilot, registry)
├── coordinator/         # Event loop + LLM planner
├── merge/               # Tiered conflict resolver
├── tui/                 # ratatui dashboard (views, widgets)
├── watchdog/            # PID health monitoring
├── agents/              # Agent manifest + overlay renderer
├── process/             # Child process spawning + monitoring
├── worktree/            # Git worktree management
└── logging/             # Terminal output formatting
reference/               # Overstory TypeScript source (behavior reference, not to port verbatim)
agents/                  # Agent definition markdown files (language-agnostic)
templates/               # Overlay template (rendered per-agent by sling)
docs/                    # Phase specs, architecture, retro, gap analysis
tests/                   # Integration tests
```

## Quality Gates

```bash
cargo build              # Must compile clean
cargo test               # All tests pass (453 currently)
cargo clippy -- -D warnings  # No warnings
```

## Conventions

- Types in `src/types.rs`, errors in `src/errors.rs`
- DB stores in `src/db/` — one file per database
- Commands in `src/commands/` — one file per command
- Tests in same file (`#[cfg(test)] mod tests`)
- `--json` output must match overstory's JSON schema
- Runtime adapters implement the `AgentRuntime` trait in `src/runtimes/mod.rs`

## Runtime Adapters

Each adapter implements `AgentRuntime` (see `src/runtimes/mod.rs`):
- `build_headless_command()` — argv for spawning the agent
- `deploy_config()` — write overlay + hooks to the worktree
- `instruction_path()` — where the overlay file goes
- `build_env()` — environment variables for the child process

Per-capability routing via config:
```yaml
runtime:
  default: claude
  capabilities:
    builder: codex    # builders use Codex
    lead: claude      # leads use Claude
```

## SQLite Schemas (interop with overstory)

**sessions.db:** sessions (id, agent_name, capability, worktree_path, branch_name, task_id, tmux_session, state, pid, parent_agent, depth, run_id, started_at, last_activity, escalation_level, stalled_since, transcript_path)

**mail.db:** messages (id, from_agent, to_agent, subject, body, type, priority, thread_id, payload, read, created_at)

**events.db:** events (id, run_id, agent_name, session_id, event_type, tool_name, tool_args, tool_duration_ms, level, data, created_at)

**metrics.db:** sessions (...token counts, cost, duration) + token_snapshots

**merge-queue.db:** merge_queue (id, branch_name, task_id, agent_name, files_modified, enqueued_at, status, resolved_tier)

## Current Status

27,315 lines, 453 tests, 35 commands, 4 runtime adapters. See `CONTEXT.md` for detailed state and `docs/retro.md` for 38 entries of build history and lessons learned.

## Critical: Incremental Commits

**Commit after every meaningful change.** Sessions can be interrupted at any time. Uncommitted work is lost permanently.

```bash
# After every function/module:
git add <files> && git commit -m "feat: <what>"
```

Commit every 3-5 minutes. Small, frequent commits. This is a worktree branch — history doesn't matter, survival does.
