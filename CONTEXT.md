# Grove Project Context

**Last updated:** 2026-03-10 (Phase 9B complete)

## What Is Grove

Grove is a Rust rebuild of [overstory](https://github.com/jayminwest/overstory), a multi-agent orchestration system for AI coding agents. It fixes architectural problems in the TypeScript original — most importantly, grove spawns agents as direct child processes instead of tmux sessions.

**Repo:** https://github.com/tehreet/grove
**VPS:** Hetzner CCX33 (8 dedicated AMD cores, 32GB RAM) at `ubuntu-32gb-hil-1`
**Owner:** Josh (GitHub: tehreet)

## Current State

- **27,315 lines of Rust** across ~85 source files
- **453 passing tests**, 0 failures
- **35 working commands**, 1 hidden stub (eval)
- **Compiles clean**, clippy clean
- **Interoperates** with overstory — reads/writes the same `.overstory/` databases
- **4 runtime adapters:** Claude Code, Codex (OpenAI), Gemini (Google), Copilot (GitHub)
- **Zero tmux dependency** — all agents spawned as direct child processes
- **Proven end-to-end:** Claude Code agent and Codex agent both successfully spawned, did work, and committed via grove

## Phase Status

| Phase | Status | What It Covers |
|-------|--------|----------------|
| Phase 0-5 | ✅ Done + Verified | All core commands, DB layer, process management, coordinator |
| Phase 6 | ✅ Done + Verified | TUI dashboard (ratatui) — 7 views |
| Phase 6.5 | ✅ Done + Verified | TUI enhancements — terminal viewer, mail reader, rich feed |
| Phase 6.6 | ✅ Done + Verified | TUI polish — Dracula theme, cost analytics, timeline/Gantt, toasts |
| Phase 7 | ✅ Done | Distribution — completions, update, upgrade, CI/CD, install.sh |
| Phase 8 | ✅ Done + Proven | Headless agent spawning (--headless flag, no tmux) |
| Phase 8.5 | ✅ Done + Proven | Headless lifecycle (spawn→working→completed via monitor daemon) |
| Phase 9A | ✅ Done | Eliminate tmux — deleted tmux.rs, 0 tmux binary calls |
| Phase 9B | ✅ Done + Proven | Runtime adapters — Codex, Gemini, Copilot + per-capability routing |
| Phase 9C | 📝 Spec Written | AI merge tiers 3-4, watchdog triage |
| Phase 9D | 📝 Spec Written | Coordinator ask, group close, eval, init bootstrap |
| Phase 9E | 📝 Spec Written | Config gaps (merge settings, print runtime) |

## Key Architecture Decisions

1. **No tmux.** Zero tmux binary calls. Agents are child processes with stdout/stderr piped to log files. PIDs tracked in sessions.db. Monitor daemon detects process death via /proc/<pid>.
2. **Multi-runtime.** Claude Code, Codex, Gemini, Copilot adapters. Each writes overlay to the correct instruction file (CLAUDE.md, AGENTS.md, GEMINI.md, copilot-instructions.md). Per-capability routing from config.
3. **Nudge via mail.** Not tmux send-keys. Async, reliable, works without tmux.
4. **Coordinator is a Rust event loop.** Daemon mode with PID file. LLM called only for one-shot task decomposition.
5. **Typed merge outcomes.** `MergeOutcome::ContentDisplaced` forces handling of silently dropped content.
6. **Single binary distribution.** SQLite bundled via rusqlite. No Bun/npm/Node dependency.

## How We Build Grove

We use **grove itself** (+ overstory as fallback when Claude Max quota is exhausted) to orchestrate builds:

**Primary workflow (grove):**
1. Write spec in `docs/phase-N.md`
2. `grove sling <task> --runtime codex --capability builder --name <n> --spec docs/phase-N.md --files <scope>`
3. `grove monitor start` — watches PIDs for lifecycle transitions
4. `grove status` or `grove dashboard` to monitor
5. `grove merge --branch overstory/<agent>/<task>` when complete
6. Verify, fix, push

**Fallback workflow (overstory):**
1. `ov coordinator start --no-attach && ov coordinator send --subject "..." --body "..."`
2. `ov dashboard` to monitor
3. When agents complete, merge and verify

**Rules:**
- Only use `sloperations:ov_status` MCP tool (read-only). For dispatching: `ov sling` or `grove sling` via `run_command`.
- Update `docs/retro.md` (RETRO-NNN format) for every process failure or architectural insight.
- Agents must commit incrementally.
- Specs need verification commands — agents won't self-verify (RETRO-001).
- Codex agents: use `--no-directives` to avoid parallel cargo deadlock (RETRO-036). Or ensure quality gates are a single sequential command.

## Remaining Gaps (vs overstory)

See `docs/gap-analysis.md` for full analysis. Key gaps:
- **Runtime trait methods:** buildPrintCommand, parseTranscript, parseEvents (needed for AI merge + cost tracking)
- **AI features:** Merge tiers 3-4 (LLM conflict resolution), watchdog triage (AI failure classification)
- **Commands:** coordinator ask (request-reply), group close/auto-close, eval system, init ecosystem bootstrap
- **Adapters:** Pi, Sapling, OpenCode are stubs (basic command generation only)

## Retro

38 entries in `docs/retro.md` covering every process failure, architectural insight, and milestone since Phase 0. Key themes:
- Verification gaps (RETRO-001, 002, 014): agents check "compiles" not "works"
- Merge gaps (RETRO-008, 009, 016): coordinator must merge sequentially as agents complete
- tmux elimination (RETRO-017, 031, 032, 034): proven architecture, zero tmux
- Multi-runtime (RETRO-033, 036, 037): Codex end-to-end success, sandbox/parallelism lessons
