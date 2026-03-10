---
name: grove-orchestrate
description: Use this skill whenever orchestrating multi-agent builds with grove — dispatching tasks to AI coding agents, monitoring their progress, and merging their work. Triggers include any mention of 'grove sling', 'dispatch agents', 'multi-agent build', 'orchestrate', 'spawn builders', coordinating parallel agents, or planning how to decompose a task for multiple agents. Also use when reviewing a grove phase spec or discussing agent decomposition strategy. This skill encodes 38 retro entries of hard-won lessons from building a 27,000-line Rust project with multi-agent orchestration.
---

# Grove Orchestration

How to orchestrate multi-agent builds with grove. This encodes patterns and anti-patterns from 38 retro entries building a 27,000-line Rust codebase with AI agents.

## The Workflow

```
Write spec → Sling agents → Monitor → Merge → Verify
```

1. **Write the spec** in `docs/` with deliverables, file scope, verification commands, acceptance criteria
2. **Sling agents** with `grove sling <task> --runtime <rt> --capability builder --name <n> --spec <path> --files <scope>`
3. **Start the monitor** with `grove monitor start` — watches PIDs for lifecycle transitions
4. **Watch progress** with `grove status` or `grove dashboard`
5. **Merge** with `grove merge --branch overstory/<agent>/<task>` when agents complete
6. **Verify** by running the spec's verification commands
7. **Clean** with `grove clean --worktrees --sessions`

## Runtime Selection

Pick the right runtime for the job:

- **Claude Code** (`--runtime claude`): Best reasoning, follows complex specs, handles ambiguity. Use for architectural work, complex refactors, lead agents. Expensive.
- **Codex** (`--runtime codex`): Cheaper, good for straightforward implementation tasks. Use `--no-directives` to prevent parallel cargo build deadlock (RETRO-036). Best for: create file X, implement function Y, write tests for Z.
- **Gemini** (`--runtime gemini`): Alternative option. Use `--runtime gemini`.
- **Copilot** (`--runtime copilot`): GitHub-native. Reads `.github/copilot-instructions.md`.

Per-capability routing lets you mix runtimes:
```yaml
# .overstory/config.yaml
runtime:
  default: claude
  capabilities:
    builder: codex      # cheap parallel builders
    lead: claude        # smart leads
    coordinator: claude # coordinator needs reasoning
```

## Decomposition Rules

These rules prevent the merge conflicts and integration failures that plagued early phases:

**Rule 1: One builder per integration point.** If main.rs, mod.rs, or any shared file needs changes from multiple builders, assign ONE builder to own that file. Other builders create new files only.

**Rule 2: Explicit file scope.** Every builder gets `--files <list>` restricting what they touch. Builders that share file scope WILL conflict.

**Rule 3: No more than 3 parallel builders on a Rust project.** Cargo builds are CPU-intensive. 3 parallel compilations are fine on 8 cores. 6 will thrash (RETRO-006).

**Rule 4: Sequential merge, not batch.** Merge each builder's branch as it completes. Don't wait for all builders to finish — conflicts compound.

**Rule 5: Spec before dispatch.** Design decisions go in the spec, not in mail sent mid-build. Agents that have already committed won't see late-arriving mail (RETRO-028).

## Writing Good Dispatch Messages

The dispatch message to `grove coordinator send` or the spec given to `grove sling --spec` must include:

1. **What to build** — concrete deliverables, not vague goals
2. **File scope** — which files each builder owns
3. **Verification commands** — shell commands that prove the work is correct
4. **What NOT to do** — especially for Codex agents: "Do NOT run quality gates" or "Run gates sequentially, not in parallel"
5. **Retro callouts** — reference specific RETRO entries for known pitfalls

Bad dispatch:
> "Build the login page"

Good dispatch:
> "Create src/auth/login.rs implementing the LoginPage component. Write tests in the same file. Verification: `cargo test auth::login` must pass. Do NOT modify main.rs — that's owned by the integration builder. Commit after each function."

## Monitoring

```bash
# Start lifecycle monitor (detects when agent PIDs die)
grove monitor start

# Check status
grove status           # summary
grove status --json    # machine-readable
grove dashboard        # TUI with 7 views

# Agent completed? Check the branch
git log --oneline overstory/<agent>/<task> -5

# Agent stuck? Check its log
cat .overstory/logs/<agent>/*/stderr.log | tail -20

# Agent dead with no commits? Check why
grove status --json | jq '.agents[] | select(.agentName=="<n>")'
```

## Merge Workflow

```bash
# Merge one agent's branch
grove merge --branch overstory/<agent>/<task>

# If conflicts, resolve manually then:
grep -rn "<<<<<<" src/ --include="*.rs"  # ALWAYS check for conflict markers
cargo build && cargo test                 # verify

# Merge all completed branches
grove merge --all
```

## Anti-Patterns (from retro)

**Don't trust "cargo test passes" as proof of correctness.** Agents write tests that test their own assumptions. Quality gates prove compilation, not behavior. Always run the spec's verification commands (RETRO-001, 002).

**Don't send design changes via mail after dispatch.** Agents may have already committed by the time mail arrives. Put everything in the spec upfront (RETRO-028).

**Don't let the coordinator go idle.** If using `grove coordinator start`, monitor it. Coordinators can get stuck in poll loops (RETRO-004) or fail to merge branches (RETRO-009, 016).

**Don't skip E2E testing.** At the end of every phase: build, test, then manually run every new/changed command against real data. Not mocks, not unit tests — real commands in a real project (RETRO-014).

**Don't have multiple builders modify main.rs.** This causes merge conflicts that break struct definitions. One builder owns main.rs, period (RETRO-008).

**Don't use Codex with quality gates in the overlay.** Codex fires all tool calls in parallel. Three parallel `cargo` commands deadlock on the Cargo lock file. Use `--no-directives` or make gates a single sequential command (RETRO-036).

## Quick Reference

```bash
# Initialize a project
grove init

# Sling a Claude builder
grove sling my-task --capability builder --name my-builder \
  --spec docs/my-spec.md --files src/feature/

# Sling a Codex builder (cheaper, skip directives)
grove sling my-task --runtime codex --capability builder --name my-builder \
  --spec docs/my-spec.md --files src/feature/ --no-directives \
  --skip-task-check --no-scout-check

# Monitor
grove monitor start
grove status
grove dashboard

# Merge
grove merge --branch overstory/my-builder/my-task

# Clean up
grove clean --worktrees --sessions
```
