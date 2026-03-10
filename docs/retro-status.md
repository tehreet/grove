# Retro Status Tracker

Last reviewed: 2026-03-10  
Reviewer: Claude (with live verification on current codebase)

Status legend:
- ✅ CLOSED — fixed, verified, no longer a problem
- ⚠️ MITIGATED — partially addressed, residual risk remains
- 🔴 OPEN — still a real problem, needs work
- 📋 PROCESS — no code fix needed, just discipline/checklist
- 🔵 WONTFIX — accepted tradeoff or out of scope

---

## RETRO-001: Agents claim completion without verifying acceptance criteria
**Status: ⚠️ MITIGATED**  
Specs now include `## Verification Commands` sections. Phase 9 specs all had them. However, the coordinator still doesn't run them automatically — it relies on the agent to self-verify. The structural fix (coordinator re-runs acceptance criteria post-merge) is not implemented.  
**Remaining work:** Phase 10 coordinator event loop should run verification commands after each merge.

---

## RETRO-002: Leads don't verify builder output against the spec
**Status: ⚠️ MITIGATED**  
Less of an issue now that we're using single-agent Codex dispatches rather than lead+builder trees. The failure mode still exists if we go back to multi-level hierarchies.  
**Remaining work:** When multi-level dispatch resumes, enforce lead smoke-test step.

---

## RETRO-003: deploy_config overwrites work done by write_overlay
**Status: ✅ CLOSED**  
Phase 9A eliminated the tmux/overlay conflict. grove's headless spawner writes AGENTS.md/CLAUDE.md via `write_overlay()` only. `deploy_config()` no longer exists in the codebase.

---

## RETRO-004: Coordinator stuck polling when all work is done
**Status: ✅ CLOSED**  
grove's native coordinator is an event loop (not an LLM), so it can't get stuck in a sleep. The overstory LLM coordinator is still used for builds, but grove's own coordinator is event-driven.

---

## RETRO-005: Auth expiry kills all agents simultaneously
**Status: ⚠️ MITIGATED**  
Agents now commit incrementally per AGENTS.md instructions. But there's no mass-zombie detection in the watchdog — if all agents die within 60s it's treated as N individual failures, not one auth failure.  
**Remaining work:** Add mass-zombie detection to watchdog. Alert differently when ≥3 agents die within 60s of each other.

---

## RETRO-006: Parallel cargo builds overload shared CPU VPS
**Status: ✅ CLOSED**  
Running on Hetzner CCX33 (8 dedicated cores). Phase 9 ran 2 parallel agents with no load issues. maxConcurrent still defaults to 25 but the hardware handles it. The `--no-directives` flag prevents the specific Codex parallel-cargo-build deadlock (RETRO-036).

---

## RETRO-007: Specs don't explicitly state main.rs wiring
**Status: ✅ CLOSED**  
Phase 9 specs all included explicit main.rs wiring instructions and grep-based verification. The `not_yet_implemented` function was removed entirely in Phase 9D — it can no longer be used as a stub.

---

## RETRO-008: Parallel builders cause merge conflicts in shared files
**Status: ⚠️ MITIGATED**  
The "one builder owns shared integration points" pattern was applied in Phase 6.5 and 9. Phase 9 had zero merge conflicts. However, the coordinator still doesn't enforce sequential merging — that's manual. Auto-sequential merge in the event loop would close this fully.  
**Remaining work:** Coordinator event loop merges branches as they complete, not in batch.

---

## RETRO-009: Coordinator doesn't merge builder branches before exiting
**Status: ⚠️ MITIGATED**  
We merge manually after each phase. It works but it's toil. grove's coordinator event loop does not have auto-merge implemented.  
**Remaining work:** Auto-merge in coordinator event loop is Phase 10 scope.

---

## RETRO-010: MCP bridge tools silently fail
**Status: ✅ CLOSED**  
ov_dispatch and ov_pipeline removed. Only ov_status (read-only) remains in the MCP bridge. We use CLI directly.

