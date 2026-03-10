---
name: grove-spec-writer
description: Use this skill when writing task specs for grove multi-agent builds — phase documents, task decompositions, or any spec that will be given to AI agents via `grove sling --spec`. Triggers include 'write a spec', 'phase spec', 'task decomposition', 'plan a build', 'break this into tasks', or any time you need to define work for grove agents. Good specs are the #1 predictor of build success — this skill encodes what makes specs succeed or fail based on 38 retro entries.
---

# Grove Spec Writing

How to write task specs that AI agents actually execute correctly. Based on 38 retro entries — the specs that followed these patterns succeeded, the ones that didn't caused rework.

## Spec Template

Every spec lives in `docs/` and follows this structure:

```markdown
# Phase N: Title

## Context
Why this work exists. What it depends on. One paragraph.

## Deliverables
Numbered list. Each deliverable is concrete and testable.

### 1. Feature Name
What to build. Be specific about behavior, not just "implement X".
Include the file path where the code should go.
Include what the output should look like.

### 2. Another Feature
...

## File Scope
Which files each builder owns. This is the most important section
for multi-builder specs.

**Builder 1 — Name (OWNS file-a.rs, file-b.rs):**
- What builder 1 does
- Files: src/commands/file-a.rs, src/commands/file-b.rs

**Builder 2 — Name (OWNS file-c.rs, file-d.rs):**
- What builder 2 does
- Files: src/new-file-c.rs, src/new-file-d.rs

**Do NOT modify:** src/main.rs (owned by integration builder)

## Verification Commands
Shell commands that prove the work is correct.
Copy-paste-and-run. Not prose descriptions.

\```bash
# Feature 1 works
grove my-command --flag value
# Expected: output contains "success"

# Feature 2 works
cargo test my_module::tests
# Expected: all pass

# Integration check
grove status --json | jq '.agents | length'
# Expected: 0

# Quality gates
cargo build && cargo test && cargo clippy -- -D warnings
\```

## Acceptance Criteria
Numbered list. Each is binary pass/fail.

1. `grove my-command` produces correct output
2. All new code has unit tests
3. All existing tests still pass
4. Clippy clean
```

## What Makes Specs Fail

These are the actual failure modes from building grove, each traced to a retro entry:

**Vague deliverables (RETRO-001, 002):**
> Bad: "Implement the merge command"
> Good: "Implement `grove merge --branch <name>` that merges the named branch into the canonical branch using a 4-tier resolver. Tier 1: git merge --no-ff. Tier 2: auto-resolve via `similar` crate diffing. Return JSON with `{merged: bool, tier: string, conflicts: [string]}`."

**Missing verification commands (RETRO-001):**
Without executable verification, agents declare victory after `cargo test` passes. cargo test checks compilation and unit tests — it does NOT check that the feature works as intended. Every deliverable needs a shell command that tests the actual behavior.

**Missing main.rs wiring (RETRO-007, 011):**
This happened twice. Builders created implementation files but never added the clap struct, enum variant, and match arm in main.rs. The spec must explicitly say: "Add `Commands::MyCmd(MyCmdArgs)` variant and wire the match arm in main.rs."

**Shared file conflicts (RETRO-008):**
Three builders all modified main.rs. Merge produced corrupted Rust code — struct definitions jammed together with missing closing braces. The fix: ONE builder owns main.rs. Others create new files only.

**Design after dispatch (RETRO-028):**
Theme direction was sent as mail after builders had already committed. They never saw it. All design decisions must be in the spec BEFORE dispatch.

**Parallel quality gates with Codex (RETRO-036):**
Codex runs tool calls in parallel. Three parallel `cargo` commands deadlock. For Codex specs: either use `--no-directives`, or make quality gates a single command: `cargo build && cargo test && cargo clippy -- -D warnings` (one shell command, sequential).

## File Ownership Rules

For multi-builder specs, these rules prevent merge conflicts:

1. **Each builder gets exclusive files.** No two builders share a file in their scope.
2. **New files preferred.** Builders creating new files never conflict with each other.
3. **One builder owns all integration points.** main.rs, mod.rs, app.rs — one builder wires everything.
4. **Explicitly list "Do NOT modify" files.** Prevents builders from helpfully "fixing" things outside their scope.

Example decomposition for a 3-builder phase:

```
Builder 1 — Feature Implementation (OWNS src/commands/new_cmd.rs):
  Creates the new file, writes implementation + tests

Builder 2 — Another Feature (OWNS src/commands/other_cmd.rs):
  Creates another new file, writes implementation + tests

Builder 3 — Integration (OWNS src/main.rs, src/commands/mod.rs):
  Wires both new commands into the CLI
  Runs final verification
```

## Verification Command Patterns

Good verification commands are copy-paste-runnable and check behavior:

```bash
# Check a command exists and runs
grove my-cmd --help | grep "description text"

# Check output format
grove my-cmd --json | python3 -c "import sys,json; d=json.load(sys.stdin); assert 'field' in d"

# Check file was created
[ -f .overstory/some-file ] && echo "PASS" || echo "FAIL"

# Check no stubs remain
grep "not_yet_implemented" src/main.rs | grep -c "MyCmd"
# Expected: 0

# Check interop with overstory
ov my-cmd --json | python3 -c "..." # same check against ov
```

Bad verification:
> "Verify that the merge command works correctly"

Good verification:
```bash
# Create a test branch with a change
git checkout -b test-merge && echo "test" > test.txt && git add -A && git commit -m "test"
git checkout main
grove merge --branch test-merge --json | python3 -c "import sys,json; d=json.load(sys.stdin); assert d['merged']==True"
git branch -D test-merge
```

## Single-Builder vs Multi-Builder

**Use a single builder when:**
- All changes are in tightly coupled files
- The task is small (<500 lines of changes)
- Files need to be modified together atomically

**Use multiple builders when:**
- Changes span independent modules
- Each module can be tested independently
- You want speed (parallel execution)

For multi-builder, always include an integration builder or integration step.

## Estimating Builder Count

Rule of thumb from our build:
- 1 builder: up to ~500 lines, single module
- 2 builders: ~500-1500 lines, 2 independent modules
- 3 builders: ~1500-3000 lines, 3+ independent modules + integration
- Never more than 4 on a Rust project (cargo build contention)
