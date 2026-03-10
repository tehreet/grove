# Grove Build Retro — Lessons for Improving Overstory

## What Went Wrong and What We Learned

This doc tracks process failures we discovered while building grove with overstory. Each entry is a concrete lesson about how overstory's orchestration model fails and what we should fix — both in overstory and in grove's eventual coordinator.

---

### RETRO-001: Agents claim completion without verifying acceptance criteria

**What happened:** Phase 2 agents wrote `clean.rs`, `mail.rs`, `init.rs` etc. and reported done. But `mail send --from` didn't exist (it was `--agent`), `clean --force` wasn't implemented, clean didn't delete git branches, and 6 commands weren't wired in main.rs at all.

**Root cause:** Builder agents ran `cargo build` and `cargo test` (quality gates) and those passed. But the quality gates don't test feature completeness — they test compilation and unit tests. The spec said "mail send with --from flag" but no quality gate verified that `grove mail send --from x --to y` actually works.

**Lesson for overstory/grove:**
- Quality gates are necessary but not sufficient. They prove "it compiles" not "it works."
- Specs need **executable acceptance criteria** — actual commands to run with expected output. Not prose descriptions.
- The lead agent should run the acceptance criteria before reporting completion, not just check that builders passed quality gates.
- Consider a post-merge verification step: after all builders merge, run an integration test suite against main.

**Proposed fix for grove's coordinator:**
- Specs should include a `## Verification Commands` section with literal shell commands
- The coordinator (or a reviewer agent) runs those commands after merge
- If any fail, the task is reopened automatically

---

### RETRO-002: Leads don't verify builder output against the spec

**What happened:** The `write-cmds-lead` spawned a `system-cmds-builder` and a `scaffold-cmds-builder`. Both completed. The lead merged both and reported done. But the lead never checked that the commands were actually wired in main.rs — it just verified the files existed and cargo build passed.

**Root cause:** Lead agents verify builder work by checking quality gates and reading diffs. They don't actually run the built commands. The lead saw new `.rs` files, saw cargo build pass, and declared victory.

**Lesson for overstory/grove:**
- Leads should be required to run at least one smoke test per deliverable
- "File exists" is not verification. "Command runs and produces expected output" is verification.
- The lead's review phase should include executing the acceptance criteria from the spec

---

### RETRO-003: deploy_config overwrites work done by write_overlay

**What happened:** `sling.rs` correctly called `write_overlay()` to render the template and write CLAUDE.md. Then it called `runtime.deploy_config()` which writes settings.local.json AND overwrites CLAUDE.md with an empty string (because overlay_content was passed as "").

**Root cause:** Two functions both write to the same file. The second one doesn't check if the first one already wrote something. This is a coordination bug between `agents/overlay.rs` and `runtimes/claude.rs` — they were built by different builders in different worktrees who didn't know about each other's work.

**Lesson for overstory/grove:**
- When two builders touch related files, the lead must specify the interface contract between them
- File ownership should be explicit: "overlay.rs OWNS CLAUDE.md, runtimes/claude.rs OWNS settings.local.json, neither touches the other's file"
- Parallel builders working on the same subsystem need shared interface specs, not just file scope

---

### RETRO-004: Coordinator gets stuck polling when all work is done

**What happened:** All Phase 0 agents completed but the coordinator sat in a `sleep 300` polling loop for 20+ minutes. We had to manually nudge it with "All agents are done — merge remaining branches and push."

**Root cause:** The coordinator is an LLM session. It chose to poll by running `sleep 300 && ov status`. Once in that sleep, it can't be interrupted until the timeout. It also didn't check exit triggers because `allAgentsDone` was set to `false`.

**Lesson for overstory/grove:**
- Default exit triggers should be `true` not `false` for task-based coordinator runs
- The coordinator's poll interval should be shorter (30-60s, not 300s)
- Grove's native coordinator (event loop, not LLM) won't have this problem — it checks state on every tick