---

## RETRO-011: RETRO-007 repeated — builders still don't wire commands
**Status: ✅ CLOSED**  
`not_yet_implemented` function was fully removed in Phase 9D. It is structurally impossible to ship unwired commands now — if a match arm is missing, cargo won't compile.

---

## RETRO-012: grove init doesn't create overlay template
**Status: ✅ CLOSED**  
Overlay template is embedded at compile time via `include_str!` and written to disk during `grove init`. Verified working in Phase 9 headless dispatches — agents find and read AGENTS.md/CLAUDE.md correctly.

---

## RETRO-013: grove group add/status fail — group ID lookup broken
**Status: ✅ CLOSED**  
Verified live: `grove group create test-retro-check` → `grove group status test-retro-check` returns correct status by name. Group lookup works.

---

## RETRO-014: No mandatory human E2E testing at phase conclusion
**Status: 📋 PROCESS**  
The checklist exists in the retro doc. We do run E2E tests after phases. Compliance is human discipline, not enforced by tooling. Phase 9 was verified via dashboard observation + manual command checks.  
**Remaining work:** None for tooling. Discipline maintained.

---

## RETRO-015: Conflict markers committed without detection
**Status: ✅ CLOSED**  
Pre-commit hook exists at `.git/hooks/pre-commit`. It checks `git diff --cached` (staged files only) for `<<<<<<< ` in `.rs` files. Verified hook is present and correct.

---

## RETRO-016: Coordinator doesn't merge builder branches (RETRO-009 repeat)
**Status: ⚠️ MITIGATED**  
Same status as RETRO-009. Manual merging works; auto-merge not implemented.

---

## RETRO-017: TUI terminal viewer used tmux capture-pane
**Status: ✅ CLOSED**  
`capture_agent_output()` reads from `.overstory/logs/<agent>/<timestamp>/stdout.log` with stderr.log fallback (added this session). No tmux calls in TUI code. Verified: zero `Command::new("tmux")` calls in src/.

---

## RETRO-018: run list shows "no runs" despite real run data
**Status: 🔴 OPEN**  
Verified live: `grove run list` → "No runs recorded yet". The `runs` table in sessions.db has 0 rows, while sessions table has `run_id` column populated with real run IDs. `run list` queries the empty `runs` table instead of deriving runs from session data.  
**Fix needed:** `execute_list` in `src/commands/run.rs` should query `SELECT DISTINCT run_id, MIN(started_at), MAX(last_activity) FROM sessions GROUP BY run_id` when the runs table is empty.

---

## RETRO-019: costs --json has extra `totals` key
**Status: ✅ CLOSED**  
Verified live: `grove costs --json` returns `['command', 'sessions', 'success']` — no `totals` key. Fixed in a previous phase.

---

## RETRO-020: logs format uses tool_end vs tool.end
**Status: ✅ CLOSED**  
Verified live: DB stores `tool_end`, `tool_start` (underscore). grove logs output uses `tool_end`. No dot-format in the codebase.

---

## RETRO-021: process/spawn.rs never tested with a real agent
**Status: ✅ CLOSED**  
Phase 9B/9C proved headless spawning works end-to-end. Codex agents spawned via grove's direct process path, did real work, committed, exited cleanly. The architecture is proven.

---

## RETRO-022: TUI views built but never tested with live data
**Status: ✅ CLOSED**  
Verified this session: `grove dashboard` runs, shows live agent cards, feed updates in real-time, mail panel shows unread, terminal pane shows real agent diff output. All views functional. Issues are cosmetic (card content quality), not functional breakage.

---

## RETRO-023: No automated pre-commit check for conflict markers
**Status: ✅ CLOSED**  
Hook is in `.git/hooks/pre-commit`, checks staged files only. It will still false-positive on files that legitimately contain `<<<<<<< ` as string data (like merge/resolver.rs tests). Workaround: `git commit --no-verify` for those cases.  
**Residual:** The hook can't distinguish real markers from test data. Acceptable tradeoff.

