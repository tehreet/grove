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

## Pattern Summary

Most failures fall into two categories:

1. **Verification gap:** Agents check "does it compile?" but not "does it work?" Quality gates need to include runtime behavior checks, not just static analysis.

2. **Interface gap:** Parallel builders don't coordinate on shared files/APIs. When builder A writes to a file and builder B also writes to the same file, the last one wins and may clobber the first. Leads need to enforce interface contracts.

Both of these are solvable in grove's coordinator by making verification commands and interface contracts first-class parts of the spec format.
