# Grove Project Context

**Last updated:** 2026-03-10

## What Is Grove

Grove is a Rust rebuild of [overstory](https://github.com/jayminwest/overstory), a multi-agent orchestration system for AI coding agents. It's not a line-by-line port — it fixes architectural problems in the TypeScript original. See `docs/architecture.md` for the full rationale.

**Repo:** https://github.com/tehreet/grove
**VPS:** Hetzner CCX33 (8 dedicated AMD cores, 32GB RAM) at `ubuntu-32gb-hil-1`
**Owner:** Josh (GitHub: tehreet)

## Current State

- **21,610 lines of Rust** across 48 source files
- **387 passing tests** (7 failures — all `*_no_db` edge cases from VPS migration, not real bugs)
- **31 working commands**, 4 stubs remaining (dashboard, eval, update, upgrade, completions)
- **Compiles clean** with `cargo build`
- **Interoperates** with overstory — reads/writes the same `.overstory/` databases

## Phase Status

| Phase | Spec | Status | What It Covers |
|-------|------|--------|----------------|
| Phase 0 | `docs/phase-0.md` | ✅ Done + Verified | Types, config, errors, DB layer, CLI skeleton, logging |
| Phase 1 | `docs/phase-1.md` | ✅ Done + Verified | Read commands: status, mail list/check, costs, doctor |
| Phase 2 | `docs/phase-2.md` | ✅ Done + Verified | Write commands: mail send/reply, clean, stop, nudge, init, spec, hooks, merge |
| Phase 3 | `docs/phase-3.md` | ✅ Done + Verified | Process management: worktree, runtimes, overlay, sling, log, watchdog |
| Phase 4 | `docs/phase-4.md` + `phase-4-bugfix.md` | ✅ Done + Verified | Coordinator daemon + observability: agents, group, run, feed, errors, inspect, trace |
| Phase 5 | `docs/phase-5.md` | ✅ Done + Verified | Feature parity: logs, replay, metrics, monitor, watch, prime, ecosystem, supervisor |
| Phase 6 | `docs/phase-6.md` | 📝 Spec Written | TUI dashboard (ratatui) |
| Phase 7 | `docs/phase-7.md` | 📝 Spec Written | Distribution: completions, update, upgrade, CI, install script |

## Command Status (35 total)

**Live (31):** agents, init, sling, spec, prime, stop, status, inspect, clean, doctor, coordinator, hooks, monitor, mail (list/check/send/reply/read/purge), merge, nudge, group, worktree, log, logs, watch, trace, ecosystem, feed, errors, replay, run, costs, metrics

**Stubs (4):** dashboard (Phase 6), eval (Phase 7 or later), update (Phase 7), upgrade (Phase 7), completions (Phase 7)

**Deprecated (1):** supervisor (prints deprecation message)

## Key Architecture Decisions

1. **No tmux for agent spawning.** Agents are child processes with stdin/stdout pipes. Tmux kept as optional fallback for interactive runtimes only.
2. **Coordinator is a Rust event loop, not an LLM.** Daemon mode with PID file + log file. LLM called only for one-shot task decomposition via Claude API.
3. **Typed merge outcomes.** `MergeOutcome::ContentDisplaced` forces handling of silently dropped content — fixing overstory's critical bug #89.
4. **Single binary distribution.** SQLite bundled via rusqlite. No Bun/npm/Node dependency.

## How We Build Grove

We use **overstory itself** to orchestrate the Rust build. The grove repo has `.overstory/config.yaml` with Rust quality gates (`cargo build`, `cargo test`, `cargo clippy`).

**Workflow:**
1. Write a phase spec in `docs/phase-N.md` with deliverables, file scope, verification commands, acceptance criteria
2. Start overstory coordinator: `cd /home/joshf/grove && ov coordinator start --no-attach`
3. Send the task: `ov coordinator send --subject "Build Phase N" --body "Read docs/phase-N.md..."`
4. Monitor: `ov status` or `ov dashboard` from the grove directory
5. After agents complete, merge any unmerged branches
6. Run verification commands from the spec
7. Fix bugs via `ov sling` dispatch with explicit bug specs
8. Push to GitHub

**Important rules:**
- Only use `sloperations:ov_status` MCP tool (read-only). Never use ov_dispatch or ov_pipeline — removed from server.
- For dispatching work: `ov sling` or `ov coordinator send` via `run_command`
- Agents must commit incrementally (CLAUDE.md has this rule)
- Every spec needs verification commands — agents won't self-verify otherwise (RETRO-001)
- main.rs wiring is ALWAYS missed — include it explicitly in file scope and verification (RETRO-007, RETRO-011)

## Known Issues

- **7 test failures:** `*_no_db` tests in metrics_cmd, run, worktree_cmd. These panic when no database exists instead of handling gracefully. Minor fix needed.
- **supervisor:** Still a stub. Should print deprecation message. Very low priority.

## File Layout

```
grove/
├── Cargo.toml              # Dependencies: clap, rusqlite, serde, ratatui, tokio, reqwest...
├── CLAUDE.md               # Agent instructions (read by overstory agents working on grove)
├── CONTEXT.md              # THIS FILE
├── agents/                 # Agent .md definitions (copied from overstory, used as-is)
├── templates/
│   └── overlay.md.tmpl     # Overlay template (rendered by grove sling)
├── reference/              # TypeScript source from overstory (behavioral reference, not ported 1:1)
├── docs/
│   ├── architecture.md     # Grand vision spec (the gist)
│   ├── phase-0.md through phase-7.md  # Phase specs
│   ├── retro.md            # 11 lessons learned (RETRO-001 through RETRO-011)
│   ├── bugs-and-gaps.md    # Bug tracker from Phase 0-2 testing
│   └── testing-plan.md     # Integration test plan
├── src/
│   ├── main.rs             # CLI entry point (clap, all 35 commands)
│   ├── types.rs            # All shared types (1,730 lines)
│   ├── config.rs           # YAML config loader
│   ├── errors.rs           # thiserror error types
│   ├── json.rs             # JSON output envelope
│   ├── logging/mod.rs      # Brand colors, formatters
│   ├── db/                 # Database layer (sessions, mail, events, metrics, merge_queue)
│   ├── commands/           # One file per command (~16 command files)
│   ├── agents/             # Overlay rendering, manifest parsing
│   ├── runtimes/           # Claude runtime adapter + registry
│   ├── process/            # Direct process spawning (grove's tmux replacement)
│   ├── worktree/           # Git worktree + tmux management
│   ├── merge/              # 4-tier resolver with content displacement detection
│   ├── coordinator/        # Native event loop + planner
│   ├── watchdog/           # Health monitoring
│   └── tui/                # (Phase 6 — not yet built)
├── tests/
│   └── smoke_real_db.rs    # Integration tests against real .overstory/ databases
└── .overstory/             # Overstory project config (overstory manages grove's own build)
```

## Overstory Fork

Our overstory fork with custom additions (verifier agent, TUI, mulch directives, eval system):
- **Repo:** https://github.com/tehreet/overstory
- **Location:** `/home/joshf/overstory`
- **35 commits ahead** of upstream jayminwest/overstory
- **PR #100** open: verifier agent + agent-browser integration

## Related Projects

- **slop-dash:** Next.js observability dashboard at `/home/joshf/slop-dash` (https://github.com/tehreet/slop-dash)
- **sloperations:** MCP server bridge at `/home/joshf/claude-bridge` — provides `ov_status`, `run_command`, `tmux_read/send`, `read_file`, `write_file`, `list_directory`

## What's Next

**Phase 6: TUI Dashboard** (`docs/phase-6.md`)
- ratatui terminal UI with overview, agent detail, event log, help views
- Keyboard navigation, live data polling, brand-themed styling

**Phase 7: Distribution** (`docs/phase-7.md`)
- Shell completions (clap_complete), self-update, cross-compilation CI, install script
- Goal: `curl -fsSL https://grove.sh/install | sh`

**Before Phase 6:** Fix the 7 `*_no_db` test failures from the VPS migration.