---

### RETRO-005: Auth expiry kills all agents simultaneously with no recovery

**What happened:** OAuth token expired mid-Phase 3. All 6 agents + coordinator went zombie simultaneously. Uncommitted work was lost.

**Root cause:** All agents share the same OAuth session. When it expires, every agent fails at once. There's no checkpoint mechanism — agents commit only at the end of their work.

**Lesson for overstory/grove:**
- Agents MUST commit incrementally (we added this to CLAUDE.md)
- The watchdog should detect mass-zombie events (all agents die within 60s) and report it as an auth/infrastructure failure, not individual agent failures
- Grove should consider token refresh as a first-class concern

---

### RETRO-006: Parallel cargo builds overload shared CPU VPS

**What happened:** 6 agents all running `cargo build` simultaneously pushed the VPS to load average 24.64 on 8 cores. SSH connections dropped.

**Root cause:** Each Rust compilation is CPU-intensive. 6 parallel compilations on a shared-CPU VPS is too much. Overstory's maxConcurrent defaults to 25 which is fine for TypeScript (low CPU) but not for Rust (high CPU).

**Lesson for overstory/grove:**
- maxConcurrent should be tuned per-project based on the build system's resource needs
- Consider adding a `maxParallelBuilds` config separate from `maxConcurrent` — you can have 10 agents thinking but only 2 compiling at once
- The watchdog could monitor system load and delay new spawns when load is high

---

### RETRO-007: Specs need to explicitly state what commands to wire in main.rs

**What happened:** Phase 2 spec said "Modified files: src/main.rs — wire new commands to implementations." Builders wrote the implementation files but didn't touch main.rs. The lead didn't catch it.

**Root cause:** "Wire new commands" is vague. The spec should have said: "Replace `Commands::Init(_) => not_yet_implemented(...)` with `Commands::Init(args) => commands::init::execute(args)` for each new command."

**Lesson for overstory/grove:**
- Specs for integration work must be explicit about the exact code changes needed at integration points
- "Wire X to Y" is not a spec. "In file Z, replace line A with line B" is a spec.
- Consider having the spec include a grep-based verification: `grep 'not_yet_implemented.*init' src/main.rs` should return 0 matches after Phase 2

---

### RETRO-008: Parallel builders cause merge conflicts in shared files (main.rs, mod.rs)

**What happened:** Phase 4 had 3 parallel builders all adding commands. When merging their branches, `src/commands/mod.rs` and `src/main.rs` had conflicts. The mod.rs conflict was simple (both sides adding `pub mod` lines). The main.rs conflict was destructive — the "keep both sides" resolution jammed struct definitions together, removing closing braces. `InspectArgs` got `CoordinatorStopArgs` pasted into the middle of its definition. Three separate manual fixes were needed to get it compiling.

**Root cause:** Three builders all modified main.rs (adding clap struct definitions and match arms) and mod.rs (adding module declarations). Overstory's merge system didn't handle this — the coordinator didn't merge at all (it was idle), so we had to merge manually. The auto-merge resolution ("keep both") doesn't understand Rust syntax and jammed code blocks together without proper delimiters.

**Lesson for overstory/grove:**
- `main.rs` is a merge bottleneck. When 3+ builders all add to it, conflicts are guaranteed.
- The coordinator MUST merge branches sequentially, not leave them for manual resolution. Each merge resolves before the next starts.
- Better: have a single "integration builder" whose job is solely to wire everything into main.rs after the implementation builders finish. This builder gets the exclusive file scope for main.rs and mod.rs.
- Even better: structure the code so main.rs is generated or uses a registration pattern (like an inventory of commands) so parallel additions don't conflict.

**Proposed fix for grove's coordinator:**
- Detect when multiple builders have `main.rs` or `mod.rs` in their file scope
- Merge them sequentially, not in parallel
- Or: assign a dedicated wiring task after all builders complete