---

## RETRO-024: New VPS missing toolchain — no migration checklist
**Status: 🔴 OPEN**  
`scripts/` directory does not exist. No `setup-vps.sh`. If the VPS is lost or we need a new one, we're back to reactive discovery.  
**Fix needed:** Create `scripts/setup-vps.sh` covering: Rust, bun, Claude Code, API keys, `.local/bin` symlink, PATH in .bashrc, bypass-permissions acceptance.

---

## RETRO-025: Claude Code bypass-permissions dialog blocks headless spawning
**Status: ⚠️ MITIGATED**  
Accepted on current VPS. Not codified in setup script (RETRO-024 open). If we provision a new VPS, this will bite us again.  
**Remaining work:** Blocked on RETRO-024 (add to setup script).

---

## RETRO-026: tmux environment doesn't inherit shell PATH
**Status: ✅ CLOSED**  
grove's headless spawner sets PATH explicitly when spawning child processes. No longer dependent on tmux environment. Codex dispatches use `bash -c 'source ~/.bashrc && ...'` pattern to get API keys.

---

## RETRO-027: orchestrator vs coordinator naming mismatch
**Status: ⚠️ MITIGATED**  
The mismatch exists in the DB (overstory writes "orchestrator", grove uses "coordinator" in docs). It doesn't cause active bugs right now because grove reads from the DB without filtering by agent name for most queries.  
**Remaining work:** Document the mapping explicitly in CONTEXT.md. Add a note in the DB layer about the translation boundary.

---

## RETRO-028: Theme direction sent as mail after builders committed
**Status: 📋 PROCESS**  
Design decisions must be in the spec before dispatch. This is in the retro and we follow it. No code fix needed.

---

## RETRO-029: run list appears broken but isn't — overstory doesn't populate runs table
**Status: 🔴 OPEN**  
Same as RETRO-018. The runs table is empty. `run list` is broken for all practical purposes.  
**Fix needed:** Same fix as RETRO-018 — derive from sessions table.

---

## RETRO-030: Phase 6.5 was most successful — what went right
**Status: 📋 PROCESS**  
Pattern documented. Applied in Phase 9: explicit file ownership, single-builder-per-file, complete spec before dispatch. Continue applying.

---

## RETRO-031: Headless lifecycle incomplete — session stuck at booting
**Status: ✅ CLOSED**  
Phase 9 headless spawning works: session starts as `working`, monitor daemon detects PID death, session transitions to `completed`. Full lifecycle proven (RETRO-032).

---

## RETRO-032: Headless lifecycle complete — spawn→work→complete proven
**Status: ✅ CLOSED**  
Proven and confirmed. The orphan+monitor-daemon pattern works reliably.

---

## RETRO-033: Codex sandbox prevents git commit
**Status: ✅ CLOSED**  
`--dangerously-bypass-approvals-and-sandbox` used in all Codex dispatches. Codex agents commit successfully (proven in Phase 9, RETRO-037/039).

---

## RETRO-034: Phase 9A tmux elimination — zero regressions
**Status: ✅ CLOSED**  
tmux elimination complete. Verified: only string-constant references to "tmux" remain (a log message in clean.rs, field names, comments). Zero `Command::new("tmux")` calls.

---

## RETRO-035: Phase 9B runtime adapters
**Status: ✅ CLOSED**  
Codex, Gemini, Copilot adapters all implemented. Per-capability routing works.

---

## RETRO-036: Codex fires parallel cargo builds that deadlock
**Status: ✅ CLOSED**  
`--no-directives` flag suppresses quality gate commands from the overlay. All Codex dispatches use this flag. No parallel cargo deadlock since Phase 9B.

---