---

### RETRO-009: Coordinator doesn't merge builder branches before exiting

**What happened:** Phase 4 coordinator sat idle while all 6 agents completed. When we stopped it, no branches had been merged to main. We had to manually merge 3 builder branches, encountering the conflicts described in RETRO-008.

**Root cause:** The coordinator (LLM-based) received mail from leads saying "done" but didn't act on it — it was in a poll-sleep loop or had lost context. The exit triggers fired (allAgentsDone=true) before it could merge.

**Lesson for overstory/grove:**
- The coordinator must merge completed branches as part of its normal operation, not as a separate cleanup step
- Exit triggers should not fire until all pending merges are complete
- Grove's native coordinator should have a merge step in its event loop: "if agent completed and branch not merged, merge it"

---

### RETRO-010: MCP bridge tools (ov_dispatch, ov_pipeline) silently fail

**What happened:** We tried to dispatch a bugfix task using the `ov_dispatch` MCP tool. It returned `{"dispatched":true}` but no agent was spawned. The dashboard was empty. We had to fall back to `ov sling` directly.

**Root cause:** `ov_dispatch` was a custom wrapper in the claude-bridge MCP server that called `ov dispatch` — a command that doesn't exist in overstory. It was leftover experimental code that was never removed. The tool reported success because the spawn was detached and the error was swallowed.

**Lesson for overstory/grove:**
- MCP tools that wrap CLI commands must verify the command exists before wrapping it
- "Fire and forget" (detached spawn with no exit code check) is dangerous — always verify the subprocess succeeded
- We removed ov_dispatch and ov_pipeline from the MCP server entirely. Only ov_status (read-only) remains.
- Going forward: use overstory's actual CLI commands directly (`ov sling`, `ov coordinator send`), not MCP wrappers

---

### RETRO-011: RETRO-007 repeated — builders still don't wire commands in main.rs

**What happened:** Phase 5 dispatch message explicitly said "RETRO-007: Wire every command in main.rs — no not_yet_implemented stubs remaining." The builders wrote 8 implementation files (logs.rs, replay.rs, metrics_cmd.rs, monitor.rs, watch_cmd.rs, prime.rs, ecosystem.rs). All 8 are still stubs in main.rs.

**Root cause:** The retro lesson was communicated to the coordinator but not enforced structurally. The builders' file scope didn't include main.rs. The leads didn't check main.rs. The verification commands in the spec would have caught this, but the coordinator went zombie before running them.

**Lesson for overstory/grove:**
- Calling out retro lessons in the dispatch message is not enough. The lesson must be encoded in the spec's file scope and acceptance criteria.
- Every phase spec must include main.rs in file scope if new commands are being added
- The spec's verification commands must include: `grep 'not_yet_implemented.*<command>' src/main.rs` should return 0 matches
- Consider making "no remaining stubs" a quality gate, not just an acceptance criterion

**Proposed fix:** Add a CI check or quality gate that fails if any command in the clap enum dispatches to not_yet_implemented when an implementation file exists in src/commands/

---

### RETRO-012: `grove init` doesn't create templates/overlay.md.tmpl

**What happened:** `grove init` creates `.overstory/config.yaml`, agent manifest, hooks, etc. But it doesn't copy `templates/overlay.md.tmpl` into the project. When `grove sling` tries to spawn an agent, it fails with "Failed to read overlay template — No such file or directory."

**Root cause:** The init command was modeled after overstory's `ov init` which also doesn't copy the template — overstory finds the template from its npm package installation directory. But grove is a standalone binary with no package directory. The template needs to be either embedded at compile time (via `include_str!` in `build.rs`) or written to disk during `grove init`.

**Lesson for overstory/grove:**
- When porting from a package-manager-distributed tool to a standalone binary, all runtime assets must be embedded or bundled
- `init` must create EVERYTHING needed for `sling` to work. If sling depends on a file, init must create it.
- E2E testing catches this — unit tests don't. The sling unit tests worked because the grove repo already had the template. A fresh `grove init` project did not.

**Proposed fix:** Embed the overlay template at compile time with `include_str!("../../templates/overlay.md.tmpl")` and write it during `grove init`. Also update `sling` to fall back to the embedded template if the file doesn't exist on disk.

---

### RETRO-013: `grove group add` and `grove group status` fail — group ID lookup broken

**What happened:** After `grove group create e2e-group`, both `grove group add e2e-group e2e-test` and `grove group status e2e-group` return "Group not found." The group was created (shows in `grove group list`) but can't be referenced by name.

**Root cause:** The group is stored with an auto-generated ID (e.g., `group-0a43bfbf`) but the `add` and `status` commands may be looking up by the generated ID rather than the name, or vice versa. The create command prints the name but stores with a different key.

**Lesson for overstory/grove:**
- Commands that create resources must clearly communicate the identifier needed to reference them later
- Integration testing must cover the full lifecycle: create → list → reference → modify → status
- If a resource has both a name and an ID, both should work as lookup keys

---

### RETRO-014: Mandatory human E2E testing at phase conclusion

**What happened:** Multiple phases shipped with bugs that unit tests didn't catch: empty overlays, unwired commands, missing templates, broken group lookups. Every one of these was caught immediately by running the actual commands manually.

**Root cause:** We relied on AI-generated unit tests to verify correctness. These tests test the functions the agents wrote, using the assumptions the agents had. They don't test the integrated system from a user's perspective. The real bugs are at integration boundaries — between commands, between init and sling, between create and status.

**Lesson for overstory/grove:**
- At the conclusion of EVERY phase, we MUST manually E2E test every command that was created or changed
- The testing must follow a user workflow: init → sling → status → mail → clean. Not isolated command checks.
- Document results and improvement ideas in the retro
- No phase is complete until E2E testing passes AND results are documented
- AI-generated tests verify AI's assumptions. Human testing verifies actual behavior.

---

## Process Improvement: Phase Conclusion Checklist

After every phase, before moving to the next:

1. **Merge all branches** to main
2. **Build and run unit tests:** `cargo build && cargo test`
3. **E2E test every new/changed command** against real data (not mocks)
4. **Test the user workflow:** init → sling → status → mail → log → clean
5. **Test interop:** grove reads what ov writes and vice versa
6. **Document bugs found** in this retro
7. **Dispatch bugfixes** with explicit verification commands
8. **Verify bugfixes** pass the same E2E tests
9. **Push to GitHub**
10. **Update CONTEXT.md** with current state

---

## Pattern Summary

Most failures fall into four categories:

1. **Verification gap:** Agents check "does it compile?" but not "does it work?" Quality gates need to include runtime behavior checks, not just static analysis.

2. **Interface gap:** Parallel builders don't coordinate on shared files/APIs. When builder A writes to a file and builder B also writes to the same file, the last one wins and may clobber the first. Leads need to enforce interface contracts.

3. **Merge gap:** The coordinator doesn't merge branches reliably. Branches pile up, merge conflicts accumulate, and manual intervention is required. The coordinator must merge sequentially as agents complete, not batch at the end.

4. **Completeness gap:** Individual components work but the integrated system doesn't. Init creates a project, but the project can't spawn agents. Groups can be created but not referenced. Commands compile but aren't wired. Only end-to-end user-workflow testing catches these.

All four are solvable in grove's coordinator by making verification commands, interface contracts, sequential merge steps, and E2E test suites first-class parts of the orchestration loop.

---

### RETRO-015: Conflict markers committed without detection — need pre-commit check

**What happened:** Phase 6.5 had 4 builder branches merged manually. The mail-reader-builder conflicted with terminal-view-builder in 3 files (app.rs, views/mod.rs, status_bar.rs). I resolved status_bar.rs but committed app.rs and views/mod.rs with conflict markers still in them (`<<<<<<< HEAD`, `=======`, `>>>>>>>`). cargo build then failed with cryptic syntax errors.