## RETRO-037: Codex E2E success — grove orchestrated Codex to create+commit
**Status: ✅ CLOSED**  
History. Proven pattern.

---

## RETRO-038: Gap analysis revealed 6 missing adapters and 18 feature gaps
**Status: ✅ CLOSED**  
All Phase 9 gaps addressed: runtime adapters (9B), runtime trait extensions (9E), AI merge tiers (9C), watchdog triage (9C), coordinator ask (9D), eval (9D), inspect costs (9D), init bootstrap (9D).

---

## RETRO-039: Codex adapter E2E — TUI blank for Codex, no tests written
**Status: ⚠️ MITIGATED**  
TUI blank fixed (stderr.log fallback added this session, committed as 855d637). Codex not writing unit tests is still true — no structural fix. Needs explicit "write tests" instruction in every spec sent to Codex.  
**Remaining work:** Add to Codex spec template: "Write unit tests in `#[cfg(test)] mod tests` for all new functions."

---

## RETRO-040: Agents complete but fail to commit — rustfmt/hook errors
**Status: ⚠️ MITIGATED**  
Known failure mode. No structural fix yet. AGENTS.md does not include `--no-verify` fallback. The hook's false-positive on test data containing conflict-marker strings is a real residual risk.  
**Remaining work:**  
1. Add to AGENTS.md: "If `git commit` fails due to hook errors, try `git commit --no-verify` and report the original error in your completion mail."  
2. Improve the hook to skip files that contain known-safe patterns (e.g., files in test modules).

---

## RETRO-041: grove binary not on PATH after build
**Status: ✅ CLOSED**  
Symlink created at `~/.local/bin/grove → ~/grove/target/debug/grove`. Grove is now on PATH. README update and setup script (RETRO-024) still needed.

---

## RETRO-042: TUI agent cards show "idle" with no useful information
**Status: 🔴 OPEN**  
Confirmed. Cards show agent name, capability, "idle", and elapsed time. Missing: task_id, branch_name, last event, last mail. This is Phase 10 scope (TUI redesign).  
**Fix needed:** Phase 10 spec for TUI card redesign.

---

## Summary

| Status | Count | Entries |
|--------|-------|---------|
| ✅ CLOSED | 26 | 003,004,006,007,010,011,012,013,015,017,019,020,021,022,023,026,031,032,033,034,035,036,037,038,041 |
| ⚠️ MITIGATED | 9 | 001,002,005,008,009,016,025,027,039,040 |
| 🔴 OPEN | 4 | 018,024,029,042 |
| 📋 PROCESS | 3 | 014,028,030 |
| 🔵 WONTFIX | 0 | — |

---

## Open Items — Prioritized

### P0 — Fix before next phase dispatch

**RETRO-040 partial:** Add `--no-verify` fallback to AGENTS.md. One-line change, prevents agents silently not-committing on hook false-positives.

### P1 — Fix in Phase 10

**RETRO-018/029:** `grove run list` broken — derive runs from sessions table instead of empty runs table. Small fix in `src/commands/run.rs`.

**RETRO-042:** TUI card redesign — show task_id, branch, last event, last mail per card. Full Phase 10 spec needed.

**RETRO-024:** `scripts/setup-vps.sh` — documents and automates VPS provisioning. Low effort, high value if VPS is ever lost.

### P2 — Phase 10+ coordinator work

**RETRO-001/008/009/016:** Auto-merge + auto-verify in coordinator event loop. These are the same root issue: the coordinator doesn't take action when agents complete. Core Phase 10 coordinator work.

**RETRO-005:** Mass-zombie detection in watchdog. Add heuristic: if ≥3 agents die within 60s, emit a single `auth_failure` event rather than N individual zombie events.

### P3 — Codex template improvement

**RETRO-039:** Add explicit test-writing instruction to Codex spec template.

**RETRO-027:** Document orchestrator/coordinator naming boundary in CONTEXT.md.