**Root cause:** Manual merge conflict resolution is error-prone. `git add -A && git commit` happily commits conflict markers. No pre-commit hook or quality gate checks for them.

**Lesson for overstory/grove:**
- ALWAYS run `grep -rn "<<<<<<" src/` before committing after any merge
- Add this as a git pre-commit hook: reject commits containing conflict markers in source files
- The coordinator should run this check after every merge as part of its event loop
- When resolving conflicts manually, resolve ALL files before committing — don't do partial commits
- `cargo build` catches it eventually but the error messages are misleading ("unexpected token" rather than "you have conflict markers")

**Proposed fix:** Add to grove's `hooks install` command: a pre-commit hook that runs `grep -rn "<<<<<<< " src/ && echo "CONFLICT MARKERS FOUND" && exit 1`

---

### RETRO-016: Coordinator doesn't merge builder branches (RETRO-009 repeat)

**What happened:** Phase 6.5 coordinator spawned 3 leads, each spawned builders. All completed. The coordinator went idle and never merged any of the 4 builder branches. We had to merge manually, encountering conflicts.

**Root cause:** Same as RETRO-009. The LLM-based coordinator doesn't reliably merge branches as part of its workflow. It receives "done" signals but doesn't act on them.

**Lesson:** This is the third time. The coordinator merge gap is structural, not incidental. Grove's native Rust coordinator MUST have automatic merge as a first-class step in its event loop — not an LLM decision.

---

### RETRO-017: TUI terminal viewer uses tmux capture-pane — contradicts grove's architecture

**What happened:** The Phase 6.5 terminal viewer reads agent output via `tmux capture-pane`. This reintroduces a tmux dependency into grove when the entire architecture (doc/architecture.md) was built to eliminate tmux for agent spawning.

**Root cause:** The spec said "reads tmux session content" because the TUI needs to show agent output, and right now agents are spawned by overstory (which uses tmux). The builder followed the spec literally.

**Why it's OK for now:** Grove's TUI currently monitors overstory-managed agents, which DO use tmux sessions. The tmux read is backward-compatible monitoring, not agent spawning.

**What must change when grove spawns its own agents:**
- grove agents use direct process pipes (process/spawn.rs + process/monitor.rs)
- Agent stdout is captured as NDJSON events in events.db
- The terminal viewer should read from the agent's log file (`.overstory/logs/<agent>.log`) or from the NDJSON event stream, not tmux
- `capture_tmux()` becomes a fallback for legacy tmux sessions, not the primary path

**Proposed fix:** Add a `capture_agent_output(agent)` function that tries: (1) read from agent log file, (2) read from NDJSON events, (3) fall back to tmux capture. Wire this into the TUI instead of `capture_tmux()` directly.

---

### RETRO-018: `run list` shows "no runs" despite real run data existing

**What happened:** `grove run list` returns "No runs recorded yet" even though we've completed multiple runs (Phase 5, 6, 6.5 all had coordinator runs with run IDs visible in `grove status`).

**Root cause:** The `run` command likely queries a different DB table or field than where overstory stores run records. Overstory may store run state in sessions.db as metadata on the coordinator session, or in a separate runs table. The grove `run` command was built by an agent that assumed a specific DB schema that may not match.

**Lesson:** Commands that query databases need their DB layer tested against REAL data written by overstory, not just mock data in unit tests. Every DB query should be verified by: (1) writing data with ov, (2) reading it with grove.

**Proposed fix:** Inspect the actual sessions.db schema, compare with what `src/commands/run.rs` queries, and fix the mismatch.

---

### RETRO-019: `costs --json` has extra `totals` key not in overstory's output

**What happened:** `grove costs --json` returns `{command, sessions, success, totals}` but `ov costs --json` returns `{command, sessions, success}`. The `totals` key is an addition that breaks JSON schema compatibility.

**Root cause:** The costs builder added a convenience `totals` aggregate field. This is actually useful, but it breaks the interop contract that grove's JSON output must match overstory's schema exactly.

**Lesson:** Any JSON output enhancement must be documented as a grove-specific extension, or added to overstory as well. JSON consumers (like slop-dash) may break on unexpected keys.

**Proposed fix:** Either remove `totals` from grove's output, or add it to overstory. Document any intentional schema differences in CONTEXT.md.

---

### RETRO-020: `logs` format differs from overstory — `tool_end` vs `tool.end`

**What happened:** Grove's `logs` command outputs event types as `tool_end`, `tool_start`. Overstory outputs them as `tool.end`, `tool.start` (dot-separated). This means any tooling that parses log output (slop-dash, scripts) will break when switching between grove and ov.

**Root cause:** The events are stored in events.db with the dot format (that's what overstory writes). Grove's logs formatter likely transforms dots to underscores, or the event type is stored differently in grove's types.

**Lesson:** Output format must be byte-identical where interop is claimed. Don't "improve" formatting without checking compatibility.

---

### RETRO-021: grove's process/spawn.rs has never been tested with a real agent

**What happened:** Grove's core architectural advantage — direct process spawning without tmux — has never been E2E tested. Every actual agent run during grove's development went through overstory's tmux-based spawning. The Rust code in `process/spawn.rs` and `process/monitor.rs` compiles and passes unit tests but has never launched a real Claude Code process.

**Root cause:** We built grove iteratively using overstory to orchestrate the build. Overstory uses tmux. Grove's native spawning path was built in Phase 3 but we never switched to using it because overstory was working. The irony: we're building the replacement but never testing the replacement.

**Lesson:** The most important architectural decision (no tmux) is the least tested. Critical path code must be E2E tested even if there's a working alternative. "We'll test it later" means "we'll discover it's broken later."

**Proposed fix:** Before Phase 7 (distribution), do a dedicated integration test: grove spawns a real Claude Code agent through process/spawn.rs, captures its stdout, monitors it with the watchdog, and completes a simple task. If this doesn't work, grove's entire architectural thesis is unproven.

---

### RETRO-022: TUI views built but never tested with live data

**What happened:** Phase 6.5 added terminal viewer, split terminal, and mail reader views. All three compile and have unit tests. None have ever been run with actual live agent data. We don't know if the terminal viewer actually shows tmux content, if the split view actually renders multiple panels, or if the mail reader actually displays message bodies.

**Root cause:** The TUI views were built by agents who ran `cargo test` (unit tests pass) but couldn't test the actual rendering because the tests run headless. Manual TUI testing requires live agents in tmux sessions, which requires a separate dispatch — it's a chicken-and-egg problem.

**Lesson:** TUI features must be tested during the NEXT agent run after they're built, not "someday." The Phase 6.5 conclusion checklist should have included: "Run Phase 6.6 dispatch, then use the TUI to monitor it and verify terminal/split/mail views work."

**Proposed fix:** When we dispatch Phase 6.6, explicitly include a manual TUI testing step: while agents are building, open `grove dashboard`, navigate to terminal view, split view, and mail reader. Document what works and what doesn't.

---

### RETRO-023: No automated pre-merge quality gate for conflict markers

**What happened:** RETRO-015 documented committing conflict markers. This happened because there's no automated check.

**Root cause:** `git add -A && git commit` doesn't check file contents. `cargo build` eventually catches syntax errors from conflict markers, but with misleading error messages.

**Lesson:** This should be a git hook, not a human memory item.

**Proposed fix:** Add to `grove hooks install`:
```bash
# pre-commit hook
if grep -rn "<<<<<<< " src/ --include="*.rs" 2>/dev/null; then
    echo "ERROR: Conflict markers found in source files"
    exit 1
fi
```
